//! 会话对外视图（`SessionInfoView` 线形状的唯一产出口，会话引擎下沉 P1-b）
//!
//! 宿主 `session/session_event.rs` 的 `SessionInfoView` 是**前端命令、移动端 HTTP DTO
//! 与控制帧共同消费的对外形状**（`SessionInfo` 展平 + 四个任务字段）。真源切换到本插件
//! 后，这个形状由本模块独家产出——漂移一格，移动端与前端就静默错位一格，故：
//!
//! 1. **字段集合逐字对齐宿主**：`id` / `configId` / `name` / `status` / `createdAt` /
//!    `startedAt` / `stoppedAt` / `sessionType` 恒出现（宿主 `SessionInfo` 的两个 `Option`
//!    **没有** `skip_serializing_if`，缺值是 `null` 而不是「键不存在」），任务四字段
//!    仅在槽有值时出现；
//! 2. **记录里的插件私有字段不外溢**：`ptyId`（本插件的句柄事实）、`canonicalRenderer`
//!    （裁决登记）、`owner`、`updatedAt` 都不在这份视图里——它们是本插件的内部事实，
//!    历史上也不曾出现在线协议里，改视图时不得顺手加；
//! 3. **键名语义归本插件**：`taskStatus` / `taskReason` / `taskUpdatedAt` / `taskQuestions`
//!    是本插件任务域写入注解槽的键，宿主 `task_fields_from_slot` 那份「机械转发」随
//!    真源切换一并退役（同一批键名，两处解释本来就是漂移源）。
//!
//! 时间戳口径差（记账，非缺陷）：宿主用 `chrono`（纳秒精度），本域用秒级 RFC3339
//! （[`super::super::config::model::now_rfc3339`]，插件侧不引 chrono）。两者都能被
//! `DateTime<Utc>` 反序列化，前端展示按秒渲染——同一会话在两真源并存期时间戳尾数不同，
//! 属迁移期正常现象，P1-b 切换后只剩本域一份。

use super::model::SessionRecord;
use std::collections::BTreeMap;

/// 注解槽 → 对外任务字段的键名（顺序即出现顺序）
const TASK_TEXT_KEYS: [(&str, &str); 3] = [
    ("taskStatus", "taskStatus"),
    ("taskReason", "taskReason"),
    ("taskUpdatedAt", "taskUpdatedAt"),
];

/// `taskQuestions` 槽键名（值是 JSON 文本，需解析后以数组形态出现）
const TASK_QUESTIONS_KEY: &str = "taskQuestions";

