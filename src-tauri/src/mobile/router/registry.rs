//! Client Route Registry - 路由注册器
//!
//! 负责注册和管理消息类型到处理器的映射

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

use crate::shared::model::message::Message;
use crate::Result;

use super::ClientRouteContext;

/// 客户端路由处理器 trait
///
/// 业务层实现此 trait 来定义消息处理逻辑
#[async_trait]
pub trait ClientRouteHandler: Send + Sync {
    /// 处理消息
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>>;

    /// 处理器名称
    fn name(&self) -> &str;
}

/// 从 Message 获取变体名称作为路由 key
pub fn message_type_key(msg: &Message) -> &'static str {
    match msg {
        Message::Terminal { .. } => "Terminal",
        Message::Auth { .. } => "Auth",
        Message::SyncData { .. } => "SyncData",
        Message::ServerClosed { .. } => "ServerClosed",
        Message::Error { .. } => "Error",
        Message::Ack { .. } => "Ack",
        Message::SessionControl { .. } => "SessionControl",
        Message::SessionConfig { .. } => "SessionConfig",
        Message::ClientDisconnected { .. } => "ClientDisconnected",
        Message::SessionEvent { .. } => "SessionEvent",
    }
}

/// 客户端路由注册表
pub struct ClientRouteRegistry {
    handlers: HashMap<&'static str, Arc<dyn ClientRouteHandler>>,
    fallback: Option<Arc<dyn ClientRouteHandler>>,
}

impl ClientRouteRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            fallback: None,
        }
    }

    /// 注册（或替换）某消息类型的处理器
    pub fn route(&mut self, msg_type: &'static str, handler: Arc<dyn ClientRouteHandler>) -> &mut Self {
        self.handlers.insert(msg_type, handler);
        self
    }

    /// 设置 fallback 处理器（无匹配类型时）
    pub fn fallback(&mut self, handler: Arc<dyn ClientRouteHandler>) -> &mut Self {
        self.fallback = Some(handler);
        self
    }

    /// 查找处理器
    pub fn get(&self, msg_type: &str) -> Option<&Arc<dyn ClientRouteHandler>> {
        self.handlers.get(msg_type).or(self.fallback.as_ref())
    }
}

impl Default for ClientRouteRegistry {
    fn default() -> Self {
        Self::new()
    }
}