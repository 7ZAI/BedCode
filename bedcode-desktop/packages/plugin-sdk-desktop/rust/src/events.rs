//! 共享类型化载荷
//!
//! 宿主 ↔ 插件之间的事件载荷定义。两端引用同一份类型，
//! serde 表示即线协议 —— 新增/修改事件时编译器强制两端同步，
//! 杜绝字符串契约漂移（如历史上 TaskQueueChanged 广播静默丢失）。

use serde::{Deserialize, Serialize};

/// 进程执行完成事件（宿主 host-process → 插件回调）
///
/// 由 [`WasmPlugin::on_process_done`](crate::wasm::WasmPlugin::on_process_done)
/// 接收。三种结束形态：正常退出（exit_code 为 Some）、被信号终止
/// （exit_code 为 None）、超时 kill（timed_out = true）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProcessDoneEvent {
    /// 宿主返回的 run-id（对应 `process_run` 的返回值）
    pub run_id: String,
    /// 退出码（正常退出 = Some(code)；被信号终止 = None）
    pub exit_code: Option<i32>,
    /// 是否因超时被宿主 kill
    pub timed_out: bool,
}

/// 同步事件（插件 → 宿主 → 移动端客户端）
///
/// 通过 `HostEvents::broadcast_sync` 发布，宿主转发给所有已认证的
/// WebSocket 客户端（移动端）。
///
/// 线协议：`{ "type": "TaskStatusChanged" | "SessionModeChanged" | "TaskQueueChanged" | "TaskScheduledChanged" | "SessionCreated" | "SessionStatusChanged" | "SessionStopped" | "SessionRemoved", ...字段 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncEvent {
    // ==================== 会话生命周期（会话引擎下沉 P1-b 起由插件真源发布） ====================
    //
    // 宿主不再持有会话事实后，这些事件由 `com.bedcode.terminal-session` 经
    // `broadcast_sync` 发布；**载荷必须自足**（会话概要 / 会话名随事件携带）——
    // 宿主转发时不回查内核（内核已无会话可查，回查只会得到空）。

    /// 会话创建：载荷 = 完整会话概要（wire 形状与移动端 `SessionSummary` 同构）
    ///
    /// `session` 是插件登记域视图（`session-list` 同源）的子集 JSON：
    /// `{id, name, status, createdAt, startedAt?, sessionType, configId, taskStatus?, taskReason?}`
    /// （camelCase；宿主要么原样包进 `SyncPayload::SessionCreated.session`，要么
    /// 反序列化为宿主 `SessionSummary`——两条路都要求形状与之一致，故插件侧
    /// 产出口被 `session::view` 的形状锁钉住）。
    SessionCreated {
        /// 会话概要（camelCase，见上；宿主不再回查内核，故此字段必填）
        session: serde_json::Value,
        /// 触发操作的设备名称（桌面本地操作为空串）
        #[serde(default)]
        source_device: String,
    },
    /// 会话状态变化（真源在插件：`old/new_status` 是 `SessionStatus` 的
    /// serde wire 形态——简单变体为字符串 `"running"` 等，`Error` 为
    /// `{"error": …}` 对象）
    SessionStatusChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 变更前状态（wire 形态）
        old_status: serde_json::Value,
        /// 变更后状态（wire 形态）
        new_status: serde_json::Value,
        /// 会话名（真源在插件；宿主不回查内核）
        session_name: String,
    },
    /// 会话停止（含会话名，宿主不回查内核）
    SessionStopped {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 会话名
        session_name: String,
        /// 触发操作的设备名称（自然退出为空串）
        #[serde(default)]
        source_device: String,
    },
    /// 会话移除（含会话名，宿主不回查内核）
    SessionRemoved {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 会话名
        session_name: String,
        /// 触发操作的设备名称（桌面本地操作为空串）
        #[serde(default)]
        source_device: String,
    },

    /// 任务状态变更
    TaskStatusChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 任务状态：idle / in_progress / asking / completed / interrupted
        task_status: String,
        /// 状态原因说明
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_reason: Option<String>,
        /// 等待用户回答的问题列表（asking 状态）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_questions: Option<Vec<PluginQuestion>>,
    },
    /// 会话自动授权模式变更
    SessionModeChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 是否自动授权
        auto_approve: bool,
    },
    /// 会话任务队列变更
    TaskQueueChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 变更后的待执行任务数量
        queue_count: i64,
        /// 触发动作：add / remove / clear / dequeue / done / update / reorder / cancel
        action: String,
        /// 关联的队列项 ID（done 广播携带，供移动端预设任务完成匹配；其余动作可选）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        /// 队列项状态（done 广播为 "done"）
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    /// 定时自动任务变更（v6，ADR 0003）
    TaskScheduledChanged {
        /// 定时任务 ID
        job_id: String,
        /// 变更后的任务状态：pending / creating / executed / failed / missed
        status: String,
        /// 触发动作：create / delete / trigger / missed / failed
        action: String,
    },
}

