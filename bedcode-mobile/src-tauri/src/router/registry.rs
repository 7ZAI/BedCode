//! Client Route Registry - 路由注册器
//!
//! 负责注册和管理消息类型到处理器的映射

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

use crate::model::message::Message;
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
        // 移动端仅为发送方（Announce/Withdraw → 桌面）；登记路由名供日志/fallback 使用
        Message::FileService { .. } => "FileService",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::auth::{AuthPayload, AuthStage};
    use crate::enums::control::{SessionConfigAction, SessionControlAction};
    use crate::enums::file_service::FileServicePayload;
    use crate::enums::sumary::SessionSummary;
    use crate::enums::SyncPayload;
    use tokio::sync::broadcast;

    /// 测试用处理器：记录收到消息的 message_type_key，供断言分发路径
    struct RecordingHandler {
        name: &'static str,
        calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl ClientRouteHandler for RecordingHandler {
        async fn handle(
            &self,
            message: Message,
            _ctx: &ClientRouteContext,
        ) -> Result<Option<Message>> {
            self.calls.lock().unwrap().push(message_type_key(&message));
            Ok(None)
        }

        fn name(&self) -> &str {
            self.name
        }
    }

    fn handler(name: &'static str, calls: &Arc<std::sync::Mutex<Vec<&'static str>>>) -> Arc<RecordingHandler> {
        Arc::new(RecordingHandler {
            name,
            calls: calls.clone(),
        })
    }

    /// 构建 11 种 Message 变体 + 期望的 message_type_key，覆盖全枚举
    fn all_variant_messages() -> Vec<(Message, &'static str)> {
        let session = SessionSummary {
            id: "s1".to_string(),
            name: "session-1".to_string(),
            status: "running".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            started_at: None,
            session_type: None,
            config_id: None,
            task_status: None,
            task_reason: None,
        };
        vec![
            (Message::output("s", b"hi", false, 0), "Terminal"),
            (
                Message::auth(
                    None,
                    AuthPayload {
                        stage: AuthStage::Reauthenticate,
                        device_id: None,
                        device_name: None,
                        device_fingerprint: None,
                        pairing_code: None,
                        session_token: None,
                        error: None,
                        qr_token: None,
                        public_key: None,
                        challenge_nonce: None,
                        signature: None,
                        auth_method: None,
                    },
                ),
                "Auth",
            ),
            (
                Message::sync_data(SyncPayload::SessionCreated {
                    session: session.clone(),
                    source_device: "phone".to_string(),
                }),
                "SyncData",
            ),
            (Message::server_closed("bye", false), "ServerClosed"),
            (Message::error("E1", "boom"), "Error"),
            (Message::ack("req-1"), "Ack"),
            (
                Message::session_control(SessionControlAction::ListSessions, None),
                "SessionControl",
            ),
            (
                Message::session_config(SessionConfigAction::ListSessionConfigs, None),
                "SessionConfig",
            ),
            (Message::client_disconnected("phone", "offline"), "ClientDisconnected"),
            (
                Message::session_event("created", session.clone(), "phone"),
                "SessionEvent",
            ),
            (Message::file_service(FileServicePayload::Query {}), "FileService"),
        ]
    }

    #[test]
    fn message_type_key_covers_all_variants() {
        // 全部 11 个变体都必须映射到固定路由 key（新增变体时此处应同步更新）
        for (msg, expected) in all_variant_messages() {
            assert_eq!(
                message_type_key(&msg),
                expected,
                "variant {:?} 应映射到 {}",
                msg,
                expected
            );
        }
    }

    #[test]
    fn route_then_get_returns_registered_handler() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = ClientRouteRegistry::new();
        let h = handler("terminal-handler", &calls);
        registry.route("Terminal", h.clone());

        let got = registry.get("Terminal").expect("注册后应能查到");
        assert_eq!(got.name(), "terminal-handler");
        // 查到的应是同一实例（Arc 指针相同；先 coerce 到 trait object 再比指针）
        let h: Arc<dyn ClientRouteHandler> = h;
        assert!(Arc::ptr_eq(got, &h));
    }

    #[test]
    fn route_returns_self_for_chaining() {
        // builder 风格链式注册依赖 route() 返回 &mut Self
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = ClientRouteRegistry::new();
        registry
            .route("Terminal", handler("t", &calls))
            .route("Auth", handler("a", &calls));

        assert!(registry.get("Terminal").is_some());
        assert!(registry.get("Auth").is_some());
    }

    #[test]
    fn re_route_replaces_previous_handler() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = ClientRouteRegistry::new();
        registry.route("Terminal", handler("old", &calls));
        registry.route("Terminal", handler("new", &calls));

        let got = registry.get("Terminal").expect("替换后仍能查到");
        assert_eq!(got.name(), "new");
    }

    #[test]
    fn get_unregistered_type_falls_back() {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = ClientRouteRegistry::new();
        registry
            .route("Terminal", handler("t", &calls))
            .fallback(handler("fallback", &calls));

        // 已注册类型命中自身，不落入 fallback
        assert_eq!(registry.get("Terminal").unwrap().name(), "t");
        // 未注册类型落入 fallback
        assert_eq!(registry.get("Unknown").unwrap().name(), "fallback");
    }

    #[test]
    fn get_without_registration_or_fallback_returns_none() {
        let registry = ClientRouteRegistry::new();
        assert!(registry.get("Terminal").is_none());
        assert!(registry.get("").is_none());
    }

    #[test]
    fn default_registry_is_empty() {
        // Default 实现应等价于 new()：空注册表
        let registry = ClientRouteRegistry::default();
        assert!(registry.get("Terminal").is_none());
    }

    #[tokio::test]
    async fn dispatched_handler_receives_message() {
        // 注册表 + 真实 handle 调用链：验证注册的处理器能收到消息
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut registry = ClientRouteRegistry::new();
        registry.route("Terminal", handler("t", &calls));

        let h = registry.get("Terminal").cloned().unwrap();
        let (tx, _rx) = broadcast::channel(16);
        let ctx = ClientRouteContext::new(tx);
        h.handle(Message::output("s", b"data", false, 0), &ctx)
            .await
            .expect("handle 不应失败");
        assert_eq!(calls.lock().unwrap().as_slice(), &["Terminal"]);
    }
}