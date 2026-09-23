//! HTTP/WS Server
//!
//! Actix-web 服务器 - HTTP API、WebSocket 终端、认证和会话管理
//!
//! 另含对等网络引擎域（`peer_net`）：与 HTTP/WS 传输面并列的宿主侧引擎接入层，
//! 依赖方向不变量同 core——只向下依赖传输无关内核。

pub mod core;
pub mod http;
pub mod peer_net;
pub mod websocket;

pub use websocket::connection_types::DeviceConnectionInfo;
