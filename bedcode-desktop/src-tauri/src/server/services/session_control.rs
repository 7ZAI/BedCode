//! Session Control Service
//!
//! 处理会话启动/停止/缩放等控制逻辑
//! JoinSession/LeaveSession 通过 GlobalOutputManager 管理输出订阅

use crate::session::{GlobalOutputManager, SessionManager};
use crate::server::message::{SessionControlAction, Message, SessionSummary};
use crate::Result;
use std::net::SocketAddr;
use std::sync::Arc;
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
                payload: crate::server::message::SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions: all_sessions },
                },
            }))
        }

        SessionControlAction::StartSession { config_id } => {
            let session_id = session_manager.create_session_with_source(&config_id, device_name.clone()).await?;
            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::server::message::SessionControlPayload {
                    action: SessionControlAction::StartSession { config_id },
                },
            }))
        }

        SessionControlAction::StopSession { session_id } => {
            session_manager.kill_session_with_source(&session_id, device_name.clone()).await?;

            // 取消该客户端对此会话的输出订阅
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe(&session_id, &addr.to_string()).await;

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::server::message::SessionControlPayload {
                    action: SessionControlAction::StopSession { session_id },
                },
            }))
        }

        SessionControlAction::RemoveSession { session_id } => {
            session_manager.remove_session_with_source(&session_id, device_name.clone()).await?;

            // 取消该客户端对此会话的输出订阅
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe(&session_id, &addr.to_string()).await;

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::server::message::SessionControlPayload {
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
                tracing::warn!(error = %e, session_id = %session_id, "Failed to resize PTY session");
            }

            Ok(None)
        }

        SessionControlAction::JoinSession { session_id } => {
            // 检查会话是否存在
            let sessions = session_manager.list_sessions().await;
            if !sessions.iter().any(|s| s.id == session_id) {
                return Ok(Some(Message::error_with_id(&request_message_id, "SESSION_NOT_FOUND", &format!("Session not found: {}", session_id))));
            }

            // 通过 GlobalOutputManager 订阅会话输出
            // 使用 mpsc 通道 + WS actor 转发，与 TerminalAction::Subscribe 路径一致
            let client_id = addr.to_string();
            let global_manager = GlobalOutputManager::global();

            if !global_manager.has_session(&session_id).await {
                tracing::warn!(session_id = %session_id, addr = %addr, "JoinSession: session not registered in GlobalOutputManager");
                return Ok(Some(Message::error_with_id(&request_message_id, "SESSION_NOT_FOUND", &format!("Session {} output not available", session_id))));
            }

            let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<crate::session::OutputEvent>(256);
            let subscribe_result = global_manager.subscribe(&session_id, &client_id, output_tx, None).await;

            match subscribe_result {
                Some(response) => {
                    tracing::info!(
                        session_id = %session_id,
                        addr = %addr,
                        history_count = response.history_count,
                        "Client joined session via SessionControl"
                    );

                    // 启动输出转发任务：将 OutputEvent 编码为 WS 消息发送给客户端
                    let ws_manager = crate::server::ws::WebSocketManager::global();
                    let session_id_for_fwd = session_id.clone();
                    let config = crate::system::config::AppConfig::global();
                    let flush_interval = std::time::Duration::from_millis(config.terminal.flush_interval_ms);
                    let max_buffer_size = config.terminal.max_buffer_size;

                    tokio::spawn(async move {
                        let mut buffer = OutputBuffer::new();

                        loop {
                            match tokio::time::timeout(flush_interval, output_rx.recv()).await {
                                Ok(Some(event)) => {
                                    buffer.append(&event);
                                    if buffer.data.len() >= max_buffer_size {
                                        let text = buffer.flush(&session_id_for_fwd);
                                        let message = match crate::server::ws::message::Message::from_json(&text) {
                                            Ok(m) => m,
                                            Err(_) => break,
                                        };
                                        let _ = ws_manager.send_to_client(&client_id, &message).await;
                                    }
                                }
                                Ok(None) => {
                                    // channel 关闭（会话结束或服务器停机）
                                    if !buffer.is_empty() {
                                        let text = buffer.flush(&session_id_for_fwd);
                                        let message = match crate::server::ws::message::Message::from_json(&text) {
                                            Ok(m) => m,
                                            Err(_) => break,
                                        };
                                        let _ = ws_manager.send_to_client(&client_id, &message).await;
                                    }
                                    break;
                                }
                                Err(_) => {
                                    // 超时，flush 缓冲区
                                    if !buffer.is_empty() {
                                        let text = buffer.flush(&session_id_for_fwd);
                                        let message = match crate::server::ws::message::Message::from_json(&text) {
                                            Ok(m) => m,
                                            Err(_) => break,
                                        };
                                        let _ = ws_manager.send_to_client(&client_id, &message).await;
                                    }
                                }
                            }
                        }
                    });

                    Ok(Some(Message::SessionControl {
                        message_id: request_message_id,
                        expect_response: false,
                        session_id: Some(session_id.clone()),
                        timestamp: chrono::Utc::now().timestamp_millis(),
                        token: String::new(),
                        payload: crate::server::message::SessionControlPayload {
                            action: SessionControlAction::JoinSession { session_id },
                        },
                    }))
                }
                None => {
                    tracing::warn!(session_id = %session_id, addr = %addr, "JoinSession: GlobalOutputManager.subscribe returned None");
                    Ok(Some(Message::error_with_id(&request_message_id, "SESSION_NOT_FOUND", &format!("Session {} not found", session_id))))
                }
            }
        }

        SessionControlAction::LeaveSession { session_id } => {
            // 通过 GlobalOutputManager 取消输出订阅
            let global_manager = GlobalOutputManager::global();
            global_manager.unsubscribe(&session_id, &addr.to_string()).await;

            tracing::info!(session_id = %session_id, addr = %addr, "Client left session via SessionControl");

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                token: String::new(),
                payload: crate::server::message::SessionControlPayload {
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
    _session_id: Option<String>,
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
                let result = handle_control(
                    action.clone(),
                    message_id,
                    sm,
                    addr,
                    device_name.clone(),
                ).await?;

                // 移动端操作成功后，发送刷新事件通知桌面端前端
                if let Some(handle) = app_handle {
                    let source = device_name.unwrap_or_else(|| "mobile".to_string());
                    match &action {
                        SessionControlAction::StopSession { .. }
                        | SessionControlAction::RemoveSession { .. } => {
                            if let Err(e) = handle.emit("sessions-refresh", RefreshEvent {
                                refresh_type: "sessions".to_string(),
                                source: source.clone(),
                            }) {
                                tracing::error!(error = %e, "Failed to emit sessions-refresh event");
                            }
                            tracing::info!(source = %source, "[SessionControl] Emitted sessions-refresh event");
                        }
                        SessionControlAction::StartSession { .. } => {
                            if let Err(e) = handle.emit("sessions-refresh", RefreshEvent {
                                refresh_type: "sessions".to_string(),
                                source: source.clone(),
                            }) {
                                tracing::error!(error = %e, "Failed to emit sessions-refresh event");
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

// ==================== Output Buffer ====================

/// 输出缓冲区 — 累积多条 PTY 输出，减少 WS 消息数量
struct OutputBuffer {
    data: Vec<u8>,
    start_index: u64,
    end_index: u64,
    last_is_waiting: bool,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            end_index: 0,
            last_is_waiting: false,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
        }
        self.end_index = event.index;
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为 WS 消息 JSON
    fn flush(&mut self, session_id: &str) -> String {
        let data_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &self.data,
        );
        let end_index = if self.end_index > self.start_index {
            Some(self.end_index as usize)
        } else {
            None
        };
        let message = crate::server::ws::message::Message::output_from_base64(
            session_id,
            &data_base64,
            self.last_is_waiting,
            self.start_index as usize,
            end_index,
        );
        self.data.clear();
        message.to_json().unwrap_or_default()
    }
}
