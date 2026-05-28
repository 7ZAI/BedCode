//! Control Types
//!
//! 会话控制和会话配置消息类型定义

use serde::{Deserialize, Serialize};

use super::sumary::{QuickActionSummary, SessionConfigSummary, SessionSummary};

// ==================== Session Control ====================

/// 会话控制载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionControlPayload {
    /// 控制动作
    pub action: SessionControlAction,
}

/// 会话控制动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionControlAction {
    /// 列出会话
    ListSessions,
    /// 会话列表响应
    SessionList { sessions: Vec<SessionSummary> },
    /// 启动会话
    StartSession { config_id: String },
    /// 停止会话
    StopSession { session_id: String },
    /// 删除会话
    RemoveSession { session_id: String },
    /// 调整终端大小
    ResizeSession { session_id: String, cols: u16, rows: u16 },
    /// 加入会话，开始接收输出
    JoinSession { session_id: String },
    /// 离开会话，停止接收输出
    LeaveSession { session_id: String },
    /// 会话变更通知 (created/stopped/removed)
    SessionChanged { change_type: String, session: SessionSummary },

    // === Plugin 会话相关 ===
    /// 注册 Plugin 会话
    RegisterPluginSession {
        project_name: String,
        project_path: String,
        jsonl_path: String,
    },
    /// 注册响应
    RegisteredPluginSession {
        session_id: String,
    },
    /// 注销 Plugin 会话
    UnregisterPluginSession {
        session_id: String,
    },
    /// Plugin 心跳
    PluginHeartbeat {
        session_id: String,
    },
}

// ==================== Session Config ====================

/// 会话配置载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfigPayload {
    /// 配置动作
    pub action: SessionConfigAction,
}

/// 会话配置动作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionConfigAction {
    /// 列出会话配置
    ListSessionConfigs,
    /// 会话配置列表响应
    SessionConfigList { configs: Vec<SessionConfigSummary> },
    /// 列出快捷指令
    ListQuickActions,
    /// 快捷指令列表响应
    QuickActionList { actions: Vec<QuickActionSummary> },
}