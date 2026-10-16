//! 正在使用的项目会话（扫描配置 + 最新回退）
//!
//! claude 优先读 `~/.claude.json` 的 `projects`（lastStartTime 最大者即当前
//! 项目，比文件 mtime 权威；未授权/损坏静默回退最新会话）；其余适配器取
//! 各自最新会话（started_at 最大）兜底。列表接口按 (adapter, cli_session_id)
//! 命中注入 `active: true`。

use super::ADAPTERS;
use crate::HOME;
use bedcode_plugin_api::host::{HostFs, HostLog, HostPluginDatabase};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 正在使用的项目会话（扫描配置 + 最新回退） ====================

/// claude 配置解析（纯函数，可测）：读 `~/.claude.json` 的 `projects` 映射
/// （key=项目绝对路径 → { lastSessionId, lastStartTime }），返回最后启动
/// 时间（epoch 毫秒）最大的项目 → (项目路径, lastSessionId)。
/// 缺键/非数字的条目跳过；无有效条目返回 None。
fn claude_active_from_config(projects: &Value) -> Option<(String, String)> {
    let obj = projects.as_object()?;
    let mut best: Option<(String, String, i64)> = None;
    for (proj, meta) in obj {
        let sid = meta.get("lastSessionId").and_then(|v| v.as_str());
        let ts = meta.get("lastStartTime").and_then(|v| v.as_i64());
        match (sid, ts) {
            (Some(sid), Some(ts)) if best.as_ref().map_or(true, |(_, _, t)| ts > *t) => {
                best = Some((proj.clone(), sid.to_string(), ts));
            }
            _ => {}
        }
    }
    best.map(|(proj, sid, _)| (proj, sid))
}

/// 正在使用的项目会话（键=适配器 → { project, session_id }，无则 null）：
/// claude 优先读 `~/.claude.json` 配置（lastStartTime 最大者为当前项目会话，
/// 比文件 mtime 权威；未授权/损坏时跳过并回退）；pi 无配置指针，取各自
/// 最新会话（started_at 最大）兜底。
pub(super) fn compute_active_sessions(h: &WasmHost) -> Value {
    let mut active = serde_json::Map::new();
    for adapter in ADAPTERS {
        // 回退：该适配器已入库的最新会话
        let mut entry = h
            .plugin_db_query_params(
                "SELECT project, cli_session_id FROM usage_session \
                 WHERE adapter = ?1 ORDER BY started_at DESC, id DESC LIMIT 1",
                &sql_params![adapter],
            )
            .ok()
            .flatten()
            .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
            .and_then(|row| {
                let project = row
                    .get("project")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string());
                row.get("cli_session_id")
                    .and_then(|c| c.as_str())
                    .map(|sid| json!({ "project": project, "session_id": sid.to_string() }))
            })
            .unwrap_or(Value::Null);
        if adapter == "claude" {
            if let Some(home) = HOME.get() {
                let claude_json = format!("{home}/.claude.json");
                match h.fs_read(&claude_json) {
                    Ok(Some(content)) => match serde_json::from_str::<Value>(&content) {
                        Ok(cfg) => {
                            if let Some((proj, sid)) =
                                cfg.get("projects").and_then(claude_active_from_config)
                            {
                                entry = json!({ "project": proj, "session_id": sid });
                            }
                        }
                        Err(e) => h.log_warn(&format!(
                            "usage: parse {claude_json} failed, fallback to newest session: {e}"
                        )),
                    },
                    // 未授权（fs_auth 拒绝）或文件缺失：回退最新会话，不中断扫描
                    Ok(None) | Err(_) => {}
                }
            }
        }
        active.insert(adapter.to_string(), entry);
    }
    Value::Object(active)
}

/// 列表行注入 active 标记（纯函数，可测）：命中 `activeSessions[adapter]
/// .session_id` 的行置 `active: true`；无标记/不匹配保持原样。
pub(super) fn mark_active_rows(rows: &mut Value, active: &Value) {
    let Some(arr) = rows.as_array_mut() else {
        return;
    };
    for row in arr.iter_mut() {
        let adapter = row.get("adapter").and_then(|v| v.as_str()).unwrap_or("");
        let sid = row
            .get("cli_session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if adapter.is_empty() || sid.is_empty() {
            continue;
        }
        let hit = active
            .get(adapter)
            .and_then(|e| e.as_object())
            .and_then(|e| e.get("session_id"))
            .and_then(|v| v.as_str())
            .map(|a| a == sid)
            .unwrap_or(false);
        if hit {
            row["active"] = json!(true);
        }
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// claude 配置解析：取 lastStartTime 最大项目；缺键/非数字条目跳过
    #[test]
    fn claude_active_picks_latest_project() {
        let projects = json!({
            "/home/u/old": { "lastSessionId": "s-1", "lastStartTime": 1000 },
            "/home/u/new": { "lastSessionId": "s-2", "lastStartTime": 3000 },
            // 缺 lastStartTime / lastSessionId 的条目跳过，不中断
            "/home/u/broken-a": { "lastSessionId": "s-3" },
            "/home/u/broken-b": { "lastStartTime": 5000 },
        });
        let got = claude_active_from_config(&projects);
        assert_eq!(got, Some(("/home/u/new".to_string(), "s-2".to_string())));
        // 空/非对象输入 → None
        assert_eq!(claude_active_from_config(&json!([])), None);
        assert_eq!(claude_active_from_config(&json!({})), None);
    }

    /// 列表注入 active 标记：按 (adapter, cli_session_id) 命中；不匹配行不动
    #[test]
    fn mark_active_rows_by_adapter_session() {
        let mut rows = json!([
            { "adapter": "claude", "cli_session_id": "s-1", "title": "a" },
            { "adapter": "claude", "cli_session_id": "s-2", "title": "b" },
            { "adapter": "pi", "cli_session_id": "s-1", "title": "c" },
            { "adapter": "custom", "cli_session_id": "s-9", "title": "d" },
        ]);
        let active = json!({
            "claude": { "project": "/home/u/new", "session_id": "s-2" },
            "pi": null,
        });
        mark_active_rows(&mut rows, &active);
        let arr = rows.as_array().expect("array");
        // 命中标记仅限同适配器同会话 id（claude/s-1 不标记；pi 条目为 null 不标记）
        assert!(arr[0].get("active").is_none());
        assert_eq!(arr[1]["active"], json!(true));
        assert!(arr[2].get("active").is_none());
        assert!(arr[3].get("active").is_none());
        // 空 activeSessions → 无标记
        let mut rows2 = json!([{ "adapter": "claude", "cli_session_id": "s-2" }]);
        mark_active_rows(&mut rows2, &json!({}));
        assert!(rows2[0].get("active").is_none());
    }
}
