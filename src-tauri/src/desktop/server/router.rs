//! Business Router Module
//!
//! 桌面端 WebSocket 业务消息路由器
//! 提供基于消息类型的路由、中间件洋葱模型、处理器注册

use crate::desktop::server::message::Message;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::router::middleware::Next;
use crate::desktop::server::router::registry::RouteRegistry;
use crate::Result;
use std::sync::Arc;

pub mod context;
pub mod handler;
pub mod middleware;
pub mod middlewares;
pub mod registry;
pub mod adapter;
pub mod example;

pub use adapter::RouterAdapter;
pub use example::create_router;
pub use handler::BoxedHandler;
pub use middleware::BusinessMiddleware;
pub use registry::MessageType;

/// 业务消息路由器
///
/// 职责：将解析后的 Message 按类型分发给已注册的处理器
/// 支持中间件链式调用（洋葱模型）
pub struct BusinessRouter {
    registry: RouteRegistry,
    middlewares: Vec<Arc<dyn BusinessMiddleware>>,
}

impl BusinessRouter {
    /// 使用 Builder 模式创建路由器
    pub fn builder() -> BusinessRouterBuilder {
        BusinessRouterBuilder::new()
    }

    /// 处理单条消息
    ///
    /// 1. 提取消息类型
    /// 2. 查找对应处理器（若无则使用 fallback）
    /// 3. 执行中间件链 → 处理器
    pub async fn handle(
        &self,
        message: Message,
        ctx: &RouteContext,
    ) -> Result<Option<Message>> {
        let msg_type = MessageType::from(&message);
        let handler = self.registry.get(msg_type).ok_or_else(|| {
            crate::AppError::WebSocket(format!(
                "No handler registered for message type: {}",
                msg_type
            ))
        })?;

        let next = Next::new(&self.middlewares, handler.as_ref());
        next.run(message, ctx).await
    }
}

/// 路由器构建器
pub struct BusinessRouterBuilder {
    registry: RouteRegistry,
    middlewares: Vec<Arc<dyn BusinessMiddleware>>,
}

impl BusinessRouterBuilder {
    pub fn new() -> Self {
        Self {
            registry: RouteRegistry::new(),
            middlewares: Vec::new(),
        }
    }

    /// 注册消息类型处理器
    pub fn route(
        mut self,
        msg_type: MessageType,
        handler: Arc<dyn RouteHandler>,
    ) -> Self {
        self.registry.route(msg_type, handler);
        self
    }

    /// 设置 fallback 处理器（当没有类型匹配时使用）
    pub fn fallback(mut self, handler: Arc<dyn RouteHandler>) -> Self {
        self.registry.fallback(handler);
        self
    }

    /// 添加中间件（按添加顺序执行）
    pub fn middleware(mut self, mid: Arc<dyn BusinessMiddleware>) -> Self {
        self.middlewares.push(mid);
        self
    }

    /// 构建最终路由器
    pub fn build(self) -> BusinessRouter {
        BusinessRouter {
            registry: self.registry,
            middlewares: self.middlewares,
        }
    }
}

impl Default for BusinessRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}
