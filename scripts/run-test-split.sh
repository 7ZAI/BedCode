#!/usr/bin/env bash
# 按 crate 批量拆分 Rust 单元测试，并逐 crate 编译 + 跑测试。
#
# 每个 crate 一个「拆分 → cargo test → 比对用例数」闭环：一个 crate 红了就停下，
# 不把错误滚到下一个 crate（那样定位成本会指数上升）。
#
# 用法：
#   scripts/run-test-split.sh --crate <子串> [--verdict private|public] [--min-test N]
#   scripts/run-test-split.sh --crate <子串> --list     # 只列待办不拆
#   DRY=1 scripts/run-test-split.sh --crate <子串>       # 只打印拆分计划
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CRATE=""
VERDICT=""
MIN_TEST=150
LIST_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --crate) CRATE="$2"; shift 2 ;;
    --verdict) VERDICT="$2"; shift 2 ;;
    --min-test) MIN_TEST="$2"; shift 2 ;;
    --list) LIST_ONLY=1; shift ;;
    *) shift ;;
  esac
done

if [ -z "$CRATE" ]; then
  echo "用法：$0 --crate <子串> [--verdict private|public] [--min-test N] [--list]" >&2
  exit 2
fi

# ==================== 待办清单 ====================
# 注意：审计的 JSON 有十几万字节，**管道会把它截断**，必须先落临时文件再读

AUDIT_JSON=$(mktemp)
trap 'rm -f "$AUDIT_JSON"' EXIT
node scripts/audit-rust-tests.mjs --json >"$AUDIT_JSON" 2>/dev/null

