//! 宿主能力：server 在册连接清单（票 04，自 `HostSession` 迁出）
//!
//! 主语是**宿主 WS 服务的连接注册表**，不是插件自己开的 socket（那是
//! [`crate::host::HostWebsocket`] 的 `ws:client` / `ws:server`），也不是会话
//! （会话真源在 `com.bedcode.terminal-session` 登记域）。这份事实只有宿主机进程
//! 知道，故必须是宿主原语；又与会话无关，故不随 `host-session` interface 退役。

use super::HostError;

/// 宿主 server 的在册连接事实
///
/// 需要 `connection:read` 权限（票 04 起不再挂 `session:read`：审计要回答的是
/// 「谁能枚举在册连接（含设备指纹）」，与会话读写无关）。域名与权限位同源
/// （WIT `host-connection` ↔ `connection:read`）。
pub trait HostConnection {
    /// 连接注册表原始记录清单（JSON 数组）
    ///
    /// **无排序无解读**：返回宿主 WS 连接注册表的全部原始条目
    /// （`{clientId, deviceName?, fingerprint?, addr, authenticated, connectedAt}`），
    /// 不过滤未认证连接、不合并配对记录、不加派生字段——在线判定 / 会话数 /
    /// 任务状态合并是插件侧派生视图的职责（spec D3/D4）。
    fn connections_list(&self) -> Result<serde_json::Value, HostError>;
}
