//! 会话线协议形状（票 02 自内核 `session/` 目录迁入）
//!
//! 内核会话登记实现退役（票 11）后，本文件仍是对外协议真源：Tauri 命令面返回
//! [`SessionInfoView`]、移动端 HTTP 与 WS 控制帧回 [`ResizeOutcome`]、窄转发层按
//! [`SessionInfo`] 的字段形状解析插件登记域视图。
//!
//! **形状即协议**：以下每个类型的 serde 形状都有形状锁用例逐一钉死（含 `Error` 的
//! 两形态、`SessionInfoView` 的字段集合、`ResizeOutcome` 的四态回执）。改任何字段名、
//! 大小写或可选性，先确认前端 / 移动端 / 插件登记域三处同步，否则锁会红。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ==================== 会话状态 / 类型（线协议真源，票 08 自 enums/session.rs 归位） ====================

/// 会话状态（**线协议形状**，票 08 归位 self.protocol 线协议域）
///
/// 会话事实真源在 `com.bedcode.terminal-session` 登记域；宿主侧本类型是它的**对外
/// 透传形状**——宿主业务上**不构造不推进**（PTY 存活用独立的引擎枚举
/// [`crate::enums::PtySessionStatus`]），只做三件事：从插件视图读出状态名做存活过滤
/// （关窗守卫）、在事件广播 / WS 消息里透传、复化串行化 wire。取值的完整形状锁见
/// 本文件底部 `shape_lock_session_status_wire_forms`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    /// 空闲（移动端使用）
    Idle,
    /// 正在启动
    Starting,
    /// 运行中
    Running,
    /// 等待输入
    WaitingInput,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 出错（可选错误信息）
    Error(Option<String>),
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// 会话类型（**线协议形状**，票 08 归位）：只有 `Pty` 一个变体，是
/// `SessionInfo.session_type` 字段的 wire 取值（`"pty"`）；宿主只透传不解释。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionType {
    Pty,
}

impl Default for SessionType {
    fn default() -> Self {
        Self::Pty
    }
}

// ==================== 会话记录与对外视图 ====================

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

/// 注解槽 → 对外任务字段的迁移期兼容映射（**机械转发，不解释语义**）
///
/// spec D5 要求内核只按 `session-id → map<string,string>` 搬运注解、绝不解释键名；
/// 但 contract 期（票 12）对外形状必须逐字段不变（移动端零改动），故保留这一处
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
    (text("taskStatus"), text("taskReason"), text("taskUpdatedAt"), questions)
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

// ==================== 尺寸裁决回执 ====================

/// 正统渲染端身份：当前 PTY 网格尺寸的权威归属端
///
/// 桌面端与移动端同时查看同一会话时 PTY 只能有一个尺寸，输出格式必须
/// 匹配实际渲染的那个端。每次 resize 后归属即确立为请求方，其他端再
/// 调整需先确认覆盖（裁决规则在插件登记域，内核只提供登记事实）。
///
/// serde 注意：容器级 rename_all 只作用于变体名（tag 值），字段名需另用
/// rename_all_fields（serde ≥1.0.186）转为 camelCase，与两端前端的 TS 类型
/// （`{ kind: 'mobile'; deviceName }` / `{ status: 'needsConfirmation'; currentCanonical }`）
/// 对齐——曾因字段保持 snake_case 导致前端读到 undefined 崩溃、确认弹窗不显示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RendererSource {
    /// 桌面端（会话宿主：本地命令 / 本地环回 WS）
    Desktop,
    /// 移动端设备（device_name 来自 JWT claims）
    Mobile { device_name: String },
}

impl RendererSource {
    /// 是否为桌面端（桌面本地路径恒为 Desktop）
    pub fn is_desktop(&self) -> bool {
        matches!(self, RendererSource::Desktop)
    }
}

