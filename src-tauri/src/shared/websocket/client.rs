//! WebSocket Client Module
//!
//! 模块化重构后的 WebSocket 客户端，按职责分为：
//! - `connection` - 连接建立与断开
//! - `io` - IO 收发（读循环、写通道）
//! - `heartbeat` - 心跳保活
//! - `lifecycle` - 生命周期状态机
//! - `router` - 消息路由
//! - `reconnect` - 重连策略

pub mod connection;
pub mod heartbeat;
pub mod io;
pub mod lifecycle;
pub mod reconnect;
pub mod router;

// 主客户端
pub mod ws_client;
pub use ws_client::WsClient;

// Re-exports
pub use connection::{ConnectionManager, WsClientConfig};
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager};
pub use io::{IoEvent, IoManager};
pub use lifecycle::{ConnectionStatus, LifecycleEvent, LifecycleManager};
pub use reconnect::{ReconnectConfig, ReconnectEvent, ReconnectManager, ReconnectState};
pub use router::{MessageRouter, MessageRouterManager, RouterConfig, RouterEvent};

// 客户端事件（对外使用）
use serde::{Deserialize, Serialize};

/// WebSocket 客户端事件（对外暴露的事件）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsClientEvent {
    Connected,
    Disconnected,
    TextMessage {
        message_id: Option<String>,
        content: String,
    },
    BinaryMessage {
        message_id: Option<String>,
        data: Vec<u8>,
    },
    HeartbeatResponse,
    Ack {
        message_id: String,
    },
    Error {
        message: String,
    },
    ServerClosed {
        reason: String,
    },
}