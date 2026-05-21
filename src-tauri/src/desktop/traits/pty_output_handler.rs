//! Pty Output Handler Trait
//!
//! 异步 PTY 输出事件处理器 trait 定义

use crate::desktop::model::PtyOutputEvent;
use async_trait::async_trait;

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