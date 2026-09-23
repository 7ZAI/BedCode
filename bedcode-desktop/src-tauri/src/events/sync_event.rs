//! Desktop Sync Event
//!
//! 桌面端数据变更事件，用于内部事件总线
//! 触发向客户端的增量数据广播

use super::app_event::AppEvent;
use crate::enums::summary::SessionSummary;
use crate::enums::PluginQuestion;
use crate::enums::SessionStatus;

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
        /// P1-b 起会话真源在插件：宿主不再持有会话时，会话概要随事件携带
        /// （内核路径为 None → 处理器回查内核登记）
        session: Option<SessionSummary>,
    },
    /// 会话状态变化
    SessionStatusChanged {
        session_id: String,
        old_status: SessionStatus,
        new_status: SessionStatus,
        /// P1-b 起会话真源在插件：会话名随事件携带（内核路径为 None → 回查内核）
        session_name: Option<String>,
    },
    /// 会话停止
    SessionStopped {
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
        /// P1-b 起会话真源在插件：会话名随事件携带（内核路径为 None → 回查内核）
        session_name: Option<String>,
    },
    /// 会话删除
    SessionRemoved {
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        source_device: Option<String>,
        /// P1-b 起会话真源在插件：会话名随事件携带（内核路径为 None → 回查内核）
        session_name: Option<String>,
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
    SessionModeChanged { session_id: String, auto_approve: bool },

    // === 任务队列相关 ===
    /// 会话任务队列变更（由 com.bedcode.terminal-session 任务域发布）
    TaskQueueChanged {
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带）
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        status: Option<String>,
    },

    // === 定时自动任务相关（v6，ADR 0003） ===
    /// 定时任务变更（由 com.bedcode.terminal-session 任务域发布）
    TaskScheduledChanged {
        /// 定时任务 ID
        job_id: String,
        /// 变更后的状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
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
            SyncEvent::SessionCreated {
                session,
                source_device,
            } => {
                let session_id = session
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                // 插件携带的会话概要（camelCase SessionSummary 形状）→ 宿主类型；
                // 形状非法则视为缺省（处理器回查内核，会得到 None → 不广播）
                let parsed: Option<SessionSummary> = serde_json::from_value(session).ok();
                DesktopSyncEvent::SessionCreated {
                    session_id,
                    source_device: (!source_device.is_empty()).then_some(source_device),
                    session: parsed,
                }
            }
            SyncEvent::SessionStatusChanged {
                session_id,
                old_status,
                new_status,
                session_name,
            } => DesktopSyncEvent::SessionStatusChanged {
                session_id,
                old_status: serde_json::from_value(old_status)
                    .unwrap_or(SessionStatus::Stopped),
                new_status: serde_json::from_value(new_status)
                    .unwrap_or(SessionStatus::Stopped),
                session_name: Some(session_name),
            },
            SyncEvent::SessionStopped {
                session_id,
                session_name,
                source_device,
            } => DesktopSyncEvent::SessionStopped {
                session_id,
                source_device: (!source_device.is_empty()).then_some(source_device),
                session_name: Some(session_name),
            },
            SyncEvent::SessionRemoved {
                session_id,
                session_name,
                source_device,
            } => DesktopSyncEvent::SessionRemoved {
                session_id,
                source_device: (!source_device.is_empty()).then_some(source_device),
                session_name: Some(session_name),
            },
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
                task_id,
                status,
            } => DesktopSyncEvent::TaskQueueChanged {
                session_id,
                queue_count,
                action,
                task_id,
                status,
            },
            SyncEvent::TaskScheduledChanged { job_id, status, action } => {
                DesktopSyncEvent::TaskScheduledChanged { job_id, status, action }
            }
        }
    }
}
