//! Mobile-to-Frontend Event Forwarding
//!
//! 将 MobileEvent 转发为 Tauri 前端事件
//!
//! 事件类型：
//! - ws_connecting: 连接开始
//! - ws_disconnected: 断开连接
//! - ws_error: 错误
//! - ws_auth_success / ws_auth_failed: 认证结果
//! - ws_pairing_request / ws_pairing_verified / ws_paired: 配对流程
//! - ws_sync_*: 会话同步事件

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Listener};
use tracing;

use crate::state::get_connection_manager;
use crate::system::error_boundary::spawn_with_error_boundary;

// ==================== MobileEvent Definition ====================

/// 移动端业务事件
///
/// 用于路由层向事件转发层传递业务事件
/// 最终由 event.rs 转换为 Tauri 前端事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MobileEvent {
    // === 认证事件 ===
    /// 认证成功
    AuthSuccess { session_token: String },
    /// 认证失败
    AuthFailed { reason: String },
    /// 配对请求（桌面端显示配对码）
    PairingRequest,
    /// 配对验证成功
    PairingVerified,
    /// 配对完成（认证成功）
    Paired,

    // === 链路加密事件 ===
    /// 链路加密 pin 刷新（auth 响应携带桌面端身份公钥/指纹时广播，供前端落盘）
    LinkCryptoPin {
        kd_public_b64: Option<String>,
        kd_fingerprint: Option<String>,
    },

    // === 系统事件 ===
    /// 服务器关闭
    ServerClosed { reason: String },
    /// 错误事件
    Error { message: String },
    /// ACK 响应
    Ack { request_id: String },

    // === 会话同步事件 ===
    /// 会话创建
    SyncSessionCreated {
        session: crate::enums::SessionSummary,
        source_device: String,
    },
    /// 会话状态变化
    SyncSessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止
    SyncSessionStopped { session_id: String, session_name: String },
    /// 会话删除
    SyncSessionRemoved { session_id: String, session_name: String },

    // === 配置同步事件 ===
    /// 配置创建
    SyncConfigCreated {
        config: crate::enums::SessionConfigSummary,
        source_device: String,
    },
    /// 配置更新
    SyncConfigUpdated {
        config: crate::enums::SessionConfigSummary,
        source_device: String,
    },
    /// 配置删除
    SyncConfigRemoved { config_id: String, config_name: String },

    // === 任务状态同步事件 ===
    /// 任务状态变更
    SyncTaskStatusChanged {
        session_id: String,
        task_status: String,
        task_reason: Option<String>,
        task_questions: Option<Vec<crate::enums::plugin::PluginQuestion>>,
    },

    // === 会话模式同步事件 ===
    /// 会话自动授权模式变更
    SyncSessionModeChanged { session_id: String, auto_approve: bool },

    // === 任务队列同步事件 ===
    /// 会话任务队列变更
    SyncTaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带，供预设任务完成匹配）
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        status: Option<String>,
    },

    // === 定时自动任务同步事件（v6，ADR 0003） ===
    /// 定时自动任务变更
    SyncTaskScheduledChanged {
        job_id: String,
        /// 变更后的状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
        action: String,
    },
}

// ==================== Event Forwarding ====================

/// 输出事件转发标志（只启动一次）
static OUTPUT_FORWARDING_STARTED: AtomicBool = AtomicBool::new(false);

/// 启动事件转发任务
///
/// 将 MobileEvent 转发为 Tauri 前端事件：
/// - SyncSessionCreated → ws_sync_session_created
/// - SyncSessionStatusChanged → ws_sync_session_status_changed
/// - SyncSessionStopped → ws_sync_session_stopped
/// - SyncSessionRemoved → ws_sync_session_removed
/// - SyncConfigCreated → ws_sync_config_created
/// - SyncConfigUpdated → ws_sync_config_updated
/// - SyncConfigRemoved → ws_sync_config_removed
pub fn start_event_forwarding(app_handle: AppHandle) {
    // 仅启动一次
    if OUTPUT_FORWARDING_STARTED.swap(true, Ordering::SeqCst) {
        tracing::debug!("[EventForwarder] Already started, skipping");
        return;
    }

    let conn_fwd = get_connection_manager();
    let mut event_rx = conn_fwd.subscribe();
    let app_clone = app_handle.clone();

    spawn_with_error_boundary("output_forwarder", async move {
        tracing::info!("[EventForwarder] Started forwarding output events");

        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    forward_event(&app_clone, event).await;
                }
                Err(e) => {
                    tracing::error!("[EventForwarder] Event recv error: {:?}", e);
                    break;
                }
            }
        }

        tracing::warn!("[EventForwarder] Event channel closed");
    });
}

