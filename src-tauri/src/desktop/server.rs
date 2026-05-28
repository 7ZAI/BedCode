//! Desktop Server Module
//!
//! 提供移动端远程控制功能的 WebSocket 服务

// 模块声明 - 使用目录名.rs模式
pub mod message;
pub mod client_info;
pub mod connection_types;
pub mod services;
pub mod handlers;
pub mod router;

// 重新导出所有公开类型
pub use message::*;
pub use client_info::ClientInfo;
pub use connection_types::*;
pub use crate::shared::enums::control::SessionControlAction;