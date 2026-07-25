//! BedCode Plugin API
//!
//! 插件系统核心接口和共享类型定义。
//! 插件 crate 依赖此 crate 实现 `BedcodePlugin` trait，
//! 主应用通过 `inventory::collect()` 收集所有静态注册的插件。
//!
//! 启用 `wasm` feature 后，额外提供 `WasmPlugin` trait 和 `wasm_entry!` 宏，
//! 用于编译为 WASM 模块的插件。

pub mod command;
pub mod context;
pub mod permission;
pub mod terminal;
pub mod traits;
pub mod types;

/// 消息总线消息
///
/// 插件间通信的统一消息封装，通过 Topic 消息总线传递
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BusMessage {
    /// 消息主题（格式：domain:action，如 task:status-changed）
    pub topic: String,
    /// 发送者插件 ID
    pub sender: String,
    /// 消息负载（任意 JSON）
    pub payload: serde_json::Value,
    /// 时间戳（毫秒 Unix）
    pub timestamp: u64,
}

#[cfg(feature = "wasm")]
pub mod wasm;
#[cfg(feature = "wasm")]
pub mod wasm_host;
#[cfg(feature = "test-plugin")]
pub mod test_plugin;

pub use command::{PluginCommand, PluginCommandEntry};
pub use context::RustPluginContext;
pub use permission::PermissionManager;
pub use terminal::TerminalHandler;
pub use traits::{BedcodePlugin, BedcodePluginEntry};
pub use types::*;

#[cfg(feature = "wasm")]
pub use wasm::WasmPlugin;
#[cfg(feature = "wasm")]
pub use wasm_host::WasmHost;
