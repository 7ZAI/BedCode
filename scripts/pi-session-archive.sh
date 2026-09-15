#!/usr/bin/env bash
# ============================================================================
# pi-session-archive.sh — 归档本项目过期的 pi session 日志
#
# 作用：把本项目 .pi/sessions/ 中「距离最新 session 超过 N 天」的 session
#       jsonl 日志（含复合 session 目录）移动到 pi 安装目录的 session 归档区，
#       归档文件夹以项目全路径命名（/ 替换为 -，前后加 --），与 pi 自身的
#       归档命名约定一致（如 --home-binblink-project-tauriProject-BedCode--）。
#
# 规则：
#   - 基准日期 = 本项目 .pi/sessions/ 中最新 session 的时间戳（非今天）
#   - 早于（基准 - N 天）的 session 视为过期并移动；默认 N=15，可参数覆盖
#   - 只处理顶层 *.jsonl 与形如 YYYY-MM-DDThh-mm-ss-msZ_<ulid> 的 session 目录；
#     sol-pi / subagent-artifacts 等非 session 目录绝不触碰
#   - 目标目录已存在同名条目时跳过并警告，绝不覆盖
#   - 本脚本由项目 scripts/ 位置推导项目根，天然只在项目范围内生效
#
# 用法：
#   scripts/pi-session-archive.sh            # 实际移动（默认 15 天）
#   scripts/pi-session-archive.sh -n         # dry-run，只打印不移动
#   scripts/pi-session-archive.sh -d 30      # 阈值改为 30 天
#   PI_AGENT_DIR=/custom/pi scripts/pi-session-archive.sh -n   # 覆盖 pi 安装目录
# ============================================================================
set -u

# ---------- 路径推导 ----------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SESSIONS_DIR="$PROJECT_ROOT/.pi/sessions"
PI_AGENT_DIR="${PI_AGENT_DIR:-$HOME/.pi/agent}"
ARCHIVE_ROOT="$PI_AGENT_DIR/sessions"

# 项目全路径 → 归档目录名：去掉开头 /，/ 替换为 -，前后加 --
ARCHIVE_NAME="--$(echo "$PROJECT_ROOT" | sed 's|^/||; s|/|-|g')--"
ARCHIVE_DIR="$ARCHIVE_ROOT/$ARCHIVE_NAME"

# ---------- 参数 ----------
DRY_RUN=0
DAYS=15
while [ $# -gt 0 ]; do
  case "$1" in
    -n|--dry-run) DRY_RUN=1 ;;
    -d) DAYS="${2:-15}"; shift ;;
    -d*) DAYS="${1#-d}" ;;
    -h|--help) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "未知参数: $1（-n dry-run / -d DAYS / -h help）" >&2; exit 2 ;;
  esac
  shift
done

# ---------- 工具函数 ----------
# 从 session 名解析 UTC 时间戳（YYYY-MM-DDThh-mm-ss-msZ_... 或目录同名），
# 解析失败回退文件 mtime。返回 epoch 秒。
name_to_epoch() {
  local name="$1" path="$2" dt date_part time_part epoch
  # 取前 19 字符：2026-09-14T22-15-05
  dt="${name:0:19}"
  if [[ "$dt" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}-[0-9]{2}-[0-9]{2}$ ]]; then
    # 只把时间段的 - 换为 :，日期段保持原样：2026-08-30 12:31:55
    date_part="${dt:0:10}"
    time_part="${dt:11}"
    time_part="${time_part//-/:}"
    epoch=$(date -u -d "$date_part $time_part" +%s 2>/dev/null)
    [ -n "$epoch" ] && { echo "$epoch"; return; }
  fi
  # fallback：mtime
  date -u -r "$path" +%s 2>/dev/null || echo 0
}

is_session_entry() {
  # 顶层 *.jsonl 或时间戳 session 目录；sol-pi/subagent-artifacts 等排除
  local name="$1" type="$2"
  [[ "$name" == *.jsonl ]] && return 0
  [[ "$type" == "d" && "$name" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9-]+Z_ ]] && return 0
  return 1
}

