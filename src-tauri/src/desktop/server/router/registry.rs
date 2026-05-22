//! Route Registry - 处理器注册表
//!
//! 维护 MessageType → RouteHandler 的映射关系

use crate::desktop::server::message::Message;
use crate::desktop::server::router::handler::RouteHandler;
use std::collections::HashMap;
use std::sync::Arc;

/// 业务消息类型（可路由类型）
///
/// 从 `Message` 枚举的变体提取，作为注册表的 Key。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageType {
    Auth,
    Control,
    Input,
    Output,
    Heartbeat,
    Subscribe,
    Unsubscribe,
    ServerClosed,
    ClientDisconnected,
    SessionEvent,
    SubscribeResponse,
    UnsubscribeResponse,
    Error,
}

impl From<&Message> for MessageType {
    fn from(msg: &Message) -> Self {
        match msg {
            Message::Auth { .. } => MessageType::Auth,
            Message::Control { .. } => MessageType::Control,
            Message::Input { .. } => MessageType::Input,
            Message::Output { .. } => MessageType::Output,
            Message::Heartbeat { .. } => MessageType::Heartbeat,
            Message::Subscribe { .. } => MessageType::Subscribe,
            Message::Unsubscribe { .. } => MessageType::Unsubscribe,
            Message::ServerClosed { .. } => MessageType::ServerClosed,
            Message::ClientDisconnected { .. } => MessageType::ClientDisconnected,
            Message::SessionEvent { .. } => MessageType::SessionEvent,
            Message::SubscribeResponse { .. } => MessageType::SubscribeResponse,
            Message::UnsubscribeResponse { .. } => MessageType::UnsubscribeResponse,
            Message::Error { .. } => MessageType::Error,
        }
    }
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MessageType::Auth => "auth",
            MessageType::Control => "control",
            MessageType::Input => "input",
            MessageType::Output => "output",
            MessageType::Heartbeat => "heartbeat",
            MessageType::Subscribe => "subscribe",
            MessageType::Unsubscribe => "unsubscribe",
            MessageType::ServerClosed => "server_closed",
            MessageType::ClientDisconnected => "client_disconnected",
            MessageType::SessionEvent => "session_event",
            MessageType::SubscribeResponse => "subscribe_response",
            MessageType::UnsubscribeResponse => "unsubscribe_response",
            MessageType::Error => "error",
        };
        write!(f, "{}", s)
    }
}

/// 处理器注册表
///
/// 提供类型安全的路由注册和查找。
pub struct RouteRegistry {
    handlers: HashMap<MessageType, Arc<dyn RouteHandler>>,
    fallback: Option<Arc<dyn RouteHandler>>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            fallback: None,
        }
    }

    /// 注册（或替换）某消息类型的处理器
    pub fn route(
        &mut self,
        msg_type: MessageType,
        handler: Arc<dyn RouteHandler>,
    ) -> &mut Self {
        self.handlers.insert(msg_type, handler);
        self
    }

    /// 设置 fallback 处理器（无匹配类型时）
    pub fn fallback(&mut self, handler: Arc<dyn RouteHandler>) -> &mut Self {
        self.fallback = Some(handler);
        self
    }

    /// 查找处理器
    pub fn get(&self, msg_type: MessageType) -> Option<&Arc<dyn RouteHandler>> {
        self.handlers.get(&msg_type).or(self.fallback.as_ref())
    }

    /// 获取已注册类型数量
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for RouteRegistry {
    fn default() -> Self {
        Self::new()
    }
}
