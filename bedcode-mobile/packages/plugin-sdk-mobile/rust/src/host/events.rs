//! 宿主能力：事件发射与通知

use super::HostError;

/// 事件发射与系统通知
pub trait HostEvents {
    /// 向前端发送 Tauri 事件（fire-and-forget 语义，失败仅记录宿主日志）
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);

    /// 发送系统通知（宿主侧走 tauri-plugin-notification）
    fn notify(&self, title: &str, body: &str) -> Result<(), HostError>;
}
