//! Desktop Sync Event
//!
//! 桌面端数据变更事件，用于内部事件总线
//! 触发向客户端的增量数据广播

use crate::enums::SessionStatus;
use super::app_event::AppEvent;
use crate::enums::PluginQuestion;

/// 桌面端数据变更事件
///
/// 用于内部事件总线，触发向客户端的广播
/// 所有事件都会被 SyncEventHandler 处理并转换为 SyncData WebSocket 消息
#[derive(Debug, Clone)]
pub enum DesktopSyncEvent {
    // === 会话相关 ===
    /// 会话创建
    SessionCreated {
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },
    /// 会话状态变化
    SessionStatusChanged {
        session_id: String,
        old_status: SessionStatus,
        new_status: SessionStatus,
    },
    /// 会话停止
    SessionStopped {
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },
    /// 会话删除
    SessionRemoved {
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },

    // === 配置相关 ===
    /// 配置创建
    ConfigCreated {
        config_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },
    /// 配置更新
    ConfigUpdated {
        config_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },
    /// 配置删除
    ConfigRemoved {
        config_id: String,
        /// 配置名称（用于通知客户端）
        config_name: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
    },

    // === 任务状态相关 ===
    /// Plugin 任务状态变更
    TaskStatusChanged {
        session_id: String,
        task_status: String,
        task_reason: Option<String>,
        task_questions: Option<Vec<PluginQuestion>>,
    },

    // === 会话模式相关 ===
    /// 会话自动授权模式变更
    SessionModeChanged {
        session_id: String,
        auto_approve: bool,
    },

    // === 任务队列相关 ===
    /// 会话任务队列变更（由 auto-task 插件发布）
    TaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue
        action: String,
    },
}

impl AppEvent for DesktopSyncEvent {}

impl From<bedcode_plugin_api::events::SyncEvent> for DesktopSyncEvent {
    /// 插件 SDK 类型化同步事件 → 内部事件总线事件
    ///
    /// 穷尽 match：SDK `SyncEvent` 新增变体时此处编译失败，强制同步
    fn from(event: bedcode_plugin_api::events::SyncEvent) -> Self {
        use bedcode_plugin_api::events::SyncEvent;
        match event {
            SyncEvent::TaskStatusChanged {
                session_id,
                task_status,
                task_reason,
                task_questions,
            } => DesktopSyncEvent::TaskStatusChanged {
                session_id,
                task_status,
                task_reason,
                task_questions,
            },
            SyncEvent::SessionModeChanged {
                session_id,
                auto_approve,
            } => DesktopSyncEvent::SessionModeChanged {
                session_id,
                auto_approve,
            },
            SyncEvent::TaskQueueChanged {
                session_id,
                queue_count,
                action,
            } => DesktopSyncEvent::TaskQueueChanged {
                session_id,
                queue_count,
                action,
            },
        }
    }
}
