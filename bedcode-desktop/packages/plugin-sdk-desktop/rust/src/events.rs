//! 共享类型化载荷
//!
//! 宿主 ↔ 插件之间的事件载荷定义。两端引用同一份类型，
//! serde 表示即线协议 —— 新增/修改事件时编译器强制两端同步，
//! 杜绝字符串契约漂移（如历史上 TaskQueueChanged 广播静默丢失）。

use serde::{Deserialize, Serialize};

/// 会话生命周期事件（宿主 → 插件）
///
/// 插件通过 `session_lifecycle_register()` 注册后，经
/// [`WasmPlugin::on_session_lifecycle`](crate::wasm::WasmPlugin::on_session_lifecycle)
/// 回调接收，不走消息总线。
///
/// 线协议：`{ "event_type": "creating" | "created" | "stopping" | "stopped", ...字段 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum SessionLifecycleEvent {
    /// 会话创建前（PTY 启动前，同步阻塞）— 用于前置准备（如 hooks 设置）
    Creating {
        /// 会话配置 ID
        config_id: String,
        /// 启动命令（如 claude）
        command: String,
        /// 工作目录
        working_dir: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入），含 hook 脚本等资源文件
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话创建后（PTY 启动后，异步通知）
    Created {
        /// PTY 会话 ID
        session_id: String,
        /// 会话配置 ID
        config_id: String,
        /// 会话名称
        name: String,
        /// 工作目录
        working_dir: String,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话停止前（异步通知）
    Stopping {
        /// PTY 会话 ID
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
    /// 会话停止后（异步通知）
    Stopped {
        /// PTY 会话 ID
        session_id: String,
        /// 触发操作的设备名称（桌面本地操作为 None）
        #[serde(default)]
        source_device: Option<String>,
        /// 插件安装目录（宿主注入）
        #[serde(default)]
        resource_dir: String,
    },
}

/// 提交输入行事件（宿主 → 插件）
///
/// 用户在终端会话中完成输入并提交（回车触发）时，由宿主 SessionManager
/// 从原始输入字节流重建出完整文本行后分发。插件通过
/// `session_input_register()` 注册后，经
/// [`WasmPlugin::on_input_submitted`](crate::wasm::WasmPlugin::on_input_submitted)
/// 回调接收，不走消息总线。
///
/// 纯观察通知：异步分发、无顺序保证，回调出错或超时不影响输入本身。
/// 宿主不做语义过滤——空提交（空行回车）同样触发，是否忽略由插件决定。
/// 注册需要 `terminal:observe` 权限。见 ADR 0001。
///
/// 线协议：`{ "session_id": "...", "text": "..." }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputSubmittedEvent {
    /// PTY 会话 ID
    pub session_id: String,
    /// 提交的输入行内容（仅普通输入；多行粘贴时含换行符）
    pub text: String,
}

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
/// 线协议：`{ "type": "TaskStatusChanged" | "SessionModeChanged" | "TaskQueueChanged" | "TaskScheduledChanged", ...字段 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncEvent {
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

    // ==================== SessionLifecycleEvent ====================

    #[test]
    fn test_session_lifecycle_creating_wire_format() {
        // 线协议：event_type 内部标签 + snake_case 变体名；
        // source_device/resource_dir 无 skip 标记，None/默认值也序列化
        let event = SessionLifecycleEvent::Creating {
            config_id: "c1".into(),
            command: "claude".into(),
            working_dir: "/work".into(),
            source_device: None,
            resource_dir: "/plugins/x".into(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "event_type": "creating",
                "config_id": "c1",
                "command": "claude",
                "working_dir": "/work",
                "source_device": null,
                "resource_dir": "/plugins/x"
            })
        );
    }

    #[test]
    fn test_session_lifecycle_created_wire_format() {
        let event = SessionLifecycleEvent::Created {
            session_id: "s1".into(),
            config_id: "c1".into(),
            name: "daily".into(),
            working_dir: "/work".into(),
            resource_dir: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "event_type": "created",
                "session_id": "s1",
                "config_id": "c1",
                "name": "daily",
                "working_dir": "/work",
                "resource_dir": ""
            })
        );
    }

    #[test]
    fn test_session_lifecycle_stopping_and_stopped_wire_format() {
        let stopping = SessionLifecycleEvent::Stopping {
            session_id: "s1".into(),
            source_device: Some("phone".into()),
            resource_dir: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&stopping).unwrap(),
            serde_json::json!({
                "event_type": "stopping",
                "session_id": "s1",
                "source_device": "phone",
                "resource_dir": ""
            })
        );

        let stopped = SessionLifecycleEvent::Stopped {
            session_id: "s1".into(),
            source_device: None,
            resource_dir: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&stopped).unwrap(),
            serde_json::json!({
                "event_type": "stopped",
                "session_id": "s1",
                "source_device": null,
                "resource_dir": ""
            })
        );
    }

    #[test]
    fn test_session_lifecycle_parse_with_missing_optionals() {
        // 宿主旧版本可能不携带 source_device/resource_dir，#[serde(default)] 保证可解析
        let json = serde_json::json!({
            "event_type": "creating",
            "config_id": "c1",
            "command": "claude",
            "working_dir": "/work"
        });
        let event: SessionLifecycleEvent = serde_json::from_value(json).unwrap();
        match event {
            SessionLifecycleEvent::Creating { source_device, resource_dir, .. } => {
                assert_eq!(source_device, None);
                assert_eq!(resource_dir, "");
            }
            other => panic!("expected Creating, got {:?}", other),
        }
    }

    #[test]
    fn test_session_lifecycle_rejects_unknown_variant() {
        // 未知 event_type 必须失败（协议严格性，防静默吞掉拼写漂移）
        let json = serde_json::json!({ "event_type": "paused", "session_id": "s1" });
        assert!(serde_json::from_value::<SessionLifecycleEvent>(json).is_err());
    }

    // ==================== InputSubmittedEvent ====================

    #[test]
    fn test_input_submitted_wire_format() {
        let event = InputSubmittedEvent {
            session_id: "s1".into(),
            text: "npm test".into(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "session_id": "s1", "text": "npm test" })
        );
        // 空提交（空行回车）同样是合法事件，宿主不做语义过滤
        let empty = InputSubmittedEvent { session_id: "s1".into(), text: String::new() };
        let back: InputSubmittedEvent =
            serde_json::from_value(serde_json::to_value(&empty).unwrap()).unwrap();
        assert_eq!(back.text, "");
    }

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