/// 插件推送的问题结构（任务询问，随 TaskStatusChanged 同步到移动端）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestion {
    /// 问题文本
    pub question: String,
    /// 问题简短标题
    pub header: String,
    /// 是否多选
    #[serde(default)]
    pub multi_select: bool,
    /// 选项列表
    #[serde(default)]
    pub options: Vec<PluginQuestionOption>,
}

/// 插件推送的问题选项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestionOption {
    /// 选项标签
    pub label: String,
    /// 选项描述
    #[serde(default)]
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== SyncEvent ====================

    #[test]
    fn test_sync_task_status_changed_full_and_minimal() {
        let full = SyncEvent::TaskStatusChanged {
            session_id: "s1".into(),
            task_status: "asking".into(),
            task_reason: Some("need input".into()),
            task_questions: Some(vec![PluginQuestion {
                question: "pick one".into(),
                header: "choose".into(),
                multi_select: true,
                options: vec![
                    PluginQuestionOption { label: "a".into(), description: "opt a".into() },
                    PluginQuestionOption { label: "b".into(), description: String::new() },
                ],
            }]),
        };
        assert_eq!(
            serde_json::to_value(&full).unwrap(),
            serde_json::json!({
                "type": "TaskStatusChanged",
                "session_id": "s1",
                "task_status": "asking",
                "task_reason": "need input",
                "task_questions": [{
                    "question": "pick one",
                    "header": "choose",
                    "multi_select": true,
                    "options": [
                        { "label": "a", "description": "opt a" },
                        { "label": "b", "description": "" }
                    ]
                }]
            })
        );

        // 可选字段缺失时序列化必须跳过（skip_serializing_if），保持负载精简
        let minimal = SyncEvent::TaskStatusChanged {
            session_id: "s1".into(),
            task_status: "idle".into(),
            task_reason: None,
            task_questions: None,
        };
        let json = serde_json::to_value(&minimal).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "TaskStatusChanged", "session_id": "s1", "task_status": "idle" })
        );
        assert!(json.get("task_reason").is_none());
        assert!(json.get("task_questions").is_none());
    }

    #[test]
    fn test_sync_session_mode_changed() {
        let event = SyncEvent::SessionModeChanged { session_id: "s1".into(), auto_approve: true };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "SessionModeChanged", "session_id": "s1", "auto_approve": true })
        );
    }

    #[test]
    fn test_sync_task_queue_changed() {
        let with_id = SyncEvent::TaskQueueChanged {
            session_id: "s1".into(),
            queue_count: 3,
            action: "done".into(),
            task_id: Some("t1".into()),
            status: Some("done".into()),
        };
        assert_eq!(
            serde_json::to_value(&with_id).unwrap(),
            serde_json::json!({
                "type": "TaskQueueChanged",
                "session_id": "s1",
                "queue_count": 3,
                "action": "done",
                "task_id": "t1",
                "status": "done"
            })
        );

        let minimal = SyncEvent::TaskQueueChanged {
            session_id: "s1".into(),
            queue_count: 0,
            action: "clear".into(),
            task_id: None,
            status: None,
        };
        let json = serde_json::to_value(&minimal).unwrap();
        assert!(json.get("task_id").is_none());
        assert!(json.get("status").is_none());
    }

    #[test]
    fn test_sync_task_scheduled_changed() {
        let event = SyncEvent::TaskScheduledChanged {
            job_id: "job-1".into(),
            status: "pending".into(),
            action: "create".into(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "type": "TaskScheduledChanged",
                "job_id": "job-1",
                "status": "pending",
                "action": "create"
            })
        );
    }

    #[test]
    fn test_sync_event_parse_round_trip() {
        // 移动端按同一 JSON 反序列化 —— 解析往返锁死两侧共享的线协议
        let json = serde_json::json!({
            "type": "TaskStatusChanged",
            "session_id": "s1",
            "task_status": "completed",
            "task_reason": null,
            "task_questions": null
        });
        let event: SyncEvent = serde_json::from_value(json).unwrap();
        match event {
            SyncEvent::TaskStatusChanged { task_reason, task_questions, .. } => {
                assert_eq!(task_reason, None);
                // PluginQuestion 未实现 PartialEq，按空判断
                assert!(task_questions.is_none());
            }
            other => panic!("expected TaskStatusChanged, got {:?}", other),
        }
    }


    #[test]
    fn test_sync_event_rejects_unknown_type() {
        // 未知 type 必须失败 —— 宿主穷尽 match 的前提是解析器严格
        let json = serde_json::json!({ "type": "WhateverChanged", "session_id": "s1" });
        assert!(serde_json::from_value::<SyncEvent>(json).is_err());
    }

    // ==================== PluginQuestion ====================

    #[test]
    fn test_plugin_question_defaults() {
        // 宿主/移动端可能构造缺省字段的旧载荷，default 保证可解析
        let json = serde_json::json!({ "question": "q", "header": "h" });
        let q: PluginQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
        assert!(q.options.is_empty());
    }
}
