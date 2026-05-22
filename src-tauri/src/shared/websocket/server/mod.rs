//! WebSocket Server Module

pub mod wsserver;
pub mod server_config;
pub mod heartbeat;
pub mod connection_manager;

// Re-exports
pub use wsserver::{HandlerResult, MessageHandler, WsServer, WsServerEvent};
pub use server_config::{IpFilter, WsServerConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use connection_manager::{Connection, ConnectionEvent, ConnectionId, ConnectionManager};