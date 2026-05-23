//! Business Thread Pool Module
//!
//! 业务处理线程池，用于将 CPU 密集型或阻塞型业务逻辑从 IO 协程中分离出来

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

/// 业务任务类型
pub enum BusinessTask {
    /// 文本消息处理
    TextMessage {
        /// 原始文本
        text: String,
        /// 回调通道（处理完成后发送结果）
        callback: mpsc::Sender<BusinessTaskResult>,
    },
}

/// 业务任务结果
pub enum BusinessTaskResult {
    /// 消息处理完成（需要发送响应）
    Response {
        /// JSON 响应
        json: String,
    },
    /// 消息处理完成（无响应）
    Completed,
    /// 处理失败
    Error(String),
}

/// 业务线程池管理器
pub struct BusinessThreadPool {
    /// 任务发送通道
    tx: mpsc::Sender<BusinessTask>,
}

impl BusinessThreadPool {
    /// 创建新的业务线程池
    #[allow(unused_variables)]
    pub fn new(pool_size: usize) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<BusinessTask>(1024);

        // 根据配置决定是否创建专用线程池
        if pool_size > 0 {
            // 创建专用线程池（目前未使用，仅保留接口）
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(pool_size)
                    .enable_all()
                    .build()
                    .expect("Failed to create business thread pool");

                runtime.block_on(async {
                    let _ = rx;
                    // 线程池运行中，永久阻塞
                    std::future::pending::<()>().await;
                });
            });
        }
        // 如果 pool_size == 0，使用 tokio 默认的阻塞线程池

        Arc::new(Self { tx })
    }

    /// 提交业务任务到线程池
    pub fn submit(&self, task: BusinessTask) -> impl std::future::Future<Output = ()> + Send {
        let tx = self.tx.clone();
        async move {
            if let Err(e) = tx.send(task).await {
                warn!("[BusinessThreadPool] Failed to submit task: {}", e);
            }
        }
    }
}

/// 在线程池中执行阻塞型业务逻辑
///
/// # Example
/// ```rust
/// let result = execute_in_pool(async {
///     // 模拟阻塞操作
///     tokio::time::sleep(std::time::Duration::from_millis(100)).await;
///     "result"
/// }).await;
/// ```
pub async fn execute_in_pool<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("Business task panicked")
}

/// 在线程池中执行异步业务逻辑（使用 work-stealing）
pub async fn execute_async_in_pool<F, R>(f: F) -> R
where
    F: std::future::Future<Output = R> + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn(f)
        .await
        .expect("Business task panicked")
}