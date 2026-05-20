//! PTY Handler
//!
//! PTY 生命周期管理抽象 - 负责 PTY 会话的创建、运行、终止

use crate::desktop::pty::{PtyOutputEvent, PtySession, SessionLaunchConfig};
use crate::Result;
use std::sync::Arc;
use tokio::sync::broadcast;

/// PTY 会话处理器 trait
///
/// 将 PTY 操作抽象为 trait，便于测试和替换实现
pub trait PtyHandler: Send + Sync {
    /// 创建新的 PTY 会话
    fn create_session(&self, config: SessionLaunchConfig) -> Result<PtySession>;

    /// 使用指定 ID 创建 PTY 会话（用于重启时复用旧 ID）
    fn create_session_with_id(&self, id: String, config: SessionLaunchConfig) -> Result<PtySession>;

    /// 订阅 PTY 输出事件
    fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent>;
}

/// PTY 会话处理器实现
pub struct PtySessionHandler {
    /// 运行标志
    running: Arc<std::sync::atomic::AtomicBool>,
    /// 输出广播通道
    output_tx: broadcast::Sender<PtyOutputEvent>,
}

impl PtySessionHandler {
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel(2048);
        Self {
            running: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            output_tx,
        }
    }

    pub fn output_tx(&self) -> broadcast::Sender<PtyOutputEvent> {
        self.output_tx.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_running(&self, running: bool) {
        self.running.store(running, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for PtySessionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyHandler for PtySessionHandler {
    fn create_session(&self, config: SessionLaunchConfig) -> Result<PtySession> {
        PtySession::new(config)
    }

    fn create_session_with_id(&self, id: String, config: SessionLaunchConfig) -> Result<PtySession> {
        PtySession::with_id(id, config)
    }

    fn subscribe_output(&self) -> broadcast::Receiver<PtyOutputEvent> {
        self.output_tx.subscribe()
    }
}