/// 记录 + 注解槽 → 对外视图 JSON。
///
/// 第二个返回值是**告警文本**（`None` = 无需留痕）：本函数保持纯逻辑（native 可全量断言），
/// 日志由 wasm 门面打——与宿主那处「非法 JSON 按缺失处理并 warn，展示字段不该打挂会话链路」
/// 同口径。
pub fn view_json(
    record: &SessionRecord,
    annotations: &BTreeMap<String, String>,
) -> (serde_json::Value, Option<String>) {
    let mut view = serde_json::Map::new();
    view.insert("id".to_string(), serde_json::json!(record.id));
    view.insert("configId".to_string(), serde_json::json!(record.config_id));
    view.insert("name".to_string(), serde_json::json!(record.name));
    view.insert(
        "status".to_string(),
        serde_json::to_value(&record.status).expect("SessionStatus 可序列化"),
    );
    view.insert(
        "createdAt".to_string(),
        serde_json::json!(record.created_at),
    );
    view.insert(
        "startedAt".to_string(),
        record
            .started_at
            .as_ref()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
    );
    view.insert(
        "stoppedAt".to_string(),
        record
            .stopped_at
            .as_ref()
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
    );
    // 宿主该字段恒为 pty（会话即一条 PTY），本域同样只有这一形态
    view.insert("sessionType".to_string(), serde_json::json!("pty"));

    let mut warning = None;
    for (slot_key, field_key) in TASK_TEXT_KEYS {
        // 空串 = 缺失（宿主 annotate 允许写空值，但空值不该产出「有键无值」的半成品形状）
        if let Some(value) = annotations.get(slot_key).filter(|v| !v.is_empty()) {
            view.insert(field_key.to_string(), serde_json::json!(value));
        }
    }
    if let Some(raw) = annotations
        .get(TASK_QUESTIONS_KEY)
        .filter(|v| !v.is_empty())
    {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(parsed) => {
                view.insert(TASK_QUESTIONS_KEY.to_string(), parsed);
            }
            Err(e) => {
                warning = Some(format!(
                    "session annotation '{TASK_QUESTIONS_KEY}' is not valid JSON, treated as absent: {e}"
                ));
            }
        }
    }
    (serde_json::Value::Object(view), warning)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::SessionStatus;

    fn record() -> SessionRecord {
        SessionRecord {
            id: "s-1".to_string(),
            pty_id: Some("pty-9".to_string()),
            config_id: "cfg-1".to_string(),
            name: "dev".to_string(),
            status: SessionStatus::Running,
            created_at: "2026-09-23T00:00:00Z".to_string(),
            started_at: Some("2026-09-23T00:00:05Z".to_string()),
            stopped_at: None,
            canonical_renderer: Some(crate::actions::RendererSource::Desktop),
            owner: Some("com.bedcode.terminal-session".to_string()),
            updated_at: "2026-09-23T00:00:05Z".to_string(),
        }
    }

    fn slot(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// 基础八字段恒出现，且缺省时间是 `null` 而不是「键不存在」（宿主 `SessionInfo`
    /// 的 Option 无 skip_serializing_if）
    #[test]
    fn base_fields_always_present_with_null_timestamps() {
        let (view, warning) = view_json(&record(), &BTreeMap::new());
        assert!(warning.is_none(), "无注解不应产告警");
        assert_eq!(view["id"], "s-1");
        assert_eq!(view["configId"], "cfg-1");
        assert_eq!(view["name"], "dev");
        assert_eq!(
            view["status"], "running",
            "状态用宿主 wire 形态（小写 camelCase）"
        );
        assert_eq!(view["createdAt"], "2026-09-23T00:00:00Z");
        assert_eq!(view["startedAt"], "2026-09-23T00:00:05Z");
        assert!(
            view.get("stoppedAt").is_some_and(|v| v.is_null()),
            "未停止 = stoppedAt: null（键必须在）: {view}"
        );
        assert_eq!(view["sessionType"], "pty");
    }

    /// 线协议形状红线：字段集合与宿主 `SessionInfoView` 逐字相等（多一格少一格都算漂移），
    /// 且本域私有事实（ptyId / canonicalRenderer / owner / updatedAt）不得外溢。
    /// 键**序**不锁（JSON 对象无序，且 `serde_json::Map` 默认按字典序落键）。
    #[test]
    fn field_set_matches_host_view_exactly() {
        let (view, _) = view_json(&record(), &slot(&[("taskStatus", "asking")]));
        let mut keys: Vec<String> = view.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "configId",
                "createdAt",
                "id",
                "name",
                "sessionType",
                "startedAt",
                "status",
                "stoppedAt",
                "taskStatus"
            ]
        );
    }

    /// 反例：注解槽缺失 → 四个任务字段全部不出现（等价于迁移前的 `None`）
    #[test]
    fn absent_slot_produces_no_task_fields() {
        let (view, _) = view_json(&record(), &BTreeMap::new());
        for key in ["taskStatus", "taskReason", "taskUpdatedAt", "taskQuestions"] {
            assert!(view.get(key).is_none(), "{key} 在空槽下不得出现: {view}");
        }
    }

    /// 槽有值 → 原样透传（键值语义本插件自己定，转发不做解释）
    #[test]
    fn slot_values_pass_through_verbatim() {
        let (view, _) = view_json(
            &record(),
            &slot(&[
                ("taskStatus", "asking"),
                ("taskReason", "等待用户答复"),
                ("taskUpdatedAt", "2026-09-20T00:00:00Z"),
            ]),
        );
        assert_eq!(view["taskStatus"], "asking");
        assert_eq!(view["taskReason"], "等待用户答复");
        assert_eq!(view["taskUpdatedAt"], "2026-09-20T00:00:00Z");
    }

    /// 空串视为缺失（与宿主同判据：不该产出「有键无值」）
    #[test]
    fn empty_slot_value_is_treated_as_absent() {
        let (view, _) = view_json(
            &record(),
            &slot(&[("taskStatus", ""), ("taskReason", "有原因")]),
        );
        assert!(view.get("taskStatus").is_none(), "空串不得出现");
        assert_eq!(view["taskReason"], "有原因", "同槽其他键不受影响");
    }

    /// `taskQuestions` 槽是 JSON 文本 → 解析为数组（形状同迁移前的 `Vec<PluginQuestion>`）
    #[test]
    fn task_questions_slot_is_parsed_as_json() {
        let (view, warning) = view_json(
            &record(),
            &slot(&[(
                "taskQuestions",
                r#"[{"question":"继续吗？","options":[{"label":"是","description":""}]}]"#,
            )]),
        );
        assert!(warning.is_none());
        assert_eq!(
            view["taskQuestions"],
            serde_json::json!([{"question": "继续吗？", "options": [{"label": "是", "description": ""}]}])
        );
    }

    /// 反例：非法 JSON → 按缺失处理并回传告警文本（展示字段不该打挂会话链路）
    #[test]
    fn malformed_task_questions_falls_back_to_absent_with_warning() {
        let (view, warning) = view_json(&record(), &slot(&[("taskQuestions", "not-json")]));
        assert!(view.get("taskQuestions").is_none(), "非法槽值不得产出字段");
        let warning = warning.expect("非法 JSON 必须留痕，不静默");
        assert!(
            warning.contains("taskQuestions") && warning.contains("treated as absent"),
            "告警须点名字段与处置口径，got: {warning}"
        );
    }

    /// 终态带因的 `Error` 状态按宿主 externally-tagged 形态产出（不是 `"error"` 字符串）
    #[test]
    fn error_status_keeps_its_payload_shape() {
        let mut record = record();
        record.status = SessionStatus::Error(Some("pty closed".to_string()));
        let (view, _) = view_json(&record, &BTreeMap::new());
        assert_eq!(view["status"], serde_json::json!({ "error": "pty closed" }));
    }
}
