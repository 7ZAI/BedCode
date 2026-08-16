//! WS 控制面公告（Announce / Withdraw，内网文件传输插件规格阶段 2）
//!
//! 移动文件服务的端口/token/挂载清单经**已认证的现有 WS** 公告给桌面端，
//! 不开新连接、不用 mDNS（规格 4.5）。
//!
//! 触发时机（由调用方保证）：
//! - 首个挂载启动服务后 / 挂载集合变更后 → [`announce`]
//! - 认证成功（含重连）→ [`announce`]（resend；重连后桌面 peer 记录已被清空）
//! - 末个挂载摘除服务停止时 → [`withdraw`]
//! - 解配/token revoke 时：连接已断则不发 Withdraw（桌面断连路径已清理）

use crate::enums::file_service::FileServicePayload;
use crate::file_service::registry::FileServiceRegistry;
use crate::file_service::server::FileServiceServer;
use crate::model::message::Message;
use std::sync::Arc;

/// 公告当前服务状态（服务未运行/无挂载时静默跳过）
///
/// 连接未建立时仅记 debug 日志 —— 认证成功的重连路径会 resend，不丢状态
pub async fn announce(registry: &Arc<FileServiceRegistry>, server: &Arc<FileServiceServer>) {
    let Some(port) = server.port().await else {
        tracing::debug!("file service announce skipped: server not running");
        return;
    };
    let Some(token) = server.token_guard().current_for_announce() else {
        tracing::debug!("file service announce skipped: no active token");
        return;
    };
    let mounts = registry.mount_announcements().await;
    if mounts.is_empty() {
        tracing::debug!("file service announce skipped: no active mounts");
        return;
    }

    let payload = FileServicePayload::Announce {
        port,
        token,
        // 携带对端真实设备名供桌面端文件传输展示；SystemInfo 可能尚未初始化
        //（try_get_system_info 为 None），此时为空串，桌面端保留原记录名
        device_name: crate::state::try_get_system_info()
            .map(|i| i.device_name.clone())
            .unwrap_or_default(),
        mounts,
    };
    send(payload).await;
}

/// 撤回公告（末个挂载摘除、服务停止后调用）
pub async fn withdraw() {
    send(FileServicePayload::Withdraw {}).await;
}

/// 经 ConnectionManager 发送 FileService 消息（自动注入 JWT）
async fn send(payload: FileServicePayload) {
    let conn = crate::state::get_connection_manager();
    if !conn.is_connected().await {
        // 连接已断：重连认证成功后 resend_if_active 会补发公告
        tracing::debug!("file service message skipped: WS not connected");
        return;
    }
    let msg = Message::file_service(payload);
    if let Err(e) = conn.send(&msg).await {
        tracing::warn!("file service message send failed: {}", e);
    }
}
