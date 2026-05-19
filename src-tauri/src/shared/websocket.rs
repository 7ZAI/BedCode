//! WebSocket Module
//!
//! WebSocket 模块 - 跨平台共享
//!
//! 模块划分:
//! - message.rs: 消息类型定义
//! - server.rs: WebSocket 服务器实现（泛型化）
//! - client.rs: WebSocket 客户端实现
//! - traits.rs: 泛型 trait 定义
//! - codec.rs: 可插拔的消息编解码器
//! - heartbeat.rs: 心跳管理模块
//! - events.rs: 泛型事件系统

mod client;
mod codec;
mod events;
mod heartbeat;
mod message;
mod server;
mod traits;

pub use client::{WsClient, WsClientConfig, WsClientEvent, ConnectionStatus};
pub use codec::{JsonCodec, MessageCodec};
pub use events::WsServerEventBuilder;
pub use heartbeat::{HeartbeatConfig, HeartbeatEvent, HeartbeatManager, HeartbeatSender};
pub use message::{BinaryPayload, TextPayload, WsMessage, WsMessageType, WsResponse};
pub use server::{
    ClientInfo, HandlerResult, MessageHandler, WsServer,
    WsServerConfig, WsServerEvent,
};
pub use traits::{
    ClientInfoTrait, DefaultClientInfo, HandlerResult as TraitHandlerResult,
    ClientMessageHandler, NoopHandler, SendStrategy, DefaultSendStrategy, RetrySendStrategy,
    SendInterceptor, LoggingInterceptor, MetricsInterceptor,
    ResponseHandler, DefaultResponseHandler,
};