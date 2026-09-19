#!/usr/bin/env bash
# ==============================================================================
# wasip3 工具链（wasm32-wasip3 target）安装 / 验证 / 构建辅助脚本（桌面端）
#
# 背景：stable 1.98.1 无 wasm32-wasip3 预编译产物（tier 2 low-tier，需 LLVM 23 +
# rustup 更新或源码构建）；nightly 自 2026-09-12 起 rust-std present。本脚本把
# 桌面插件 wasip3 编译链固定到单一 nightly 版本，镜像加速安装，全流程可复现。
#
# 版本与切换点（单一事实来源，文档化决策见 docs/knowledge/wasip3-toolchain.md）：
#   - WASIP3_NIGHTLY=nightly-2026-09-16（rustc 1.100.0-nightly，spike 验证版本）
#   - 切换点：stable >= 1.99（预计 2026-10 中，nightly 2026-09-12 起 std present）
#     → rustup update + `rustup target add wasm32-wasip3`（stable）→ 本脚本去掉
#     nightly pin，命令字眼同步 AGENTS.md §3。
#
# 用法：scripts/wasip3-toolchain.sh <install|verify|fixture|health|help>
#   install  安装 pinned nightly + wasm32-wasip3 target（幂等；含镜像加速）
#   verify   校验工具链与 target 已就绪，打印版本
#   fixture  构建 packages/plugin-wasip3-test 并以 \0asm+0d000100 magic 校验
#            （cdylib 直出 Component，免 componentize 步骤）
#   health   存量 4 个桌面插件（file-transfer/ai-chatbox/agent-hub/auto-task）
#            wasip3 target 零代码改动编译基线（产物验证仅编译链；实例化需
#            async 宿主，见票 02，不得替换 resources/plugins/ 现行产物）
#
# 环境变量覆盖：
#   WASIP3_NIGHTLY      固定 nightly 名（默认 nightly-2026-09-16）
#   RUSTUP_DIST_SERVER  已配置时尊重之（默认 USTC 镜像 https://mirrors.ustc.edu.cn/rust-static）
#   RUSTUP_UPDATE_ROOT  已配置时尊重之（默认 USTC https://mirrors.ustc.edu.cn/rust-static/rustup）
# ==============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ==================== 版本与镜像（单一事实来源） ====================
WASIP3_NIGHTLY="${WASIP3_NIGHTLY:-nightly-2026-09-16}"
MIRROR_DIST_SERVER="${RUSTUP_DIST_SERVER:-https://mirrors.ustc.edu.cn/rust-static}"
MIRROR_UPDATE_ROOT="${RUSTUP_UPDATE_ROOT:-https://mirrors.ustc.edu.cn/rust-static/rustup}"

# 镜像加速仅作用于本脚本内的 rustup 调用（导出不会污染调用方环境之外的范围）
apply_mirror_env() {
  export RUSTUP_DIST_SERVER="$MIRROR_DIST_SERVER"
  export RUSTUP_UPDATE_ROOT="$MIRROR_UPDATE_ROOT"
}

log() { printf '[wasip3] %s\n' "$*"; }
die() { printf '[wasip3] 错误: %s\n' "$*" >&2; exit 1; }

# ==================== 子命令：install ====================
cmd_install() {
  apply_mirror_env
  log "安装 pinned nightly: ${WASIP3_NIGHTLY}（镜像 ${MIRROR_DIST_SERVER}）"
  rustup toolchain install "${WASIP3_NIGHTLY}" --profile minimal
  log "安装 target wasm32-wasip3 → ${WASIP3_NIGHTLY}"
  rustup target add wasm32-wasip3 --toolchain "${WASIP3_NIGHTLY}"
  log "安装完成："
  rustup run "${WASIP3_NIGHTLY}" rustc --version
  cmd_verify
}

