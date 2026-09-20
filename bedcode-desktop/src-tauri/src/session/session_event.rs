//! Session Event Types
//!
//! 会话相关事件类型定义

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::enums::{SessionStatus, SessionType};

/// 会话信息（从 session/types.rs 移出）
///
/// **引擎记录（票 12 contract）**：只描述进程与输出，零产品语义——任务语义字段
/// （`task_status` / `task_reason` / `task_updated_at` / `task_questions`）已摘除，
/// 其对外取值一律经注解槽透传（见 [`SessionInfoView`] 与 [`task_fields_from_slot`]）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub config_id: String,
    pub name: String,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub session_type: SessionType,
}

impl SessionInfo {
    pub fn new(config_id: &str, name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: config_id.to_string(),
            name: name.to_string(),
            status: SessionStatus::Starting,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            session_type: SessionType::Pty,
        }
    }
}

/// 注解槽 → 对外任务字段的迁移期兼容映射（**机械转发，内核不解释语义**）
///
/// spec D5 要求内核只按 `session-id → map<string,string>` 搬运注解、绝不解释键名；
/// 但 contract 期（票 12）对外形状必须逐字段不变（移动端零改动），故内核保留这一处
/// **键名 → 对外字段名**的机械对应——它不知道「asking 是什么意思」，只知道「这个键
/// 的值原样填进那个字段」。键名的业务归属仍在写入方插件（会话中心插件）。
/// 线协议若在他日收敛为 `annotations` 直出，本映射随之删除。
///
/// 槽值语义：
/// - 键缺失或**空串** → 对外字段不出现（等价于迁移前的 `None`，即移动端受影响清单
///   M2「注解槽无人写 → 字段为空」的降级口径）
/// - `taskQuestions` 槽值须为 JSON 文本（数组，形状同迁移前的
///   `Vec<PluginQuestion>`）；非法 JSON 按缺失处理并 `warn`——展示字段不该打挂
///   会话链路（内核不校验其内部结构，原样透传）
pub fn task_fields_from_slot(
    annotations: &HashMap<String, String>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<serde_json::Value>,
) {
    let text = |key: &str| annotations.get(key).filter(|v| !v.is_empty()).cloned();
    let questions = text("taskQuestions").and_then(|raw| match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "session annotation 'taskQuestions' is not valid JSON, treated as absent"
            );
            None
        }
    });
    (
        text("taskStatus"),
        text("taskReason"),
        text("taskUpdatedAt"),
        questions,
    )
}

/// 会话视图（**对外形状**：前端命令 / 控制帧 / 移动端 DTO 的构造源）
///
/// `flatten` 会话记录 + 四个任务字段：JSON 形状与 contract 前逐字段一致
/// （`taskStatus` / `taskReason` / `taskUpdatedAt` / `taskQuestions`，缺省不出现）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoView {
    #[serde(flatten)]
    pub info: SessionInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_questions: Option<serde_json::Value>,
}

impl SessionInfoView {
    /// 记录 + 注解槽 → 对外视图（唯一的任务字段取值点）
    pub fn from_session(info: SessionInfo, annotations: &HashMap<String, String>) -> Self {
        let (task_status, task_reason, task_updated_at, task_questions) = task_fields_from_slot(annotations);
        Self {
            info,
            task_status,
            task_reason,
            task_updated_at,
            task_questions,
        }
    }
}

/// 会话状态变化事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusEvent {
    pub session_id: String,
    pub old_status: Option<SessionStatus>,
    pub new_status: SessionStatus,
    pub session_name: String,
}

/// 会话重启事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRestartEvent {
    pub old_session_id: String,
    pub new_session_id: String,
    pub session_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// 槽缺失（插件未激活 / 未写槽）→ 四个对外字段全部不出现（M2 降级口径）
    #[test]
    fn view_without_slot_has_no_task_fields() {
        let view = SessionInfoView::from_session(SessionInfo::new("config-1", "test"), &HashMap::new());
        let json = serde_json::to_value(&view).expect("serialize");
        for key in ["taskStatus", "taskReason", "taskUpdatedAt", "taskQuestions"] {
            assert!(json.get(key).is_none(), "{key} 在空槽下不得出现: {json}");
        }
        assert_eq!(json["configId"], "config-1", "记录字段形状不变");
        assert_eq!(json["sessionType"], "pty");
        assert!(json.get("id").is_some());
    }

    /// 槽有值 → 字段原样透传（键值语义内核不问）
    #[test]
    fn view_passes_slot_values_through() {
        let view = SessionInfoView::from_session(
            SessionInfo::new("config-1", "test"),
            &slot(&[
                ("taskStatus", "asking"),
                ("taskReason", "等待用户答复"),
                ("taskUpdatedAt", "2026-09-20T00:00:00Z"),
            ]),
        );
        let json = serde_json::to_value(&view).expect("serialize");
        assert_eq!(json["taskStatus"], "asking");
        assert_eq!(json["taskReason"], "等待用户答复");
        assert_eq!(json["taskUpdatedAt"], "2026-09-20T00:00:00Z");
    }

    /// 空串视为缺失（`annotate` 允许空值，但空值不该产出一个「有键无值」的形状）
    #[test]
    fn view_treats_empty_slot_value_as_absent() {
        let view = SessionInfoView::from_session(
            SessionInfo::new("config-1", "test"),
            &slot(&[("taskStatus", ""), ("taskReason", "有原因")]),
        );
        let json = serde_json::to_value(&view).expect("serialize");
        assert!(json.get("taskStatus").is_none(), "空串不得出现");
        assert_eq!(json["taskReason"], "有原因", "同槽其他键不受影响");
    }

    /// `taskQuestions` 槽值为 JSON 文本 → 解析为数组（形状同迁移前的 `Vec<PluginQuestion>`）
    #[test]
    fn view_parses_task_questions_json() {
        let view = SessionInfoView::from_session(
            SessionInfo::new("config-1", "test"),
            &slot(&[(
                "taskQuestions",
                r#"[{"question":"继续吗？","options":[{"label":"是","description":""}]}]"#,
            )]),
        );
        let json = serde_json::to_value(&view).expect("serialize");
        assert_eq!(
            json["taskQuestions"],
            serde_json::json!([{"question": "继续吗？", "options": [{"label": "是", "description": ""}]}])
        );
    }

    /// `taskQuestions` 槽值非法 JSON → 按缺失处理（warn 留痕，不产半成品形状）
    #[test]
    fn view_rejects_malformed_task_questions() {
        let view = SessionInfoView::from_session(
            SessionInfo::new("config-1", "test"),
            &slot(&[("taskQuestions", "not-json")]),
        );
        let json = serde_json::to_value(&view).expect("serialize");
        assert!(json.get("taskQuestions").is_none(), "非法槽值不得产出字段");
    }
}
