//! WebSocket Module
//!
//! WebSocket 模块 - 跨平台共享
//! - client/ - 客户端实现（移动端使用）
//! - 共享类型和工具

pub mod client;
pub mod codec;
mod traits;

// Re-exports from submodules
pub use client::ws_client::WsClient;
pub use client::{WsClientConfig, WsClientEvent, ConnectionStatus};
pub use client::connection::ConnectionManager as ClientConnMgr;
pub use traits::HandlerResult;
pub use traits::MessageHandler;
pub use traits::ClientMessageHandler;
pub use traits::{
    ClientInfoTrait,
    SendStrategy, DefaultSendStrategy, RetrySendStrategy,
    ResponseHandler, DefaultResponseHandler,
};
