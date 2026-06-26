//! Plugin DTOs
//!
//! 插件 HTTP API 请求/响应类型

use serde::Deserialize;

use crate::shared::enums::PluginQuestion;

/// POST /plugin/task-status request
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
    /// Claude 提问的问题列表（AskUserQuestion 工具调用时携带）
    #[serde(default)]
    pub questions: Option<Vec<PluginQuestion>>,
    /// BedCode PTY 会话 ID（由 pty_process.rs 启动时注入到进程环境变量）
    #[serde(default)]
    pub bedcode_session_id: Option<String>,
}

/// POST /api/plugin/session-mode request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionModeRequest {
    /// Claude Code 会话 ID
    pub session_id: String,
    /// 是否自动授权
    pub auto_approve: bool,
    /// 认证 token
    pub token: String,
}
