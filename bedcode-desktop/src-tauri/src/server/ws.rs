//! WebSocket Handlers
//!
//! WebSocket 连接管理 - 终端会话、注册表和消息管理

pub mod channel;
pub mod conn;
pub mod endpoint;
pub mod message;
pub mod registry;
pub mod session;
pub mod subscription;
pub mod terminal_ws;
pub mod websocket_manager;

pub use websocket_manager::{ClientSummary, ServerEvent, WebSocketManager};
