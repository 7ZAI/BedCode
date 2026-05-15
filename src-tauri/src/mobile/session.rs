//! Mobile Session Manager
//!
//! 会话管理 - 启动/停止会话、会话状态

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

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
    pub async fn start_session(&self, config_id: &str) -> Result<String> {
        // 创建会话信息
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = SessionInfo {
            id: session_id.clone(),
            name: format!("Session-{}", &session_id[..8]),
            config_id: config_id.to_string(),
            status: SessionStatus::Running,
            created_at: chrono::Utc::now().timestamp_millis(),
        };

        *self.active_session.write().await = Some(session.clone());
        self.sessions.write().await.push(session);

        tracing::info!("Session started: {}", session_id);
        Ok(session_id)
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