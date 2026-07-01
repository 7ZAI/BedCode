//! Summary Types
//!
//! 摘要类型定义

use serde::{Deserialize, Serialize};

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    /// 会话类型：pty 或 plugin
    #[serde(default)]
    pub session_type: Option<String>,
    /// 对应的会话配置 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    /// 任务执行状态（Plugin 会话使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    /// 任务状态原因
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_reason: Option<String>,
}

/// 会话配置摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigSummary {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
}

/// 快捷指令摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickActionSummary {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}