/// resize 裁决结果（统一输出给所有 entry：桌面命令 / 移动端 HTTP / WS 控制）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ResizeOutcome {
    /// 已应用：请求方就是正统端，或强制覆盖已确认
    Applied { canonical: RendererSource },
    /// 需要确认：另一个端正在渲染输出，本次未应用；客户端弹窗确认后带 force 重发
    NeedsConfirmation { current_canonical: RendererSource },
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

    /// serde 形状回归：字段必须输出 camelCase（与两端前端 TS 类型对齐）。
    /// 曾因容器级 rename_all 只转换变体名、current_canonical/device_name 保持
    /// snake_case，导致前端读 currentCanonical 为 undefined 崩溃且确认弹窗不显示。
    #[test]
    fn test_resize_outcome_and_renderer_source_json_shape_is_camel_case() {
        let outcome = ResizeOutcome::NeedsConfirmation {
            current_canonical: RendererSource::Mobile {
                device_name: "Pixel-9".to_string(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json["status"], "needsConfirmation");
        assert!(
            json.get("currentCanonical").is_some(),
            "field must be camelCase: {json}"
        );
        assert!(json.get("current_canonical").is_none());
        assert_eq!(json["currentCanonical"]["kind"], "mobile");
        assert_eq!(json["currentCanonical"]["deviceName"], "Pixel-9");

        let applied = ResizeOutcome::Applied {
            canonical: RendererSource::Desktop,
        };
        let json: serde_json::Value = serde_json::to_value(&applied).unwrap();
        assert_eq!(json["status"], "applied");
        assert_eq!(json["canonical"]["kind"], "desktop");

        // 反序列化回环（HTTP/命令边界双向兼容）
        let back: ResizeOutcome = serde_json::from_value(json).unwrap();
        assert_eq!(
            back,
            ResizeOutcome::Applied {
                canonical: RendererSource::Desktop
            }
        );
    }

    // ==================== 形状锁（票 02） ====================

    /// 会话记录字段集合锁：`SessionInfo` 的 JSON 键集是窄转发层解析插件视图的依据，
    /// 多一格（插件侧漏对齐）与少一格（前端读到 undefined）都必须在这里先红。
    #[test]
    fn shape_lock_session_info_field_set_is_exact() {
        let json = serde_json::to_value(SessionInfo::new("config-1", "会话 A")).expect("serialize");
        let mut keys: Vec<&str> = json.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
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
                "stoppedAt"
            ],
            "SessionInfo 字段集合（含 None 也出格的 startedAt/stoppedAt）不得漂移"
        );
    }

    /// 对外视图字段集合锁：记录 `flatten` + 四个任务字段的完整键集
    #[test]
    fn shape_lock_session_info_view_field_set_is_exact() {
        let view = SessionInfoView::from_session(
            SessionInfo::new("config-1", "会话 A"),
            &slot(&[
                ("taskStatus", "asking"),
                ("taskReason", "等待答复"),
                ("taskUpdatedAt", "2026-09-20T00:00:00Z"),
                ("taskQuestions", r#"[{"question":"继续吗？"}]"#),
            ]),
        );
        let json = serde_json::to_value(&view).expect("serialize");
        let mut keys: Vec<&str> = json.as_object().expect("object").keys().map(String::as_str).collect();
        keys.sort_unstable();
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
                "taskQuestions",
                "taskReason",
                "taskStatus",
                "taskUpdatedAt",
            ],
            "SessionInfoView = 记录字段 + 四个任务字段，逐格钉死: {json}"
        );
    }

    /// 会话状态取值形状锁：七个变体共八个可出现的 JSON 形态（`Error` 带值 / 带 null 两形态），
    /// 与插件登记域 `session/model.rs` 的同形枚举逐格外形相同——externally tagged，
    /// 无载荷变体出裸字符串，`Error` 出 `{"error": …}` 对象。
    #[test]
    fn shape_lock_session_status_wire_forms() {
        let cases: Vec<(SessionStatus, serde_json::Value)> = vec![
            (SessionStatus::Idle, serde_json::json!("idle")),
            (SessionStatus::Starting, serde_json::json!("starting")),
            (SessionStatus::Running, serde_json::json!("running")),
            (SessionStatus::WaitingInput, serde_json::json!("waitingInput")),
            (SessionStatus::Stopping, serde_json::json!("stopping")),
            (SessionStatus::Stopped, serde_json::json!("stopped")),
            (
                SessionStatus::Error(Some("pty closed".to_string())),
                serde_json::json!({ "error": "pty closed" }),
            ),
            (SessionStatus::Error(None), serde_json::json!({ "error": null })),
        ];
        for (status, expected) in cases.clone() {
            let json = serde_json::to_value(&status).expect("serialize");
            assert_eq!(json, expected, "会话状态 {status:?} 的 wire 形态漂移");
            let back: SessionStatus = serde_json::from_value(expected.clone()).expect("反序列化");
            assert_eq!(back, status, "会话状态 {json} 回环不等");
        }
        assert_eq!(cases.len(), 8, "八个取值形态一个都不能少");
    }

    /// 尺寸裁决四态回执锁：`Applied` / `NeedsConfirmation` × 正统端 `Desktop` / `Mobile`
    /// 四种回执整体形状逐字钉死（不只校字段存在性——前端按 `status` 分支、按
    /// `canonical`/`currentCanonical` 取归属端文案，多一格少一格都会静默错位）
    #[test]
    fn shape_lock_resize_outcome_four_receipts() {
        let mobile = || RendererSource::Mobile {
            device_name: "Pixel 9".to_string(),
        };
        let cases: Vec<(ResizeOutcome, serde_json::Value)> = vec![
            (
                ResizeOutcome::Applied {
                    canonical: RendererSource::Desktop,
                },
                serde_json::json!({ "status": "applied", "canonical": { "kind": "desktop" } }),
            ),
            (
                ResizeOutcome::Applied { canonical: mobile() },
                serde_json::json!({
                    "status": "applied",
                    "canonical": { "kind": "mobile", "deviceName": "Pixel 9" }
                }),
            ),
            (
                ResizeOutcome::NeedsConfirmation {
                    current_canonical: RendererSource::Desktop,
                },
                serde_json::json!({
                    "status": "needsConfirmation",
                    "currentCanonical": { "kind": "desktop" }
                }),
            ),
            (
                ResizeOutcome::NeedsConfirmation {
                    current_canonical: mobile(),
                },
                serde_json::json!({
                    "status": "needsConfirmation",
                    "currentCanonical": { "kind": "mobile", "deviceName": "Pixel 9" }
                }),
            ),
        ];
        for (outcome, expected) in cases {
            let json = serde_json::to_value(&outcome).expect("serialize");
            assert_eq!(json, expected, "回执 {outcome:?} 的 JSON 整体形状漂移");
            let back: ResizeOutcome = serde_json::from_value(json).expect("反序列化");
            assert_eq!(back, outcome, "回执回环不等（HTTP/命令边界双向兼容）");
        }
    }

    /// 变体名与字段名不得混用 tag：`kind`（归属端）与 `status`（裁决结果）是两个
    /// 不同 tag 键，前端按各自键分支——写反即整条确认弹窗静默失效
    #[test]
    fn shape_lock_renderer_source_tag_is_kind_not_status() {
        let json = serde_json::to_value(RendererSource::Desktop).expect("serialize");
        assert_eq!(json, serde_json::json!({ "kind": "desktop" }));
        let source: RendererSource = serde_json::from_value(json).expect("反序列化");
        assert!(source.is_desktop());
    }
}
