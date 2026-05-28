//! WebSocket Module
//!
//! WebSocket 模块 - 跨平台共享
//! 按职责分为子目录：
//! - server/ - 服务端实现
//! - client/ - 客户端实现
//! - 共享类型和工具

pub mod client;
pub mod codec;
pub mod server;
mod traits;

// Re-exports from submodules
pub use client::ws_client::WsClient;
pub use client::{WsClientConfig, WsClientEvent, ConnectionStatus};
pub use client::connection::ConnectionManager as ClientConnMgr;
pub use traits::HandlerResult;
pub use traits::MessageHandler;
pub use traits::ClientMessageHandler;
pub use server::ws_server::WsServer;
pub use server::events::WsServerEvent;
pub use server::server_config::{WsServerConfig, IpFilter};
pub use server::connection_manager::{ConnectionManager, ConnectionId, Connection, ConnectionEvent};
pub use server::heartbeat::{HeartbeatManager, HeartbeatConfig, HeartbeatEvent};
pub use server::io::{ServerIo, ServerIoConfig, ServerIoEvent};
pub use traits::{
    ClientInfoTrait,
    SendStrategy, DefaultSendStrategy, RetrySendStrategy,
    ResponseHandler, DefaultResponseHandler,
};