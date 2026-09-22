//! HTTP/WS Server
//!
//! Actix-web 服务器 - HTTP API、WebSocket 终端、认证和会话管理

pub mod core;
pub mod http;
pub mod websocket;

pub use websocket::connection_types::DeviceConnectionInfo;
