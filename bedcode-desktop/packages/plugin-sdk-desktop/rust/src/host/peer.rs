//! 对等网络基础能力（WIT `host-peer`，issue 12 切换后新增）
//!
//! 宿主只提供发现/信任/拨号/收发/浏览的能力原语；file-transfer 插件
//! 是首个上层消费者。事件（设备列表变化/连上断开/首连确认请求/任务
//! 进度）经消息总线 `peer:*` topic 推送，插件用 [`HostBus`](super::HostBus)
//! 订阅后在自己的 `on_message` 回调里消费。

use crate::host::HostError;

/// 对等网络能力 trait —— 函数签名与 WIT `host-peer` 一一对应，
/// JSON 载荷在绑定层完成字符串 ↔ Value 转换
pub trait HostPeer {
    /// 发现缓存全量列表（DiscoveredPeerDto JSON 数组）
    fn peer_list_devices(&self) -> Result<serde_json::Value, HostError>;
    /// 拨号连接指定节点（等待对端确认），返回 DialPeerResultDto
    fn peer_dial(&self, node_id: &str) -> Result<serde_json::Value, HostError>;
    /// 断开与指定节点的会话（返回是否存在该会话）
    fn peer_disconnect(&self, node_id: &str) -> Result<bool, HostError>;
    /// 首次连接确认应答（返回是否命中了待确认项）
    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, HostError>;
    /// 可信对端列表（TrustedPeerDto JSON 数组）
    fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError>;
    /// 撤销对指定节点的信任（返回是否删除了条目）
    fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, HostError>;
    /// 向单个对端发送一批文件（扇出 = 对多个对端各调一次）→ PeerTransferDto
    fn peer_send_files(&self, node_id: &str, paths: &[String]) -> Result<serde_json::Value, HostError>;
    /// 发送任务列表（含历史）
    fn peer_list_transfers(&self) -> Result<serde_json::Value, HostError>;
    /// 取消发送批
    fn peer_cancel_transfer(&self, batch_id: &str) -> Result<bool, HostError>;
    /// 重试失败/被拒/取消的发送批（断点续传）→ PeerTransferDto
    fn peer_retry_transfer(&self, batch_id: &str) -> Result<serde_json::Value, HostError>;
    /// 清空发送+接收历史（返回清除条数）
    fn peer_clear_transfer_history(&self) -> Result<u32, HostError>;
    /// 待应答/进行中的接收任务
    fn peer_list_receiving(&self) -> Result<serde_json::Value, HostError>;
    /// 接收批应答：accept=false 或超时视为拒绝
    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> Result<(), HostError>;
    /// 取消进行中的接收批（pending 视为拒绝）
    fn peer_cancel_receiving(&self, batch_id: &str) -> Result<bool, HostError>;
    /// 清空接收终态记录（返回清除条数）
    fn peer_clear_receiving_history(&self) -> Result<u32, HostError>;
    /// 接收设置 { policyMode, timeoutSecs, downloadDir }
    fn peer_get_receive_settings(&self) -> Result<serde_json::Value, HostError>;
    /// 设置接收策略：mode = "ask" | "always_accept" | "always_deny"
    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), HostError>;
    /// 本机暴露的共享目录
    fn peer_list_shared_directories(&self) -> Result<serde_json::Value, HostError>;
    /// 移除共享目录条目
    fn peer_remove_shared_directory(&self, id: &str) -> Result<bool, HostError>;
    /// 新增共享目录：request = { name?, path? }（移动端忽略 path 走 SAF 选择器）
    fn peer_add_shared_directory(&self, request: &serde_json::Value)
        -> Result<serde_json::Value, HostError>;
    /// 浏览对端共享根清单
    fn peer_list_shared_roots(&self, node_id: &str) -> Result<serde_json::Value, HostError>;
    /// 目录下钻
    fn peer_browse_directory(
        &self,
        node_id: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> Result<serde_json::Value, HostError>;
    /// 拉取对端文件（files = RemotePullFileDto 数组）→ 入队文件数
    fn peer_pull_files(
        &self,
        node_id: &str,
        dir_id: &str,
        files: &[serde_json::Value],
    ) -> Result<u32, HostError>;
    /// 系统多文件选择器（用户取消为空数组）
    fn peer_pick_files(&self) -> Result<Vec<String>, HostError>;
    /// 系统文件夹选择器（共享目录源；用户取消返回空串）
    fn peer_pick_folder(&self) -> Result<String, HostError>;
    /// 设置接收落点目录（空串 = 恢复默认）
    fn peer_set_download_dir(&self, path: &str) -> Result<(), HostError>;
}
