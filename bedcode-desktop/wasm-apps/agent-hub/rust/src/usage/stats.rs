//! 四维看板聚合（一次返回全部分组，前端本地切换维度）
//!
//! 按天 / 按 CLI 用 SQL GROUP BY；按模型在 Rust 侧展开 models_json（会话
//! 多模型时按消息级归属，数字与源数据一致）；按项目 SQL 聚合并截断展示面。

use super::schema::ensure_schema;
use super::BREAKDOWN_LIMIT;
use crate::usage_parse::ModelUsage;
use bedcode_plugin_api::host::HostPluginDatabase;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};
use std::collections::HashMap;

// ==================== 看板聚合 ====================

/// 四维看板聚合（一次返回全部分组，前端本地切换维度）：
/// 按天 / 按 CLI 用 SQL GROUP BY；按模型在 Rust 侧展开 models_json
/// （会话多模型时按消息级归属，数字与源数据一致）；按项目 SQL 聚合。
pub(crate) fn get_stats(h: &WasmHost) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let empty = json!([]);

    // 总量
    let total = h
        .plugin_db_query(
            "SELECT COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    SUM(cost_total) AS cost_total \
             FROM usage_session",
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .unwrap_or(json!({ "sessions": 0, "tokens_in": 0, "tokens_out": 0, "tokens_cache_read": 0, "tokens_cache_write": 0, "tokens_reasoning": 0, "duration_ms": 0, "cost_total": null }));

    // 按天（宿主本地时区日切；每日 tokens 为堆叠条数据源）
    let by_day = h
        .plugin_db_query(
            "SELECT date(started_at / 1000, 'unixepoch', 'localtime') AS day, \
                    COUNT(*) AS sessions, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out \
             FROM usage_session WHERE started_at IS NOT NULL \
             GROUP BY day ORDER BY day",
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // 按 CLI
    let by_cli = h
        .plugin_db_query(
            "SELECT adapter, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out, \
                    COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_read, \
                    COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_write, \
                    COALESCE(SUM(tokens_reasoning), 0) AS tokens_reasoning, \
                    SUM(cost_total) AS cost_total \
             FROM usage_session GROUP BY adapter ORDER BY tokens_in + tokens_out DESC",
        )
        .ok()
        .flatten()
        .unwrap_or(empty.clone());

    // 按项目（截断展示面）
    let by_project = h
        .plugin_db_query(
            "SELECT COALESCE(project, '') AS project, COUNT(*) AS sessions, \
                    COALESCE(SUM(duration_ms), 0) AS duration_ms, \
                    COALESCE(SUM(tokens_in), 0) AS tokens_in, \
                    COALESCE(SUM(tokens_out), 0) AS tokens_out \
             FROM usage_session GROUP BY project ORDER BY tokens_in + tokens_out DESC",
        )
        .ok()
        .flatten()
        .map(|rows| truncate_rows(rows, BREAKDOWN_LIMIT))
        .unwrap_or(empty.clone());

    // 按模型：展开各会话 models_json（消息级归属，不按主导模型摊派）
    let mut by_model: HashMap<String, Value> = HashMap::new();
    if let Some(rows) = h
        .plugin_db_query("SELECT models_json FROM usage_session")
        .ok()
        .flatten()
        .and_then(|v| v.as_array().cloned())
    {
        for row in rows {
            let Some(raw) = row.get("models_json").and_then(|m| m.as_str()) else {
                continue;
            };
            let Ok(models) = serde_json::from_str::<Vec<ModelUsage>>(raw) else {
                continue;
            };
            for m in models {
                let entry = by_model.entry(m.model.clone()).or_insert_with(|| {
                    json!({ "model": m.model, "sessions": 0, "messages": 0, "tokens_in": 0, "tokens_out": 0 })
                });
                entry["sessions"] = json!(entry["sessions"].as_i64().unwrap_or(0) + 1);
                entry["messages"] =
                    json!(entry["messages"].as_i64().unwrap_or(0) + m.messages as i64);
                entry["tokens_in"] =
                    json!(entry["tokens_in"].as_i64().unwrap_or(0) + m.tokens.input);
                entry["tokens_out"] =
                    json!(entry["tokens_out"].as_i64().unwrap_or(0) + m.tokens.output);
            }
        }
    }
    let mut by_model: Vec<Value> = by_model.into_values().collect();
    by_model.sort_by_key(|e| {
        std::cmp::Reverse(
            e["tokens_in"].as_i64().unwrap_or(0) + e["tokens_out"].as_i64().unwrap_or(0),
        )
    });
    by_model.truncate(BREAKDOWN_LIMIT);

    Ok(json!({
        "total": total,
        "byDay": by_day,
        "byCli": by_cli,
        "byProject": by_project,
        "byModel": by_model,
    }))
}

fn truncate_rows(mut rows: Value, limit: usize) -> Value {
    if let Some(arr) = rows.as_array_mut() {
        arr.truncate(limit);
    }
    rows
}
