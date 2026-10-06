//! 引擎面 system 模块（bedcode-wasm-core 整核抽出：src-tauri `system/` 只留
//! 组合根 app_context 与 lib 侧 shim，本目录是引擎级配置与工具）
//!
//! 归属纪律（ADR 0036 / AGENTS §5.1）：引擎级配置（`AppConfig`）、文件定位
//! （`opener`）、进程创建（`process`）都是**应用无关引擎**，随机制整核迁入本 crate；
//! lib 的 `system.rs` 保留组合根与业务无关宿主胶水（`app_context` / `info` /
//! `lifecycle` / `logging` / `power` 等），经 `pub use` 垫片零改动消费本 crate。

pub mod config;
pub mod opener;
pub mod process;
pub mod wsl;

pub use config::AppConfig;

// 与 lib 侧 `system/{constants,error,error_boundary}.rs` 同源：真源在
// bedcode-server-base，本 crate 内部经 `crate::system::*` 引用（wasm_core 整核
// 内大量 `crate::system::error::…` / `crate::system::constants::…`），故在此
// 复导出**整个模块**保持路径逐字一致（`crate::system::error::X` 形态）。lib 侧
// 同名模块是纯 `pub use` 垫片（spec §3.1）。
pub use bedcode_server_base::{constants, error, error_boundary};
