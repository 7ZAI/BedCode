//! Desktop Server Module
//!
//! 提供移动端远程控制功能的 HTTP REST + WebSocket 服务
//! 基于 Actix Web 统一端口

// 模块声明 - 使用目录名.rs模式
pub mod message;
pub mod client_info;
pub mod connection_types;
pub mod services;
pub mod port_checker;

// Actix Web 模块
pub mod app;
pub mod controllers;
pub mod dtos;
pub mod middleware;
pub mod ws;

// 重新导出所有公开类型
pub use message::*;
pub use client_info::ClientInfo;
pub use connection_types::*;
pub use crate::shared::enums::control::SessionControlAction;
