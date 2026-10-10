//! 测试夹具编译产物的**共享 target 目录**（单一事实来源）
//!
//! # 为什么存在
//!
//! 本仓库无根 workspace，9 个夹具 crate（`packages/plugin-*-test`）各自独立
//! workspace。`cargo build --manifest-path <夹具>/Cargo.toml` 未指定 target 目录时，
//! cargo 把产物写进**该夹具 crate 根下**的 `target/` —— 于是依赖图被重复编译 N 遍。
//!
//! 而这 9 个夹具的依赖图**几乎完全相同**：
//! `bedcode-plugin-api`（SDK）→ `wit-bindgen` + `bedcode-plugin-api-macros`
//! （proc-macro crate，须按宿主三元组编译）→ `serde` / `serde_json` / `anyhow` /
//! `inventory`。重复的是依赖，不是夹具本体。
//!
//! 2026-09-26 实测：桌面端 11 个夹具 target 合计 **6.0G**（每 crate 429M~836M）。
//! 改为共享一个 target 目录后，同 triple + 同 profile + 同 feature 集下 cargo
//! 按产物指纹复用，依赖图只编译一次。
//!
//! # 为什么不并进宿主 `src-tauri/target`
//!
//! 1. `cargo clean` 与 `check-target-size.js` 的 15G 阈值都只针对 `src-tauri/target`，
//!    混在一起会让统计失真，且清宿主缓存时连带清掉夹具缓存。
//! 2. 夹具的 `[profile.release]`（`opt-level = "s"` / `lto = true`）与宿主不同，
//!    放一起会产生两套产物、掩盖去重效果。
//!
//! # 为什么不与 `wasm-apps` 共享目录
//!
//! wasm 应用 crate **没有** `[profile.*]`（用 cargo 默认值），与夹具的 profile 不同；
//! profile 参与产物指纹 → 同目录内同一份依赖图会产出两份产物。故两者分目录：
//! `target/fixtures`（夹具）与 `target/wasm-apps`（应用）。
//!
//! # 安全性
//!
//! `cargo test` 运行测试期间**不持有** target 目录锁（2026-09-26 探针实测：测试睡眠期间
//! 并发 `cargo build` 真实重编译 0.04s 完成，无 `Blocking waiting for file lock`），
//! 因此夹具构建与父 `cargo test` 共用目录不会死锁。

use std::path::PathBuf;

/// 夹具共享 target 根目录：`<desktop>/target/fixtures`
///
/// 由 `CARGO_MANIFEST_DIR`（2026-10-08 迁根后 = 根 `packages/bedcode-wasm-core`）
/// 上溯两级再进 `bedcode-desktop` 定位（`../../bedcode-desktop/target/fixtures`），
/// 不依赖 cwd，因此测试以任意 cwd 启动都能命中同一目录。
/// 与 `packages/.cargo/config.toml`（`../target/fixtures`，基准 = `bedcode-desktop/
/// packages/`）指向同一目录——两处必须一致，否则「宿主测试内构建的夹具」与
/// 「手工 / 工具链探针构建的夹具」分裂成两个落点（历史坑见该 config 注释）。
pub(crate) fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bedcode-desktop/target/fixtures")
}

/// 夹具产物路径：`<dir>/<target>/<profile>/<lib_name>.wasm`
///
/// `lib_name` 用 crate 名（`[lib] name` / `package.name` 的下划线形式），
/// 例如 `bedcode_plugin_ws_test`；`target` 与 `profile` 分别是 cargo 的
/// `--target` 与 release/debug。
///
/// 共享目录后产物名仍由各夹具自己的 crate 名决定 → 不同夹具不会互相覆盖。
pub(crate) fn artifact(target: &str, profile: &str, lib_name: &str) -> PathBuf {
    dir().join(target).join(profile).join(format!("{lib_name}.wasm"))
}