# ---------- 前置检查 ----------
[ -d "$SESSIONS_DIR" ] || { echo "未找到项目 session 目录: $SESSIONS_DIR" >&2; exit 1; }
[ -d "$ARCHIVE_ROOT" ] || { echo "未找到 pi 安装 session 目录: $ARCHIVE_ROOT" >&2; exit 1; }
[[ "$DAYS" =~ ^[0-9]+$ ]] || { echo "阈值必须是正整数: $DAYS" >&2; exit 2; }

# ---------- 收集候选 + 计算最新基准 ----------
declare -a CAND_NAMES CAND_PATHS CAND_EPOCHS
MAX_EPOCH=0
while IFS= read -r entry; do
  name="$(basename "$entry")"
  [ "$name" = "." ] && continue
  if [ -d "$entry" ]; then type="d"; else type="f"; fi
  is_session_entry "$name" "$type" || continue
  epoch=$(name_to_epoch "$name" "$entry")
  CAND_NAMES+=("$name"); CAND_PATHS+=("$entry"); CAND_EPOCHS+=("$epoch")
  [ "$epoch" -gt "$MAX_EPOCH" ] && MAX_EPOCH=$epoch
done < <(find "$SESSIONS_DIR" -mindepth 1 -maxdepth 1 | sort)

TOTAL=${#CAND_NAMES[@]}
[ "$TOTAL" -eq 0 ] && { echo "没有可归档的 session 条目"; exit 0; }

CUTOFF=$(( MAX_EPOCH - DAYS * 86400 ))
BASE_DATE=$(date -u -d "@$MAX_EPOCH" +%Y-%m-%d)
CUTOFF_DATE=$(date -u -d "@$CUTOFF" +%Y-%m-%d)

echo "项目: $PROJECT_ROOT"
echo "session 目录: $SESSIONS_DIR"
echo "归档目标: $ARCHIVE_DIR"
echo "最新 session: $BASE_DATE | 阈值: ${DAYS} 天 | 截止线: $CUTOFF_DATE（早于即归档）"
echo "候选: $TOTAL 个条目"
echo "----------------------------------------"

# ---------- 执行移动 ----------
mkdir -p "$ARCHIVE_DIR"
MOVE_N=0; SKIP_N=0; MOVE_BYTES=0
for i in "${!CAND_NAMES[@]}"; do
  name="${CAND_NAMES[$i]}"; path="${CAND_PATHS[$i]}"; epoch="${CAND_EPOCHS[$i]}"
  if [ "$epoch" -lt "$CUTOFF" ]; then
    size=$(du -sk "$path" 2>/dev/null | awk '{print $1}')
    if [ -e "$ARCHIVE_DIR/$name" ]; then
      echo "SKIP(目标已存在)  $name"
      SKIP_N=$((SKIP_N + 1))
      continue
    fi
    echo "MOVE  $name  ($(date -u -d "@$epoch" +%Y-%m-%d), ${size}K)"
    MOVE_N=$((MOVE_N + 1)); MOVE_BYTES=$((MOVE_BYTES + size))
    if [ "$DRY_RUN" -eq 0 ]; then
      mv "$path" "$ARCHIVE_DIR/$name" || { echo "  移动失败: $name" >&2; SKIP_N=$((SKIP_N + 1)); MOVE_N=$((MOVE_N - 1)); }
    fi
  fi
done

echo "----------------------------------------"
if [ "$DRY_RUN" -eq 1 ]; then
  echo "DRY-RUN 完成：将移动 $MOVE_N 个条目（约 $((MOVE_BYTES / 1024))MB），跳过 $SKIP_N 个；未执行任何移动"
else
  echo "完成：已移动 $MOVE_N 个条目（约 $((MOVE_BYTES / 1024))MB），跳过 $SKIP_N 个"
fi
