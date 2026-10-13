//! 宿主能力：事件发射（前端事件）
//!
//! `notify` 已随 2026-10-13 功能级统一迁出（ADR 0052 D3）：原
//! `host-events-desktop.notify` 与移动 `host-notify.notify` 合并为共享域契约
//! `host-notify`，SDK 面见 [`super::notify::HostNotify`]。本 trait 只剩纯事件语义。

/// 事件发射（fire-and-forget 语义，失败仅记录宿主日志）
pub trait HostEvents {
    /// 向前端发送 Tauri 事件
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);
}
