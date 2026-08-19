//! 宿主能力：文件服务（受控目录挂载为 HTTP 端点）
//!
//! 插件通过此能力将用户配置的允许目录挂载到宿主独立 HTTP 服务上
//! （/{pluginId}/{mountPath}/**，Bearer Token 鉴权，端口经 WS 控制面公告），
//! 宿主强制目录沙箱与上传策略钩子，插件无法绕过。
//! 与桌面端 SDK `host/file_service.rs` 同构（移动端无 /api 前缀）。

use super::HostError;
use crate::types::{FileTransferRequest, MountOptions, MountResult, PeerFileService};

/// 插件文件服务宿主能力
///
/// 需要 `fileservice` 权限（未声明则拒绝挂载）。
/// 挂载随插件生命周期：deactivate/停用/卸载时宿主自动摘除。
pub trait HostFileService {
    /// 挂载文件服务
    ///
    /// roots 必须存在、是目录、通过宿主 fs 授权，否则失败；
    /// 重复/嵌套 root 由宿主去重取最外层
    fn filesrv_mount(&self, options: &MountOptions) -> Result<MountResult, HostError>;

    /// 卸载挂载点（mount_path 为本插件此前挂载的名称）
    fn filesrv_unmount(&self, mount_path: &str) -> Result<(), HostError>;

    /// 更新挂载点的允许目录根（目录变更即时生效，校验规则同 mount）
    fn filesrv_update_roots(&self, mount_path: &str, roots: &[String]) -> Result<(), HostError>;

    /// 获取对端文件服务信息；对端未公告返回 `Ok(None)`
    fn filesrv_get_peer(&self, peer_id: &str) -> Result<Option<PeerFileService>, HostError>;

    /// 主动询问对端文件服务状态（经 WS 控制面发送 Query）
    ///
    /// 对端会回复 Announce/Withdraw，宿主注册表更新后经
    /// `filesrv:peer_changed` 事件推送。peer_id 为空表示询问当前连接对端
    ///（移动端单连接场景）。用于对端状态事件遗漏时主动恢复。
    fn filesrv_query_peer(&self, peer_id: &str) -> Result<(), HostError>;

    /// 批准传输批（v2 接收端用户应答「接受全部」）
    ///
    /// 批必须处于 pending 且归属当前插件，否则返回错误；
    /// 宿主将批置 approved、发本地 resolved 事件并跨端推送发送方。
    fn filesrv_approve_transfer(&self, batch_id: &str) -> Result<(), HostError>;

    /// 拒绝传输批（v2 接收端用户应答「拒绝全部」）
    ///
    /// 批必须处于 pending 且归属当前插件；宿主将批置 rejected(user-rejected)、
    /// 发本地 resolved 事件并跨端推送发送方。
    fn filesrv_reject_transfer(&self, batch_id: &str) -> Result<(), HostError>;

    /// 设置批准超时（v2，秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
    ///
    /// 按 (plugin, mount) 维度配置；越界值返回错误。
    fn filesrv_set_approval_timeout(&self, mount_path: &str, seconds: u64) -> Result<(), HostError>;

    /// 取消接收中的上传会话（v2 接收端本地取消，session 级）
    ///
    /// 宿主删除 .part 临时文件并发出 `filesrv:receiving_done`(cancelled)。
    fn filesrv_cancel_receiving(&self, session_id: &str) -> Result<(), HostError>;

    /// 手机自主发起下载（v2.1 服务器归零：GET+Range 落 SAF/下载目录）
    ///
    /// 经宿主 client 栈执行（断点续传 + HEAD 指纹比对），返回 task_id；
    /// 进度经 `transfer:{task_id}` 通道 / `plugin:transfer:progress` 事件回报
    fn filesrv_download(&self, req: &FileTransferRequest) -> Result<String, HostError>;

    /// 手机自主发起上传（v2.1 服务器归零：POST/PUT session 编排）
    ///
    /// 经宿主 client 栈执行（断点重查 + 404 重建），返回 task_id
    fn filesrv_upload(&self, req: &FileTransferRequest) -> Result<String, HostError>;

    /// intent 应答（v2.1 push 审批门）：
    /// decision = "accepted" → 用户确认，手机回 IntentAck{accepted} 并执行下载；
    /// decision = "rejected" → 用户拒绝，手机回 IntentAck{rejected} 且不执行
    fn filesrv_respond_intent(&self, intent_id: &str, decision: &str) -> Result<(), HostError>;
}
