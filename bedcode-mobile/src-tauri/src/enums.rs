//! Enums Module
//!
//! 公共枚举类型定义
//!
//! `special_key`（KeyCode / KeyCombo 按键翻译）已随票 12 终端订阅协议客户端
//! 迁入 `com.bedcode.terminal-session` wasm app（等价移植进插件 `keys.rs`，
//! 语义逐字节一致；桌面侧同款先例 = `wasm-apps/terminal-session/rust/src/keys.rs`
//! 票 06 下沉）——宿主不再消费按键组合，只收插件算好的裸字节。

pub mod auth;
pub mod control;
pub mod plugin;
pub mod session;
pub mod sumary;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use session::{SessionStatus, TaskStatus};
pub use sumary::{SessionConfigSummary, SessionSummary};
