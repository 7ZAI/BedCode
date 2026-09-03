//! 对等网络基础能力（WIT `host-peer`，ADR 0022 v3 终态 13 原语）
//!
//! 宿主只提供拨号/关闭/信任/收发/闸门配置/广播面同步的无业务语义原语；
//! file-transfer 插件是首个上层消费者，设备列表、任务队列与历史、共享根
//! 注册表、接收策略等产品状态由插件自持（issue 13 Phase 3）。
//!
//! 事件（发现/离开/连上断开/首连确认请求/传输进度）经消息总线
//! `mdns:*` / `peer:*` topic 推送，插件用 [`HostBus`](super::HostBus)
//! 订阅后在自己的 `on_bus_message` 回调里消费。

use crate::host::HostError;

/// 对等网络能力 trait —— 函数签名与 WIT `host-peer` 一一对应，
/// JSON 载荷在绑定层完成字符串 ↔ Value 转换
pub trait HostPeer {
    /// 按 endpoint 拨号（endpoint = `{ nodeId, addr, port }`），成功返回 session
    /// 句柄；denied/unreachable 报错
    fn peer_dial(&self, endpoint: &serde_json::Value) -> Result<String, HostError>;
    /// 统一资源关闭：session 句柄 = 断开；传输句柄 = 取消。返回是否命中
    fn peer_close(&self, handle: &str) -> Result<bool, HostError>;
    /// 首次连接确认应答（返回是否命中了待确认项）
    fn peer_respond_consent(&self, request_id: &str, accepted: bool) -> Result<bool, HostError>;
    /// 可信对端列表（TrustedPeerDto JSON 数组）
    fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError>;
    /// 撤销对指定节点的信任（返回是否删除了条目）
    fn peer_revoke_trusted(&self, node_id: &str) -> Result<bool, HostError>;
    /// 向单个对端发送一批文件（扇出 = 对多个对端各调一次）。paths 元素双形态：
    /// 纯 string 或 `{ path, encrypt? }` 对象（任一元素 encrypt=true → 本批强制
    /// 加密）。返回传输句柄（batch-id 字符串）；仅接受 session 句柄寻址，
    /// 连接已断时以句柄记忆的 endpoint 自动重拨
    fn peer_send_files(
        &self,
        session: &str,
        paths: &[serde_json::Value],
    ) -> Result<String, HostError>;
    /// 接收批应答：accept=false 或超时视为拒绝
    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> Result<(), HostError>;
    /// 设置接收策略（引擎安全闸门配置原语）：mode = "ask" | "always_accept" | "always_deny"
    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), HostError>;
    /// 全量幂等替换引擎广播源：条目 `[{ id, name, safTreeUri }]`（SAF 树 URI）
    fn peer_set_shared_roots(&self, dirs: &[serde_json::Value]) -> Result<(), HostError>;
    /// 浏览对端共享根清单（仅 session 句柄寻址；断线自动重拨）
    fn peer_list_shared_roots(&self, session: &str) -> Result<serde_json::Value, HostError>;
    /// 目录下钻（仅 session 句柄寻址；断线自动重拨）
    fn peer_browse_directory(
        &self,
        session: &str,
        dir_id: &str,
        rel_path: &str,
    ) -> Result<serde_json::Value, HostError>;
    /// 拉取对端文件（files = RemotePullFileDto 数组）→ 入队文件数
    /// （仅 session 句柄寻址；断线自动重拨）
    fn peer_pull_files(
        &self,
        session: &str,
        dir_id: &str,
        files: &[serde_json::Value],
    ) -> Result<u32, HostError>;
}