/// 转发单个事件到前端
async fn forward_event(app: &AppHandle, event: MobileEvent) {
    match event.clone() {
        // 会话同步事件
        MobileEvent::SyncSessionCreated { session, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncSessionCreated: session_id={}, source={}",
                session.id,
                source_device
            );
            let _ = app.emit(
                "ws_sync_session_created",
                serde_json::json!({
                    "session": session,
                    "source_device": source_device,
                }),
            );
        }

        MobileEvent::SyncSessionStatusChanged {
            session_id,
            old_status,
            new_status,
            session_name,
        } => {
            tracing::info!(
                "[EventForwarder] SyncSessionStatusChanged: session_id={}, {} -> {}",
                session_id,
                old_status,
                new_status
            );
            let _ = app.emit(
                "ws_sync_session_status_changed",
                serde_json::json!({
                    "session_id": session_id,
                    "old_status": old_status,
                    "new_status": new_status,
                    "session_name": session_name,
                }),
            );
        }

        MobileEvent::SyncSessionStopped {
            session_id,
            session_name,
        } => {
            tracing::info!("[EventForwarder] SyncSessionStopped: session_id={}", session_id);
            let _ = app.emit(
                "ws_sync_session_stopped",
                serde_json::json!({
                    "session_id": session_id,
                    "session_name": session_name,
                }),
            );
        }

        MobileEvent::SyncSessionRemoved {
            session_id,
            session_name,
        } => {
            tracing::info!("[EventForwarder] SyncSessionRemoved: session_id={}", session_id);
            let _ = app.emit(
                "ws_sync_session_removed",
                serde_json::json!({
                    "session_id": session_id,
                    "session_name": session_name,
                }),
            );
        }

        // 配置同步事件
        MobileEvent::SyncConfigCreated { config, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncConfigCreated: config_id={}, source={}",
                config.id,
                source_device
            );
            let _ = app.emit(
                "ws_sync_config_created",
                serde_json::json!({
                    "config": config,
                    "source_device": source_device,
                }),
            );
        }

        MobileEvent::SyncConfigUpdated { config, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncConfigUpdated: config_id={}, source={}",
                config.id,
                source_device
            );
            let _ = app.emit(
                "ws_sync_config_updated",
                serde_json::json!({
                    "config": config,
                    "source_device": source_device,
                }),
            );
        }

        MobileEvent::SyncConfigRemoved { config_id, config_name } => {
            tracing::info!("[EventForwarder] SyncConfigRemoved: config_id={}", config_id);
            let _ = app.emit(
                "ws_sync_config_removed",
                serde_json::json!({
                    "config_id": config_id,
                    "config_name": config_name,
                }),
            );
        }

        MobileEvent::SyncTaskStatusChanged {
            session_id,
            task_status,
            task_reason,
            task_questions,
        } => {
            tracing::info!(
                "[EventForwarder] SyncTaskStatusChanged: session_id={}, status={}",
                session_id,
                task_status
            );
            let _ = app.emit(
                "ws_sync_task_status_changed",
                serde_json::json!({
                    "session_id": session_id,
                    "task_status": task_status,
                    "task_reason": task_reason,
                    "task_questions": task_questions,
                }),
            );
        }

        MobileEvent::SyncSessionModeChanged {
            session_id,
            auto_approve,
        } => {
            tracing::info!(
                "[EventForwarder] SyncSessionModeChanged: session_id={}, auto_approve={}",
                session_id,
                auto_approve
            );
            let _ = app.emit(
                "ws_sync_session_mode_changed",
                serde_json::json!({
                    "session_id": session_id,
                    "auto_approve": auto_approve,
                }),
            );
        }

        MobileEvent::SyncTaskQueueChanged {
            session_id,
            queue_count,
            action,
            task_id,
            status,
        } => {
            tracing::info!(
                "[EventForwarder] SyncTaskQueueChanged: session_id={}, count={}, action={}, task_id={:?}, status={:?}",
                session_id,
                queue_count,
                action,
                task_id,
                status
            );
            let _ = app.emit(
                "ws_sync_task_queue_changed",
                serde_json::json!({
                    "session_id": session_id,
                    "queue_count": queue_count,
                    "action": action,
                    "task_id": task_id,
                    "status": status,
                }),
            );
        }

        MobileEvent::SyncTaskScheduledChanged { job_id, status, action } => {
            tracing::info!(
                "[EventForwarder] SyncTaskScheduledChanged: job_id={}, status={}, action={}",
                job_id,
                status,
                action
            );
            let _ = app.emit(
                "ws_sync_task_scheduled_changed",
                serde_json::json!({
                    "job_id": job_id,
                    "status": status,
                    "action": action,
                }),
            );
        }

        MobileEvent::LinkCryptoPin {
            kd_public_b64,
            kd_fingerprint,
        } => {
            tracing::info!(
                "[EventForwarder] LinkCryptoPin: kd_public_b64={}, kd_fingerprint={}",
                kd_public_b64.as_deref().unwrap_or("none"),
                kd_fingerprint.as_deref().unwrap_or("none")
            );
            // 字段用 camelCase 与前端 notePinFromAuthData 读取约定一致；
            // Option 序列化为 null，前端仅在非空时写 pin
            let _ = app.emit(
                "ws_link_crypto_pin",
                serde_json::json!({
                    "kdPublicB64": kd_public_b64,
                    "kdFingerprint": kd_fingerprint,
                }),
            );
        }

        // 其他事件不转发
        _ => {}
    }
}

