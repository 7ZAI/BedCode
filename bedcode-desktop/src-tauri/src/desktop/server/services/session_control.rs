//! Session Control Service
//!
//! 处理会话启动/停止/缩放等控制逻辑

use crate::desktop::session::SessionManager;
use crate::desktop::server::message::{SessionControlAction, Message, SessionSummary};
use crate::shared::enums::{TerminalAction, TerminalPayload};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tauri::{AppHandle, Emitter};

/// 刷新事件类型
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshEvent {
    pub refresh_type: String,
    pub source: String,
}

/// 处理控制消息
pub async fn handle_control(
    action: SessionControlAction,
    request_message_id: String,
    session_manager: &Arc<SessionManager>,
    clients: &Arc<RwLock<HashMap<SocketAddr, crate::desktop::server::ClientInfo>>>,
    addr: SocketAddr,
    device_name: Option<String>,
) -> Result<Option<Message>> {
    match action {
        SessionControlAction::ListSessions => {
            let sessions = session_manager.list_sessions().await;

            let all_sessions: Vec<SessionSummary> = sessions
                .into_iter()
                .map(|s| SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: serde_json::to_value(&s.status)
                        .and_then(|v| serde_json::from_value::<String>(v))
                        .unwrap_or_else(|_| format!("{:?}", s.status)),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("pty".to_string()),
                    config_id: Some(s.config_id),
                    task_status: s.task_status.map(|ts| {
                        serde_json::to_string(&ts)
                            .unwrap_or_default()
                            .trim_matches('"')
                            .to_string()
                    }),
                    task_reason: s.task_reason,
                })
                .collect();

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions: all_sessions },
                },
            }))
        }

        SessionControlAction::StartSession { config_id } => {
            // 传递设备名称作为 source_device，用于同步事件广播时排除操作者
            let session_id = session_manager.create_session_with_source(&config_id, device_name.clone()).await?;
            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::StartSession { config_id },
                },
            }))
        }

        SessionControlAction::StopSession { session_id } => {
            // 传递设备名称作为 source_device
            session_manager.kill_session_with_source(&session_id, device_name.clone()).await?;

            // 从客户端订阅列表中移除该会话
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                }
            }

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::StopSession { session_id },
                },
            }))
        }

        SessionControlAction::RemoveSession { session_id } => {
            // 传递设备名称作为 source_device
            session_manager.remove_session_with_source(&session_id, device_name.clone()).await?;

            // 从客户端订阅列表中移除该会话
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                }
            }

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::RemoveSession { session_id },
                },
            }))
        }

        SessionControlAction::ResizeSession { session_id, cols, rows } => {
            // 更新 PTY 尺寸，使输出按移动端实际屏幕宽度排版
            //
            // 桌面端 PTY 的尺寸由最后一个调整尺寸的客户端决定。
            // 如果桌面端和移动端同时使用，后调整的一方会覆盖前者的设置。
            // 这是有意为之：PTY 只能有一个尺寸，输出格式必须匹配实际渲染端。
            if let Err(e) = session_manager.resize_session(&session_id, cols, rows).await {
                tracing::warn!("Failed to resize PTY session: {}", e);
            }

            // 同时更新客户端的终端尺寸记录（用于后续可能的 per-client 渲染）
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.cols = cols;
                    client.rows = rows;
                    tracing::debug!("Client {} updated terminal size to {}x{}", addr, cols, rows);
                }
            }
            Ok(None)
        }

        SessionControlAction::JoinSession { session_id } => {
            // 检查会话是否存在
            let sessions = session_manager.list_sessions().await;
            if !sessions.iter().any(|s| s.id == session_id) {
                return Ok(Some(Message::error_with_id(&request_message_id, "SESSION_NOT_FOUND", &format!("Session not found: {}", session_id))));
            }

            // 更新客户端订阅列表
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    if !client.subscribed_sessions.contains(&session_id) {
                        client.subscribed_sessions.push(session_id.clone());
                        tracing::info!("Client {} joined session {}", addr, session_id);
                    }
                }
            }

            // 发送缓存的历史输出给刚加入的客户端
            // TODO: 实现从 PTY 会话获取历史输出
            let cached_output: Vec<crate::desktop::model::PtyOutputEvent> = vec![];
            let cached_count = cached_output.len();
            if cached_count > 0 {
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                for event in &cached_output {
                    // 检测等待输入状态
                    let decoded_data = base64::Engine::decode(
                        &base64::engine::general_purpose::STANDARD,
                        &event.data,
                    ).unwrap_or_default();
                    let is_waiting = crate::desktop::parser::detect_waiting_input(
                        &String::from_utf8_lossy(&decoded_data)
                    );

                    let message = Message::Terminal {
                        message_id: uuid::Uuid::new_v4().to_string(),
                        expect_response: false,
                        timestamp: event.timestamp.timestamp_millis(),
                        session_id: event.session_id.clone(),
                        token: String::new(),
                        payload: TerminalPayload {
                            action: TerminalAction::Output {
                                data: event.data.clone(),
                                is_waiting,
                                index: event.index,
                            },
                        },
                    };

                    if let Err(e) = ws_manager.send_to_addr(&addr, &message).await {
                        tracing::warn!("Failed to send cached output to client {}: {}", addr, e);
                    }
                }
                tracing::info!("Sent {} cached output messages to client {} for session {}", cached_count, addr, session_id);
            }

            // 返回成功响应
            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::JoinSession { session_id },
                },
            }))
        }

        SessionControlAction::LeaveSession { session_id } => {
            // 从客户端订阅列表中移除
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                    tracing::info!("Client {} left session {}", addr, session_id);
                }
            }

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::LeaveSession { session_id },
                },
            }))
        }

        _ => Ok(None),
    }
}

