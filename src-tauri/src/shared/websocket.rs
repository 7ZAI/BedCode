//! WebSocket Module
//!
//! WebSocket 模块 - 跨平台共享
//!
//! 模块划分:
//! - message.rs: 消息类型定义
//! - server.rs: WebSocket 服务器实现
//! - client.rs: WebSocket 客户端实现

mod client;
mod message;
mod server;

pub use client::{WsClient, WsClientConfig, WsClientEvent, ConnectionStatus};
pub use message::{BinaryPayload, TextPayload, WsMessage, WsMessageType};
pub use server::{ClientInfo, WsServer, WsServerConfig, WsServerEvent};