//! Mobile Session Manager
//!
//! 会话管理 - 启动/停止会话、会话状态

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use tracing;

// 公开导出 SessionStatus 供外部使用
pub use crate::shared::enums::SessionStatus;

use crate::Result;

use super::connection::ConnectionManager;

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// 会话 ID
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 配置 ID
    pub config_id: String,
    /// 当前状态
    pub status: SessionStatus,
    /// 创建时间
    pub created_at: i64,
}

/// 会话管理器
pub struct SessionManager {
    /// 关联的连接管理器
    connection: Arc<ConnectionManager>,
    /// 活跃会话
    active_session: Arc<RwLock<Option<SessionInfo>>>,
    /// 全部会话列表
    sessions: Arc<RwLock<Vec<SessionInfo>>>,
    /// 输入消息发送器
    input_tx: Arc<RwLock<Option<tokio::sync::mpsc::Sender<String>>>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(connection: Arc<ConnectionManager>) -> Arc<Self> {
        Arc::new(Self {
            connection,
            active_session: Arc::new(RwLock::new(None)),
            sessions: Arc::new(RwLock::new(Vec::new())),
            input_tx: Arc::new(RwLock::new(None)),
        })
    }

    /// 启动会话
    pub async fn start_session(&self, config_id: &str, session_name: Option<&str>) -> Result<String> {
        tracing::info!("[start_session] config_id={}, session_name={:?}", config_id, session_name);

        // 通过 WebSocket 发送 StartSession 控制消息到桌面端
        let message = crate::shared::websocket::WsMessage::text(serde_json::to_string(&serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "start_session",
                    "config_id": config_id
                }
            }
        })).unwrap());

        let response = self.connection
            .send_and_wait(&message, std::time::Duration::from_secs(30))
            .await
            .map_err(|e| {
                tracing::error!("[start_session] send_and_wait failed: {}", e);
                e
            })?;
        tracing::info!("[start_session] send_and_wait succeeded");

        // 解析响应获取真实 session_id
        if let crate::shared::websocket::WsMessage::Text { payload: text_payload, .. } = &response {
            tracing::info!("[start_session] response content: {}", &text_payload.content[..text_payload.content.len().min(200)]);
            if let Ok(inner) = serde_json::from_str::<serde_json::Value>(&text_payload.content) {
                if let Some(session_id) = inner.get("session_id").and_then(|s| s.as_str()) {
                    let name = session_name.unwrap_or(&format!("Session-{}", &session_id[..8])).to_string();
                    let session = SessionInfo {
                        id: session_id.to_string(),
                        name,
                        config_id: config_id.to_string(),
                        status: SessionStatus::Running,
                        created_at: chrono::Utc::now().timestamp_millis(),
                    };

                    *self.active_session.write().await = Some(session.clone());
                    self.sessions.write().await.push(session.clone());
                    tracing::info!("[start_session] Session added to local list, total sessions: {}", self.sessions.read().await.len());
                    return Ok(session_id.to_string());
                } else {
                    tracing::error!("[start_session] No session_id in response: {:?}", inner);
                }
            } else {
                tracing::error!("[start_session] Failed to parse response JSON");
            }
        } else {
            tracing::error!("[start_session] Unexpected response type: {:?}", response);
        }

        tracing::error!("[start_session] Failed to parse StartSession response");
        Err(crate::AppError::WebSocket("Failed to start session: invalid response".to_string()))
    }

    /// 停止会话
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        // 标记会话为已停止
        if let Some(ref mut session) = *self.active_session.write().await {
            if session.id == session_id {
                session.status = SessionStatus::Stopped;
            }
        }

        tracing::info!("Session stopped: {}", session_id);
        Ok(())
    }

    /// 获取活跃会话
    pub async fn get_active_session(&self) -> Option<SessionInfo> {
        self.active_session.read().await.clone()
    }

    /// 获取所有会话
    pub async fn get_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.read().await.clone()
    }

    /// 发送输入到活跃会话
    pub async fn send_input(&self, data: String) -> Result<()> {
        if let Some(tx) = self.input_tx.read().await.as_ref() {
            tx.send(data).await.map_err(|e| crate::AppError::Internal(e.to_string()))?;
            Ok(())
        } else {
            Err(crate::AppError::NotFound("No active session".to_string()))
        }
    }

    /// 设置输入发送器
    pub fn set_input_sender(&self, sender: tokio::sync::mpsc::Sender<String>) {
        let mut guard = self.input_tx.blocking_write();
        *guard = Some(sender);
    }
}