//! Session Event Types
//!
//! 会话相关事件类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::enums::{SessionStatus, SessionType, TaskStatus};
use crate::shared::enums::PluginQuestion;

/// 会话信息（从 session/types.rs 移出）
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
    /// 任务执行状态（Plugin 会话使用，PTY 会话始终为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<TaskStatus>,
    /// 任务状态原因（简短描述）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reason: Option<String>,
    /// 任务状态更新时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_updated_at: Option<DateTime<Utc>>,
    /// Claude 提问的问题列表（AskUserQuestion 时携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_questions: Option<Vec<PluginQuestion>>,
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
            task_status: None,
            task_reason: None,
            task_updated_at: None,
            task_questions: None,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_pty_no_task_status() {
        let info = SessionInfo::new("config-1", "test");
        assert!(info.task_status.is_none());
        assert!(info.task_reason.is_none());
        assert!(info.task_updated_at.is_none());
    }
}