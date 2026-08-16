//! Sync Handler - 同步数据消息处理器

use async_trait::async_trait;

use crate::model::message::Message;
use crate::enums::SyncPayload;
use crate::Result;

use crate::router::{ClientRouteContext, MobileEvent, ClientRouteHandler};

/// 桌面端对端 peer_id 前缀（移动 → 桌面方向，peer_id = "desktop:" + 连接目标地址）
///
/// 移动端同时只连接一台桌面端，用地址区分足够唯一；前缀避免与其他
/// 可能的 peer 源冲突（如未来多桌面场景）
pub(crate) const DESKTOP_PEER_PREFIX: &str = "desktop:";

/// 同步数据消息处理器
pub struct SyncHandler;

#[async_trait]
impl ClientRouteHandler for SyncHandler {
    async fn handle(&self, message: Message, ctx: &ClientRouteContext) -> Result<Option<Message>> {
        if let Message::SyncData { payload, .. } = message {
            match payload {
                SyncPayload::SessionCreated { session, source_device } => {
                    tracing::info!("[SyncHandler] SessionCreated: session_id={}, source={}", session.id, source_device);
                    ctx.emit(MobileEvent::SyncSessionCreated {
                        session,
                        source_device,
                    });
                }
                SyncPayload::SessionStatusChanged { session_id, old_status, new_status, session_name } => {
                    tracing::info!("[SyncHandler] SessionStatusChanged: session_id={}, {} -> {}", session_id, old_status, new_status);
                    ctx.emit(MobileEvent::SyncSessionStatusChanged {
                        session_id,
                        old_status,
                        new_status,
                        session_name,
                    });
                }
                SyncPayload::SessionStopped { session_id, session_name } => {
                    tracing::info!("[SyncHandler] SessionStopped: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionStopped {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::SessionRemoved { session_id, session_name } => {
                    tracing::info!("[SyncHandler] SessionRemoved: session_id={}", session_id);
                    ctx.emit(MobileEvent::SyncSessionRemoved {
                        session_id,
                        session_name,
                    });
                }
                SyncPayload::ConfigCreated { config, source_device } => {
                    tracing::info!("[SyncHandler] ConfigCreated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigCreated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigUpdated { config, source_device } => {
                    tracing::info!("[SyncHandler] ConfigUpdated: config_id={}, source={}", config.id, source_device);
                    ctx.emit(MobileEvent::SyncConfigUpdated {
                        config,
                        source_device,
                    });
                }
                SyncPayload::ConfigRemoved { config_id, config_name } => {
                    tracing::info!("[SyncHandler] ConfigRemoved: config_id={}", config_id);
                    ctx.emit(MobileEvent::SyncConfigRemoved {
                        config_id,
                        config_name,
                    });
                }
                SyncPayload::TaskStatusChanged { session_id, task_status, task_reason, task_questions } => {
                    tracing::info!("[SyncHandler] TaskStatusChanged: session_id={}, status={}", session_id, task_status);
                    ctx.emit(MobileEvent::SyncTaskStatusChanged {
                        session_id,
                        task_status,
                        task_reason,
                        task_questions,
                    });
                }
                SyncPayload::SessionModeChanged { session_id, auto_approve } => {
                    tracing::info!("[SyncHandler] SessionModeChanged: session_id={}, auto_approve={}", session_id, auto_approve);
                    ctx.emit(MobileEvent::SyncSessionModeChanged {
                        session_id,
                        auto_approve,
                    });
                }
                SyncPayload::TaskQueueChanged { session_id, queue_count, action, task_id, status } => {
                    tracing::info!("[SyncHandler] TaskQueueChanged: session_id={}, count={}, action={}, task_id={:?}, status={:?}", session_id, queue_count, action, task_id, status);
                    ctx.emit(MobileEvent::SyncTaskQueueChanged {
                        session_id,
                        queue_count,
                        action,
                        task_id,
                        status,
                    });
                }
                SyncPayload::TaskScheduledChanged { job_id, status, action } => {
                    tracing::info!("[SyncHandler] TaskScheduledChanged: job_id={}, status={}, action={}", job_id, status, action);
                    ctx.emit(MobileEvent::SyncTaskScheduledChanged {
                        job_id,
                        status,
                        action,
                    });
                }
                SyncPayload::FileServiceChanged { plugin_id, mount_path, available, operations } => {
                    tracing::info!("[SyncHandler] FileServiceChanged: plugin_id={}, mount={}, available={}", plugin_id, mount_path, available);

                    // 同步更新桌面端 peer 记录（挂载增量合并），触发 peer_changed 推送
                    update_desktop_peer(&plugin_id, &mount_path, available, &operations).await;

                    ctx.emit(MobileEvent::SyncFileServiceChanged {
                        plugin_id,
                        mount_path,
                        available,
                        operations,
                    });
                }
                SyncPayload::TransferApproval {
                    batch_id,
                    decision,
                    reason,
                } => {
                    // 传输批应答（v2）：接收端批准/拒绝/超时 → 发送端
                    // 双通道发布 filesrv:transfer_approval，发送方插件据此调度批内任务
                    tracing::info!(
                        "[SyncHandler] TransferApproval: batch_id={}, decision={}, reason={}",
                        batch_id,
                        decision,
                        reason
                    );
                    crate::state::get_file_service()
                        .registry
                        .publish_transfer_approval(&batch_id, &decision, &reason)
                        .await;
                }
            }
        }
        Ok(None)
    }

    fn name(&self) -> &str {
        "SyncHandler"
    }
}

impl Default for SyncHandler {
    fn default() -> Self {
        Self
    }
}

// ==================== Desktop Peer Tracking ====================

/// 返回当前桌面端 peer_id（基于连接目标地址）
///
/// 断连清理路径使用此函数获取待 remove 的 peer_id；未连接时返回 None
pub async fn desktop_peer_id() -> Option<String> {
    let cm = crate::state::get_connection_manager();
    cm.get_target()
        .await
        .map(|t| format!("{}{}", DESKTOP_PEER_PREFIX, t.address))
}

/// 根据 FileServiceChanged 增量更新桌面端 peer 记录
///
/// - available=true：添加/替换挂载条目（operations 变更视为更新）
/// - available=false：移除挂载条目
///
/// 更新后调用 set_peer，由注册表内部去重并触发 peer_changed 双通道推送。
/// 首次收到事件时自动以连接目标 IP/端口构造 PeerFileService 基础信息
async fn update_desktop_peer(
    plugin_id: &str,
    mount_path: &str,
    available: bool,
    operations: &[bedcode_plugin_api_mobile::FileOperation],
) {
    let cm = crate::state::get_connection_manager();
    let Some(target) = cm.get_target().await else {
        tracing::debug!("update_desktop_peer: no connection target, skip");
        return;
    };

    let peer_id = format!("{}{}", DESKTOP_PEER_PREFIX, target.address);
    let fs = crate::state::get_file_service();
    let registry = &fs.registry;

    // 取现有 peer 信息，不存在则以连接目标 IP/端口初始化
    let mut peer = registry.get_peer(&peer_id).await.unwrap_or_else(|| {
        bedcode_plugin_api_mobile::PeerFileService {
            ip: target.address.clone(),
            // 桌面端文件服务复用 HTTP server 端口（与 WS 同端口）
            port: target.port,
            token: crate::state::get_global_token(),
            // 真实设备名由桌面端 Announce 公告携带（此路径无名称信息）
            device_name: String::new(),
            mounts: Vec::new(),
        }
    });

    // 确保已有 peer 记录也填充 ip/port/token（首次公告可能早于 token 注入，
    // 后续公告到达时补全；token 变更时同步更新）
    peer.ip = target.address.clone();
    peer.port = target.port;
    let current_token = crate::state::get_global_token();
    if !current_token.is_empty() {
        peer.token = current_token;
    }

    // 增量合并挂载列表
    peer.mounts.retain(|m| !(m.plugin_id == plugin_id && m.mount_path == mount_path));
    if available {
        peer.mounts.push(bedcode_plugin_api_mobile::PeerMountAnnouncement {
            plugin_id: plugin_id.to_string(),
            mount_path: mount_path.to_string(),
            operations: operations.to_vec(),
        });
    }

    // set_peer 内部比较新旧信息，有变化才触发推送（去重）
    registry.set_peer(&peer_id, peer).await;
}
