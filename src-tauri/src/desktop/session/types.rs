//! Session Types
//!
//! 会话相关的公共类型定义，使用共享的 SessionStatus 和 SessionType

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::enums::{SessionStatus, SessionType};

/// 会话状态变化事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    pub session_id: String,
    pub old_status: Option<SessionStatus>,
    pub new_status: SessionStatus,
    pub session_name: String,
}

/// 会话重启事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRestartEvent {
    pub old_session_id: String,
    pub new_session_id: String,
    pub session_name: String,
}

/// 运行时会话信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub config_id: String,
    pub name: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub session_type: SessionType,
}

impl SessionInfo {
    pub fn new(config_id: &str, name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: config_id.to_string(),
            name: name.to_string(),
            status: SessionStatus::Starting,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            session_type: SessionType::Pty,
        }
    }

    pub fn new_plugin(project_name: &str, _project_path: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: String::new(),
            name: project_name.to_string(),
            status: SessionStatus::Starting,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            session_type: SessionType::Plugin,
        }
    }
}