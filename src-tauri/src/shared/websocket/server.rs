//! WebSocket Server Module

pub mod ws_server;
pub mod server_config;
pub mod heartbeat;
pub mod connection_manager;
pub mod events;
pub mod io;
pub mod business_pool;
pub mod default_handler;
pub mod context;
pub mod http_router;
pub mod http_controller;

// Re-exports
pub use ws_server::WsServer;
pub use events::WsServerEvent;
pub use server_config::{IpFilter, WsServerConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use connection_manager::{Connection, ConnectionEvent, ConnectionId, ConnectionManager};
pub use io::{ServerIo, ServerIoConfig, ServerIoEvent};
pub use business_pool::{BusinessThreadPool, execute_in_pool, execute_async_in_pool};
pub use default_handler::{AuthInterceptor, DefaultMessageHandler, MessageRouter};
pub use context::RouteContext;
pub use http_router::{HttpRouter, HttpRouteHandler, HttpRequestContext, ApiResponse, HttpBody, FileTreeNode, FileType, FileTreeRequest, build_json_response, build_cors_preflight_response};
pub use http_controller::{MethodHandler, HttpController, method_handler};
