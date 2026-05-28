//! WebSocket Server Module

pub mod ws_server;
pub mod server_config;
pub mod heartbeat;
pub mod connection_manager;
pub mod events;
pub mod io;
pub mod business_pool;
pub mod auth_interceptor;
pub mod message_router;
pub mod default_handler;
pub mod context;

// Re-exports
pub use ws_server::WsServer;
pub use events::WsServerEvent;
pub use server_config::{IpFilter, WsServerConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use connection_manager::{Connection, ConnectionEvent, ConnectionId, ConnectionManager};
pub use io::{ServerIo, ServerIoConfig, ServerIoEvent};
pub use business_pool::{BusinessThreadPool, execute_in_pool, execute_async_in_pool};
pub use auth_interceptor::AuthInterceptor;
pub use message_router::MessageRouter;
pub use default_handler::DefaultMessageHandler;
pub use context::RouteContext;