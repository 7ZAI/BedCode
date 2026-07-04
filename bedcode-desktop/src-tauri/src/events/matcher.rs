//! Event Matcher Module
//!
//! 全局事件匹配处理器实现
//! 支持：
//! - 事件源注册（broadcast::Sender）
//! - 处理器注册（整体事件或特定变体）
//! - 自动桥接事件源和处理器

use super::app_event::AppEvent;
use std::any::{type_name, Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;

/// 事件处理器 trait（泛型入参）
pub trait EventHandler<E: AppEvent>: Send + Sync {
    /// 处理事件
    fn handle(&self, event: E);
}

/// 事件过滤器 trait
/// 用于过滤特定变体的事件
pub trait EventFilter<E: AppEvent>: Send + Sync {
    /// 检查事件是否匹配过滤条件
    fn matches(&self, event: &E) -> bool;
}

/// 泛型事件处理器包装器
struct TypedHandler<E: AppEvent> {
    type_name: &'static str,
    handler: Arc<dyn EventHandler<E>>,
}

impl<E: AppEvent> TypedHandler<E> {
    fn new(handler: Arc<dyn EventHandler<E>>) -> Self {
        Self {
            type_name: type_name::<E>(),
            handler,
        }
    }
}

/// 带过滤器的事件处理器
struct FilteredHandler<E: AppEvent> {
    type_name: &'static str,
    handler: Arc<dyn EventHandler<E>>,
    filter: Arc<dyn EventFilter<E>>,
}

impl<E: AppEvent> FilteredHandler<E> {
    fn new(handler: Arc<dyn EventHandler<E>>, filter: Arc<dyn EventFilter<E>>) -> Self {
        Self {
            type_name: type_name::<E>(),
            handler,
            filter,
        }
    }
}

/// 事件处理器存储 trait object
trait HandlerTraitObject: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn type_id(&self) -> TypeId;
    fn type_name_str(&self) -> &'static str;
}

impl<E: AppEvent + 'static> HandlerTraitObject for TypedHandler<E> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn type_name_str(&self) -> &'static str {
        self.type_name
    }
}

impl<E: AppEvent + 'static> HandlerTraitObject for FilteredHandler<E> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn type_name_str(&self) -> &'static str {
        self.type_name
    }
}

/// 事件源存储 trait object
trait EventSourceTraitObject: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn type_id(&self) -> TypeId;
}

/// 泛型事件源包装器
struct TypedEventSource<E: AppEvent> {
    sender: broadcast::Sender<E>,
}

impl<E: AppEvent + 'static> EventSourceTraitObject for TypedEventSource<E> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }
}

/// 全局事件匹配处理器
/// 支持：
/// - 注册事件源（broadcast::Sender<E>）
/// - 注册整体事件处理器
/// - 注册带过滤器的事件处理器（处理特定变体）
/// - 自动桥接事件源和处理器
pub struct EventMatcher {
    /// 事件类型到处理器的映射（支持多个处理器）
    handlers: Arc<RwLock<HashMap<TypeId, Vec<Box<dyn HandlerTraitObject>>>>>,
    /// 事件类型到事件源的映射
    event_sources: Arc<RwLock<HashMap<TypeId, Box<dyn EventSourceTraitObject>>>>,
    /// 自动订阅任务句柄
    subscription_tasks: Arc<RwLock<HashMap<TypeId, JoinHandle<()>>>>,
}

impl EventMatcher {
    /// 创建新的事件匹配器
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            event_sources: Arc::new(RwLock::new(HashMap::new())),
            subscription_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册事件源
    /// 事件产生者（如 WsServer）调用此方法注册自己的 Sender
    /// 注册后会自动启动订阅任务，将事件分发给已注册的处理器
    pub async fn register_source<E: AppEvent + Clone + 'static>(&self, sender: broadcast::Sender<E>) {
        let type_id = TypeId::of::<E>();

