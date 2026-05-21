//! Pty Output Listener Trait
//!
//! 异步 PTY 输出事件监听器 trait 定义

use crate::desktop::model::PtyOutputEvent;
use async_trait::async_trait;

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