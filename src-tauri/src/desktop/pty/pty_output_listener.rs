//! Pty Output Listener Implementation
//!
//! 异步 PTY 输出事件监听器实现
//! 实现 traits::PtyOutputListener trait

use crate::desktop::model::PtyOutputEvent;
use crate::desktop::traits::{PtyOutputHandler, PtyOutputListener};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

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
        tracing::debug!("Registered handler: {}", handler.name());
    }

    /// 注册一个 Handler（使用默认错误策略）
    pub async fn register(&self, handler: Arc<dyn PtyOutputHandler>) {
        self.register_handler(handler, HandlerErrorPolicy::ContinueOnError)
            .await;
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
            return;
        }

        let mut join_set = JoinSet::new();

        for entry in handlers {
            let event = event.clone();
            let name = entry.handler.name().to_string();
            let error_policy = entry.error_policy.clone();

            join_set.spawn(async move {
                match entry.handler.handle(event).await {
                    Ok(()) => tracing::debug!("Handler {} processed event", name),
                    Err(e) => {
                        tracing::error!("Handler {} error: {}", name, e);
                        Err((name, error_policy))
                    }
                }
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err((name, policy))) => {
                    if matches!(policy, HandlerErrorPolicy::StopOnError) {
                        join_set.abort_all();
                        break;
                    }
                }
                Err(e) => tracing::error!("Task join error: {}", e),
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
}