        // 存储事件源
        let mut sources = self.event_sources.write().await;
        sources.insert(type_id, Box::new(TypedEventSource { sender: sender.clone() }));
        drop(sources);

        tracing::debug!("[EventMatcher] Registered event source: {}", type_name::<E>());

        // 如果已有处理器，启动订阅任务
        if !self.handlers.read().await.get(&type_id).map(|v| v.is_empty()).unwrap_or(true) {
            self.ensure_subscription::<E>().await;
        }
    }

    /// 注册事件处理器（处理所有事件）
    pub async fn register<E: AppEvent + Clone + 'static>(&self, handler: Arc<dyn EventHandler<E>>) {
        let type_id = TypeId::of::<E>();

        // 添加处理器到列表
        let mut handlers = self.handlers.write().await;
        let handler_list = handlers.entry(type_id).or_insert_with(Vec::new);
        handler_list.push(Box::new(TypedHandler::new(handler)));
        drop(handlers);

        tracing::debug!("[EventMatcher] Registered handler for: {}", type_name::<E>());

        // 如果已有事件源，确保订阅任务运行
        if self.event_sources.read().await.contains_key(&type_id) {
            self.ensure_subscription::<E>().await;
        }
    }

    /// 注册函数式处理器（闭包，处理所有事件）
    pub async fn register_fn<E: AppEvent + Clone + 'static, F>(&self, handler_fn: F)
    where
        F: Fn(E) + Send + Sync + 'static,
    {
        let handler = FunctionEventHandler::new(handler_fn);
        self.register(Arc::new(handler)).await;
    }

    /// 注册带过滤器的事件处理器（只处理匹配的事件）
    ///
    /// # Example
    /// ```ignore
    /// matcher.on_filter::<WsServerEvent, _, _>(
    ///     |event| matches!(event, WsServerEvent::ServerStarted { .. }),
    ///     |event| {
    ///         println!("Server started!");
    ///     }
    /// ).await;
    /// ```
    pub async fn on_filter<E, F, H>(&self, filter: F, handler_fn: H)
    where
        E: AppEvent + Clone + 'static,
        F: Fn(&E) -> bool + Send + Sync + 'static,
        H: Fn(E) + Send + Sync + 'static,
    {
        let handler = FunctionEventHandler::new(handler_fn);
        let filter = FunctionEventFilter::new(filter);
        self.register_filtered(Arc::new(handler), Arc::new(filter)).await;
    }

    /// 注册带过滤器的事件处理器
    pub async fn register_filtered<E: AppEvent + Clone + 'static>(
        &self,
        handler: Arc<dyn EventHandler<E>>,
        filter: Arc<dyn EventFilter<E>>,
    ) {
        let type_id = TypeId::of::<E>();

        let filtered_handler = FilteredHandler::new(handler, filter);

        let mut handlers = self.handlers.write().await;
        let handler_list = handlers.entry(type_id).or_insert_with(Vec::new);
        handler_list.push(Box::new(filtered_handler));
        drop(handlers);

        tracing::debug!("[EventMatcher] Registered filtered handler for: {}", type_name::<E>());

        // 如果已有事件源，确保订阅任务运行
        if self.event_sources.read().await.contains_key(&type_id) {
            self.ensure_subscription::<E>().await;
        }
    }

    /// 确保订阅任务正在运行
    async fn ensure_subscription<E: AppEvent + Clone + 'static>(&self) {
        let type_id = TypeId::of::<E>();

        // 避免重复订阅
        if self.subscription_tasks.read().await.contains_key(&type_id) {
            return;
        }

        // 从事件源获取 receiver
        let sources = self.event_sources.read().await;
        let Some(source_box) = sources.get(&type_id) else {
            return;
        };
        let Some(typed_source) = source_box.as_any().downcast_ref::<TypedEventSource<E>>() else {
            return;
        };
        let rx = typed_source.sender.subscribe();
        drop(sources);

        let handlers = self.handlers.clone();
        let type_name = type_name::<E>();

        let task = tokio::spawn(async move {
            let mut rx = rx;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let handlers_read = handlers.read().await;
                        if let Some(handler_list) = handlers_read.get(&TypeId::of::<E>()) {
                            for handler_box in handler_list {
                                // 尝试作为 TypedHandler（无过滤器）
                                if let Some(typed) = handler_box.as_any().downcast_ref::<TypedHandler<E>>() {
                                    typed.handler.handle(event.clone());
                                }
                                // 尝试作为 FilteredHandler（有过滤器）
                                else if let Some(filtered) = handler_box.as_any().downcast_ref::<FilteredHandler<E>>() {
                                    if filtered.filter.matches(&event) {
                                        filtered.handler.handle(event.clone());
                                    }
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("[EventMatcher] Subscription closed: {}", type_name);
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("[EventMatcher] Lagged {} events for {}", n, type_name);
                        continue;
                    }
                }
            }
        });

        self.subscription_tasks.write().await.insert(type_id, task);
    }

    /// 直接发布事件（通过注册的事件源）
    pub async fn publish<E: AppEvent + Clone + 'static>(&self, event: E) -> Result<(), broadcast::error::SendError<E>> {
        let sources = self.event_sources.read().await;
        if let Some(source_box) = sources.get(&TypeId::of::<E>()) {
            if let Some(typed_source) = source_box.as_any().downcast_ref::<TypedEventSource<E>>() {
                typed_source.sender.send(event)?;
            }
        }
        Ok(())
    }

    /// 获取事件订阅器（供外部直接订阅）
    pub async fn subscribe<E: AppEvent + Clone + 'static>(&self) -> Option<broadcast::Receiver<E>> {
        let sources = self.event_sources.read().await;
        if let Some(source_box) = sources.get(&TypeId::of::<E>()) {
            if let Some(typed_source) = source_box.as_any().downcast_ref::<TypedEventSource<E>>() {
                return Some(typed_source.sender.subscribe());
            }
        }
        None
    }

    /// 注销事件源
    pub async fn unregister_source<E: AppEvent + 'static>(&self) {
        let type_id = TypeId::of::<E>();
        self.event_sources.write().await.remove(&type_id);
        self.stop_subscription(type_id).await;
        tracing::debug!("[EventMatcher] Unregistered event source: {}", type_name::<E>());
    }

    /// 注销所有处理器（保留事件源）
    pub async fn unregister_handlers<E: AppEvent + 'static>(&self) {
        let type_id = TypeId::of::<E>();
        self.handlers.write().await.remove(&type_id);
        tracing::debug!("[EventMatcher] Unregistered all handlers for: {}", type_name::<E>());
    }

    /// 注销特定过滤器的处理器（较难实现，暂不支持）
    /// 建议：使用 unregister_handlers 后重新注册需要的处理器

    /// 检查是否已注册某类事件的事件源
    pub async fn has_source<E: AppEvent + 'static>(&self) -> bool {
        self.event_sources.read().await.contains_key(&TypeId::of::<E>())
    }

    /// 检查是否已注册某类事件的处理器
    pub async fn has_handler<E: AppEvent + 'static>(&self) -> bool {
        self.handlers.read().await.contains_key(&TypeId::of::<E>())
    }

    /// 获取已注册的事件类型数量
    pub async fn source_count(&self) -> usize {
        self.event_sources.read().await.len()
    }

    /// 获取已注册的处理器数量
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.values().map(|v| v.len()).sum()
    }

    /// 清空所有
    pub async fn clear(&self) {
        // 停止所有订阅任务
        let mut tasks = self.subscription_tasks.write().await;
        for handle in tasks.values() {
            handle.abort();
        }
        tasks.clear();
        drop(tasks);

        // 清空处理器和事件源
        self.handlers.write().await.clear();
        self.event_sources.write().await.clear();

        tracing::debug!("[EventMatcher] Cleared all");
    }

    async fn stop_subscription(&self, type_id: TypeId) {
        if let Some(handle) = self.subscription_tasks.write().await.remove(&type_id) {
            handle.abort();
        }
    }
}