/// 处理完整的 Control 消息（路由层）
pub async fn handle_control_message(
    message_id: String,
    session_id: Option<String>,
    _timestamp: i64,
    action: SessionControlAction,
    session_manager: &Option<Arc<SessionManager>>,
    addr: SocketAddr,
    device_name: Option<String>,
    app_handle: Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    match action {
        SessionControlAction::ListSessions
        | SessionControlAction::StartSession { .. }
        | SessionControlAction::StopSession { .. }
        | SessionControlAction::ResizeSession { .. }
        | SessionControlAction::JoinSession { .. }
        | SessionControlAction::LeaveSession { .. }
        | SessionControlAction::RemoveSession { .. } => {
            if let Some(sm) = session_manager {
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                let clients = HashMap::<SocketAddr, crate::desktop::server::ClientInfo>::new();

                let result = handle_control(
                    action.clone(),
                    message_id,
                    sm,
                    &Arc::new(RwLock::new(clients)),
                    addr,
                    device_name.clone(),
                ).await?;

                // 移动端操作成功后，发送刷新事件通知桌面端前端
                // 仅在 StopSession 和 RemoveSession 时发送（会话刷新）
                // StartSession 也会触发刷新（会话列表新增）
                if let Some(handle) = app_handle {
                    let source = device_name.unwrap_or_else(|| "mobile".to_string());
                    match &action {
                        SessionControlAction::StopSession { .. }
                        | SessionControlAction::RemoveSession { .. } => {
                            // 发送会话列表刷新事件
                            if let Err(e) = handle.emit("sessions-refresh", RefreshEvent {
                                refresh_type: "sessions".to_string(),
                                source: source.clone(),
                            }) {
                                tracing::error!("Failed to emit sessions-refresh event: {}", e);
                            }
                            tracing::info!("[SessionControl] Emitted sessions-refresh event from {}", source);
                        }
                        SessionControlAction::StartSession { .. } => {
                            // 发送会话列表刷新事件（新增会话）
                            if let Err(e) = handle.emit("sessions-refresh", RefreshEvent {
                                refresh_type: "sessions".to_string(),
                                source: source.clone(),
                            }) {
                                tracing::error!("Failed to emit sessions-refresh event: {}", e);
                            }
                            tracing::info!("[SessionControl] Emitted sessions-refresh event from {}", source);
                        }
                        _ => {}
                    }
                }

                Ok(result)
            } else {
                tracing::warn!("Session manager not available");
                Ok(None)
            }
        }
        _ => {
            tracing::debug!("Unhandled control action: {:?}", action);
            Ok(None)
        }
    }
}