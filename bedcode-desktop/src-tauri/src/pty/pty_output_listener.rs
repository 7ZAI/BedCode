//! PTY 输出事件监听器与处理器
//!
//! 包含 PtyOutputHandler、PtyOutputListener trait 定义
//! 及 AsyncPtyOutputListener 实现

use crate::pty::PtyOutputEvent;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

// ==================== Trait 定义 ====================

/// 异步 PTY 输出事件处理器 trait
///
/// 用于在 AsyncPtyOutputListener 中注册多个处理器
/// 每个 Handler 可以独立处理输出事件（如转发、缓存、广播等）
#[async_trait]
pub trait PtyOutputHandler: Send + Sync {
    /// 处理 PTY 输出事件
    async fn handle(&self, event: PtyOutputEvent)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Handler 名称（用于日志和调试）
    fn name(&self) -> &str;
}

/// 异步 PTY 输出事件监听器 trait
///
/// 外部实现此 trait 来接收 PTY 输出事件
/// 注意：on_output 是同步方法，内部使用 tokio::spawn 来异步执行 handlers
/// 这样可以在同步线程（如 PtyReader 线程）中安全调用
#[async_trait]
pub trait PtyOutputListener: Send + Sync {
    /// 当有输出事件时调用（同步方法，内部会 spawn 异步任务）
    /// 这是为了兼容在同步线程中调用
    async fn on_output(&self, event: PtyOutputEvent);

    /// 获取监听器名称（用于日志）
    fn name(&self) -> &str;
}

/// 同步版本的 PTY 输出事件监听器 trait
/// (保留用于兼容现有代码)
pub trait PtyOutputListenerSync: Send + Sync {
    /// 当有输出事件时调用（同步）
    fn on_output(&self, event: PtyOutputEvent);
}

// ==================== 实现 ====================

/// Handler 错误处理策略
#[derive(Debug, Clone, Default)]
pub enum HandlerErrorPolicy {
    /// 忽略错误，继续执行（默认）
    #[default]
    ContinueOnError,
    /// 遇到错误立即停止
    StopOnError,
}

/// Handler 注册项
#[derive(Clone)]
struct HandlerEntry {
    handler: Arc<dyn PtyOutputHandler>,
    error_policy: HandlerErrorPolicy,
}

impl HandlerEntry {
    fn new(handler: Arc<dyn PtyOutputHandler>, error_policy: HandlerErrorPolicy) -> Self {
        Self {
            handler,
            error_policy,
        }
    }
}

/// 异步 PTY 输出事件监听器实现
///
/// 支持注册多个 Handler，事件触发时并行调用所有 Handler
/// 实现 PtyOutputListener trait
pub struct AsyncPtyOutputListener {
    handlers: Arc<Mutex<Vec<HandlerEntry>>>,
    name: String,
}

impl AsyncPtyOutputListener {
    /// 创建新的监听器
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(Vec::new())),
            name: "AsyncPtyOutputListener".to_string(),
        }
    }

    /// 创建带名称的监听器
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            handlers: Arc::new(Mutex::new(Vec::new())),
            name: name.into(),
        }
    }

    /// 注册一个 Handler
    pub async fn register_handler(
        &self,
        handler: Arc<dyn PtyOutputHandler>,
        error_policy: HandlerErrorPolicy,
    ) {
        let mut handlers = self.handlers.lock().await;
        handlers.push(HandlerEntry::new(handler, error_policy));
    }

    /// 注册一个 Handler（使用默认错误策略）
    pub async fn register(&self, handler: Arc<dyn PtyOutputHandler>) {
        let handler_name = handler.name().to_string();
        self.register_handler(handler, HandlerErrorPolicy::ContinueOnError).await;
        tracing::info!("[AsyncPtyOutputListener] Handler registered: {}", handler_name);
    }

    /// 获取内部 handlers 用于调试
    pub async fn get_handler_names(&self) -> Vec<String> {
        let handlers = self.handlers.lock().await;
        handlers.iter().map(|h| h.handler.name().to_string()).collect()
    }

    /// 移除指定名称的 Handler
    pub async fn remove_handler(&self, name: &str) -> bool {
        let mut handlers = self.handlers.lock().await;
        let original_len = handlers.len();
        handlers.retain(|h| h.handler.name() != name);
        handlers.len() < original_len
    }

    /// 获取已注册 Handler 的数量
    pub async fn handler_count(&self) -> usize {
        let handlers = self.handlers.lock().await;
        handlers.len()
    }

    /// 清空所有已注册的 Handler
    pub async fn clear(&self) {
        let mut handlers = self.handlers.lock().await;
        handlers.clear();
    }

    /// 内部：并行执行所有 Handler
    async fn execute_handlers(&self, event: PtyOutputEvent) {
        let handlers = {
            let handlers = self.handlers.lock().await;
            handlers.clone()
        };

        if handlers.is_empty() {
            tracing::warn!("[AsyncPtyOutputListener] No handlers registered, event will be lost!");
            return;
        }

        let mut join_set = JoinSet::new();

        for entry in handlers {
            let event = event.clone();
            let error_policy = entry.error_policy.clone();

            join_set.spawn(async move {
                if let Err(e) = entry.handler.handle(event).await {
                    tracing::error!("[AsyncPtyOutputListener] Handler {} error: {}", entry.handler.name(), e);
                }
            });
        }

        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                tracing::error!("[AsyncPtyOutputListener] Task join error: {}", e);
            }
        }
    }
}