# ==================== 子命令：verify ====================
cmd_verify() {
  log "校验工具链与 target（pinned: ${WASIP3_NIGHTLY}）"
  rustup run "${WASIP3_NIGHTLY}" rustc --version >/dev/null 2>&1 \
    || die "${WASIP3_NIGHTLY} 未安装，先执行 scripts/wasip3-toolchain.sh install"
  rustup run "${WASIP3_NIGHTLY}" rustup target list --installed \
    | grep -qx 'wasm32-wasip3' \
    || die "wasm32-wasip3 target 未安装（${WASIP3_NIGHTLY}），先执行 scripts/wasip3-toolchain.sh install"
  rustup run "${WASIP3_NIGHTLY}" rustc --version
  rustup run "${WASIP3_NIGHTLY}" rustup target list --installed | rg 'wasm32-wasip3'
  log "工具链就绪 ✅"
}

# ==================== 子命令：fixture ====================
# 组件 magic：\0asm + 0d 00 01 00（component 编码 flag；core module 为 01 00 00 00）
check_component_magic() {
  local wasm="$1"
  [ -f "$wasm" ] || die "找不到产物: $wasm"
  local magic
  magic="$(od -A n -t x1 -N 8 "$wasm" | tr -d ' \n')"
  if [ "$magic" = "0061736d0d000100" ]; then
    return 0
  fi
  return 1
}

cmd_fixture() {
  cmd_verify
  local crate="$ROOT/bedcode-desktop/packages/plugin-wasip3-test"
  log "构建 fixture（${WASIP3_NIGHTLY} + wasm32-wasip3）"
  RUSTUP_TOOLCHAIN="${WASIP3_NIGHTLY}" cargo build \
    --target wasm32-wasip3 --release --manifest-path "$crate/Cargo.toml"
  local out="$crate/target/wasm32-wasip3/release/bedcode_plugin_wasip3_test.wasm"
  if check_component_magic "$out"; then
    log "fixture 产物为 Component（magic \\0asm + 0d 00 01 00）✅"
    log "  $out（$(du -h "$out" | cut -f1)）"
  else
    die "fixture 产物不是 Component（magic 校验失败）"
  fi
}

# ==================== 子命令：health ====================
# 存量插件 wasip3 target 零代码改动编译基线（构建命令与各插件 build:rust 同形，
# 仅替换 target 与工具链）。产物仅编译链验证——p2 sync 宿主不可实例化 wasip3
# 组件（需 A0-3 async 化，票 02），不得替换 resources/plugins/ 现行产物。
cmd_health() {
  cmd_verify
  local plugins=(
    file-transfer
    ai-chatbox
    agent-hub
    auto-task
  )
  local pass=0 fail=0
  for p in "${plugins[@]}"; do
    local dir="$ROOT/bedcode-desktop/plugins/$p/rust"
    [ -f "$dir/Cargo.toml" ] || { log "跳过（无 rust/Cargo.toml）: $p"; continue; }
    log "编译（零代码改动）: ${p} → wasm32-wasip3"
    if RUSTUP_TOOLCHAIN="${WASIP3_NIGHTLY}" cargo build \
        --target wasm32-wasip3 --release --no-default-features --features wasm \
        --manifest-path "$dir/Cargo.toml"; then
      local out="$dir/target/wasm32-wasip3/release/bedcode_plugin_$(echo "$p" | tr '-' '_').wasm"
      if check_component_magic "$out" 2>/dev/null; then
        log "  ✅ ${p}  产物 Component（$(du -h "$out" | cut -f1)）"
        pass=$((pass + 1))
      else
        log "  ⚠️  ${p}  编译成功但产物 magic 异常（lib 名可能不同，见上）"
        pass=$((pass + 1))
      fi
    else
      log "  ❌ ${p}  编译失败"
      fail=$((fail + 1))
    fi
  done
  log "健康基线: ${pass} 通过 / ${fail} 失败"
  [ "$fail" -eq 0 ] || die "存在失败插件，见上方输出"
}

# ==================== 入口 ====================
case "${1:-help}" in
  install) cmd_install ;;
  verify) cmd_verify ;;
  fixture) cmd_fixture ;;
  health) cmd_health ;;
  help|-h|--help)
    sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'
    ;;
  *) die "未知子命令: $1（支持 install|verify|fixture|health|help）" ;;
esac