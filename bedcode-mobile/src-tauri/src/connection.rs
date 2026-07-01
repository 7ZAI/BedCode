//! Connection Module
//!
//! WebSocket 客户端和远程通信 - 合并了底层 WS 客户端和业务层连接管理

pub mod codec;
pub mod default_handler;
pub mod heartbeat;
pub mod io;
pub mod lifecycle;
pub mod manager;
pub mod pairing_service;
pub mod reconnect;
pub mod request;
pub mod request_response;
pub mod client_router;
pub mod traits;
pub mod ws_client;
pub mod ws_connection;

use serde::{Deserialize, Serialize};

// Re-export from ws_client layer
pub use ws_client::WsClient;
pub use ws_connection::{WsConnectionManager, WsClientConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use io::{IoEvent, IoManager};
pub use lifecycle::{ConnectionStatus, LifecycleEvent, LifecycleManager};
pub use reconnect::{ReconnectConfig, ReconnectEvent, ReconnectManager, ReconnectState};
pub use client_router::MessageRouter;
pub use default_handler::ClientDefaultMessageHandler;
pub use request_response::RequestResponseManager;
pub use codec::{JsonCodec, MessageCodec};
pub use traits::{MessageHandler, ClientMessageHandler, HandlerResult};
pub use traits::{
    ClientInfoTrait,
    SendStrategy, DefaultSendStrategy, RetrySendStrategy,
    ResponseHandler, DefaultResponseHandler,
};

// Re-export from business layer
pub use manager::ConnectionManager;
pub use pairing_service::PairingService;
pub use request::{AuthRequest, SessionRequest, TerminalRequest, ConfigRequest, ResponseParser};

/// WebSocket 客户端事件（对外暴露的事件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsClientEvent {
    Connected,
    Disconnected,
    /// 收到推送消息（非请求-响应）
    PushMessage {
        content: String,
    },
    HeartbeatResponse,
    Error {
        message: String,
    },
    ServerClosed {
        reason: String,
    },
}
