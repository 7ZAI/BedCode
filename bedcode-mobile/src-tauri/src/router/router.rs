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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    /// 测试用处理器：按名字记录调用，并可返回固定响应消息
    struct TestHandler {
        name: &'static str,
        calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
        /// Some 时作为 handle 的返回值（模拟 handler 产生响应）
        respond: Option<Message>,
    }

    #[async_trait]
    impl crate::router::ClientRouteHandler for TestHandler {
        async fn handle(
            &self,
            message: Message,
            _ctx: &ClientRouteContext,
        ) -> Result<Option<Message>> {
            self.calls.lock().unwrap().push(message_type_key(&message));
            Ok(self.respond.clone())
        }

        fn name(&self) -> &str {
            self.name
        }
    }

    fn handler(
        name: &'static str,
        calls: &Arc<std::sync::Mutex<Vec<&'static str>>>,
        respond: Option<Message>,
    ) -> Arc<TestHandler> {
        Arc::new(TestHandler {
            name,
            calls: calls.clone(),
            respond,
        })
    }

    fn context() -> Arc<ClientRouteContext> {
        let (tx, _) = broadcast::channel(16);
        ClientRouteContext::new(tx)
    }

    #[test]
    fn build_without_context_returns_error() {
        // 缺少 context 必须显式报错，不能带空上下文构建
        let result = ClientBusinessRouter::builder()
            .route("Terminal", handler("t", &Arc::new(Default::default()), None))
            .build();
        match result {
            Err(crate::AppError::WebSocket(msg)) => {
                assert!(
                    msg.contains("ClientRouteContext"),
                    "错误信息应指出缺少 context，实际: {}",
                    msg
                )
            }
            Err(_) => panic!("期望 WebSocket 错误类型"),
            Ok(_) => panic!("无 context 构建应失败"),
        }
    }

    #[tokio::test]
    async fn route_dispatches_to_registered_handler() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        // 返回固定响应，验证 route 结果透传 handler 返回值
        let response = Message::ack("req-1");
        let router = ClientBusinessRouter::builder()
            .route("Terminal", handler("terminal", &calls, Some(response.clone())))
            .context(context())
            .build()
            .unwrap();

        let result = router
            .route(Message::output("s", b"hello", false, 0))
            .await
            .expect("route 不应失败");
        assert_eq!(calls.lock().unwrap().as_slice(), &["Terminal"]);
        assert!(matches!(result, Some(Message::Ack { .. })));
    }

    #[tokio::test]
    async fn route_unknown_type_without_fallback_returns_none() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let router = ClientBusinessRouter::builder()
            .route("Terminal", handler("terminal", &calls, None))
            .context(context())
            .build()
            .unwrap();

        // 未注册类型且无 fallback：静默 Ok(None)，不报错
        let result = router
            .route(Message::error("E1", "boom"))
            .await
            .expect("未知类型应返回 Ok(None) 而非错误");
        assert!(result.is_none());
        assert!(calls.lock().unwrap().is_empty(), "未注册类型不应触发任何 handler");
    }

    #[tokio::test]
    async fn fallback_handles_unregistered_types() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let router = ClientBusinessRouter::builder()
            .route("Terminal", handler("terminal", &calls, None))
            .fallback(handler("fallback", &calls, None))
            .context(context())
            .build()
            .unwrap();

        // 已注册类型不落入 fallback
        router
            .route(Message::output("s", b"hi", false, 0))
            .await
            .unwrap();
        assert_eq!(calls.lock().unwrap().as_slice(), &["Terminal"]);

        // 未注册类型落入 fallback
        router.route(Message::ack("r")).await.unwrap();
        assert_eq!(calls.lock().unwrap().as_slice(), &["Terminal", "Ack"]);
    }

    #[test]
    fn router_name_is_client_business_router() {
        // name() 用于日志/监控标识，保持稳定
        let router = ClientBusinessRouter::builder().context(context()).build().unwrap();
        assert_eq!(router.name(), "ClientBusinessRouter");
    }

    #[test]
    fn builder_default_is_empty() {
        // Default 与 new() 等价：空注册表、无 context
        let builder = ClientBusinessRouterBuilder::default();
        match builder.build() {
            Err(crate::AppError::WebSocket(_)) => {}
            Err(_) => panic!("期望 WebSocket 错误类型"),
            Ok(_) => panic!("默认 builder 无 context 应失败"),
        }
    }
}