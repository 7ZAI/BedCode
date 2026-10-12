#!/usr/bin/env bash
# 票 06 §2 锁变异自检：逐条注入违背终态的形态 → 期望红 → cp 还原 + 核 sha256 → 期望绿
# 用法：bash .scratch/2026-10-11-wasm-core-single-form/mutation_check_t06.sh
set -u
ROOT=/home/binblink/project/tauriProject/BedCode
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
MOBILE=$ROOT/bedcode-mobile/src-tauri
CORE=$ROOT/packages/bedcode-wasm-core
BAK=$(mktemp -d)
trap 'rm -rf "$BAK"' EXIT

pass=0; fail=0
note() { printf '\n=== %s ===\n' "$1"; }

# ---- 工具：注入 → 跑指定锁 → 期望红 ----
expect_red() {
  local name=$1 filter=$2
  local out
  out=$(cd "$MOBILE" && cargo test --test mobile_wasm_core_purity_lock --test retired_mobile_wasm_core_fork_lock \
        "$filter" 2>&1)
  if printf '%s' "$out" | grep -q 'test result: FAILED'; then
    echo "  ✓ 变异被抓住（红）"; pass=$((pass+1))
  else
    echo "  ✗ 变异未被抓住（应红却绿）——判据失效"; fail=$((fail+1))
    printf '%s\n' "$out" | grep -E '^(test [a-z_]+ \.\.\.|test result:)' | sed 's/^/    /'
  fi
}
expect_green() {
  local name=$1 filter=$2
  local out
  out=$(cd "$MOBILE" && cargo test --test mobile_wasm_core_purity_lock --test retired_mobile_wasm_core_fork_lock \
        "$filter" 2>&1)
  if printf '%s' "$out" | grep -q 'test result: ok'; then
    echo "  ✓ 还原后复绿"; pass=$((pass+1))
  else
    echo "  ✗ 还原后仍红"; fail=$((fail+1))
    printf '%s\n' "$out" | grep -E '^(test [a-z_]+ \.\.\.|test result:)' | sed 's/^/    /'
  fi
}
verify_sha() {
  local f=$1 want=$2
  local got
  got=$(sha256sum "$f" | cut -d' ' -f1)
  if [ "$got" = "$want" ]; then echo "  ✓ sha256 一致（$got）"
  else echo "  ✗ sha256 不一致：want=$want got=$got"; fail=$((fail+1)); fi
}

# ================= 变异 1 · 7a：内核代码行出现 cap 分片 world 名 =================
note "变异 1 · 7a cap 分片词汇回流内核"
F=$CORE/src/manager/runtime/bare.rs
cp "$F" "$BAK/bare.rs"
BEFORE=$(sha256sum "$F" | cut -d' ' -f1)
printf '\n/// 变异注入：cap 词汇出现在代码行\nconst _T06_CAP_LEAK: &str = "cap-mobile";\n' >> "$F"
expect_red "7a" wasm_core_carries_no_end_slice_or_sdk_vocabulary
cp "$BAK/bare.rs" "$F"
verify_sha "$F" "$BEFORE"
expect_green "7a" wasm_core_carries_no_end_slice_or_sdk_vocabulary

# ================= 变异 2 · 7b：声明位出现端名 =================
note "变异 2 · 7b 声明位端名回流内核"
F=$CORE/src/manager/types.rs
cp "$F" "$BAK/types.rs"
BEFORE=$(sha256sum "$F" | cut -d' ' -f1)
printf '\n/// 变异注入：端名进自持类型名\n#[derive(Debug, Clone)]\npub struct T06MobileProbe {\n    pub id: String,\n}\n' >> "$F"
expect_red "7b" wasm_core_carries_no_end_slice_or_sdk_vocabulary
cp "$BAK/types.rs" "$F"
verify_sha "$F" "$BEFORE"
expect_green "7b" wasm_core_carries_no_end_slice_or_sdk_vocabulary

# ================= 变异 3 · 7c：移动端 SDK 词汇扩散到第二个文件 =================
note "变异 3 · 7c 移动端 SDK 词汇在内核扩散"
F=$CORE/src/bus.rs
cp "$F" "$BAK/bus.rs"
BEFORE=$(sha256sum "$F" | cut -d' ' -f1)
printf '\n/// 变异注入：移动端 SDK 词汇落到第二个内核文件\n#[allow(unused_imports)]\nuse bedcode_plugin_api_mobile::types::PluginType as _T06MobileSdkProbe;\n' >> "$F"
expect_red "7c" wasm_core_carries_no_end_slice_or_sdk_vocabulary
cp "$BAK/bus.rs" "$F"
verify_sha "$F" "$BEFORE"
expect_green "7c" wasm_core_carries_no_end_slice_or_sdk_vocabulary

# ================= 变异 4 · fork 锁判据 5：长出第二份 wasm-core 织体 =================
note "变异 4 · fork 锁判据 5 第二份 wasm-core 织体"
D=$ROOT/packages/bedcode-wasm-core-shim
mkdir -p "$D/src"
cp "$CORE/Cargo.toml" "$D/Cargo.toml" 2>/dev/null
printf '// 变异注入：第二份 wasm-core 织体\n' > "$D/src/lib.rs"
expect_red "fork⑤" repo_has_exactly_one_wasm_core_body
rm -rf "$D"
[ -d "$D" ] && { echo "  ✗ 注入目录未清掉"; fail=$((fail+1)); } || echo "  ✓ 注入目录已清掉"
expect_green "fork⑤" repo_has_exactly_one_wasm_core_body

# ================= 变异 5 · wasm-core lib.rs 反向②：移动壳复活 bindgen =================
note "变异 5 · wasm-core lib.rs 反向② 移动壳 bindgen 回流"
F=$CORE/src/manager/runtime/bare/component.rs
cp "$F" "$BAK/bare_component.rs"
BEFORE=$(sha256sum "$F" | cut -d' ' -f1)
python3 - "$F" <<'PY'
import sys
p = sys.argv[1]
s = open(p, encoding='utf-8').read()
marker = "pub(crate) mod component;"
# 在 bare/component.rs 自身注入一条带 bindgen! 的代码行（判据②扫四个壳的代码行）
s = s.replace(marker, marker, 1)
s += "\n// 变异注入：移动壳复活 bindgen\nconst _T06_BINDGEN_LEAK: &str = \"bindgen!\";\n"
open(p, 'w', encoding='utf-8').write(s)
PY
OUT=$(cd "$CORE" && cargo test --lib binding_surface_must_not_return 2>&1)
if printf '%s' "$OUT" | grep -q 'test result: FAILED\|panicked'; then
  echo "  ✓ 变异被抓住（红）"; pass=$((pass+1))
else
  echo "  ✗ 变异未被抓住（应红却绿）"; fail=$((fail+1))
fi
cp "$BAK/bare_component.rs" "$F"
verify_sha "$F" "$BEFORE"
OUT=$(cd "$CORE" && cargo test --lib binding_surface_must_not_return 2>&1)
if printf '%s' "$OUT" | grep -q 'test result: ok'; then
  echo "  ✓ 还原后复绿"; pass=$((pass+1))
else
  echo "  ✗ 还原后仍红"; fail=$((fail+1))
fi

printf '\n================ 变异自检汇总 ================\n通过 %d / 失败 %d\n' "$pass" "$fail"
[ "$fail" -eq 0 ] || exit 1