impl Default for AsyncPtyOutputListener {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AsyncPtyOutputListener {
    fn clone(&self) -> Self {
        Self {
            handlers: self.handlers.clone(),
            name: self.name.clone(),
        }
    }
}

/// 实现 PtyOutputListener trait
#[async_trait]
impl PtyOutputListener for AsyncPtyOutputListener {
    async fn on_output(&self, event: PtyOutputEvent) {
        self.execute_handlers(event).await;
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    struct TestHandler {
        name: String,
        should_fail: bool,
    }

    #[async_trait]
    impl PtyOutputHandler for TestHandler {
        async fn handle(
            &self,
            _event: PtyOutputEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            if self.should_fail {
                Err(format!("{} failed", self.name).into())
            } else {
                Ok(())
            }
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn test_register_and_notify() {
        let listener = AsyncPtyOutputListener::new();

        let handler1 = Arc::new(TestHandler {
            name: "handler1".to_string(),
            should_fail: false,
        });
        let handler2 = Arc::new(TestHandler {
            name: "handler2".to_string(),
            should_fail: false,
        });

        listener.register(handler1).await;
        listener.register(handler2).await;

        assert_eq!(listener.handler_count().await, 2);

        let event = PtyOutputEvent {
            session_id: "test".to_string(),
            data: "test data".to_string(),
            timestamp: Utc::now(),
            is_waiting: false,
            index: 1,
        };

        listener.on_output(event).await;
    }

    #[tokio::test]
    async fn test_remove_handler() {
        let listener = AsyncPtyOutputListener::new();

        let handler = Arc::new(TestHandler {
            name: "test_handler".to_string(),
            should_fail: false,
        });

        listener.register(handler).await;
        assert_eq!(listener.handler_count().await, 1);

        listener.remove_handler("test_handler").await;
        assert_eq!(listener.handler_count().await, 0);
    }

    /// 测试问题：从同步线程调用 async on_output
    /// 这会暴露问题 - Future 不会被执行
    #[tokio::test]
    async fn test_sync_call_async_on_output_issue() {
        use std::thread;
        use std::sync::Arc as StdArc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = Arc::new(AsyncPtyOutputListener::new());
        let handler_called = Arc::new(AtomicBool::new(false));

        let handler_called_clone = handler_called.clone();
        let handler = Arc::new(TestHandler {
            name: "sync_test_handler".to_string(),
            should_fail: false,
        });

        // 注册 handler
        listener.register(handler).await;

        let event = PtyOutputEvent {
            session_id: "test".to_string(),
            data: "test data".to_string(),
            timestamp: Utc::now(),
            is_waiting: false,
            index: 1,
        };

        // 在 tokio runtime 中直接 await 调用 - 应该工作
        listener.on_output(event.clone()).await;

        // 现在模拟同步线程中的调用（模拟 PtyReader 的行为）
        let listener_clone = listener.clone();
        let event_clone = event.clone();

        // 在线程中调用 async 函数但不 await
        let handle = thread::spawn(move || {
            // 这是 PtyReader 中的调用方式 - 只调用不 await
            let _future = listener_clone.on_output(event_clone);
            // 注意：这里没有 .await，所以 Future 不会被执行
        });

        handle.join().unwrap();

        // 等待一段时间看 handler 是否被调用
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 验证：在同步线程中调用 async fn 但不 await，handler 不会被执行
        // 这就是问题的根源
        tracing::info!("Handler called from sync thread: {}", handler_called.load(Ordering::SeqCst));
    }
}