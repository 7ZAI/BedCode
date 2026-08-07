//! File Service Handler - 接收桌面端文件服务控制面消息
//!
//! 移动端 → 桌面端方向的消息发送在 `file_service/announce.rs`；
//! 本处理器处理桌面端 → 移动端方向（认证成功补发快照 / Query 响应）：
//! - Announce：全量更新桌面端 peer 记录（与 SyncData 增量合并互补）
//! - Withdraw：移除桌面端 peer 记录（对端服务停止）
//! - Query：回复自身文件服务状态（有挂载 → Announce；否则 → Withdraw）

use async_trait::async_trait;

use crate::enums::file_service::{FileServicePayload, MountAnnouncement};
use crate::handler::sync::desktop_peer_id;
use crate::model::message::Message;
use crate::router::{ClientRouteContext, ClientRouteHandler};
use crate::Result;

/// 文件服务控制面处理器
pub struct FileServiceHandler;

#[async_trait]
impl ClientRouteHandler for FileServiceHandler {
    async fn handle(&self, message: Message, _ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::FileService { payload, .. } = message {
            match payload {
                FileServicePayload::Announce { port, token, mounts } => {
                    tracing::info!(port, mounts = mounts.len(), "desktop file service announced");
                    apply_desktop_announce(port, token, mounts).await;
                }
                FileServicePayload::Withdraw {} => {
                    if let Some(peer_id) = desktop_peer_id().await {
                        tracing::info!(peer_id = %peer_id, "desktop file service withdrawn");
                        crate::state::get_file_service().registry.remove_peer(&peer_id).await;
                    }
                }
                FileServicePayload::Query {} => {
                    // 主动探测：回复自身文件服务状态（无挂载/服务未运行 → Withdraw）
                    let fs = crate::state::get_file_service();
                    if fs.registry.mount_count().await > 0 && fs.server.is_running().await {
                        tracing::info!("file service query received, replying announce");
                        crate::file_service::announce::announce(&fs.registry, &fs.server).await;
                    } else {
                        tracing::info!("file service query received, replying withdraw");
                        crate::file_service::announce::withdraw().await;
                    }
                }
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "FileServiceHandler"
    }
}

/// 应用桌面端 Announce：全量覆盖桌面端 peer 记录
///
/// peer_id 取当前连接目标（单连接场景，与 `update_desktop_peer` 同规则）；
/// ip/port/token 兜底与 `update_desktop_peer` 一致（token 为空时以宿主
/// 全局 token 填充，桌面端 HTTP 复用 WS 端口时端口一致）
async fn apply_desktop_announce(port: u16, token: String, mounts: Vec<MountAnnouncement>) {
    let cm = crate::state::get_connection_manager();
    let Some(target) = cm.get_target().await else {
        tracing::debug!("apply_desktop_announce: no connection target, skip");
        return;
    };

    let peer_id = format!("{}{}", crate::handler::sync::DESKTOP_PEER_PREFIX, target.address);
    let fs = crate::state::get_file_service();
    let registry = &fs.registry;

    let mut peer = registry.get_peer(&peer_id).await.unwrap_or_else(|| {
        bedcode_plugin_api_mobile::PeerFileService {
            ip: target.address.clone(),
            port: target.port,
            token: crate::state::get_global_token(),
            mounts: Vec::new(),
        }
    });
    peer.ip = target.address.clone();
    peer.port = port;
    if !token.is_empty() {
        peer.token = token;
    }
    peer.mounts = mounts
        .into_iter()
        .map(|m| bedcode_plugin_api_mobile::PeerMountAnnouncement {
            plugin_id: m.plugin_id,
            mount_path: m.mount_path,
            operations: m.operations,
        })
        .collect();

    registry.set_peer(&peer_id, peer).await;
}
