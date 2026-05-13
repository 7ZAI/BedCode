//! WebSocket Message Handlers
//!
//! 处理各类 WebSocket 消息的模块

mod auth;
mod control;
mod message;

pub use auth::handle_auth;
pub use control::handle_control;
pub use message::handle_message;

// 重新导出 ControlAction 供外部使用
pub use crate::desktop::websocket::message::ControlAction;