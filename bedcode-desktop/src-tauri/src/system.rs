//! System Module
//!
//! 系统基础设施 - 应用上下文、配置、常量、错误类型、错误边界、电源管理和生命周期钩子
//!
//! **整核抽出（wasm-core-whole-crate）**：`config`（AppConfig，引擎级配置）、
//! `opener`（文件定位）、`process`（进程创建）已迁入 `bedcode-wasm-core` crate
//! （spec M5/M6/M7）；本文件保留 `crate::system::*` 路径（`pub use` 垫片）。
//! 组合根（`app_context`）与 lib 侧 shim（`constants` / `error` /
//! `error_boundary`）原样保留。

pub mod app_context;
pub mod constants;
pub mod error;
pub mod error_boundary;
pub mod info;
pub mod lifecycle;
pub mod logging;
pub mod power;
pub mod power_wake;

// 迁入 crate 的引擎面：经垫片保留 `crate::system::{config,opener,process}::*` 路径
pub use bedcode_wasm_core::system::{config, opener, process};

pub use app_context::AppContext;
pub use config::AppConfig;
pub use error::{AppError, Result};
pub use error_boundary::spawn_with_error_boundary;
pub use info::SystemInfo;
pub use lifecycle::lifecycle_registry;
pub use power::power_manager;
pub use power_wake::spawn_wake_monitor;
