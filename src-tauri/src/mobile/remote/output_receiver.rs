//! Output Receiver
//!
//! 移动端输出接收器 - 接收 WebSocket 输出消息并转发给前端
//! 信任桌面端顺序，不做复杂去重

use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

/// 输出事件（与桌面端结构相同）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputEvent {
    pub session_id: String,
    pub data: String,
    pub index: u64,
    pub timestamp: i64,
    pub is_waiting: bool,
}

/// 输出接收器
pub struct OutputReceiver {
    /// 当前收到的最大序号（用于调试日志）
    last_received_seq: AtomicU64,
    /// Tauri AppHandle（用于发送事件给前端）
    app_handle: AppHandle,
}

impl OutputReceiver {
    /// 创建新接收器
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            last_received_seq: AtomicU64::new(0),
            app_handle,
        }
    }

    /// 处理输出消息
    pub fn on_output(&self, event: OutputEvent) {
        // 记录序号用于调试
        let last_seq = self.last_received_seq.swap(event.index, Ordering::SeqCst);
        tracing::trace!(
            "[OutputReceiver] Received output index={}, last_seq={}, session={}",
            event.index,
            last_seq,
            event.session_id
        );

        // 直接转发给前端（信任桌面端顺序）
        if let Err(e) = self.app_handle.emit("ws_output", &event) {
            tracing::warn!("[OutputReceiver] Failed to emit ws_output: {}", e);
        }
    }

    /// 重置序号（断线重连时调用）
    pub fn reset(&self) {
        self.last_received_seq.store(0, Ordering::SeqCst);
    }
}