impl Default for EventMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== 函数式实现 ====================

/// 函数式事件处理器
struct FunctionEventHandler<E: AppEvent, F: Fn(E)> {
    handler_fn: Arc<F>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E: AppEvent, F: Fn(E)> FunctionEventHandler<E, F> {
    fn new(handler_fn: F) -> Self {
        Self {
            handler_fn: Arc::new(handler_fn),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<E: AppEvent + 'static, F: Fn(E) + Send + Sync + 'static> EventHandler<E> for FunctionEventHandler<E, F> {
    fn handle(&self, event: E) {
        (self.handler_fn)(event);
    }
}

/// 函数式事件过滤器
struct FunctionEventFilter<E: AppEvent, F: Fn(&E) -> bool> {
    filter_fn: Arc<F>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E: AppEvent, F: Fn(&E) -> bool> FunctionEventFilter<E, F> {
    fn new(filter_fn: F) -> Self {
        Self {
            filter_fn: Arc::new(filter_fn),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<E: AppEvent + 'static, F: Fn(&E) -> bool + Send + Sync + 'static> EventFilter<E> for FunctionEventFilter<E, F> {
    fn matches(&self, event: &E) -> bool {
        (self.filter_fn)(event)
    }
}

// ==================== 全局单例 ====================

/// 全局事件匹配器单例
pub fn global_matcher() -> &'static EventMatcher {
    static INSTANCE: std::sync::LazyLock<EventMatcher> =
        std::sync::LazyLock::new(EventMatcher::new);
    &INSTANCE
}

// ==================== 便捷宏 ====================

/// 便捷宏：注册事件处理器
#[macro_export]
macro_rules! on_event {
    ($event_type:ty, $handler:expr) => {{
        use $crate::events::global_matcher;
        let matcher = global_matcher();
        matcher.register_fn::<$event_type, _>($handler).await;
    }};
}

/// 便捷宏：注册带过滤器的处理器
#[macro_export]
macro_rules! on_event_filtered {
    ($event_type:ty, $filter:expr, $handler:expr) => {{
        use $crate::events::global_matcher;
        let matcher = global_matcher();
        matcher.on_filter::<$event_type, _, _>($filter, $handler).await;
    }};
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ==================== 测试事件定义 ====================

    /// 事件类型 A：模拟会话生命周期事件
    #[derive(Debug, Clone, PartialEq)]
    enum SessionEvent {
        Created { id: u32, name: String },
        Destroyed { id: u32 },
        Data { id: u32, payload: String },
    }

    impl AppEvent for SessionEvent {}

    /// 事件类型 B：模拟连接事件（与 SessionEvent 完全不同的类型）
    #[derive(Debug, Clone, PartialEq)]
    enum ConnectionEvent {
        Connected { addr: String },
        Disconnected { addr: String, reason: String },
        Heartbeat { addr: String },
    }

    impl AppEvent for ConnectionEvent {}

    /// 事件类型 C：简单结构体事件（验证非 enum 类型也能工作）
    #[derive(Debug, Clone, PartialEq)]
    struct NotificationEvent {
        level: String,
        message: String,
    }

    impl AppEvent for NotificationEvent {}

    // ==================== 辅助：结构化 EventHandler 实现 ====================

    /// 收集事件的处理器，方便断言
    struct Collector<E: AppEvent> {
        events: Arc<std::sync::Mutex<Vec<E>>>,
    }

    impl<E: AppEvent> Collector<E> {
        fn new() -> (Arc<std::sync::Mutex<Vec<E>>>, Self) {
            let events = Arc::new(std::sync::Mutex::new(Vec::new()));
            let collector = Self { events: events.clone() };
            (events, collector)
        }
    }

    impl<E: AppEvent + 'static> EventHandler<E> for Collector<E> {
        fn handle(&self, event: E) {
            self.events.lock().unwrap().push(event);
        }
    }

    // ==================== 1. 事件发送与订阅 ====================

    #[tokio::test]
    async fn test_register_source_and_publish() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);

        // 注册事件源
        matcher.register_source::<SessionEvent>(tx).await;
        assert!(matcher.has_source::<SessionEvent>().await);
        assert_eq!(matcher.source_count().await, 1);

        // 注册处理器以接收事件
        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        // 通过 matcher.publish 发送事件
        matcher
            .publish(SessionEvent::Created { id: 1, name: "test".into() })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0], SessionEvent::Created { id: 1, name: "test".into() });
    }

    #[tokio::test]
    async fn test_subscribe_returns_receiver() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);

        // 尚未注册事件源，subscribe 返回 None
        assert!(matcher.subscribe::<SessionEvent>().await.is_none());

        // 注册事件源后，subscribe 返回 receiver
        matcher.register_source::<SessionEvent>(tx).await;
        let rx = matcher.subscribe::<SessionEvent>().await;
        assert!(rx.is_some());
    }

