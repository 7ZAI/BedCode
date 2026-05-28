//! Event Handler Module
//!
//! 全局事件匹配处理器实现
//! 支持：
//! - 事件源注册（broadcast::Sender）
//! - 处理器注册（整体事件或特定变体）
//! - 自动桥接事件源和处理器

use crate::shared::event::events::AppEvent;
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
        use $crate::shared::event::global_matcher;
        let matcher = global_matcher();
        matcher.register_fn::<$event_type, _>($handler).await;
    }};
}

/// 便捷宏：注册带过滤器的处理器
#[macro_export]
macro_rules! on_event_filtered {
    ($event_type:ty, $filter:expr, $handler:expr) => {{
        use $crate::shared::event::global_matcher;
        let matcher = global_matcher();
        matcher.on_filter::<$event_type, _, _>($filter, $handler).await;
    }};
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Started { id: u32 },
        Stopped { id: u32 },
        Message { content: String },
    }

    impl AppEvent for TestEvent {}

    #[tokio::test]
    async fn test_basic_handler() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<TestEvent>(16);

        // 注册事件源
        matcher.register_source::<TestEvent>(tx.clone()).await;

        // 记录处理结果
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();

        // 注册处理器
        matcher.register_fn::<TestEvent, _>(move |event| {
            received_clone.lock().unwrap().push(format!("{:?}", event));
        }).await;

        // 发布事件
        tx.send(TestEvent::Started { id: 1 }).unwrap();
        tx.send(TestEvent::Message { content: "hello".into() }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let results = received.lock().unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_filtered_handler() {
        let matcher = EventMatcher::new();
        let (tx, _) = broadcast::channel::<TestEvent>(16);

        matcher.register_source::<TestEvent>(tx.clone()).await;

        let started_received = Arc::new(std::sync::Mutex::new(false));
        let stopped_received = Arc::new(std::sync::Mutex::new(false));

        // 只处理 Started 变体
        let started_clone = started_received.clone();
        matcher.on_filter::<TestEvent, _, _>(
            |e| matches!(e, TestEvent::Started { .. }),
            move |_| *started_clone.lock().unwrap() = true
        ).await;

        // 只处理 Stopped 变体
        let stopped_clone = stopped_received.clone();
        matcher.on_filter::<TestEvent, _, _>(
            |e| matches!(e, TestEvent::Stopped { .. }),
            move |_| *stopped_clone.lock().unwrap() = true
        ).await;

        // 发布事件
        tx.send(TestEvent::Started { id: 1 }).unwrap();
        tx.send(TestEvent::Message { content: "test".into() }).unwrap();
        tx.send(TestEvent::Stopped { id: 1 }).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert!(*started_received.lock().unwrap());
        assert!(*stopped_received.lock().unwrap());
    }
}
