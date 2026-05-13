//! Mobile WebSocket Client Module
//!
//! 移动端 WebSocket 客户端，用于连接到桌面端

pub mod client;
pub mod message;

pub use client::RemoteClient;