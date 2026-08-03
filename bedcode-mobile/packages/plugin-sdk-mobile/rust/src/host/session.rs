//! 宿主能力：会话查询

use super::HostError;

/// 会话查询
///
/// 移动端会话数据在桌面端主机，宿主通过 WebSocket 转发查询。
/// 需要 `session:read` 权限。
pub trait HostSession {
    /// 列出所有会话（JSON 数组）；能力不可用时返回 `Err(Unsupported)`
    fn session_list(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 获取单个会话；不存在返回 `Ok(None)`
    fn session_get(&self, session_id: &str) -> Result<Option<serde_json::Value>, HostError>;
}
