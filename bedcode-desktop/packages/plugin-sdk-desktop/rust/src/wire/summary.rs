//! 会话摘要线协议（原宿主 `enums/summary.rs`，票 01 收编为单一事实源）
//!
//! 字段名即 wire 键名（snake_case，无 `rename_all`）；插件产出口
//! （`com.bedcode.terminal-session` 的 `session::view`）必须逐字对齐本形状，
//! 键名漂移会让宿主反序列化 `SyncPayload::SessionCreated` 整条失败。

use serde::{Deserialize, Serialize};

/// 会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    /// 会话类型：pty 或 plugin
    #[serde(default)]
    pub session_type: Option<String>,
    /// 对应的会话配置 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    /// 任务执行状态（Plugin 会话使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_status: Option<String>,
    /// 任务状态原因
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_reason: Option<String>,
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SessionSummary {
        SessionSummary {
            id: "sess-1".to_string(),
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

    /// wire 键名逐字锁：移动端 TS 与本 crate 的使用方都按这些键取值，
    /// 改字段名 = 改协议，必须在此显式认账
    #[test]
    fn wire_keys_are_locked() {
        assert_eq!(
            serde_json::to_value(&sample()).unwrap(),
            serde_json::json!({
                "id": "sess-1",
                "name": "dev",
                "status": "running",
                "created_at": "2026-01-01T00:00:00Z",
                "started_at": "2026-01-01T00:00:05Z",
                "session_type": "pty",
                "config_id": "cfg-1"
            })
        );
    }

    /// 可选字段的两套降级口径不得互换：`started_at` / `session_type` 无 skip
    /// → 恒出现（缺值为 null）；`config_id` / `task_status` / `task_reason`
    /// 带 skip → 缺省时整键不出现（票 12 M2 口径：wire 上不得出现空键）
    #[test]
    fn optionality_shape_locked() {
        let mut s = sample();
        s.started_at = None;
        s.session_type = None;
        s.config_id = None;
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("started_at").is_some() && json["started_at"].is_null());
        assert!(json.get("session_type").is_some() && json["session_type"].is_null());
        for key in ["config_id", "task_status", "task_reason"] {
            assert!(json.get(key).is_none(), "{key} 应被 skip_serializing_if 省略: {json}");
        }
    }

    /// 老生产者（未写 session_type）的载荷仍可解析，且往返形状稳定
    #[test]
    fn legacy_payload_without_session_type_parses() {
        let legacy = serde_json::json!({
            "id": "s1",
            "name": "n",
            "status": "stopped",
            "created_at": "2026-01-01T00:00:00Z",
            "started_at": null
        });
        let parsed: SessionSummary = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.session_type, None);
        assert_eq!(parsed.config_id, None);
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            serde_json::json!({
                "id": "s1",
                "name": "n",
                "status": "stopped",
                "created_at": "2026-01-01T00:00:00Z",
                "started_at": null,
                "session_type": null
            })
        );
    }

    /// 必填键缺失必须报错，不得静默补空值（否则移动端会渲染出无名会话）
    #[test]
    fn missing_required_keys_rejected() {
        let no_name = serde_json::json!({ "id": "s1", "status": "running", "created_at": "t" });
        assert!(serde_json::from_value::<SessionSummary>(no_name).is_err());
        // status 是字符串：插件若把 Error 态直接放对象（`{"error": "…"}`）进来即拒绝
        let object_status = serde_json::json!({ "id": "s1", "name": "n", "status": {"error": "x"}, "created_at": "t" });
        assert!(serde_json::from_value::<SessionSummary>(object_status).is_err());
    }
}
