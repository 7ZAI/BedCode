//! Control Types
//!
//! 会话控制、会话配置和终端消息类型定义

use serde::{Deserialize, Serialize};

use super::special_key::KeyCombo;
use super::summary::{QuickActionSummary, SessionConfigSummary, SessionSummary};

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

// ==================== Terminal ====================

/// 终端载荷
///
/// 统一的终端消息类型，包含输出、输入、订阅/取消订阅等操作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalPayload {
    /// 终端动作
    pub action: TerminalAction,
}

/// 终端动作
///
/// 终端相关的所有操作类型：
/// - Output: PTY 输出数据推送 (服务端 → 客户端)
/// - Input: 客户端输入发送 (客户端 → 服务端)
/// - Subscribe: 订阅会话输出 (客户端 → 服务端)
/// - SubscribeResponse: 订阅响应 (服务端 → 客户端)
/// - Unsubscribe: 取消订阅 (客户端 → 服务端)
/// - UnsubscribeResponse: 取消订阅响应 (服务端 → 客户端)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalAction {
    /// 输出消息 (服务端 → 客户端)
    /// PTY 输出数据推送到客户端
    Output {
        /// Base64 编码的输出数据
        data: String,
        /// 是否等待输入
        is_waiting: bool,
        /// 合并消息的起始索引，用于去重和增量同步起点
        index: usize,
        /// 合并消息的结束索引，用于精确去重（合并多条事件时 index..=end_index）
        #[serde(skip_serializing_if = "Option::is_none")]
        end_index: Option<usize>,
    },

    /// 输入消息 (客户端 → 服务端)
    /// 客户端发送输入到 PTY
    Input {
        /// 输入数据
        data: String,
        /// 特殊键
        #[serde(skip_serializing_if = "Option::is_none")]
        special_key: Option<KeyCombo>,
    },

    /// 订阅输出 (客户端 → 服务端)
    /// 客户端订阅会话输出，实现增量同步
    Subscribe {
        /// 起始序号，不指定则从头补完
        #[serde(skip_serializing_if = "Option::is_none")]
        start_seq: Option<u64>,
    },

    /// 订阅响应 (服务端 → 客户端)
    SubscribeResponse {
        /// 最小可用序号（用于判断数据是否被覆盖）
        min_seq: u64,
        /// 当前最大序号
        max_seq: u64,
        /// 历史消息数量
        history_count: usize,
    },

    /// 取消订阅 (客户端 → 服务端)
    Unsubscribe,

    /// 取消订阅响应 (服务端 → 客户端)
    UnsubscribeResponse,
}