LIST=$(CRATE="$CRATE" VERDICT="$VERDICT" MIN_TEST="$MIN_TEST" node -e '
const fs = require("fs");
const { report } = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
const minTest = Number(process.env.MIN_TEST || 150);
const seen = new Set();
const rows = report
  .filter((r) => r.crate.includes(process.env.CRATE))
  .filter((r) => r.form === "inline")
  .filter((r) => !process.env.VERDICT || r.verdict === process.env.VERDICT)
  .filter((r) => r.testLines >= minTest)
  .sort((a, b) => b.testLines - a.testLines);
for (const r of rows) {
  if (seen.has(r.file)) continue; // 同文件只拆一次
  seen.add(r.file);
  console.log(`${r.verdict}\t${r.testLines}\t${r.file}\t${r.lib || ""}`);
}
' "$AUDIT_JSON")

if [ -z "$LIST" ]; then
  echo "没有匹配的待拆分块（crate=$CRATE verdict=${VERDICT:-全部} min-test=$MIN_TEST）"
  exit 0
fi

if [ "$LIST_ONLY" = "1" ]; then
  printf '%s\n' "$LIST"
  exit 0
fi

# ==================== crate 根与基线 ====================

FIRST_FILE=$(printf '%s\n' "$LIST" | head -1 | cut -f3)
# 从源文件向上找到第一个带 Cargo.toml 的目录（不靠路径正则猜）
CRATE_DIR=$(dirname "$FIRST_FILE")
while [ "$CRATE_DIR" != "." ] && [ ! -f "$CRATE_DIR/Cargo.toml" ]; do
  CRATE_DIR=$(dirname "$CRATE_DIR")
done
if [ ! -f "$CRATE_DIR/Cargo.toml" ]; then
  echo "找不到 crate 根（源文件 $FIRST_FILE）" >&2
  exit 3
fi
CRATE_ABS="$ROOT/$CRATE_DIR"

echo "=============================================================="
echo "crate 根：$CRATE_DIR"
echo "待拆块数：$(printf '%s\n' "$LIST" | wc -l | tr -d ' ')"
echo "=============================================================="

cd "$CRATE_DIR" || exit 3

# 基线：只跑一次，把计数解析出来
echo "--- 基线 cargo test ---"
BASE_OUT=$(~/.cargo/bin/cargo test 2>&1)
BASE_SUM=$(printf '%s' "$BASE_OUT" | grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' | head -1)
BASE_PASSED=$(printf '%s' "$BASE_SUM" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+')
BASE_FAILED=$(printf '%s' "$BASE_SUM" | grep -oE '[0-9]+ failed' | grep -oE '[0-9]+')
if [ -z "$BASE_PASSED" ]; then
  echo "基线没跑出计数，先手工确认（下面是第一段输出）："
  printf '%s\n' "$BASE_OUT" | tail -20
  exit 3
fi
BASE_NAMES=$(printf '%s' "$BASE_OUT" | sed -n '/^failures:$/,/^test result/p' | grep -E '^    [a-z_]+' | sed 's/^ *//' | sort)
echo "基线：passed=$BASE_PASSED failed=${BASE_FAILED:-0}"
[ -n "$BASE_NAMES" ] && printf '基线失败用例：\n%s\n' "$BASE_NAMES"

# ==================== 逐块拆分（逐文件事务） ====================
# 每个文件独立成事务：备份 → 拆 → cargo check。
# 红了就只回滚这一个文件并继续下一个——不累积到末尾才发现整仓爆掉
#（terminal-session 那次就是累积了 13 个文件才炸出 228 个错误，定位成本极高）。

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

SKIPPED=""
DONE=0
while IFS=$'\t' read -r verdict lines file lib; do
  [ -z "$file" ] && continue
  ARGS=(--file "$file" --target "$verdict")
  if [ "$verdict" = "public" ] && [ -n "$lib" ]; then
    ARGS+=(--lib "$lib")
  fi

  if [ "${DRY:-0}" = "1" ]; then
    echo ">>> [DRY] $file（$verdict，$lines 行）"
    node "$ROOT/scripts/split-rust-tests.mjs" "${ARGS[@]}" --dry | tail -n +2 | head -20
    continue
  fi

  # --- 事务开始：备份源文件 + 记下拆分目标目录 ---
  # 注意：LIST 里的路径相对仓库根，但此处 cwd 已在 crate 根——必须转绝对路径，
  # 否则 cp / rm -rf 会静默作用在不存在的路径上（回滚失效、残留目录）
  FILE_ABS="$ROOT/$file"
  stem=$(basename "$file" .rs)
  parentdir=$(dirname "$FILE_ABS")
  if [ "$verdict" = "public" ]; then
    # 公共目标：平铺在 crate 根的 tests/，不是 src/<stem>/tests/
    outdir="$CRATE_ABS/tests"
  elif [ "$stem" = "mod" ]; then
    # 目录模块入口 src/<dir>/mod.rs → 产物在 src/<dir>/tests/
    stem=$(basename "$parentdir")
    outdir="$parentdir/tests"
  elif [ "$stem" = "lib" ] || [ "$stem" = "main" ]; then
    outdir="$parentdir/tests"
  else
    outdir="$parentdir/$stem/tests"
  fi
  mkdir -p "$WORK/keep"
  cp "$FILE_ABS" "$WORK/keep/$(echo "$file" | tr '/' '_')"
  before_count=$(find "$outdir" -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')

  echo ">>> 拆分 $file（$verdict，$lines 行）"
  node "$ROOT/scripts/split-rust-tests.mjs" "${ARGS[@]}"
  rc=$?
  if [ "$rc" != "0" ]; then
    echo "!!! 拆分器拒绝（exit=$rc）：$file"
    cp "$WORK/keep/$(echo "$file" | tr '/' '_')" "$FILE_ABS"
    rm -rf "$outdir"
    SKIPPED="${SKIPPED}拒绝拆分\t${file}"$'\n'
    continue
  fi

  # --- 事务校验：编译 ---
  if ~/.cargo/bin/cargo check --lib --tests -q >"$WORK/check.log" 2>&1; then
    after_count=$(find "$outdir" -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')
    echo "    ✓ 编译通过（新增 $((after_count - before_count)) 个测试文件）"
    DONE=$((DONE + 1))
  else
    echo "!!! 编译失败，回滚该文件：$file"
    cp "$WORK/keep/$(echo "$file" | tr '/' '_')" "$FILE_ABS"
    rm -rf "$outdir"
    grep -E '^error' -A4 "$WORK/check.log" | head -12
    SKIPPED="${SKIPPED}编译失败\t${file}"$'\n'
  fi
done <<< "$LIST"

if [ -n "$SKIPPED" ]; then
  echo "=============================================================="
  echo "跳过清单（已逐个回滚，源文件完好）："
  printf '%s' "$SKIPPED" | awk -F'\t' '{printf "  %-10s %s\n", $1, $2}'
  echo "=============================================================="
fi
echo "成功拆分：$DONE / $(printf '%s\n' "$LIST" | grep -c . | tr -d ' ')"
echo "--- 残留检查（应只剩本次已提交在案的迁移目录）---"
cd "$ROOT/$CRATE_DIR" && git status --short . | grep '^??' || echo "  无残留"

# ==================== 编译 + 回归 ====================

echo "--- 拆分后 cargo test ---"
AFTER_OUT=$(~/.cargo/bin/cargo test 2>&1)
printf '%s' "$AFTER_OUT" | grep -E '^error' -A6 | head -40
AFTER_SUM=$(printf '%s' "$AFTER_OUT" | grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' | head -1)
AFTER_PASSED=$(printf '%s' "$AFTER_SUM" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+')
AFTER_FAILED=$(printf '%s' "$AFTER_SUM" | grep -oE '[0-9]+ failed' | grep -oE '[0-9]+')
AFTER_NAMES=$(printf '%s' "$AFTER_OUT" | sed -n '/^failures:$/,/^test result/p' | grep -E '^    [a-z_]+' | sed 's/^ *//' | sort)

echo "=============================================================="
echo "基线  ：passed=$BASE_PASSED failed=${BASE_FAILED:-0}"
echo "拆分后：passed=$AFTER_PASSED failed=${AFTER_FAILED:-0}"
if [ "$BASE_PASSED" = "$AFTER_PASSED" ] && [ "${BASE_FAILED:-0}" = "${AFTER_FAILED:-0}" ]; then
  echo "判定  ：✅ 用例数与失败数与基线完全一致"
else
  echo "判定  ：❌ 与基线不一致（用例丢失或新增失败）"
fi
NEW=$(comm -13 <(printf '%s\n' "$BASE_NAMES") <(printf '%s\n' "$AFTER_NAMES"))
if [ -n "$NEW" ]; then
  echo "新增失败用例："
  printf '%s\n' "$NEW"
else
  echo "新增失败用例：无"
fi