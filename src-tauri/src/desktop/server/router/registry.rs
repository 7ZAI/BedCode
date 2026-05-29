//! Route Registry - 处理器注册表
//!
//! 维护消息类型名称 → RouteHandler 的映射关系

use crate::desktop::server::message::Message;
use crate::desktop::server::router::handler::RouteHandler;
use std::collections::HashMap;
use std::sync::Arc;

/// 从 Message 获取变体名称作为路由 key
pub fn message_type_key(msg: &Message) -> &'static str {
    match msg {
        Message::Auth { .. } => "Auth",
        Message::SessionControl { .. } => "SessionControl",
        Message::SessionConfig { .. } => "SessionConfig",
        Message::Terminal { .. } => "Terminal",
        Message::ServerClosed { .. } => "ServerClosed",
        Message::ClientDisconnected { .. } => "ClientDisconnected",
        Message::SessionEvent { .. } => "SessionEvent",
        Message::Error { .. } => "Error",
        Message::Ack { .. } => "Ack",
        Message::SyncData { .. } => "SyncData",
    }
}

/// 根据类型名称获取 Message 变体名称
pub fn type_name_to_key(type_name: &str) -> &'static str {
    match type_name {
        "Auth" => "Auth",
        "SessionControl" => "SessionControl",
        "SessionConfig" => "SessionConfig",
        "Terminal" => "Terminal",
        "ServerClosed" => "ServerClosed",
        "ClientDisconnected" => "ClientDisconnected",
        "SessionEvent" => "SessionEvent",
        "Error" => "Error",
        "Ack" => "Ack",
        "SyncData" => "SyncData",
        _ => "Error", // 默认 fallback
    }
}

/// 处理器注册表
///
/// 使用消息变体名称作为 key，提供类型安全的路由注册和查找。
#[derive(Clone)]
pub struct RouteRegistry {
    handlers: HashMap<&'static str, Arc<dyn RouteHandler>>,
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
        msg_type: &'static str,
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
    pub fn get(&self, msg_type: &str) -> Option<&Arc<dyn RouteHandler>> {
        self.handlers.get(msg_type).or(self.fallback.as_ref())
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
