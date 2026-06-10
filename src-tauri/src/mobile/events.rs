//! Mobile-to-Frontend Event Forwarding
//!
//! 将 MobileEvent 转发为 Tauri 前端事件
//!
//! 事件类型：
//! - ws_output: 终端输出
//! - ws_connecting: 连接开始
//! - ws_disconnected: 断开连接
//! - ws_error: 错误
//! - ws_auth_success / ws_auth_failed: 认证结果
//! - ws_pairing_request / ws_pairing_verified / ws_paired: 配对流程
//! - ws_sync_*: 会话同步事件

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tracing;

use crate::shared::system::error_boundary::spawn_with_error_boundary;
use crate::mobile::get_connection_manager;
use crate::mobile::router::event::MobileEvent;

/// 输出事件转发标志（只启动一次）
static OUTPUT_FORWARDING_STARTED: AtomicBool = AtomicBool::new(false);

/// 启动事件转发任务
///
/// 将 MobileEvent 转发为 Tauri 前端事件：
/// - Output → ws_output（终端输出）
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
                    tracing::info!("[EventForwarder] Received event: {:?}", event);
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
    tracing::info!("[EventForwarder] forward_event called with: {:?}", std::mem::discriminant(&event));
    match event.clone() {
        // 终端输出事件：解码 Base64 并发射 ws_output
        MobileEvent::Output { session_id, data, is_waiting, index: global_index } => {
            tracing::info!(
                "[EventForwarder] Output: session_id={}, data_len={}, index={}",
                session_id, data.len(), global_index
            );

            // 解码 Base64 并发射 ws_output
            let decoded_data = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &data,
            ).unwrap_or_default();
            let decoded_str = String::from_utf8_lossy(&decoded_data).to_string();

            let emit_result = app.emit("ws_output", serde_json::json!({
                "session_id": session_id,
                "data": decoded_str,
                "is_waiting": is_waiting,
                "index": global_index,
            }));

            if let Err(e) = emit_result {
                tracing::error!("[EventForwarder] Failed to emit ws_output: {}", e);
            } else {
                tracing::info!("[EventForwarder] Successfully emitted ws_output, data_len={}", decoded_str.len());
            }
        }

        // 会话同步事件
        MobileEvent::SyncSessionCreated { session, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncSessionCreated: session_id={}, source={}",
                session.id, source_device
            );
            let _ = app.emit("ws_sync_session_created", serde_json::json!({
                "session": session,
                "source_device": source_device,
            }));
        }

        MobileEvent::SyncSessionStatusChanged { session_id, old_status, new_status, session_name } => {
            tracing::info!(
                "[EventForwarder] SyncSessionStatusChanged: session_id={}, {} -> {}",
                session_id, old_status, new_status
            );
            let _ = app.emit("ws_sync_session_status_changed", serde_json::json!({
                "session_id": session_id,
                "old_status": old_status,
                "new_status": new_status,
                "session_name": session_name,
            }));
        }

        MobileEvent::SyncSessionStopped { session_id, session_name } => {
            tracing::info!("[EventForwarder] SyncSessionStopped: session_id={}", session_id);
            let _ = app.emit("ws_sync_session_stopped", serde_json::json!({
                "session_id": session_id,
                "session_name": session_name,
            }));
        }

        MobileEvent::SyncSessionRemoved { session_id, session_name } => {
            tracing::info!("[EventForwarder] SyncSessionRemoved: session_id={}", session_id);
            let _ = app.emit("ws_sync_session_removed", serde_json::json!({
                "session_id": session_id,
                "session_name": session_name,
            }));
        }

        // 配置同步事件
        MobileEvent::SyncConfigCreated { config, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncConfigCreated: config_id={}, source={}",
                config.id, source_device
            );
            let _ = app.emit("ws_sync_config_created", serde_json::json!({
                "config": config,
                "source_device": source_device,
            }));
        }

        MobileEvent::SyncConfigUpdated { config, source_device } => {
            tracing::info!(
                "[EventForwarder] SyncConfigUpdated: config_id={}, source={}",
                config.id, source_device
            );
            let _ = app.emit("ws_sync_config_updated", serde_json::json!({
                "config": config,
                "source_device": source_device,
            }));
        }

        MobileEvent::SyncConfigRemoved { config_id, config_name } => {
            tracing::info!("[EventForwarder] SyncConfigRemoved: config_id={}", config_id);
            let _ = app.emit("ws_sync_config_removed", serde_json::json!({
                "config_id": config_id,
                "config_name": config_name,
            }));
        }

        // 其他事件不转发
        _ => {}
    }
}

// ==================== Event Helpers ====================

/// 发射连接开始事件
pub fn emit_connecting(app: &AppHandle, address: &str, port: u16) {
    tracing::info!("[EventHelper] Emitting ws_connecting");
    let _ = app.emit("ws_connecting", serde_json::json!({
        "address": address,
        "port": port,
    }));
}

/// 发射断开连接事件
pub fn emit_disconnected(app: &AppHandle, reason: &str) {
    tracing::info!("[EventHelper] Emitting ws_disconnected");
    let _ = app.emit("ws_disconnected", serde_json::json!({
        "reason": reason,
    }));
}

/// 发射错误事件
pub fn emit_error(app: &AppHandle, message: &str) {
    tracing::error!("[EventHelper] Emitting ws_error: {}", message);
    let _ = app.emit("ws_error", serde_json::json!({
        "message": message,
    }));
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
    let _ = app.emit("ws_auth_failed", serde_json::json!({
        "reason": reason,
    }));
}