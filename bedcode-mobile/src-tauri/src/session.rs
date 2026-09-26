//! Session Manager
//!
//! 会话管理 - 启动/停止会话、会话状态
//!
//! 票 04：控制面从 WS `Message` 信封迁到桌面 HTTP 面——起停删经
//! [`session::http::SessionHttpClient`] 直连桌面 `/api/sessions*`，不再依赖
//! WS 请求-响应链路（旧信封协议随消费者清零退役）。本地会话列表仍保留：它是
//! 命令层（`ws_start_session` 等）的会话名与活跃态簿记，桌面真源以事件 + HTTP
//! 对账为准（见 `handler/plugin_event.rs`）。

pub mod http;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// 公开导出 SessionStatus 供外部使用
pub use crate::enums::SessionStatus;

use crate::auth::http::resolve_base_url;
use crate::session::http::SessionHttpClient;
use crate::system::constants::terminal::SESSION_NAME_ID_PREFIX_LEN;
use crate::Result;

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
    /// 关联的连接管理器（目标设备 base URL 解析用）
    connection: Arc<crate::connection::manager::ConnectionManager>,
    /// 会话控制 HTTP 客户端
    http: Arc<SessionHttpClient>,
    /// 活跃会话
    active_session: Arc<RwLock<Option<SessionInfo>>>,
    /// 全部会话列表
    sessions: Arc<RwLock<Vec<SessionInfo>>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new(connection: Arc<crate::connection::manager::ConnectionManager>) -> Arc<Self> {
        Arc::new(Self {
            connection,
            http: SessionHttpClient::new(),
            active_session: Arc::new(RwLock::new(None)),
            sessions: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// 启动会话（`POST /api/sessions/start`）
    pub async fn start_session(&self, config_id: &str, session_name: Option<&str>) -> Result<String> {
        tracing::info!(
            "[start_session] config_id={}, session_name={:?}",
            config_id,
            session_name
        );

        // 桌面 HTTP：目标设备缺失 / 业务拒绝（code!=0）显性报错（fail-visible）
        let base_url = resolve_base_url(&self.connection).await?;
        let session_id = self.http.start_session(&base_url, config_id, None, None).await?;
        tracing::info!("[start_session] HTTP start succeeded: {}", session_id);

        let name = session_name.map(|n| n.to_string()).unwrap_or_else(|| {
            let short_id = if session_id.len() > SESSION_NAME_ID_PREFIX_LEN {
                &session_id[..SESSION_NAME_ID_PREFIX_LEN]
            } else {
                &session_id
            };
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
        tracing::info!(
            "[start_session] Session added to local list, total sessions: {}",
            self.sessions.read().await.len()
        );

        // 通知插件会话创建
        {
            let pm = crate::state::get_plugin_manager();
            pm.dispatch_lifecycle_event(crate::plugin::types::PluginLifecycleEvent::SessionCreated {
                session_id: session_id.clone(),
            })
            .await;
        }

        Ok(session_id)
    }

    /// 停止会话（`POST /api/sessions/{id}/stop`）
    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("[stop_session] Sending stop request for session_id={}", session_id);

        let base_url = resolve_base_url(&self.connection).await?;
        match self.http.stop_session(&base_url, session_id).await {
            Ok(_) => {
                tracing::info!("[stop_session] Desktop confirmed session stopped: {}", session_id);
            }
            Err(e) => {
                // 桌面端可能已停（幂等）：留痕但继续本地收敛——与旧 WS 路径语义一致
                tracing::warn!(
                    "[stop_session] Desktop stop request failed (session may already be stopped): {}",
                    e
                );
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
            pm.dispatch_lifecycle_event(crate::plugin::types::PluginLifecycleEvent::SessionStopped {
                session_id: session_id.to_string(),
            })
            .await;
        }

        Ok(())
    }

    /// 删除会话（`DELETE /api/sessions/{id}/remove`）
    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        tracing::info!("[remove_session] Entry: session_id={}", session_id);

        let base_url = resolve_base_url(&self.connection).await?;
        match self.http.remove_session(&base_url, session_id).await {
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
            let active = self.active_session.read().await;
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
}
