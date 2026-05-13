//! Mobile Session Manager
//!
//! 会话管理 - 启动/停止会话、会话状态

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::shared::websocket::WsMessage;
use crate::Result;

use super::connection::ConnectionManager;

/// 会话状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// 空闲
    Idle,
    /// 启动中
    Starting,
    /// 运行中
    Running,
    /// 等待输入
    WaitingInput,
    /// 停止中
    Stopping,
    /// 已停止
    Stopped,
    /// 错误
    Error(String),
}

/// 会话信息
#[derive(Debug, Clone)]
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
    /// 当前活跃的会话
    active_session: RwLock<Option<SessionInfo>>,
    /// 所有会话列表
    sessions: RwLock<Vec<SessionInfo>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(connection: Arc<ConnectionManager>) -> Arc<Self> {
        Arc::new(Self {
            connection,
            active_session: RwLock::new(None),
            sessions: RwLock::new(Vec::new()),
        })
    }

    /// 获取当前活跃会话
    pub async fn get_active_session(&self) -> Option<SessionInfo> {
        self.active_session.read().await.clone()
    }

    /// 设置当前活跃会话
    pub async fn set_active_session(&self, session: Option<SessionInfo>) {
        *self.active_session.write().await = session;
    }

    /// 获取所有会话
    pub async fn get_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.read().await.clone()
    }

    /// 从服务器加载会话列表
    pub async fn load_sessions(&self) -> Result<Vec<SessionInfo>> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let message = WsMessage::text(serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "list_sessions"
                }
            }
        }));

        let response = self.connection.send_and_wait(&message, std::time::Duration::from_secs(30)).await?;

        if let Ok(json) = response.to_json() {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(sessions_array) = payload.get("payload").and_then(|p| p.get("action")).and_then(|a| a.get("sessions")) {
                    if let Ok(sessions) = serde_json::from_value::<Vec<SessionInfo>>(sessions_array.clone()) {
                        *self.sessions.write().await = sessions.clone();
                        return Ok(sessions);
                    }
                }
            }
        }

        Ok(Vec::new())
    }

    /// 启动会话
    pub async fn start_session(&self, config_id: &str) -> Result<String> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let message = WsMessage::text(serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "start_session",
                    "config_id": config_id
                }
            }
        }));

        let response = self.connection.send_and_wait(&message, std::time::Duration::from_secs(60)).await?;

        if let Ok(json) = response.to_json() {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(session_id) = payload.get("session_id").and_then(|v| v.as_str()) {
                    let session = SessionInfo {
                        id: session_id.to_string(),
                        name: format!("Session-{}", &session_id[..8]),
                        config_id: config_id.to_string(),
                        status: SessionStatus::Running,
                        created_at: chrono::Utc::now().timestamp_millis(),
                    };

                    *self.active_session.write().await = Some(session.clone());
                    self.sessions.write().await.push(session);

                    return Ok(session_id.to_string());
                }
            }
        }

        Err(crate::AppError::WebSocket("Failed to start session".to_string()))
    }

    /// 停止会话
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let message = WsMessage::text(serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "stop_session",
                    "session_id": session_id
                }
            }
        }));

        self.connection.send(&message).await?;

        // 移除会话
        self.sessions.write().await.retain(|s| s.id != session_id);

        // 如果是当前活跃会话，清除
        if let Some(active) = self.active_session.read().await.as_ref() {
            if active.id == session_id {
                *self.active_session.write().await = None;
            }
        }

        Ok(())
    }

    /// 发送输入到会话
    pub async fn send_input(&self, session_id: &str, data: &str, special_key: Option<String>) -> Result<()> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let message = WsMessage::text(serde_json::json!({
            "type": "input",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "session_id": session_id,
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "data": data,
                "special_key": special_key
            }
        }));

        self.connection.send(&message).await
    }

    /// 调整终端大小
    pub async fn resize(&self, session_id: &str, cols: u32, rows: u32) -> Result<()> {
        if !self.connection.is_connected().await {
            return Err(crate::AppError::WebSocket("Not connected".to_string()));
        }

        let message = WsMessage::text(serde_json::json!({
            "type": "control",
            "message_id": uuid::Uuid::new_v4().to_string(),
            "session_id": session_id,
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "payload": {
                "action": {
                    "type": "resize_session",
                    "session_id": session_id,
                    "cols": cols,
                    "rows": rows
                }
            }
        }));

        self.connection.send(&message).await
    }

    /// 清除所有会话状态（断开连接时调用）
    pub async fn clear(&self) {
        *self.active_session.write().await = None;
        self.sessions.write().await.clear();
    }
}