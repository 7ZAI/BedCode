//! Handlers Module
//!
//! WebSocket 消息处理层
//! 注意：auth 和 control 委托直接到 services 层

pub mod business;
pub mod input;

pub use crate::desktop::server::services::auth::handle_auth;
pub use business::BusinessMessageHandler;
pub use crate::desktop::server::services::session_control::handle_control;
pub use input::handle_input;

pub use crate::desktop::server::message::ControlAction;