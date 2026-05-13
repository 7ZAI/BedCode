//! Control Handler
//!
//! 处理控制命令相关逻辑

use crate::desktop::plugin::PluginManager;
use crate::desktop::session::SessionManager;
use crate::shared::db::Database;
use crate::desktop::websocket::message::{ControlAction, Message, SessionSummary, SessionConfigSummary, QuickActionSummary};
use crate::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 处理控制消息
pub async fn handle_control(
    action: ControlAction,
    request_message_id: String,
    session_manager: &Arc<SessionManager>,
    plugin_manager: &Arc<PluginManager>,
    db: &Arc<Mutex<Database>>,
    clients: &Arc<RwLock<HashMap<SocketAddr, crate::desktop::connection::ClientInfo>>>,
    addr: SocketAddr,
    _device_name: Option<String>,
) -> Result<Option<Message>> {
    match action {
        ControlAction::ListSessions => {
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
                    status: format!("{:?}", s.status),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("pty".to_string()),
                });
            }

            // 添加 Plugin 会话
            for s in plugin_sessions {
                all_sessions.push(SessionSummary {
                    id: s.id,
                    name: s.name,
                    status: format!("{:?}", s.status),
                    created_at: s.created_at.to_rfc3339(),
                    started_at: s.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("plugin".to_string()),
                });
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::SessionList { sessions: all_sessions },
                },
            }))
        }

        ControlAction::ListSessionConfigs => {
            let db = db.lock().await;
            let configs = db.get_session_configs()?;
            drop(db);

            let summaries = configs
                .into_iter()
                .map(|c| SessionConfigSummary {
                    id: c.id,
                    name: c.name,
                    environment: c.environment,
                    wsl_distro: c.wsl_distro,
                    working_dir: c.working_dir,
                    command: c.command,
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::SessionConfigList { configs: summaries },
                },
            }))
        }

        ControlAction::StartSession { config_id } => {
            let session_id = session_manager.create_session(&config_id).await?;
            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::StartSession { config_id },
                },
            }))
        }

        ControlAction::StopSession { session_id } => {
            session_manager.kill_session(&session_id).await?;

            // 从客户端订阅列表中移除该会话
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                }
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::StopSession { session_id },
                },
            }))
        }

        ControlAction::RemoveSession { session_id } => {
            session_manager.remove_session(&session_id).await?;

            // 从客户端订阅列表中移除该会话
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                }
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::RemoveSession { session_id },
                },
            }))
        }

        ControlAction::ResizeSession { session_id: _, cols, rows } => {
            // 只更新客户端的终端尺寸，不再修改全局 PTY 尺寸
            // 这样桌面端和移动端可以各自保持独立的终端尺寸
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

        ControlAction::ListQuickActions => {
            let db = db.lock().await;
            let actions = db.get_quick_actions()?;
            drop(db);

            let summaries = actions
                .into_iter()
                .map(|a| QuickActionSummary {
                    id: a.id,
                    name: a.name,
                    content: a.content,
                    icon: a.icon,
                    color: a.color,
                })
                .collect();

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: None,
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::QuickActionList { actions: summaries },
                },
            }))
        }

        ControlAction::JoinSession { session_id } => {
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

            // 返回成功响应
            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::JoinSession { session_id },
                },
            }))
        }

        ControlAction::LeaveSession { session_id } => {
            // 从客户端订阅列表中移除
            {
                let mut clients = clients.write().await;
                if let Some(client) = clients.get_mut(&addr) {
                    client.subscribed_sessions.retain(|s| s != &session_id);
                    tracing::info!("Client {} left session {}", addr, session_id);
                }
            }

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::LeaveSession { session_id },
                },
            }))
        }

        // === Plugin 会话相关 ===
        ControlAction::RegisterPluginSession {
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

            Ok(Some(Message::Control {
                message_id: request_message_id,
                session_id: Some(session_id.clone()),
                timestamp: chrono::Utc::now().timestamp_millis(),
                payload: crate::desktop::websocket::message::ControlPayload {
                    action: ControlAction::RegisteredPluginSession { session_id },
                },
            }))
        }

        ControlAction::UnregisterPluginSession { session_id } => {
            plugin_manager.unregister_session(&session_id).await?;
            Ok(None)
        }

        ControlAction::PluginHeartbeat { session_id } => {
            plugin_manager.handle_heartbeat(&session_id).await?;
            Ok(None)
        }

        _ => Ok(None),
    }
}