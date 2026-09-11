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
    /// 调整终端大小（force：覆盖确认后置位，见正统渲染端裁决）
    ResizeSession {
        session_id: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        force: bool,
    },
    /// 会话变更通知 (created/stopped/removed)
    SessionChanged {
        change_type: String,
        session: SessionSummary,
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
/// - Input: 客户端输入发送 (客户端 → 服务端)
/// - Subscribe: 订阅会话输出 (客户端 → 服务端)
/// - SubscribeResponse: 订阅响应 (服务端 → 客户端)
/// - Unsubscribe: 取消订阅 (客户端 → 服务端)
/// - UnsubscribeResponse: 取消订阅响应 (服务端 → 客户端)
///
/// PTY 输出不再经 JSON 文本帧（v2 base64 Output action 已随 JoinSession 链
/// 删除），统一走 TB v3 二进制帧（server/ws/terminal_ws/forward.rs）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalAction {
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
    Subscribe,

    /// 订阅响应 (服务端 → 客户端)
    /// TB v3：字段名保留旧协议（增量演进），值承载字节语义——
    /// min_seq = min_offset（最早存续字节）、max_seq = snapshot_offset（订阅时刻
    /// 累计字节）、history_count = history_bytes（驻留历史总字节）
    SubscribeResponse {
        /// 环形保留区间最小字节偏移（更早头部已被淘汰）
        min_seq: u64,
        /// 订阅时刻累计字节数（历史边界）
        max_seq: u64,
        /// 驻留历史总字节数
        history_count: usize,
    },

    /// 取消订阅 (客户端 → 服务端)
    Unsubscribe,

    /// 取消订阅响应 (服务端 → 客户端)
    UnsubscribeResponse,
}
