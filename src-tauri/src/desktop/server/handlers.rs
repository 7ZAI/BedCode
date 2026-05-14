//! Handlers Module
//!
//! WebSocket 消息处理层

pub mod auth;
pub mod control;
pub mod input;

pub use auth::handle_auth;
pub use control::handle_control;
pub use input::handle_input;

pub use crate::desktop::server::message::ControlAction;