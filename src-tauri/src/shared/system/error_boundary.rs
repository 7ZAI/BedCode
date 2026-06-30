//! Error Boundary
//!
//! 为 tokio::spawn 提供 panic 防护，防止后台任务静默崩溃。
//! 所有重要的后台任务都应使用 spawn_with_error_boundary 启动。

use futures_util::FutureExt;
use std::future::Future;

/// 使用错误边界包装 tokio::spawn
///
/// 捕获 spawned 任务中的 panic 并记录日志，防止任务静默终止。
///
/// # Example
///
/// ```ignore
/// spawn_with_error_boundary("connection_monitor", async move {
///     // ... 可能 panic 的任务逻辑 ...
/// });
/// ```
pub fn spawn_with_error_boundary<F>(
    task_name: &'static str,
    future: F,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let result = std::panic::AssertUnwindSafe(future)
            .catch_unwind()
            .await;

        if let Err(panic_err) = result {
            let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            tracing::error!(
                target: "error_boundary",
                task = %task_name,
                error = %msg,
                "Task panicked and was caught by error boundary",
            );
        }
    })
}
