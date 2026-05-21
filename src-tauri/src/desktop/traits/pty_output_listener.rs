//! Pty Output Listener Trait
//!
//! 异步 PTY 输出事件监听器 trait 定义

use crate::desktop::model::PtyOutputEvent;
use async_trait::async_trait;

/// 异步 PTY 输出事件监听器 trait
///
/// 外部实现此 trait 来接收 PTY 输出事件
/// 与同步版本 PtyOutputListener (pty_process.rs) 对应
#[async_trait]
pub trait PtyOutputListener: Send + Sync {
    /// 当有输出事件时调用（异步）
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