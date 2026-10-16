//! 对等网络基础能力（WIT `host-peer`，ADR 0022 v3 终态 13 原语）
//!
//! 宿主只提供拨号/关闭/信任/收发/闸门配置/广播面同步的无业务语义原语；
//! file-transfer 插件是首个上层消费者，设备列表、任务队列与历史、共享根
//! 注册表、接收策略等产品状态由插件自持（issue 13 Phase 3）。
//!
//! 事件（发现/离开/连上断开/首连确认请求/传输进度）经消息总线
//! `mdns:*` / `peer:*` topic 推送，插件用 [`HostBus`](super::HostBus)
//! 订阅后在自己的 `on_message` 回调里消费。

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
    /// 连接已断时以句柄记忆的 endpoint 自动重拨。**v31 收窄**：一次调用 =
    /// 一个会话立即发起（宿主并发闸门删除，节流由调用方自控）；载荷
    /// `concurrency` 字段退役，出现即显性报错
    fn peer_send_files(
        &self,
        session: &str,
        paths: &[serde_json::Value],
    ) -> Result<String, HostError>;
    /// 接收批应答：accept=false 或超时视为拒绝
    fn peer_respond_transfer(&self, batch_id: &str, accept: bool) -> Result<(), HostError>;
    /// 设置接收策略（引擎安全闸门配置原语）：mode = "ask" | "always_accept" | "always_deny"
    fn peer_set_receive_policy(&self, mode: &str, timeout_secs: u64) -> Result<(), HostError>;
    /// 显式暂停进行中的发送批（batch-id 寻址）：会话数据面门控（wire Pause 帧 +
    /// 供方停推流，连接保持），任务保留（含已传字节）不落历史；无命中报错
    fn peer_pause_transfer(&self, batch_id: &str) -> Result<(), HostError>;
    /// 恢复暂停的发送批：活跃会话写 Resume 帧续流；会话已中断的以句柄表记忆
    /// 的源清单重新拨号续传（断点真源在接收端落盘侧）。v31 起「全部恢复」
    /// 编排归插件（遍历自身暂停批逐个调本原语），resume-all-transfers 退役
    fn peer_resume_transfer(&self, batch_id: &str) -> Result<(), HostError>;
    /// 全量幂等替换引擎广播源：条目 `[{ id, name, path }]`（移动端 safTreeUri）
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
    /// 设置接收落点目录（空串 = 恢复默认；引擎落盘配置原语）
    fn peer_set_download_dir(&self, path: &str) -> Result<(), HostError>;
    /// 按需启动本机 peer 节点（引擎级生命周期原语，审计票 12）：幂等。
    /// 返回 `true` = 本次调用把节点从「未跑」带到「跑」（调用方即成为节点属主）；
    /// `false` = 节点本就在跑。已被别的插件起着的节点不可被本调用接管（报错，
    /// 文案不回带他方身份）——谁起谁停，内核不再按硬编码插件 id 猜归属
    fn peer_start_node(&self) -> Result<bool, HostError>;
    /// 让本机节点下线（幂等；未跑为 no-op）：停广播 / 关监听 / 排水连接与入站记账。
    /// 仅节点属主可关停，非属主报错（文案不回带属主身份）
    fn peer_stop_node(&self) -> Result<bool, HostError>;
    /// 活跃传输批清单（v30）：宿主会话表投影 `[{ batchId, direction, status,
    /// totalBytes, transferredBytes, rateBps, updatedAtMs }]`（仅引擎会话事实，
    /// 无 peerName 等业务字段）；供插件事件归约状态机首屏重建
    fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError>;
    /// 收集发送源（v30）：paths = string[]，目录递归展开 + 批内同名去重
    /// → `[{ path, size }]`（仅元数据；路径应来自 pick-* 用户选择）
    fn peer_collect_outgoing(&self, paths: &[serde_json::Value]) -> Result<serde_json::Value, HostError>;
}
