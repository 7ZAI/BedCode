//! Client Business Router - 业务路由器
//!
//! 将解析后的 Message 按类型分发给已注册的处理器

use std::sync::Arc;
use async_trait::async_trait;

use crate::model::message::Message;
use crate::connection::MessageRouter;
use crate::Result;

use super::{ClientRouteContext, ClientRouteRegistry, message_type_key};

/// 客户端业务路由器
///
/// 职责：将解析后的 Message 按类型分发给已注册的处理器
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
}

#[async_trait]
impl MessageRouter for ClientBusinessRouter {
    async fn route(&self, message: Message) -> Result<Option<Message>> {
        // 查找 handler
        let msg_type = message_type_key(&message);
        let handler = self.registry.get(msg_type);

        // 调用 handler
        if let Some(h) = handler {
            h.handle(message, &self.context).await
        } else {
            tracing::debug!("[ClientBusinessRouter] No handler for type: {}", msg_type);
            Ok(None)
        }
    }

    fn name(&self) -> &str {
        "ClientBusinessRouter"
    }
}

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

    /// 注册消息类型到处理器的映射
    pub fn route(mut self, msg_type: &'static str, handler: Arc<dyn super::ClientRouteHandler>) -> Self {
        self.registry.route(msg_type, handler);
        self
    }

    /// 设置 fallback 处理器
    pub fn fallback(mut self, handler: Arc<dyn super::ClientRouteHandler>) -> Self {
        self.registry.fallback(handler);
        self
    }

    /// 设置路由上下文
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