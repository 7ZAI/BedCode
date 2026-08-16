//! Session Manager
//!
//! 会话管理 - 启动/停止会话、会话状态

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

// 公开导出 SessionStatus 供外部使用
pub use crate::enums::SessionStatus;

use crate::Result;
use crate::connection::request::{SessionRequest, ResponseParser, timeouts};

use crate::connection::manager::ConnectionManager;
use crate::system::constants::terminal::SESSION_NAME_ID_PREFIX_LEN;

/// 会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        let message = SessionRequest::start_session(config_id);

        let response = self.connection
            .send_and_wait(&message, timeouts::SESSION_CONTROL)
            .await
            .map_err(|e| {
                tracing::error!("[start_session] send_and_wait failed: {}", e);
                e
            })?;
        tracing::info!("[start_session] send_and_wait succeeded");

        // 解析响应获取真实 session_id
        if let Some(session_id) = ResponseParser::parse_start_session_response(&response) {
            let name = session_name
                .map(|n| n.to_string())
                .unwrap_or_else(|| {
                    let short_id = if session_id.len() > SESSION_NAME_ID_PREFIX_LEN { &session_id[..SESSION_NAME_ID_PREFIX_LEN] } else { &session_id };
                    format!("Session-{}", short_id)
                });
            let session = SessionInfo {
                id: session_id.clone(),
                name,
                config_id: config_id.to_string(),
                status: SessionStatus::Running,
                created_at: chrono::Utc::now().timestamp_millis(),
            };

            *self.active_session.write().await = Some(session.clone());
            self.sessions.write().await.push(session.clone());
            tracing::info!("[start_session] Session added to local list, total sessions: {}", self.sessions.read().await.len());

            // 通知插件会话创建
            {
                let pm = crate::state::get_plugin_manager();
                pm.dispatch_lifecycle_event(
                    crate::plugin::types::PluginLifecycleEvent::SessionCreated {
                        session_id: session_id.clone(),
                    }
                ).await;
            }

            return Ok(session_id);
        }

        tracing::error!("[start_session] Failed to parse StartSession response");
        Err(crate::AppError::WebSocket("Failed to start session: invalid response".to_string()))
    }

    /// 停止会话
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("[stop_session] Sending StopSession request for session_id={}", session_id);

        // 通过 WebSocket 发送 StopSession 控制消息到桌面端
        let message = SessionRequest::stop_session(session_id);

        match self.connection.send_and_wait(&message, timeouts::SESSION_CONTROL).await {
            Ok(_) => {
                tracing::info!("[stop_session] Desktop confirmed session stopped: {}", session_id);
            }
            Err(e) => {
                tracing::warn!("[stop_session] Desktop stop request failed (session may already be stopped): {}", e);
            }
        }

        // 标记会话为已停止（本地状态）
        if let Some(ref mut session) = *self.active_session.write().await {
            if session.id == session_id {
                session.status = SessionStatus::Stopped;
            }
        }

        // 从本地会话列表中移除
        {
            let mut sessions = self.sessions.write().await;
            sessions.retain(|s| s.id != session_id);
        }

        tracing::info!("[stop_session] Session stopped: {}", session_id);

        // 通知插件会话停止
        {
            let pm = crate::state::get_plugin_manager();
            pm.dispatch_lifecycle_event(
                crate::plugin::types::PluginLifecycleEvent::SessionStopped {
                    session_id: session_id.to_string(),
                }
            ).await;
        }

        Ok(())
    }

    /// 删除会话
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("[remove_session] Entry: session_id={}", session_id);

        // 通过 WebSocket 发送 RemoveSession 控制消息到桌面端
        let message = SessionRequest::remove_session(session_id);

        let result = self.connection.send_and_wait(&message, timeouts::SESSION_CONTROL).await;
        tracing::info!("[remove_session] send_and_wait result: {:?}", result.as_ref().map(|m| m.message_type().unwrap_or("unknown")));

        match result {
            Ok(_) => {
                tracing::info!("[remove_session] Desktop confirmed session removed: {}", session_id);
            }
            Err(e) => {
                tracing::warn!("[remove_session] Desktop remove request failed: {}", e);
            }
        }

        // 从本地会话列表中移除（不等待桌面端响应，因为桌面端可能已经删除了）
        {
            let mut sessions = self.sessions.write().await;
            sessions.retain(|s| s.id != session_id);
        }

        // 如果是活跃会话，也清除
        {
            let active = self.active_session.write().await;
            if let Some(ref session) = *active {
                if session.id == session_id {
                    drop(active);
                    *self.active_session.write().await = None;
                }
            }
        }

        tracing::info!("[remove_session] Exit: returning Ok(()) for session_id={}", session_id);
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

    /// 根据 ID 获取会话
    pub async fn get_session_by_id(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions.read().await.iter().find(|s| s.id == session_id).cloned()
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
}
