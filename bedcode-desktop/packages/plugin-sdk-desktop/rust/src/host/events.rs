//! 宿主能力：事件发射（前端事件 / 通知）

/// 事件发射（fire-and-forget 语义，失败仅记录宿主日志）
pub trait HostEvents {
    /// 向前端发送 Tauri 事件
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);

    /// 发送系统通知（前端 toast；移动端语义由平台决定）
    fn notify(&self, title: &str, body: &str) -> Result<(), super::HostError>;
}