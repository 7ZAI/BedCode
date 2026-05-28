//! Business Router Implementation

use crate::desktop::server::message::Message;
use crate::desktop::server::router::handler::RouteHandler;
use crate::desktop::server::router::middleware::{BusinessMiddleware, Next};
pub use crate::shared::websocket::server::context::RouteContext;
use crate::desktop::server::router::registry::{message_type_key, RouteRegistry};
use crate::shared::websocket::server::connection_manager::ConnectionManager;
use crate::shared::websocket::server::events::WsServerEvent;
use crate::shared::websocket::server::message_router::MessageRouter;
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

/// 业务消息路由器
///
/// 职责：将解析后的 Message 按类型分发给已注册的处理器
/// 支持中间件链式调用（洋葱模型）
pub struct BusinessRouter {
    registry: RouteRegistry,
    middlewares: Vec<Arc<dyn BusinessMiddleware>>,
    connection_manager: Arc<ConnectionManager>,
    event_tx: broadcast::Sender<WsServerEvent>,
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
        let msg_type = message_type_key(&message);
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

/// 实现共享层的 MessageRouter trait
impl MessageRouter for BusinessRouter {
    fn route(
        &self,
        message: &Message,
        addr: SocketAddr,
        client_id: Option<&str>,
        _sender: Option<mpsc::Sender<WsMsg>>,
    ) {
        let msg_type = message_type_key(message);

        // 查找处理器
        let handler = match self.registry.get(msg_type) {
            Some(h) => h,
            None => {
                tracing::warn!("No handler registered for message type: {}", msg_type);
                return;
            }
        };

        // 创建 RouteContext
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            // 获取 connection_id
            let connection_id = self.connection_manager.get_id_by_addr(&addr).await;

            let client_id = client_id.map(|s| s.to_string()).unwrap_or_else(|| addr.to_string());

            let ctx = RouteContext::new(
                connection_id.unwrap_or_default(),
                addr,
                client_id,
                self.connection_manager.clone(),
                self.event_tx.clone(),
            );

            // 调用处理器
            if let Err(e) = handler.handle(message.clone(), &ctx).await {
                tracing::error!("Handler error: {}", e);
            }
        });
    }

    fn name(&self) -> &str {
        "BusinessRouter"
    }
}

/// 路由器构建器
pub struct BusinessRouterBuilder {
    registry: RouteRegistry,
    middlewares: Vec<Arc<dyn BusinessMiddleware>>,
    connection_manager: Option<Arc<ConnectionManager>>,
    event_tx: Option<broadcast::Sender<WsServerEvent>>,
}

impl BusinessRouterBuilder {
    pub fn new() -> Self {
        Self {
            registry: RouteRegistry::new(),
            middlewares: Vec::new(),
            connection_manager: None,
            event_tx: None,
        }
    }

    /// 注册消息类型处理器
    pub fn route(
        mut self,
        msg_type: &'static str,
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

    /// 设置连接管理器
    pub fn connection_manager(mut self, cm: Arc<ConnectionManager>) -> Self {
        self.connection_manager = Some(cm);
        self
    }

    /// 设置事件发送器
    pub fn event_tx(mut self, tx: broadcast::Sender<WsServerEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// 构建最终路由器
    pub fn build(self) -> BusinessRouter {
        BusinessRouter {
            registry: self.registry,
            middlewares: self.middlewares,
            connection_manager: self.connection_manager.unwrap_or_else(|| {
                Arc::new(ConnectionManager::new(&crate::shared::websocket::server::server_config::WsServerConfig::default()))
            }),
            event_tx: self.event_tx.unwrap_or_else(|| {
                let (tx, _) = broadcast::channel(1);
                tx
            }),
        }
    }
}

impl Default for BusinessRouterBuilder {
    fn default() -> Self {
        Self::new()
    }
}