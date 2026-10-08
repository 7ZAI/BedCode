//! Connection Module
//!
//! WebSocket 客户端和远程通信 - 合并了底层 WS 客户端和业务层连接管理

pub mod client_router;
pub mod codec;
pub mod default_handler;
pub mod event_ws;
pub mod heartbeat;
pub mod io;
pub mod lifecycle;
pub mod manager;
pub mod reconnect;
pub mod request;
pub mod request_response;
pub mod traits;
pub mod ws_client;
pub mod ws_connection;

use serde::{Deserialize, Serialize};

// Re-export from ws_client layer
pub use client_router::MessageRouter;
pub use codec::{JsonCodec, MessageCodec};
pub use default_handler::ClientDefaultMessageHandler;
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use io::{IoEvent, IoManager};
pub use lifecycle::{ConnectionStatus, LifecycleEvent, LifecycleManager};
pub use reconnect::{ReconnectConfig, ReconnectEvent, ReconnectManager, ReconnectState};
pub use request_response::{MatchOutcome, RequestResponseManager};
pub use traits::{
    ClientInfoTrait, DefaultResponseHandler, DefaultSendStrategy, ResponseHandler, RetrySendStrategy, SendStrategy,
};
pub use traits::{ClientMessageHandler, HandlerResult, MessageHandler};
pub use ws_client::WsClient;
pub use ws_connection::{WsClientConfig, WsConnectionManager};

// Re-export from business layer
pub use manager::ConnectionManager;
pub use request::AuthRequest;

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
    /// 服务端主动关闭（保留 close **code**，M1/ADR 0031）：认证类 code（4001 /
    /// 4003）是**致命**关闭（重新配对前重连无意义），ConnMonitor / 自愈监督据此
    /// 不自愈、只提示重新配对；非致命 code 走既有网络断连自愈路径
    ServerClosed {
        code: u16,
        reason: String,
    },
}