// ==================== Terminal Output Activity ====================

/// 监听前端 `terminal_output_activity` 事件（前端终端 WS 收到输出帧时发出）
///
/// 09 迁移后终端输出不再经 Rust 中转，插件 TerminalOutput 通知改由
/// 前端 socket 路径在此触发（保持仅传 session_id 语义，D6）。
/// 异步分发不 await：插件 WASM 回调串行执行，阻塞会拖慢事件处理
static TERMINAL_OUTPUT_LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

pub fn init_terminal_output_listener(app: &AppHandle) {
    if TERMINAL_OUTPUT_LISTENER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let pm = crate::state::get_plugin_manager();
    let _ = app.listen("terminal_output_activity", move |event| {
        // Tauri 事件 payload 为 JSON 字符串，解析出 session_id（仅传 session_id 语义）
        let session_id = serde_json::from_str::<serde_json::Value>(event.payload())
            .ok()
            .and_then(|v| v.get("session_id").and_then(|s| s.as_str()).map(str::to_string))
            .unwrap_or_default();
        if session_id.is_empty() {
            return;
        }
        let pm = pm.clone();
        spawn_with_error_boundary("plugin_terminal_output_notify", async move {
            pm.dispatch_lifecycle_event(crate::plugin::types::PluginLifecycleEvent::TerminalOutput {
                session_id,
                data: String::new(),
            })
            .await;
        });
    });
}

// ==================== Event Helpers ====================

/// 发射连接开始事件
pub fn emit_connecting(app: &AppHandle, address: &str, port: u16) {
    tracing::debug!("[EventHelper] Emitting ws_connecting");
    let _ = app.emit(
        "ws_connecting",
        serde_json::json!({
            "address": address,
            "port": port,
        }),
    );
}

/// 发射断开连接事件
pub fn emit_disconnected(app: &AppHandle, reason: &str) {
    tracing::info!("[EventHelper] Emitting ws_disconnected: {}", reason);
    let _ = app.emit(
        "ws_disconnected",
        serde_json::json!({
            "reason": reason,
        }),
    );
}

/// 发射错误事件
pub fn emit_error(app: &AppHandle, message: &str) {
    tracing::error!("[EventHelper] Emitting ws_error: {}", message);
    let _ = app.emit(
        "ws_error",
        serde_json::json!({
            "message": message,
        }),
    );
}

/// 发射认证成功事件
pub fn emit_auth_success(app: &AppHandle) {
    tracing::info!("[EventHelper] Emitting ws_auth_success");
    let _ = app.emit("ws_auth_success", ());
}

/// 发射配对成功事件
pub fn emit_paired(app: &AppHandle) {
    tracing::info!("[EventHelper] Emitting ws_paired");
    let _ = app.emit("ws_paired", ());
}

/// 发射配对请求事件
pub fn emit_pairing_request(app: &AppHandle) {
    tracing::info!("[EventHelper] Emitting ws_pairing_request");
    let _ = app.emit("ws_pairing_request", ());
}

/// 发射配对验证成功事件
pub fn emit_pairing_verified(app: &AppHandle) {
    tracing::info!("[EventHelper] Emitting ws_pairing_verified");
    let _ = app.emit("ws_pairing_verified", ());
}

/// 发射认证失败事件
pub fn emit_auth_failed(app: &AppHandle, reason: &str) {
    tracing::error!("[EventHelper] Emitting ws_auth_failed: {}", reason);
    let _ = app.emit(
        "ws_auth_failed",
        serde_json::json!({
            "reason": reason,
        }),
    );
}
