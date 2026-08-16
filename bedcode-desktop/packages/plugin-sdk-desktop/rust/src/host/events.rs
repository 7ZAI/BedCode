//! 宿主能力：事件发射（前端 / 移动端同步 / 通知）

use crate::events::SyncEvent;

/// 事件发射（fire-and-forget 语义，失败仅记录宿主日志）
pub trait HostEvents {
    /// 向前端发送 Tauri 事件
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);

    /// 广播同步事件到所有客户端（移动端同步通道）
    ///
    /// 事件为类型化 [`SyncEvent`] 枚举，宿主侧穷尽 match 转换，
    /// 未知事件类型在编译期即不可能出现。需要 `broadcast` 权限。
    fn broadcast_sync(&self, event: &SyncEvent);

    /// 发送系统通知（前端 toast；移动端语义由平台决定）
    fn notify(&self, title: &str, body: &str) -> Result<(), super::HostError>;
}
