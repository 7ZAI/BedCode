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
pub fn spawn_with_error_boundary<F>(task_name: &'static str, future: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(wrap_with_error_boundary(task_name, future))
}

/// 带显式运行时句柄的错误边界 spawn
///
/// 与 [`spawn_with_error_boundary`] 同一防护，但提交目标由调用方指定：
/// 调用线程可能**没有**当前 runtime 上下文（spawn_blocking / 纯 std 线程上的
/// host fn），此时裸 `tokio::spawn` 直接 panic——句柄版在任意线程均合法。
pub fn spawn_with_error_boundary_on<F>(
    handle: &tokio::runtime::Handle,
    task_name: &'static str,
    future: F,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    handle.spawn(wrap_with_error_boundary(task_name, future))
}

/// 防护包装（共用内部：panic → error! 日志，任务自身吞掉不外泄）
async fn wrap_with_error_boundary<F>(task_name: &'static str, future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let result = std::panic::AssertUnwindSafe(future).catch_unwind().await;

    if let Err(panic_err) = result {
        let msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_err.downcast_ref::<String>().cloned() {
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
}
