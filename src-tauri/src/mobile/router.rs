//! Mobile Router Module
//!
//! 移动端消息路由实现，与桌面端 BusinessRouter 架构一致

pub mod terminal;
pub mod auth;
pub mod sync;
pub mod system;

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::shared::model::message::Message;
use crate::shared::websocket::client::{MessageRouter, WsClientEvent};
use crate::shared::enums::{TerminalAction, TerminalPayload, SyncPayload};
use crate::shared::enums::auth::AuthStage;
use crate::shared::enums::sumary::{SessionConfigSummary, SessionSummary};
use crate::Result;

// Re-export handlers
pub use terminal::TerminalRouter;
pub use auth::AuthRouter;
pub use sync::SyncRouter;
pub use system::SystemRouter;

// ==================== MobileEvent ====================

/// Mobile 业务事件（发送给前端）
#[derive(Debug, Clone)]
pub enum MobileEvent {
    /// 连接成功
    Connected,
    /// 断开连接
    Disconnected,
    /// 收到输出
    Output {
        session_id: String,
        data: String,
        is_waiting: bool,
        /// 全局递增索引，用于去重
        index: usize,
    },
    /// 订阅响应
    SubscribeResponse {
        session_id: String,
        min_seq: u64,
        max_seq: u64,
        history_count: usize,
    },
    /// 取消订阅响应
    UnsubscribeResponse {
        session_id: String,
    },
    /// 认证成功
    AuthSuccess {
        device_id: String,
        session_token: String,
    },
    /// 认证失败
    AuthFailed {
        reason: String,
    },
    /// 配对请求
    PairingRequest {
        device_name: String,
    },
    /// 配对码验证
    PairingVerified,
    /// 错误
    Error {
        message: String,
    },
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 确认响应（服务端默认响应）
    Ack {
        request_id: String,
    },

    // === 同步数据事件 ===
    /// 会话创建同步
    SyncSessionCreated {
        session: SessionSummary,
        source_device: String,
    },
    /// 会话状态变化同步
    SyncSessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止同步
    SyncSessionStopped {
        session_id: String,
        session_name: String,
    },
    /// 会话删除同步
    SyncSessionRemoved {
        session_id: String,
        session_name: String,
    },
    /// 配置创建同步
    SyncConfigCreated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置更新同步
    SyncConfigUpdated {
        config: SessionConfigSummary,
        source_device: String,
    },
    /// 配置删除同步
    SyncConfigRemoved {
        config_id: String,
        config_name: String,
    },
}

// ==================== ClientRouteContext ====================

/// 客户端路由上下文
///
/// 包含事件发送器，供 handler 发送业务事件
pub struct ClientRouteContext {
    /// 业务事件发送器（发送 MobileEvent 给前端）
    event_tx: broadcast::Sender<MobileEvent>,
    /// WebSocket 事件发送器（用于 send_and_wait 响应匹配）
    ws_event_tx: broadcast::Sender<WsClientEvent>,
}

impl ClientRouteContext {
    pub fn new(
        event_tx: broadcast::Sender<MobileEvent>,
        ws_event_tx: broadcast::Sender<WsClientEvent>,
    ) -> Arc<Self> {
        Arc::new(Self { event_tx, ws_event_tx })
    }

    /// 发送业务事件
    pub fn emit(&self, event: MobileEvent) {
        tracing::debug!("[ClientRouteContext] emit: {:?}", event);
        if let Err(e) = self.event_tx.send(event) {
            tracing::error!("[ClientRouteContext] Failed to send event: {}", e);
        }
    }

    /// 发送 WebSocket 事件（用于 send_and_wait）
    pub fn emit_ws(&self, event: WsClientEvent) {
        tracing::debug!("[ClientRouteContext] emit_ws: {:?}", event);
        if let Err(e) = self.ws_event_tx.send(event) {
            tracing::error!("[ClientRouteContext] Failed to send ws event: {}", e);
        }
    }
}

// ==================== ClientRouteHandler ====================

/// 客户端路由处理器 trait
#[async_trait]
pub trait ClientRouteHandler: Send + Sync {
    /// 处理消息
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>>;

    /// 处理器名称
    fn name(&self) -> &str;
}

// ==================== ClientRouteRegistry ====================

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

// ==================== ClientBusinessRouter ====================

/// 客户端业务路由器
pub struct ClientBusinessRouter {
    registry: ClientRouteRegistry,
    context: Arc<ClientRouteContext>,
}

impl ClientBusinessRouter {
    pub fn new(registry: ClientRouteRegistry, context: Arc<ClientRouteContext>) -> Self {
        Self { registry, context }
    }

    pub fn builder() -> ClientBusinessRouterBuilder {
        ClientBusinessRouterBuilder::new()
    }

    pub async fn handle(&self, message: Message) -> Result<Option<Message>> {
        // 1. 发送 TextMessage 事件（send_and_wait 响应匹配）
        let message_id = message.message_id().map(|s| s.to_string());
        let content = message.to_json().unwrap_or_default();
        self.context.emit_ws(WsClientEvent::TextMessage {
            message_id,
            content,
        });

        // 2. 查找 handler
        let msg_type = message_type_key(&message);
        let handler = self.registry.get(msg_type);

        // 3. 调用 handler
        if let Some(h) = handler {
            h.handle(message, &self.context).await
        } else {
            tracing::debug!("[ClientBusinessRouter] No handler for type: {}", msg_type);
            Ok(None)
        }
    }
}

impl MessageRouter for ClientBusinessRouter {
    async fn handle(&self, message: Message) -> Result<Option<Message>> {
        self.handle(message).await
    }

    fn name(&self) -> &str {
        "ClientBusinessRouter"
    }
}

// ==================== ClientBusinessRouterBuilder ====================

/// 路由器构建器（Builder 模式）
pub struct ClientBusinessRouterBuilder {
    registry: ClientRouteRegistry,
    context: Option<Arc<ClientRouteContext>>,
}

impl ClientBusinessRouterBuilder {
    pub fn new() -> Self {
        Self {
            registry: ClientRouteRegistry::new(),
            context: None,
        }
    }

    pub fn route(mut self, msg_type: &'static str, handler: Arc<dyn ClientRouteHandler>) -> Self {
        self.registry.route(msg_type, handler);
        self
    }

    pub fn fallback(mut self, handler: Arc<dyn ClientRouteHandler>) -> Self {
        self.registry.fallback(handler);
        self
    }

    pub fn context(mut self, ctx: Arc<ClientRouteContext>) -> Self {
        self.context = Some(ctx);
        self
    }

    pub fn build(self) -> Result<ClientBusinessRouter> {
        let context = self.context.ok_or_else(|| {
            crate::AppError::WebSocket("ClientRouteContext is required".to_string())
        })?;
        Ok(ClientBusinessRouter {
            registry: self.registry,
            context,
        })
    }
}

impl Default for ClientBusinessRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}
