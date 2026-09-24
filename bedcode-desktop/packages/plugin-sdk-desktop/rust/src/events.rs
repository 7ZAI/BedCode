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
/// **线协议 = 出站 [`crate::wire::SyncPayload`] 的线协议**（专项票 02 对齐，此前
/// 是内部标签 PascalCase + 字段平铺，与出站格式差一层改写）：
/// `{ "type": "<snake_case 变体名>", "data": { …字段 } }`。
/// 插件产出即移动端所收，宿主不再改写格式，只做薄适配与源设备排除。
///
/// 与 `SyncPayload` 的**唯一**差异是 `SessionStopped` / `SessionRemoved` 多带的
/// `source_device`：它只服务宿主的「排除发起设备」语义，出站 `data` 里没有这个键
/// （移动端形状不变）。除此之外八个变体的标签与字段逐一同构，
/// 对照表与锁定用例见本文件末尾 `sync_event_variants_mirror_sync_payload`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SyncEvent {
    // ==================== 会话生命周期（会话引擎下沉 P1-b 起由插件真源发布） ====================
    //
    // 宿主不再持有会话事实后，这些事件由 `com.bedcode.terminal-session` 经
    // `broadcast_sync` 发布；**载荷必须自足**（会话概要 / 会话名随事件携带）——
    // 宿主转发时不回查内核（内核已无会话可查，回查只会得到空）。

    /// 会话创建：载荷 = 完整会话概要（类型即 [`crate::wire::SessionSummary`]，
    /// 与出站 wire 同一份定义，无中间 Value 解析步）
    ///
    /// `status` 是**字符串** wire 取值（`"running"` / `"stopped"` / `"error"`…）：
    /// 状态机在插件侧，折算成展示字符串是**生产者**的活，宿主不解读。
    SessionCreated {
        /// 会话概要（snake_case 字段名，见 `wire::summary`）
        session: crate::wire::SessionSummary,
        /// 触发操作的设备名称（桌面本地操作为空串）
        #[serde(default)]
        source_device: String,
    },
    /// 会话状态变化（真源在插件：状态一律以 wire 字符串透传，宿主**不**解析为
    /// 宿主 `SessionStatus`，也不再 `format!("{:?}")` 重格式化）
    SessionStatusChanged {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 变更前状态（wire 字符串）
        old_status: String,
        /// 变更后状态（wire 字符串）
        new_status: String,
        /// 会话名（真源在插件；宿主不回查内核）
        session_name: String,
    },
    /// 会话停止（含会话名，宿主不回查内核）
    SessionStopped {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 会话名
        session_name: String,
        /// 触发操作的设备名称（自然退出为空串；仅宿主排除语义消费，不出站）
        #[serde(default)]
        source_device: String,
    },
    /// 会话移除（含会话名，宿主不回查内核）
    SessionRemoved {
        /// BedCode PTY 会话 ID
        session_id: String,
        /// 会话名
        session_name: String,
        /// 触发操作的设备名称（桌面本地操作为空串；仅宿主排除语义消费，不出站）
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

    /// 会话概要样本（`SessionCreated` 载荷）
    fn summary(id: &str) -> crate::wire::SessionSummary {
        crate::wire::SessionSummary {
            id: id.to_string(),
            name: "dev".to_string(),
            status: "running".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            started_at: Some("2026-01-01T00:00:05Z".to_string()),
            session_type: Some("pty".to_string()),
            config_id: Some("cfg-1".to_string()),
            task_status: None,
            task_reason: None,
        }
    }

    /// 八个变体各一条样本（对齐锁与标签锁共用同一批载荷）
    fn sample_events() -> Vec<SyncEvent> {
        vec![
            SyncEvent::SessionCreated {
                session: summary("s1"),
                source_device: "pixel-9".to_string(),
            },
            SyncEvent::SessionStatusChanged {
                session_id: "s1".to_string(),
                old_status: "running".to_string(),
                new_status: "stopped".to_string(),
                session_name: "dev".to_string(),
            },
            SyncEvent::SessionStopped {
                session_id: "s1".to_string(),
                session_name: "dev".to_string(),
                source_device: "pixel-9".to_string(),
            },
            SyncEvent::SessionRemoved {
                session_id: "s1".to_string(),
                session_name: "dev".to_string(),
                source_device: String::new(),
            },
            SyncEvent::TaskStatusChanged {
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
            },
            SyncEvent::SessionModeChanged { session_id: "s1".into(), auto_approve: true },
            SyncEvent::TaskQueueChanged {
                session_id: "s1".into(),
                queue_count: 3,
                action: "done".into(),
                task_id: Some("t1".into()),
                status: Some("done".into()),
            },
            SyncEvent::TaskScheduledChanged {
                job_id: "job-1".into(),
                status: "pending".into(),
                action: "create".into(),
            },
        ]
    }

    /// D1 对齐锁：`SyncEvent` 的线格式与出站 `SyncPayload` **同构**
    ///
    /// 唯一允许的例外是 `session_stopped` / `session_removed` 的 `source_device`：
    /// 它只给宿主做「排除发起设备」，不在出站 `data` 里。例外必须**被走到**——
    /// 若哪天 `SyncPayload` 也收了 source_device，下面的 stripped 断言即红。
    #[test]
    fn sync_event_variants_mirror_sync_payload() {
        for event in sample_events() {
            let event_json = serde_json::to_value(&event).unwrap();
            let label = event_json["type"].as_str().expect("变体必须有 type 标签").to_string();
            let payload: crate::wire::SyncPayload =
                serde_json::from_value(event_json.clone())
                    .unwrap_or_else(|e| panic!("{label}: SyncEvent 载荷转不成 SyncPayload: {e}\n{event_json}"));
            let payload_json = serde_json::to_value(&payload).unwrap();

            let envelope_only_device = matches!(label.as_str(), "session_stopped" | "session_removed");
            let event_has_device = event_json["data"].get("source_device").is_some();
            let payload_has_device = payload_json["data"].get("source_device").is_some();
            assert_eq!(
                event_has_device, payload_has_device || envelope_only_device,
                "{label}: source_device 的去留不符合约定（事件侧 {event_has_device} / 载荷侧 {payload_has_device}）"
            );

            let mut normalized = event_json.clone();
            if envelope_only_device {
                normalized["data"]
                    .as_object_mut()
                    .expect("data 必须是对象")
                    .remove("source_device");
            }
            assert_eq!(
                payload_json, normalized,
                "{label}: SyncEvent 与 SyncPayload 线形状不再同构"
            );
        }
    }

    /// 变体标签集合锁：插件能发的 = 宿主能出的（多一个即静默丢推送，少一个即死变体）
    #[test]
    fn sync_event_and_sync_payload_label_sets_match() {
        use crate::wire::SyncPayload;
        let mut from_event: Vec<String> = sample_events()
            .iter()
            .map(|e| serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string())
            .collect();
        from_event.sort();
        // SyncPayload 侧同样按样本取值（wire/sync.rs 的 all_variants 口径）
        let from_payload = vec![
            SyncPayload::SessionCreated { session: summary("s1"), source_device: "d".into() },
            SyncPayload::SessionStatusChanged {
                session_id: "s".into(),
                old_status: "running".into(),
                new_status: "stopped".into(),
                session_name: "n".into(),
            },
            SyncPayload::SessionStopped { session_id: "s".into(), session_name: "n".into() },
            SyncPayload::SessionRemoved { session_id: "s".into(), session_name: "n".into() },
            SyncPayload::TaskStatusChanged {
                session_id: "s".into(),
                task_status: "idle".into(),
                task_reason: None,
                task_questions: None,
            },
            SyncPayload::SessionModeChanged { session_id: "s".into(), auto_approve: false },
            SyncPayload::TaskQueueChanged {
                session_id: "s".into(),
                queue_count: 0,
                action: "clear".into(),
                task_id: None,
                status: None,
            },
            SyncPayload::TaskScheduledChanged {
                job_id: "j".into(),
                status: "pending".into(),
                action: "create".into(),
            },
        ]
        .iter()
        .map(|p| serde_json::to_value(p).unwrap()["type"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
        let mut from_payload = from_payload;
        from_payload.sort();
        assert_eq!(
            from_event, from_payload,
            "SyncEvent 与 SyncPayload 的变体面不再一一对应"
        );
    }

    /// 出站 wire 逐字节锁（adjacently tagged + snake_case 标签 + `data` 嵌套）
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
            serde_json::to_string(&full).unwrap(),
            r#"{"type":"task_status_changed","data":{"session_id":"s1","task_status":"asking","task_reason":"need input","task_questions":[{"question":"pick one","header":"choose","multi_select":true,"options":[{"label":"a","description":"opt a"},{"label":"b","description":""}]}]}}"#
        );

        // 可选字段缺失时序列化必须跳过（skip_serializing_if），保持负载精简
        let minimal = SyncEvent::TaskStatusChanged {
            session_id: "s1".into(),
            task_status: "idle".into(),
            task_reason: None,
            task_questions: None,
        };
        assert_eq!(
            serde_json::to_string(&minimal).unwrap(),
            r#"{"type":"task_status_changed","data":{"session_id":"s1","task_status":"idle"}}"#
        );
    }

    #[test]
    fn test_sync_session_created_wire() {
        let event = SyncEvent::SessionCreated {
            session: summary("s-new"),
            source_device: String::new(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "type": "session_created",
                "data": {
                    "session": {
                        "id": "s-new",
                        "name": "dev",
                        "status": "running",
                        "created_at": "2026-01-01T00:00:00Z",
                        "started_at": "2026-01-01T00:00:05Z",
                        "session_type": "pty",
                        "config_id": "cfg-1"
                    },
                    "source_device": ""
                }
            })
        );
    }

    #[test]
    fn test_sync_session_mode_changed() {
        let event = SyncEvent::SessionModeChanged { session_id: "s1".into(), auto_approve: true };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "session_mode_changed", "data": { "session_id": "s1", "auto_approve": true } })
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
                "type": "task_queue_changed",
                "data": {
                    "session_id": "s1",
                    "queue_count": 3,
                    "action": "done",
                    "task_id": "t1",
                    "status": "done"
                }
            })
        );

        let minimal = SyncEvent::TaskQueueChanged {
            session_id: "s1".into(),
            queue_count: 0,
            action: "clear".into(),
            task_id: None,
            status: None,
        };
        let data = &serde_json::to_value(&minimal).unwrap()["data"];
        assert!(data.get("task_id").is_none());
        assert!(data.get("status").is_none());
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
                "type": "task_scheduled_changed",
                "data": { "job_id": "job-1", "status": "pending", "action": "create" }
            })
        );
    }

    #[test]
    fn test_sync_event_parse_round_trip() {
        // 插件按同一 JSON 构造、宿主反序列化 —— 解析往返锁死共享的线协议
        let json = serde_json::json!({
            "type": "task_status_changed",
            "data": {
                "session_id": "s1",
                "task_status": "completed",
                "task_reason": null,
                "task_questions": null
            }
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
        // 未知 type 必须失败 —— 宿主薄适配的前提是解析器严格
        let json = serde_json::json!({ "type": "whatever_changed", "data": { "session_id": "s1" } });
        assert!(serde_json::from_value::<SyncEvent>(json).is_err());
    }

    /// 旧格式（内部标签 + PascalCase + 字段平铺）必须**显性拒绝**
    ///
    /// D1 换格式后，未随包重建的旧插件产物不能被判成「没有事件」：
    /// 宿主 `broadcast_sync` 反序列化即失败并把错误回给 WASM 调用方。
    #[test]
    fn legacy_internal_tag_format_is_rejected() {
        let legacy = serde_json::json!({
            "type": "TaskQueueChanged",
            "session_id": "s1",
            "queue_count": 3,
            "action": "done"
        });
        assert!(serde_json::from_value::<SyncEvent>(legacy).is_err());
        // 新格式缺 data 同样拒绝（adjacently tagged 的 content 是必填）
        assert!(
            serde_json::from_value::<SyncEvent>(serde_json::json!({ "type": "session_mode_changed" }))
                .is_err()
        );
    }

    /// 会话概要的状态必须是**字符串**：插件把状态机折算成展示字符串是生产者的活，
    /// 放 `{"error": "…"}` 对象进来必须拒绝（宿主不解读、也不兜底成空值）
    #[test]
    fn session_summary_status_must_be_wire_string() {
        let object_status = serde_json::json!({
            "type": "session_created",
            "data": {
                "session": {
                    "id": "s1", "name": "n", "status": {"error": "spawn failed"},
                    "created_at": "2026-01-01T00:00:00Z", "started_at": null
                },
                "source_device": ""
            }
        });
        assert!(serde_json::from_value::<SyncEvent>(object_status).is_err());
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
