//! Event Handler Module
//!
//! 全局事件匹配处理器实现
//! 使用泛型自动处理 AppEvent 子类，无需显式指定事件类型

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

/// 全局事件匹配处理器
/// 使用 TypeId 作为 key 存储不同事件类型的处理器
/// 支持订阅事件源并根据事件类型自动分发
pub struct EventMatcher {
    /// 事件类型到处理器的映射
    handlers: Arc<RwLock<HashMap<TypeId, Box<dyn HandlerTraitObject>>>>,
    /// 订阅任务句柄
    subscription_tasks: Arc<RwLock<HashMap<TypeId, JoinHandle<()>>>>,
}

impl EventMatcher {
    /// 创建新的事件匹配器
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            subscription_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册事件处理器（泛型）
    pub async fn register<E: AppEvent + 'static>(&self, handler: Arc<dyn EventHandler<E>>) {
        let mut handlers = self.handlers.write().await;
        let boxed = TypedHandler::new(handler);
        handlers.insert(TypeId::of::<E>(), Box::new(boxed));
    }

    /// 注册函数式处理器（闭包）
    pub async fn register_fn<E: AppEvent + 'static, F>(&self, handler_fn: F)
    where
        F: Fn(E) + Send + Sync + 'static,
    {
        let handler = FunctionEventHandler::new(handler_fn);
        self.register(Arc::new(handler)).await;
    }

    /// 注销事件处理器
    pub async fn unregister<E: AppEvent + 'static>(&self) {
        let mut handlers = self.handlers.write().await;
        handlers.remove(&TypeId::of::<E>());
    }

    /// 分发事件（泛型自动匹配）
    pub async fn dispatch<E: AppEvent + 'static>(&self, event: E) -> bool {
        let handlers = self.handlers.read().await;

        if let Some(handler_box) = handlers.get(&TypeId::of::<E>()) {
            if let Some(typed) = handler_box.as_any().downcast_ref::<TypedHandler<E>>() {
                typed.handler.handle(event);
                return true;
            }
        }

        false
    }

    /// 订阅事件源（泛型订阅）
    /// 自动将接收到的事件分发到对应的处理器
    pub async fn subscribe<E: AppEvent + Clone + 'static>(&self, mut rx: broadcast::Receiver<E>) {
        let handlers = self.handlers.clone();

        let task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let handlers_read = handlers.read().await;

                        if let Some(handler_box) = handlers_read.get(&TypeId::of::<E>()) {
                            if let Some(typed) = handler_box.as_any().downcast_ref::<TypedHandler<E>>() {
                                typed.handler.handle(event.clone());
                                tracing::debug!("Dispatched event: {:?}", type_name::<E>());
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Event subscription closed: {:?}", type_name::<E>());
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Lagged {} events for {:?}", n, type_name::<E>());
                        continue;
                    }
                }
            }
        });

        let mut tasks = self.subscription_tasks.write().await;
        tasks.insert(TypeId::of::<E>(), task);
    }

    /// 取消订阅
    pub async fn unsubscribe<E: AppEvent + 'static>(&self) {
        let mut tasks = self.subscription_tasks.write().await;
        if let Some(handle) = tasks.remove(&TypeId::of::<E>()) {
            handle.abort();
        }
    }

    /// 检查是否已注册某类事件的处理器
    pub async fn has_handler<E: AppEvent + 'static>(&self) -> bool {
        let handlers = self.handlers.read().await;
        handlers.contains_key(&TypeId::of::<E>())
    }

    /// 获取已注册的事件类型数量
    pub async fn handler_count(&self) -> usize {
        let handlers = self.handlers.read().await;
        handlers.len()
    }

    /// 清空所有已注册的处理器
    pub async fn clear(&self) {
        {
            let mut tasks = self.subscription_tasks.write().await;
            for handle in tasks.values() {
                handle.abort();
            }
            tasks.clear();
        }

        let mut handlers = self.handlers.write().await;
        handlers.clear();
    }
}

impl Default for EventMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// 函数式事件处理器
struct FunctionEventHandler<E: AppEvent, F: Fn(E) -> ()> {
    handler_fn: Arc<F>,
    _phantom: std::marker::PhantomData<E>,
}

impl<E: AppEvent, F: Fn(E) -> ()> FunctionEventHandler<E, F> {
    fn new(handler_fn: F) -> Self {
        Self {
            handler_fn: Arc::new(handler_fn),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<E: AppEvent + 'static, F: Fn(E) -> () + Send + Sync + 'static> EventHandler<E> for FunctionEventHandler<E, F> {
    fn handle(&self, event: E) {
        (self.handler_fn)(event);
    }
}

/// 全局事件匹配器单例
pub fn global_matcher() -> &'static EventMatcher {
    static INSTANCE: std::sync::LazyLock<EventMatcher> =
        std::sync::LazyLock::new(EventMatcher::new);
    &INSTANCE
}

/// 便捷宏：自动推断类型注册处理器
#[macro_export]
macro_rules! on_event {
    ($event_type:ty, $handler:expr) => {{
        use $crate::shared::event::global_matcher;
        let matcher = global_matcher();
        matcher.register_fn::<$event_type, _>($handler).await;
    }};
}

/// 便捷宏：订阅事件
#[macro_export]
macro_rules! subscribe_events {
    ($rx:expr) => {{
        use $crate::shared::event::global_matcher;
        let matcher = global_matcher();
        // 类型推断自动订阅
    }};
}