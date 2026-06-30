//! Sync Types
//!
//! 数据同步相关类型定义

use serde::{Deserialize, Serialize};

use super::sumary::{SessionConfigSummary, SessionSummary};
use super::plugin::PluginQuestion;

/// 同步载荷 - 支持多种数据类型的增量同步
///
/// 用于 WebSocket 消息，向客户端推送增量数据变更
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SyncPayload {
    // === 会话状态同步 ===
    /// 会话创建
    SessionCreated {
        session: SessionSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 会话状态变化
    SessionStatusChanged {
        session_id: String,
        old_status: String,
        new_status: String,
        session_name: String,
    },
    /// 会话停止
    SessionStopped {
        session_id: String,
        session_name: String,
    },
    /// 会话删除
    SessionRemoved {
        session_id: String,
        session_name: String,
    },

    // === 会话配置同步 ===
    /// 配置创建
    ConfigCreated {
        config: SessionConfigSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 配置更新
    ConfigUpdated {
        config: SessionConfigSummary,
        /// 触发操作的设备名称（桌面本地操作为空字符串）
        source_device: String,
    },
    /// 配置删除
    ConfigRemoved {
        config_id: String,
        config_name: String,
    },

    // === 任务状态同步 ===
    /// Plugin 任务状态变更
    TaskStatusChanged {
        session_id: String,
        task_status: String,
        task_reason: Option<String>,
        task_questions: Option<Vec<PluginQuestion>>,
    },

    // === 会话模式同步 ===
    /// 会话自动授权模式变更
    SessionModeChanged {
        session_id: String,
        auto_approve: bool,
    },
}
