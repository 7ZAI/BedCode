//! 宿主能力：事件发射（v18 起纯事件语义：原 notify 收编迁入 `host-notify`）

/// 事件发射
pub trait HostEvents {
    /// 向前端发送 Tauri 事件（fire-and-forget 语义，失败仅记录宿主日志）
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);
}
