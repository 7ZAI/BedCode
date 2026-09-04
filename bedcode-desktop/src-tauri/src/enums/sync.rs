//! Sync Types
//!
//! 数据同步相关类型定义

use serde::{Deserialize, Serialize};

use super::plugin::PluginQuestion;
use super::summary::{SessionConfigSummary, SessionSummary};

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
    SessionStopped { session_id: String, session_name: String },
    /// 会话删除
    SessionRemoved { session_id: String, session_name: String },

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
    ConfigRemoved { config_id: String, config_name: String },

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
    SessionModeChanged { session_id: String, auto_approve: bool },

    // === 任务队列同步 ===
    /// 会话任务队列变更
    TaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },

    // === 定时自动任务同步（v6，ADR 0003） ===
    /// 定时自动任务变更（与移动端 enums/sync.rs 同名变体保持同构）
    TaskScheduledChanged {
        job_id: String,
        /// 变更后的状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
        action: String,
    },
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_scheduled_changed_wire_format() {
        let payload = SyncPayload::TaskScheduledChanged {
            job_id: "job-1".into(),
            status: "pending".into(),
            action: "create".into(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"task_scheduled_changed\""));
        let back: SyncPayload = serde_json::from_str(&json).unwrap();
        match back {
            SyncPayload::TaskScheduledChanged { job_id, status, action } => {
                assert_eq!(job_id, "job-1");
                assert_eq!(status, "pending");
                assert_eq!(action, "create");
            }
            _ => panic!("expected TaskScheduledChanged"),
        }
    }
}
