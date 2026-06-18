//! WebSocket Client Module
//!
//! 移动端专用 WebSocket 客户端

pub mod codec;
pub mod connection;
pub mod default_handler;
pub mod heartbeat;
pub mod io;
pub mod lifecycle;
pub mod reconnect;
pub mod request_response;
pub mod router;
pub mod traits;
pub mod ws_client;

use serde::{Deserialize, Serialize};

// Re-exports
pub use ws_client::WsClient;
pub use connection::{ConnectionManager, WsClientConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use io::{IoEvent, IoManager};
pub use lifecycle::{ConnectionStatus, LifecycleEvent, LifecycleManager};
pub use reconnect::{ReconnectConfig, ReconnectEvent, ReconnectManager, ReconnectState};
pub use router::MessageRouter;
pub use default_handler::ClientDefaultMessageHandler;
pub use request_response::RequestResponseManager;
pub use codec::{JsonCodec, MessageCodec};
pub use traits::{MessageHandler, ClientMessageHandler, HandlerResult};
pub use traits::{
    ClientInfoTrait,
    SendStrategy, DefaultSendStrategy, RetrySendStrategy,
    ResponseHandler, DefaultResponseHandler,
};

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
