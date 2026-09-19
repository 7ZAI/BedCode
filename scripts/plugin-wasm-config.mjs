/**
 * 桌面插件 WASM 构建统一配置（票 03 构建链全量 wasip3）
 *
 * 单一事实来源 = `scripts/wasip3-toolchain.sh`（WASIP3_NIGHTLY）；
 * stable 1.99（预计 2026-10 中）发布后移除 nightly pin，见
 * `docs/knowledge/wasip3-toolchain.md`。各插件 `scripts/build.js` 与本文件同步。
 */

/** 桌面插件统一 wasm target（wasm32-wasip3 产物 cdylib 直出 Component） */
export const WASM_TARGET = 'wasm32-wasip3'

/** 提供 wasm32-wasip3 预编译 std 的 pinned nightly（stable 1.98.1 无该 target） */
export const WASIP3_NIGHTLY = 'nightly-2026-09-16'

/** cargo 构建时注入 pinned nightly 的 env 片段（execSync env 选项用） */
export function wasip3CargoEnv() {
  return { ...process.env, RUSTUP_TOOLCHAIN: WASIP3_NIGHTLY }
}