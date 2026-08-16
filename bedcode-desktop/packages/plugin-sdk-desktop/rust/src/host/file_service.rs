//! 宿主能力：文件服务（受控目录挂载为 HTTP 端点）
//!
//! 插件通过此能力将用户配置的允许目录挂载到宿主现有 HTTP 服务上
//! （/api/plugins/{pluginId}/{mountPath}/**，自动经过宿主鉴权），
//! 宿主强制目录沙箱与上传策略钩子，插件无法绕过。

use super::HostError;
use crate::types::{MountOptions, MountResult, PeerFileService};

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
    /// `filesrv:peer_changed` 事件推送。peer_id 为空表示询问全部已认证
    /// 客户端（桌面端多连接场景）。用于对端状态事件遗漏时主动恢复。
    fn filesrv_query_peer(&self, peer_id: &str) -> Result<(), HostError>;

    /// v2：批准传输批（接收端用户应答「接受全部」）
    ///
    /// 批必须处于 pending 且属于当前插件；批准后批内 session 创建免钩子。
    fn filesrv_approve_transfer(&self, batch_id: &str) -> Result<(), HostError>;

    /// v2：拒绝传输批（接收端用户应答「拒绝全部」）
    ///
    /// 批必须处于 pending；拒绝后发送方任务转为 rejected(user-rejected)。
    fn filesrv_reject_transfer(&self, batch_id: &str) -> Result<(), HostError>;

    /// v2：设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
    fn filesrv_set_approval_timeout(&self, mount_path: &str, seconds: u64) -> Result<(), HostError>;

    /// v2：取消接收中的上传会话（接收端本地取消，session 级）
    ///
    /// 清理 .part 并推送 `filesrv:receiving_done(cancelled)`；
    /// 发送方 session 丢失后自动重建从头传（v1 语义兜底）。
    fn filesrv_cancel_receiving(&self, session_id: &str) -> Result<(), HostError>;
}
