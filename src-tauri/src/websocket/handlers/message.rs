//! Message Handler
//!
//! WebSocket 消息路由入口

use crate::auth::PairingService;
use crate::auth::QrTokenManager;
use crate::db::Database;
use crate::plugin::PluginManager;
use crate::session::{SessionManager, SessionType};
use crate::websocket::message::{ControlAction, Message};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tauri::{AppHandle, Emitter};

/// 处理 WebSocket 消息的入口函数
pub async fn handle_message(
    message: Message,
    addr: SocketAddr,
    session_manager: &Arc<SessionManager>,
    plugin_manager: &Arc<PluginManager>,
    db: &Arc<Mutex<Database>>,
    pairing_service: &Arc<PairingService>,
    qr_manager: &Arc<QrTokenManager>,
    clients: &Arc<RwLock<HashMap<SocketAddr, crate::websocket::server::ClientInfo>>>,
    client_senders: &Arc<RwLock<HashMap<SocketAddr, tokio::sync::mpsc::UnboundedSender<tokio_tungstenite::tungstenite::protocol::Message>>>>,
    app_handle: &Option<Arc<AppHandle>>,
) -> Result<Option<Message>> {
    // 更新客户端心跳时间（任何消息都算作活跃）
    {
        let mut clients = clients.write().await;
        if let Some(client) = clients.get_mut(&addr) {
            client.last_heartbeat = Instant::now();
        }
    }

    match message {
        Message::Auth { message_id, payload, .. } => {
            super::auth::handle_auth(
                payload,
                message_id,
                addr,
                db,
                pairing_service,
                qr_manager,
                clients,
                app_handle,
            ).await
        }

        Message::Input { message_id, session_id, payload, .. } => {
            // 检查认证
            {
                let clients = clients.read().await;
                let client = clients.get(&addr);
                if client.map(|c| !c.authenticated).unwrap_or(true) {
                    return Ok(Some(Message::error_with_id(&message_id, "UNAUTHORIZED", "Not authenticated")));
                }
            }

            // 检查会话类型（Plugin 会话使用文件监听，PTY 会话使用 PTY）
            let is_plugin = session_manager
                .get_session(&session_id)
                .await
                .map(|s| s.session_type == SessionType::Plugin)
                .unwrap_or(false);

            if is_plugin {
                // Plugin 会话：写入到 pending 文件
                plugin_manager.write_input(&session_id, &payload.data).await?;
            } else {
                // PTY 会话：发送到 PTY
                if let Some(key) = &payload.special_key {
                    session_manager.send_special_key(&session_id, key.as_str()).await?;
                } else {
                    session_manager.write_input(&session_id, &payload.data).await?;
                }
            }

            Ok(None)
        }

        Message::Control { message_id, payload, .. } => {
            // 检查认证并获取设备名称
            let device_name = {
                let clients = clients.read().await;
                let client = clients.get(&addr);
                if client.map(|c| !c.authenticated).unwrap_or(true) {
                    return Ok(Some(Message::error_with_id(&message_id, "UNAUTHORIZED", "Not authenticated")));
                }
                client.and_then(|c| c.device_name.clone())
            };

            let result = super::control::handle_control(
                payload.action,
                message_id,
                session_manager,
                plugin_manager,
                db,
                clients,
                addr,
                device_name.clone(),
            ).await;

            // 如果操作成功，根据操作类型广播消息
            if let Ok(Some(Message::Control { payload: resp_payload, .. })) = &result {
                let broadcast_msg = match &resp_payload.action {
                    ControlAction::StartSession { .. } => {
                        // 获取新创建的会话信息
                        if let Ok(Some(Message::Control { session_id: Some(session_id), .. })) = &result {
                            let session = session_manager.get_session(&session_id).await;
                            session.map(|s| {
                                let summary = crate::websocket::message::SessionSummary {
                                    id: s.id.clone(),
                                    name: s.name.clone(),
                                    status: format!("{:?}", s.status),
                                    created_at: s.created_at.to_rfc3339(),
                                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                                    session_type: Some("pty".to_string()),
                                };
                                crate::websocket::message::Message::session_event("created", summary, device_name.as_deref().unwrap_or("Mobile"))
                            })
                        } else { None }
                    },
                    ControlAction::StopSession { session_id, .. } => {
                        let summary = crate::websocket::message::SessionSummary {
                            id: session_id.clone(),
                            name: session_id.clone(),
                            status: "Stopped".to_string(),
                            created_at: chrono::Utc::now().to_rfc3339(),
                            started_at: None,
                            session_type: Some("pty".to_string()),
                        };
                        Some(crate::websocket::message::Message::session_event("stopped", summary, device_name.as_deref().unwrap_or("Mobile")))
                    },
                    _ => None,
                };

                // 广播消息给其他客户端
                if let Some(msg) = broadcast_msg {
                    // Emit Tauri event for desktop frontend
                    if let Some(ref handle) = app_handle {
                        let event_name = match &msg {
                            crate::websocket::message::Message::ClientDisconnected { .. } => "device-disconnected",
                            crate::websocket::message::Message::SessionEvent { event_type, .. } => {
                                match event_type.as_str() {
                                    "created" => "session-created-from-mobile",
                                    "stopped" => "session-stopped-from-mobile",
                                    _ => "session-event-from-mobile",
                                }
                            },
                            _ => "device-event",
                        };
                        let _ = handle.emit(event_name, &msg);
                    }

                    if let Ok(json) = msg.to_json() {
                        let ws_msg = tokio_tungstenite::tungstenite::protocol::Message::Text(json);
                        let clients = clients.read().await;
                        let senders = client_senders.read().await;
                        for (a, client) in clients.iter() {
                            if a != &addr && client.authenticated {
                                if let Some(tx) = senders.get(a) {
                                    let _ = tx.send(ws_msg.clone());
                                }
                            }
                        }
                    }
                }
            }

            result
        }

        Message::Heartbeat { .. } => {
            // 心跳时间已在 handle_message 开头更新
            Ok(Some(Message::heartbeat()))
        }

        _ => Ok(Some(Message::error("UNKNOWN_MESSAGE", "Unknown message type"))),
    }
}