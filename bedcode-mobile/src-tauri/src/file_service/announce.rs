//! WS 控制面公告（Announce / Withdraw，内网文件传输插件规格阶段 2）
//!
//! v2.1 服务器归零后移动端不再监听端口：Announce 不再携带有效 port/token
//! （置 0 / 空串），仅向桌面同步**挂载清单**（供桌面浏览与拉取）；port/token
//! 字段保留在 wire 上仅为兼容桌面旧解析，桌面不因 port=0 丢弃 peer 记录。
//! 经**已认证的现有 WS** 公告给桌面端，不开新连接、不用 mDNS（规格 4.5）。
//!
//! 触发时机（由调用方保证）：
//! - 挂载集合变更后 → [`announce`]
//! - 认证成功（含重连）→ [`announce`]（resend；重连后桌面 peer 记录已被清空）
//! - 末个挂载摘除时 → [`withdraw`]
//! - 解配/token revoke 时：连接已断则不发 Withdraw（桌面断连路径已清理）

use crate::enums::file_service::FileServicePayload;
use crate::file_service::registry::FileServiceRegistry;
use crate::model::message::Message;
use std::sync::Arc;

/// 公告当前挂载清单（无挂载时静默跳过）
///
/// 连接未建立时仅记 debug 日志 —— 认证成功的重连路径会 resend，不丢状态
pub async fn announce(registry: &Arc<FileServiceRegistry>) {
    let mounts = registry.mount_announcements().await;
    if mounts.is_empty() {
        tracing::debug!("file service announce skipped: no active mounts");
        return;
    }

    let payload = FileServicePayload::Announce {
        // 服务器归零：port/token 置 0/空（桌面权威 HTTP 端点经 WS 端口派生，
        // 手机自身不监听；桌面侧不因 port=0 丢弃挂载记录）
        port: 0,
        token: String::new(),
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
///
/// pub(crate)：handler（FileListResponse 回包等）复用同一发送路径
pub(crate) async fn send(payload: FileServicePayload) {
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