    #[tokio::test]
    async fn test_publish_without_source_returns_ok() {
        let matcher = EventMatcher::new();
        // 未注册事件源时 publish 不 panic，返回 Ok
        let result = matcher
            .publish(SessionEvent::Destroyed { id: 99 })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_publish_when_channel_closed() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx).await;

        // 注销事件源后 publish 不 panic
        matcher.unregister_source::<SessionEvent>().await;
        let result = matcher
            .publish(SessionEvent::Destroyed { id: 99 })
            .await;
        assert!(result.is_ok());
    }

    // ==================== 2. 事件处理解耦 ====================

    #[tokio::test]
    async fn test_source_before_handler() {
        // 先注册事件源，再注册处理器 — 应自动桥接
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);

        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        tx.send(SessionEvent::Created { id: 1, name: "first".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
    }

    #[tokio::test]
    async fn test_handler_before_source() {
        // 先注册处理器，再注册事件源 — 应自动桥接
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);

        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        matcher.register_source::<SessionEvent>(tx.clone()).await;

        tx.send(SessionEvent::Created { id: 2, name: "second".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_handlers_decoupled_from_source() {
        // 多个处理器独立于事件源，各自接收所有事件
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let (events_a, collector_a) = Collector::new();
        let (events_b, collector_b) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector_a)).await;
        matcher.register::<SessionEvent>(Arc::new(collector_b)).await;

        tx.send(SessionEvent::Created { id: 1, name: "a".into() }).unwrap();
        tx.send(SessionEvent::Destroyed { id: 1 }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert_eq!(events_a.lock().unwrap().len(), 2);
        assert_eq!(events_b.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_unregister_handlers_stops_processing() {
        // 注销处理器后，事件源仍可发送但无处理器接收
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        tx.send(SessionEvent::Created { id: 1, name: "before".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(events.lock().unwrap().len(), 1);

        // 注销处理器
        matcher.unregister_handlers::<SessionEvent>().await;
        assert!(!matcher.has_handler::<SessionEvent>().await);

        // 后续事件不会被任何处理器接收
        tx.send(SessionEvent::Created { id: 2, name: "after".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_unregister_source_stops_delivery() {
        // 注销事件源后，处理器不再收到事件
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        tx.send(SessionEvent::Created { id: 1, name: "before".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(events.lock().unwrap().len(), 1);

        matcher.unregister_source::<SessionEvent>().await;
        assert!(!matcher.has_source::<SessionEvent>().await);

        // 原始 sender 仍然可用但已与 matcher 解耦
        tx.send(SessionEvent::Created { id: 2, name: "orphan".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    // ==================== 3. 多种具体事件类型 ====================

    #[tokio::test]
    async fn test_multiple_event_types_independent() {
        // SessionEvent 和 ConnectionEvent 完全独立，互不干扰
        let matcher = EventMatcher::new();
        let (tx_session, _) = broadcast::channel::<SessionEvent>(16);
        let (tx_conn, _) = broadcast::channel::<ConnectionEvent>(16);

        matcher.register_source::<SessionEvent>(tx_session.clone()).await;
        matcher.register_source::<ConnectionEvent>(tx_conn.clone()).await;

        let (session_events, session_collector) = Collector::new();
        let (conn_events, conn_collector) = Collector::new();

        matcher.register::<SessionEvent>(Arc::new(session_collector)).await;
        matcher.register::<ConnectionEvent>(Arc::new(conn_collector)).await;

        // 发送 Session 事件
        tx_session
            .send(SessionEvent::Created { id: 1, name: "s1".into() })
            .unwrap();
        // 发送 Connection 事件
        tx_conn
            .send(ConnectionEvent::Connected { addr: "192.168.1.1".into() })
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 各自只收到自己类型的事件
        assert_eq!(session_events.lock().unwrap().len(), 1);
        assert_eq!(conn_events.lock().unwrap().len(), 1);

        assert_eq!(
            session_events.lock().unwrap()[0],
            SessionEvent::Created { id: 1, name: "s1".into() }
        );
        assert_eq!(
            conn_events.lock().unwrap()[0],
            ConnectionEvent::Connected { addr: "192.168.1.1".into() }
        );
    }

    #[tokio::test]
    async fn test_three_event_types_concurrently() {
        // 三种事件类型同时注册和发送
        let matcher = EventMatcher::new();
        let (tx_s, _) = broadcast::channel::<SessionEvent>(16);
        let (tx_c, _) = broadcast::channel::<ConnectionEvent>(16);
        let (tx_n, _) = broadcast::channel::<NotificationEvent>(16);

        matcher.register_source::<SessionEvent>(tx_s.clone()).await;
        matcher.register_source::<ConnectionEvent>(tx_c.clone()).await;
        matcher.register_source::<NotificationEvent>(tx_n.clone()).await;

        let (s_events, s_collector) = Collector::new();
        let (c_events, c_collector) = Collector::new();
        let (n_events, n_collector) = Collector::new();

        matcher.register::<SessionEvent>(Arc::new(s_collector)).await;
        matcher.register::<ConnectionEvent>(Arc::new(c_collector)).await;
        matcher.register::<NotificationEvent>(Arc::new(n_collector)).await;

        // 交替发送三种事件
        tx_s.send(SessionEvent::Created { id: 1, name: "s1".into() }).unwrap();
        tx_c.send(ConnectionEvent::Connected { addr: "10.0.0.1".into() }).unwrap();
        tx_n.send(NotificationEvent { level: "info".into(), message: "hello".into() }).unwrap();
        tx_s.send(SessionEvent::Destroyed { id: 1 }).unwrap();
        tx_c.send(ConnectionEvent::Disconnected { addr: "10.0.0.1".into(), reason: "timeout".into() }).unwrap();
        tx_n.send(NotificationEvent { level: "warn".into(), message: "degraded".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(s_events.lock().unwrap().len(), 2);
        assert_eq!(c_events.lock().unwrap().len(), 2);
        assert_eq!(n_events.lock().unwrap().len(), 2);
        assert_eq!(matcher.source_count().await, 3);
    }

    #[tokio::test]
    async fn test_unregister_one_type_does_not_affect_others() {
        // 注销 SessionEvent 的事件源不影响 ConnectionEvent
        let matcher = EventMatcher::new();
        let (tx_s, _) = broadcast::channel::<SessionEvent>(16);
        let (tx_c, _) = broadcast::channel::<ConnectionEvent>(16);

        matcher.register_source::<SessionEvent>(tx_s.clone()).await;
        matcher.register_source::<ConnectionEvent>(tx_c.clone()).await;

        let (s_events, s_collector) = Collector::new();
        let (c_events, c_collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(s_collector)).await;
        matcher.register::<ConnectionEvent>(Arc::new(c_collector)).await;

        // 注销 Session 事件源
        matcher.unregister_source::<SessionEvent>().await;
        assert!(!matcher.has_source::<SessionEvent>().await);
        assert!(matcher.has_source::<ConnectionEvent>().await);

        // Connection 事件仍然正常
        tx_c.send(ConnectionEvent::Heartbeat { addr: "10.0.0.1".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(c_events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_struct_event_type() {
        // 非 enum 类型（struct）也能正常工作
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<NotificationEvent>(16);
        matcher.register_source::<NotificationEvent>(tx).await;

        let (events, collector) = Collector::new();
        matcher.register::<NotificationEvent>(Arc::new(collector)).await;

        matcher
            .publish(NotificationEvent { level: "error".into(), message: "disk full".into() })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let received = events.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].level, "error");
        assert_eq!(received[0].message, "disk full");
    }

    // ==================== 4. 过滤器 ====================

    #[tokio::test]
    async fn test_filter_only_matches_specific_variants() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let created_events: Arc<std::sync::Mutex<Vec<SessionEvent>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let destroyed_count = Arc::new(AtomicU32::new(0));

        // 过滤器：只处理 Created
        let created_clone = created_events.clone();
        matcher
            .on_filter::<SessionEvent, _, _>(
                |e| matches!(e, SessionEvent::Created { .. }),
                move |e| created_clone.lock().unwrap().push(e),
            )
            .await;

        // 过滤器：只处理 Destroyed
        let destroyed_clone = destroyed_count.clone();
        matcher
            .on_filter::<SessionEvent, _, _>(
                |e| matches!(e, SessionEvent::Destroyed { .. }),
                move |_| {
                    destroyed_clone.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await;

        tx.send(SessionEvent::Created { id: 1, name: "a".into() }).unwrap();
        tx.send(SessionEvent::Destroyed { id: 1 }).unwrap();
        tx.send(SessionEvent::Data { id: 1, payload: "payload".into() }).unwrap();
        tx.send(SessionEvent::Created { id: 2, name: "b".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Created 过滤器收到 2 个，Destroyed 过滤器收到 1 个，Data 无处理器
        assert_eq!(created_events.lock().unwrap().len(), 2);
        assert_eq!(destroyed_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_filter_with_complex_predicate() {
        // 过滤器使用复杂谓词（id > 5）
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let high_id_events: Arc<std::sync::Mutex<Vec<u32>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let high_clone = high_id_events.clone();

        matcher
            .on_filter::<SessionEvent, _, _>(
                |e| matches!(e, SessionEvent::Created { id, .. } if *id > 5),
                move |e| {
                    if let SessionEvent::Created { id, .. } = e {
                        high_clone.lock().unwrap().push(id);
                    }
                },
            )
            .await;

        tx.send(SessionEvent::Created { id: 1, name: "low".into() }).unwrap();
        tx.send(SessionEvent::Created { id: 10, name: "high".into() }).unwrap();
        tx.send(SessionEvent::Created { id: 3, name: "low".into() }).unwrap();
        tx.send(SessionEvent::Created { id: 99, name: "high".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let ids = high_id_events.lock().unwrap();
        assert_eq!(*ids, vec![10, 99]);
    }

    #[tokio::test]
    async fn test_mixed_handler_and_filtered_handler() {
        // 全局处理器 + 过滤处理器共存
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<ConnectionEvent>(16);
        matcher.register_source::<ConnectionEvent>(tx.clone()).await;

        // 全局处理器：收到所有事件
        let all_count = Arc::new(AtomicU32::new(0));
        let all_clone = all_count.clone();
        matcher
            .register_fn::<ConnectionEvent, _>(move |_| {
                all_clone.fetch_add(1, Ordering::SeqCst);
            })
            .await;

        // 过滤处理器：只处理 Connected
        let connected_count = Arc::new(AtomicU32::new(0));
        let conn_clone = connected_count.clone();
        matcher
            .on_filter::<ConnectionEvent, _, _>(
                |e| matches!(e, ConnectionEvent::Connected { .. }),
                move |_| {
                    conn_clone.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await;

        tx.send(ConnectionEvent::Connected { addr: "a".into() }).unwrap();
        tx.send(ConnectionEvent::Heartbeat { addr: "a".into() }).unwrap();
        tx.send(ConnectionEvent::Disconnected { addr: "a".into(), reason: "r".into() }).unwrap();
        tx.send(ConnectionEvent::Connected { addr: "b".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 全局处理器收到 4 个，过滤处理器收到 2 个
        assert_eq!(all_count.load(Ordering::SeqCst), 4);
        assert_eq!(connected_count.load(Ordering::SeqCst), 2);
    }

    // ==================== 5. 边界情况与健壮性 ====================

    #[tokio::test]
    async fn test_no_duplicate_subscription() {
        // 重复注册事件源不会创建多个订阅任务
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);

        matcher.register_source::<SessionEvent>(tx.clone()).await;
        // 再次注册同一类型（替换事件源）
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let (events, collector) = Collector::new();
        matcher.register::<SessionEvent>(Arc::new(collector)).await;

        tx.send(SessionEvent::Created { id: 1, name: "dup".into() }).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 每个事件只被处理一次（不会因重复订阅而重复处理）
        assert_eq!(events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_handler_count_tracking() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(16);
        matcher.register_source::<SessionEvent>(tx).await;

        assert_eq!(matcher.handler_count().await, 0);

        let (_, c1) = Collector::<SessionEvent>::new();
        let (_, c2) = Collector::<SessionEvent>::new();
        matcher.register::<SessionEvent>(Arc::new(c1)).await;
        assert_eq!(matcher.handler_count().await, 1);
        matcher.register::<SessionEvent>(Arc::new(c2)).await;
        assert_eq!(matcher.handler_count().await, 2);

        matcher.unregister_handlers::<SessionEvent>().await;
        assert_eq!(matcher.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_clear_removes_everything() {
        let matcher = EventMatcher::new();
        let (tx_s, _) = broadcast::channel::<SessionEvent>(16);
        let (tx_c, _) = broadcast::channel::<ConnectionEvent>(16);

        matcher.register_source::<SessionEvent>(tx_s).await;
        matcher.register_source::<ConnectionEvent>(tx_c).await;

        let (_, sc) = Collector::<SessionEvent>::new();
        let (_, cc) = Collector::<ConnectionEvent>::new();
        matcher.register::<SessionEvent>(Arc::new(sc)).await;
        matcher.register::<ConnectionEvent>(Arc::new(cc)).await;

        assert_eq!(matcher.source_count().await, 2);
        assert_eq!(matcher.handler_count().await, 2);

        matcher.clear().await;

        assert_eq!(matcher.source_count().await, 0);
        assert_eq!(matcher.handler_count().await, 0);
        assert!(!matcher.has_source::<SessionEvent>().await);
        assert!(!matcher.has_handler::<ConnectionEvent>().await);
    }

    #[tokio::test]
    async fn test_high_volume_events() {
        // 大量事件不丢失（在 channel 容量内）
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<SessionEvent>(256);
        matcher.register_source::<SessionEvent>(tx.clone()).await;

        let count = Arc::new(AtomicU32::new(0));
        let count_clone = count.clone();
        matcher
            .register_fn::<SessionEvent, _>(move |_| {
                count_clone.fetch_add(1, Ordering::Relaxed);
            })
            .await;

        for i in 0..100 {
            tx.send(SessionEvent::Data { id: i, payload: format!("p{}", i) }).unwrap();
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let received = count.load(Ordering::SeqCst);
        assert_eq!(received, 100);
    }

    #[tokio::test]
    async fn test_default_trait() {
        let matcher = EventMatcher::default();
        assert_eq!(matcher.source_count().await, 0);
        assert_eq!(matcher.handler_count().await, 0);
    }
}
