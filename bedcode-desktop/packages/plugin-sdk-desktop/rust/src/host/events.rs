//! 宿主能力：事件发射（前端 / 移动端同步 / 通知）

use crate::events::SyncEvent;

/// 事件发射（fire-and-forget 语义，失败仅记录宿主日志）
pub trait HostEvents {
    /// 向前端发送 Tauri 事件
    fn emit_event(&self, event_name: &str, payload: &serde_json::Value);

    /// 广播同步事件到所有客户端（移动端同步通道）
    ///
    /// 事件为类型化 [`SyncEvent`]，其 serde 表示与出站 `SyncPayload` 同构：
    /// 宿主反序列化后只做「信封 + 源设备排除 + 广播」，不再按会话变体改写格式
    /// 或解释语义（会话事件下沉专项票 02/03）。未知/畸形 type 在宿主反序列化期
    /// 即显性报错，不静默丢弃。需要 `broadcast` 权限。
    fn broadcast_sync(&self, event: &SyncEvent);

    /// 发送系统通知（前端 toast；移动端语义由平台决定）
    fn notify(&self, title: &str, body: &str) -> Result<(), super::HostError>;
}
