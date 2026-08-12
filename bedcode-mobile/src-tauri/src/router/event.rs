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
use serde::{Deserialize, Serialize};

use crate::system::error_boundary::spawn_with_error_boundary;
use crate::state::get_connection_manager;

// ==================== MobileEvent Definition ====================

/// 移动端业务事件
///
/// 用于路由层向事件转发层传递业务事件
/// 最终由 event.rs 转换为 Tauri 前端事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MobileEvent {
    // === 终端事件 ===
    /// 终端输出事件
    Output {
        session_id: String,
        data: String,
        is_waiting: bool,
        index: u64,
        /// 合并消息的结束索引，None 表示单条事件
        end_index: Option<u64>,
        /// 起始字节偏移（会话流坐标），供字节级游标续传（旧版服务端不发送）
        start_offset: Option<u64>,
        /// 结束字节偏移（会话流坐标）
        end_offset: Option<u64>,
    },

    // === 认证事件 ===
    /// 认证成功
    AuthSuccess {
        session_token: String,
    },
    /// 认证失败
    AuthFailed {
        reason: String,
    },
    /// 配对请求（桌面端显示配对码）
    PairingRequest,
    /// 配对验证成功
    PairingVerified,
    /// 配对完成（认证成功）
    Paired,

    // === 系统事件 ===
    /// 服务器关闭
    ServerClosed {
        reason: String,
    },
    /// 错误事件
    Error {
        message: String,
    },
    /// ACK 响应
    Ack {
        request_id: String,
    },

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
    SyncSessionStopped {
        session_id: String,
        session_name: String,
    },
    /// 会话删除
    SyncSessionRemoved {
        session_id: String,
        session_name: String,
    },

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
    SyncConfigRemoved {
        config_id: String,
        config_name: String,
    },

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
    SyncSessionModeChanged {
        session_id: String,
        auto_approve: bool,
    },

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

    // === 文件服务同步事件（桌面 → 移动，内网文件传输插件规格阶段 2） ===
    /// 桌面侧插件挂载点可用性变更
    SyncFileServiceChanged {
        plugin_id: String,
        mount_path: String,
        available: bool,
        operations: Vec<bedcode_plugin_api_mobile::FileOperation>,
    },
}

// ==================== Event Forwarding ====================

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
        // 终端输出事件：直接传递 Base64 到前端，由前端解码
        // 避免在 Rust 层做 Base64 解码 + UTF-8 lossy 转换的双重开销
        // 前端用 atob() 解码为 Uint8Array 传给 xterm.write()，比 string 更高效且无损
        MobileEvent::Output { session_id, data, is_waiting, index: global_index, end_index, start_offset, end_offset } => {
            if let Err(e) = app.emit("ws_output", serde_json::json!({
                "session_id": session_id,
                "data_base64": data,
                "is_waiting": is_waiting,
                "index": global_index,
                "end_index": end_index,
                "start_offset": start_offset,
                "end_offset": end_offset,
            })) {
                tracing::error!("[EventForwarder] Failed to emit ws_output: {}", e);
            }

            // 通知插件终端输出（只读通知，仅传递 session_id 避免大量数据拷贝）。
            // 必须异步分发（不 await）：插件 WASM 回调串行执行，若在此 await，
            // 输出转发循环被插件回调阻塞 → broadcast 通道积压溢出 → 静默丢帧 →
            // 移动端游标连续性破坏（violation 风暴）
            {
                let pm = crate::state::get_plugin_manager();
                // 用 error boundary 包装：插件 WASM 回调 panic 时记录日志而非静默吞掉
                spawn_with_error_boundary("plugin_terminal_output_notify", async move {
                    pm.dispatch_lifecycle_event(
                        crate::plugin::types::PluginLifecycleEvent::TerminalOutput {
                            session_id,
                            data: String::new(),
                        }
                    ).await;
                });
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

        MobileEvent::SyncTaskStatusChanged { session_id, task_status, task_reason, task_questions } => {
            tracing::info!(
                "[EventForwarder] SyncTaskStatusChanged: session_id={}, status={}",
                session_id, task_status
            );
            let _ = app.emit("ws_sync_task_status_changed", serde_json::json!({
                "session_id": session_id,
                "task_status": task_status,
                "task_reason": task_reason,
                "task_questions": task_questions,
            }));
        }

        MobileEvent::SyncSessionModeChanged { session_id, auto_approve } => {
            tracing::info!(
                "[EventForwarder] SyncSessionModeChanged: session_id={}, auto_approve={}",
                session_id, auto_approve
            );
            let _ = app.emit("ws_sync_session_mode_changed", serde_json::json!({
                "session_id": session_id,
                "auto_approve": auto_approve,
            }));
        }

        MobileEvent::SyncTaskQueueChanged { session_id, queue_count, action, task_id, status } => {
            tracing::info!(
                "[EventForwarder] SyncTaskQueueChanged: session_id={}, count={}, action={}, task_id={:?}, status={:?}",
                session_id, queue_count, action, task_id, status
            );
            let _ = app.emit("ws_sync_task_queue_changed", serde_json::json!({
                "session_id": session_id,
                "queue_count": queue_count,
                "action": action,
                "task_id": task_id,
                "status": status,
            }));
        }

        MobileEvent::SyncTaskScheduledChanged { job_id, status, action } => {
            tracing::info!(
                "[EventForwarder] SyncTaskScheduledChanged: job_id={}, status={}, action={}",
                job_id, status, action
            );
            let _ = app.emit("ws_sync_task_scheduled_changed", serde_json::json!({
                "job_id": job_id,
                "status": status,
                "action": action,
            }));
        }

        // 文件服务同步事件：前端事件 + 插件消息总线双通道
        // （插件阶段 4 经 bus topic `sync:file_service` 订阅对端挂载可用性）
        MobileEvent::SyncFileServiceChanged { plugin_id, mount_path, available, operations } => {
            tracing::info!(
                "[EventForwarder] SyncFileServiceChanged: plugin_id={}, mount={}, available={}",
                plugin_id, mount_path, available
            );
            let payload = serde_json::json!({
                "plugin_id": plugin_id,
                "mount_path": mount_path,
                "available": available,
                "operations": operations,
            });
            let _ = app.emit("ws_sync_file_service_changed", payload.clone());
            crate::state::get_plugin_manager()
                .message_bus()
                .publish("sync:file_service", "host", payload);
        }

        // 其他事件不转发
        _ => {}
    }
}

// ==================== Event Helpers ====================

/// 发射连接开始事件
pub fn emit_connecting(app: &AppHandle, address: &str, port: u16) {
    tracing::debug!("[EventHelper] Emitting ws_connecting");
    let _ = app.emit("ws_connecting", serde_json::json!({
        "address": address,
        "port": port,
    }));
}

/// 发射断开连接事件
pub fn emit_disconnected(app: &AppHandle, reason: &str) {
    tracing::info!("[EventHelper] Emitting ws_disconnected: {}", reason);
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
