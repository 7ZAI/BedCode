//! 同步载荷线协议（原宿主 `enums/sync.rs`，票 01 收编为单一事实源）
//!
//! 宿主 → 移动端的 `sync_data` 载荷形状真源。宿主 `bedcode-desktop` 与移动端
//! `bedcode-mobile` 的 `enums/sync.rs` 均引用本定义（移动端保留平行副本，靠
//! `mobile_parallel_copy_shape_lock` 逐变体钉住，不直接依赖桌面 SDK crate）。

use serde::{Deserialize, Serialize};

use super::summary::SessionSummary;
use crate::events::PluginQuestion;

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
    use crate::events::PluginQuestionOption;

    /// 会话样本（字段值只作线协议入参，与真源无关）
    fn summary(id: &str) -> SessionSummary {
        SessionSummary {
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

    /// 桌面端当前发布的全变体样本集（移动端多出的 Config* 变体不在此列，
    /// 见 `mobile_extra_variants_are_not_produced`）
    fn all_variants() -> Vec<SyncPayload> {
        vec![
            SyncPayload::SessionCreated {
                session: summary("s1"),
                source_device: "pixel-9".to_string(),
            },
            SyncPayload::SessionStatusChanged {
                session_id: "s1".to_string(),
                old_status: "running".to_string(),
                new_status: "stopped".to_string(),
                session_name: "dev".to_string(),
            },
            SyncPayload::SessionStopped {
                session_id: "s1".to_string(),
                session_name: "dev".to_string(),
            },
            SyncPayload::SessionRemoved {
                session_id: "s1".to_string(),
                session_name: String::new(),
            },
            SyncPayload::TaskStatusChanged {
                session_id: "s1".to_string(),
                task_status: "asking".to_string(),
                task_reason: Some("需要授权".to_string()),
                task_questions: Some(vec![PluginQuestion {
                    question: "pick one".to_string(),
                    header: "choose".to_string(),
                    multi_select: false,
                    options: vec![PluginQuestionOption {
                        label: "a".to_string(),
                        description: String::new(),
                    }],
                }]),
            },
            SyncPayload::SessionModeChanged {
                session_id: "s1".to_string(),
                auto_approve: true,
            },
            SyncPayload::TaskQueueChanged {
                session_id: "s1".to_string(),
                queue_count: 3,
                action: "done".to_string(),
                task_id: Some("t1".to_string()),
                status: Some("done".to_string()),
            },
            SyncPayload::TaskScheduledChanged {
                job_id: "job-1".to_string(),
                status: "pending".to_string(),
                action: "create".to_string(),
            },
        ]
    }

    /// 出站同步线协议逐字节锁：adjacently tagged（`type` + `data`）+ snake_case
    /// 变体标签。移动端按 `data` 取值，改成内部标签或平铺字段即静默丢推送。
    #[test]
    fn all_variants_wire_shape_locked() {
        let json = serde_json::to_string(&SyncPayload::TaskScheduledChanged {
            job_id: "job-1".into(),
            status: "pending".into(),
            action: "create".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"task_scheduled_changed","data":{"job_id":"job-1","status":"pending","action":"create"}}"#
        );

        let json = serde_json::to_string(&SyncPayload::SessionStopped {
            session_id: "s1".into(),
            session_name: "dev".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"session_stopped","data":{"session_id":"s1","session_name":"dev"}}"#
        );
    }

    /// 全变体：type 标签锁 + JSON 往返全等（协议错位不得静默吞掉）
    #[test]
    fn every_variant_label_round_trips() {
        let mut seen = Vec::new();
        for payload in all_variants() {
            let value = serde_json::to_value(&payload).unwrap();
            let label = value["type"].as_str().unwrap_or_default().to_string();
            assert!(!label.is_empty(), "变体缺 type 标签: {value}");
            assert!(
                value.get("data").is_some(),
                "变体 {label} 未按 content = \"data\" 嵌套载荷: {value}"
            );
            seen.push(label.clone());
            let back: SyncPayload = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                value,
                "{label} 反序列化后再序列化漂移"
            );
        }
        seen.sort();
        assert_eq!(
            seen,
            vec![
                "session_created",
                "session_mode_changed",
                "session_removed",
                "session_status_changed",
                "session_stopped",
                "task_queue_changed",
                "task_scheduled_changed",
                "task_status_changed",
            ],
            "桌面出站同步事件面发生变化（新增/改名/退役）"
        );
    }

    /// 跨端形状锁：移动端 `enums/sync.rs` 平行副本产出的 JSON 必须能被本真源
    /// 解析，且解析出的字段值与移动端一致。样例逐字抄自
    /// `bedcode-mobile/src-tauri/src/enums/sync.rs` + `sumary.rs` 的字段顺序与可选性口径。
    #[test]
    fn mobile_parallel_copy_shape_lock() {
        let cases: Vec<(&str, &str)> = vec![
            (
                "session_created",
                r#"{"type":"session_created","data":{"session":{"id":"s1","name":"dev","status":"running","created_at":"2026-01-01T00:00:00Z","started_at":null,"session_type":"pty","config_id":"cfg-1"},"source_device":"pixel-9"}}"#,
            ),
            (
                "session_status_changed",
                r#"{"type":"session_status_changed","data":{"session_id":"s1","old_status":"running","new_status":"stopped","session_name":"dev"}}"#,
            ),
            (
                "session_stopped",
                r#"{"type":"session_stopped","data":{"session_id":"s1","session_name":"dev"}}"#,
            ),
            (
                "session_removed",
                r#"{"type":"session_removed","data":{"session_id":"s1","session_name":""}}"#,
            ),
            (
                "task_status_changed",
                r#"{"type":"task_status_changed","data":{"session_id":"s1","task_status":"asking","task_reason":"需要授权","task_questions":[{"question":"pick one","header":"choose","multi_select":false,"options":[{"label":"a","description":""}]}]}}"#,
            ),
            (
                // 移动端对 task_* 无 skip 属性 → 出 null；桌面出「键缺失」，
                // 语义等价（flatten 后比对），差异不得被当成解析失败
                "task_status_changed_minimal",
                r#"{"type":"task_status_changed","data":{"session_id":"s1","task_status":"idle","task_reason":null,"task_questions":null}}"#,
            ),
            (
                "session_mode_changed",
                r#"{"type":"session_mode_changed","data":{"session_id":"s1","auto_approve":true}}"#,
            ),
            (
                "task_queue_changed",
                r#"{"type":"task_queue_changed","data":{"session_id":"s1","queue_count":3,"action":"done","task_id":"t1","status":"done"}}"#,
            ),
            (
                "task_queue_changed_minimal",
                r#"{"type":"task_queue_changed","data":{"session_id":"s1","queue_count":0,"action":"clear","task_id":null,"status":null}}"#,
            ),
            (
                "task_scheduled_changed",
                r#"{"type":"task_scheduled_changed","data":{"job_id":"job-1","status":"pending","action":"create"}}"#,
            ),
        ];
        for (name, json) in cases {
            let parsed: SyncPayload = serde_json::from_str(json).unwrap_or_else(|e| panic!("{name} 解析失败: {e}"));
            let actual = serde_json::to_value(&parsed).unwrap();
            let expected: serde_json::Value = serde_json::from_str(json).unwrap();
            assert_eq!(actual["type"], expected["type"], "{name} 变体标签漂移: {actual}");
            assert_eq!(
                flatten_data(actual),
                flatten_data(expected),
                "{name} 与移动端副本语义不一致"
            );
        }
    }

    /// 把 `{type, data}` 展平为 `{type, data.k → v}` 并丢弃 null 值，用于忽略
    /// 「键缺失」vs「值为 null」这一对双端序列化属性差异后的语义比对
    fn flatten_data(v: serde_json::Value) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        if let Some(t) = v.get("type") {
            out.insert("type".to_string(), t.clone());
        }
        if let Some(serde_json::Value::Object(data)) = v.get("data") {
            for (k, val) in data {
                if !val.is_null() {
                    out.insert(k.clone(), val.clone());
                }
            }
        }
        serde_json::Value::Object(out)
    }

    /// 未知 type 必须拒绝（宿主据此广播，静默降级会把协议错位藏成长期空推送）
    #[test]
    fn unknown_type_rejected() {
        assert!(serde_json::from_str::<SyncPayload>(r#"{"type":"bogus","data":{}}"#).is_err());
        // 缺 data 的载荷同样拒绝：adjacently tagged 的 content 字段是必填
        assert!(serde_json::from_str::<SyncPayload>(r#"{"type":"session_stopped"}"#).is_err());
    }

    /// 移动端多出的 Config* 变体在桌面侧已随宿主 config 域退役：
    /// 本真源不得悄悄恢复它们（恢复即意味着宿主重新解释配置语义）
    #[test]
    fn config_variants_are_not_produced() {
        for label in ["config_created", "config_updated", "config_removed"] {
            let json = format!(r#"{{"type":"{label}","data":{{}}}}"#);
            assert!(
                serde_json::from_str::<SyncPayload>(&json).is_err(),
                "桌面 SDK 不应接受已退役的 {label} 变体"
            );
        }
    }
}
