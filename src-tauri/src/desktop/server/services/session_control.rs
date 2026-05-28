//! Session Control Service
//!
//! 处理会话启动/停止/缩放等控制逻辑

use crate::desktop::plugin::PluginManager;
use crate::desktop::session::SessionManager;
use crate::desktop::server::message::{SessionControlAction, Message, SessionSummary};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 处理控制消息
pub async fn handle_control(
    action: SessionControlAction,
    request_message_id: String,
    session_manager: &Arc<SessionManager>,
    plugin_manager: &Arc<PluginManager>,
    clients: &Arc<RwLock<HashMap<SocketAddr, crate::desktop::server::ClientInfo>>>,
    addr: SocketAddr,
    _device_name: Option<String>,
) -> Result<Option<Message>> {
    match action {
        SessionControlAction::ListSessions => {
            // 合并 PTY 会话和 Plugin 会话
            let pty_sessions = session_manager.list_sessions().await;
            let plugin_sessions = plugin_manager.list_sessions().await;

            // 收集所有会话（PTY 优先）
            let mut all_sessions = Vec::new();

            // 添加 PTY 会话
            for s in pty_sessions {
                all_sessions.push(SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: serde_json::to_value(&s.status).and_then(|v| serde_json::from_value::<String>(v)).unwrap_or_else(|_| format!("{:?}", s.status)),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("pty".to_string()),
                    config_id: Some(s.config_id),
                });
            }

            // 添加 Plugin 会话
            for s in plugin_sessions {
                all_sessions.push(SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: serde_json::to_value(&s.status).and_then(|v| serde_json::from_value::<String>(v)).unwrap_or_else(|_| format!("{:?}", s.status)),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("plugin".to_string()),
                    config_id: Some(s.config_id),
                });
            }

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions: all_sessions },
                },
            }))
        }

        SessionControlAction::StartSession { config_id } => {
            let session_id = session_manager.create_session(&config_id).await?;
            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::StartSession { config_id },
                },
            }))
        }

        SessionControlAction::StopSession { session_id } => {
            session_manager.kill_session(&session_id).await?;

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
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::StopSession { session_id },
                },
            }))
        }

        SessionControlAction::RemoveSession { session_id } => {
            session_manager.remove_session(&session_id).await?;

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
                    let is_waiting = crate::shared::parser::detect_waiting_input(
                        &String::from_utf8_lossy(&decoded_data)
                    );

                    let message = Message::Output {
                        message_id: uuid::Uuid::new_v4().to_string(),
                        expect_response: false,
                        session_id: event.session_id.clone(),
                        timestamp: event.timestamp.timestamp_millis(),
                        payload: crate::shared::model::message::OutputPayload {
                            data: event.data.clone(),
                            is_waiting,
                            index: event.index,
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
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::LeaveSession { session_id },
                },
            }))
        }

        // === Plugin 会话相关 ===
        SessionControlAction::RegisterPluginSession {
            project_name,
            project_path,
            jsonl_path,
        } => {
            use uuid::Uuid;

            let session_id = Uuid::new_v4().to_string();

            plugin_manager
                .register_session(
                    session_id.clone(),
                    project_name,
                    project_path,
                    jsonl_path,
                )
                .await?;

            Ok(Some(Message::SessionControl {
                message_id: request_message_id,
                expect_response: false,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::server::message::SessionControlPayload {
                    action: SessionControlAction::RegisteredPluginSession { session_id },
                },
            }))
        }

        SessionControlAction::UnregisterPluginSession { session_id } => {
            plugin_manager.unregister_session(&session_id).await?;
            Ok(None)
        }

        SessionControlAction::PluginHeartbeat { session_id } => {
            plugin_manager.handle_heartbeat(&session_id).await?;
            Ok(None)
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
    plugin_manager: &Option<Arc<PluginManager>>,
    addr: SocketAddr,
) -> Result<Option<Message>> {
    match action {
        SessionControlAction::ListSessions
        | SessionControlAction::StartSession { .. }
        | SessionControlAction::StopSession { .. }
        | SessionControlAction::ResizeSession { .. }
        | SessionControlAction::JoinSession { .. }
        | SessionControlAction::LeaveSession { .. }
        | SessionControlAction::RemoveSession { .. } => {
            if let (Some(sm), Some(pm)) = (session_manager, plugin_manager) {
                let ws_manager = crate::desktop::websocket_manager::WebSocketManager::global();
                let clients = HashMap::<SocketAddr, crate::desktop::server::ClientInfo>::new();

                handle_control(
                    action,
                    message_id,
                    sm,
                    pm,
                    &Arc::new(RwLock::new(clients)),
                    addr,
                    None,
                ).await
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