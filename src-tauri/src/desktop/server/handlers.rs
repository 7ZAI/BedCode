//! Handlers Module
//!
//! WebSocket 消息处理层
//! 注意：auth 和 control 委托直接到 services 层

pub mod business;
pub mod auth_handler;
pub mod control_handler;
pub mod input_handler;
pub mod subscribe_handler;
pub mod heartbeat_handler;

pub use business::BusinessMessageHandler;
pub use auth_handler::AuthHandler;
pub use control_handler::ControlHandler;
pub use input_handler::InputHandler;
pub use subscribe_handler::SubscribeHandler;
pub use heartbeat_handler::HeartbeatHandler;

pub use crate::desktop::server::services::auth_service::handle_auth;
pub use crate::desktop::server::services::session_control::handle_control;

pub use crate::desktop::server::message::ControlAction;