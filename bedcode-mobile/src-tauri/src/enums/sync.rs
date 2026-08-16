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

    // === 任务队列同步 ===
    /// 会话任务队列变更
    TaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带，供预设任务完成匹配）
        #[serde(default)]
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        #[serde(default)]
        status: Option<String>,
    },

    // === 定时自动任务同步（v6，ADR 0003） ===
    /// 定时自动任务变更（与桌面端 enums/sync.rs 同名变体保持同构）
    TaskScheduledChanged {
        job_id: String,
        /// 变更后的状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
        action: String,
    },

    // === 文件服务同步（桌面 → 移动，内网文件传输插件规格阶段 2） ===
    /// 桌面侧插件挂载点可用性变更（mount/unmount/update_roots 后由宿主自动发出）
    ///
    /// 与桌面端 `enums/sync.rs` 同名变体保持同构
    FileServiceChanged {
        plugin_id: String,
        mount_path: String,
        /// true = 挂载可用（mount/update_roots），false = 已摘除（unmount）
        available: bool,
        /// 挂载支持的操作集合（unmount 时为空）
        operations: Vec<bedcode_plugin_api_mobile::FileOperation>,
    },

    // === 传输批应答（v2，桌面 → 移动，发送端=移动） ===
    /// 传输批应答推送（接收端批准/拒绝/超时 → 发送端）
    ///
    /// 移动端作为发送方时收到（对端桌面接收方经 WS 推送）；宿主发布
    /// `filesrv:transfer_approval` 双通道事件，发送方插件据此调度批内任务。
    /// 与桌面端 `enums/sync.rs` 同名变体保持同构（逐字一致）
    TransferApproval {
        /// 批 ID
        batch_id: String,
        /// "approved" | "rejected"
        decision: String,
        /// "" | "user-rejected" | "timeout"
        reason: String,
    },
}
