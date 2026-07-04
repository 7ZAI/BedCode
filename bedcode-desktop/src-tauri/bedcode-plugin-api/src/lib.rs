//! BedCode Plugin API
//!
//! 插件系统核心接口和共享类型定义。
//! 插件 crate 依赖此 crate 实现 `BedcodePlugin` trait，
//! 主应用通过 `inventory::collect()` 收集所有静态注册的插件。

pub mod command;
pub mod context;
pub mod permission;
pub mod terminal;
pub mod traits;
pub mod types;

pub use command::{PluginCommand, PluginCommandEntry};
pub use context::RustPluginContext;
pub use permission::PermissionManager;
pub use terminal::TerminalHandler;
pub use traits::{BedcodePlugin, BedcodePluginEntry};
pub use types::*;
