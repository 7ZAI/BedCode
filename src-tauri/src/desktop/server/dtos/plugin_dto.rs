//! Plugin DTOs
//!
//! 插件 HTTP API 请求/响应类型

use serde::{Deserialize, Serialize};

/// POST /api/plugin/task-status request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskStatusRequest {
    /// Claude Code 会话 ID
    pub session_id: String,
    /// 任务状态：idle, in_progress, asking, completed, interrupted
    pub status: String,
    /// 状态原因
    #[serde(default)]
    pub reason: Option<String>,
    /// 认证 token
    pub token: String,
}
