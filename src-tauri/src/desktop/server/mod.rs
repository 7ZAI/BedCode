//! Desktop Server Module
//!
//! 提供移动端远程控制功能的 WebSocket 服务

pub mod message;
pub mod client_info;

pub use message::*;
pub use client_info::ClientInfo;

// Re-export DeviceConnectionInfo from connection module
pub use crate::desktop::connection::DeviceConnectionInfo;
