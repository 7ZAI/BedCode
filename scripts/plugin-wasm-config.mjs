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

/**
 * wasm 应用共享 target 目录（**相对各 wasm 应用工程目录**，即 build.js 的 ROOT）
 *
 * 背景：本仓库无根 workspace，4 个 wasm 应用各自独立 workspace。cargo 默认把产物写进
 * **各自 crate 根的 `rust/target/`**，于是 4 个应用共用的依赖（SDK
 * `bedcode-plugin-api`、proc-macro `bedcode-plugin-api-macros`、wit-bindgen、serde…）
 * 被重复编译 4 遍并重复占盘（2026-09-26 实测：4 个应用合计 5.8G）。
 *
 * 改由 `--target-dir` 指向本目录后，依赖图只编译一次（= `bedcode-desktop/target/wasm-apps`）。
 * 与测试夹具的共享目录（`bedcode-desktop/target/fixtures`，见
 * `src-tauri/.../runtime/fixture_target.rs`）**刻意分开**：夹具 crate 定义了
 * `[profile.release] opt-level="s" / lto=true`，而 wasm 应用无 `[profile.*]`
 * （用 cargo 默认）；profile 参与产物指纹，同目录会为同一份依赖图产出两份产物。
 *
 * 同步点：`scripts/wasip3-toolchain.sh` 的 `health` 子命令同样构建这 4 个应用，
 * 那里也必须传同一个目录（两处注释互相指认）。
 */
export const WASM_TARGET_DIR = '../../target/wasm-apps'

/** cargo 构建时注入 pinned nightly 的 env 片段（execSync env 选项用） */
export function wasip3CargoEnv() {
  return { ...process.env, RUSTUP_TOOLCHAIN: WASIP3_NIGHTLY }
}
