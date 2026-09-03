//! Session DTOs

use serde::{Deserialize, Serialize};

/// GET /api/sessions response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResponseData {
    pub sessions: Vec<SessionItem>,
}

/// Single session item in list response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub session_type: Option<String>,
    pub config_id: Option<String>,
    /// 任务执行状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    /// 任务状态原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reason: Option<String>,
}

/// POST /api/sessions/start request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionRequest {
    pub config_id: String,
    /// 启动端终端组件默认网格列数（与 rows 同时提供且 >0 才生效）
    #[serde(default)]
    pub cols: Option<u16>,
    /// 启动端终端组件默认网格行数
    #[serde(default)]
    pub rows: Option<u16>,
}

/// POST /api/sessions/start response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResponseData {
    pub session_id: String,
    pub status: String,
}

/// POST /api/sessions/{id}/resize request
#[derive(Debug, Clone, Deserialize)]
pub struct ResizeSessionRequest {
    pub cols: u16,
    pub rows: u16,
    /// 覆盖确认标志：服务端裁决返回 needsConfirmation 后，客户端弹窗确认以 force=true 重发
    #[serde(default)]
    pub force: bool,
}

/// POST /api/sessions/{id}/input request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInputRequest {
    /// 输入文本数据
    pub data: String,
    /// 特殊按键（如 "enter", "ctrl_c", "arrow_up" 等）
    #[serde(default)]
    pub special_key: Option<String>,
}
