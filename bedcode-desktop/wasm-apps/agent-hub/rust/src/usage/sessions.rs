//! 会话列表（统计明细 + 日志主从共用）与会话日志视图（事件流 + 原始行）
//!
//! 列表（started_at 倒序，多条件：adapter / 关键词 / 时间范围）与看板共用
//! `usage_session` 聚合表；打开单会话时 JSONL 源以同一适配器重解析该文件
//! 产出归一事件流 + 原始行（体积不受控，不落库），opencode 源改现查
//! message/part 联表（原始行视图对它天然不存在）。

use super::active::mark_active_rows;
use super::ingest::parse_by_adapter;
use super::schema::ensure_schema;
use super::{read_state, OPENCODE_ADAPTER, PAGE_SIZE, RAW_LINE_CAP};
use crate::usage_parse::NormalizedEvent;
use crate::HOME;
use bedcode_plugin_api::host::{HostFs, HostPluginDatabase};
use bedcode_plugin_api::sql_params;
use bedcode_plugin_api::wasm_host::WasmHost;
use serde_json::{json, Value};

// ==================== 会话列表（统计明细 + 日志主从共用） ====================

/// 转义 LIKE 模式串里的通配符与转义字符本身。
///
/// 约定转义字符为 `\`（与 SQL 侧 `ESCAPE '\'` 对应）。依次转义 `\` → `\\`、
/// `%` → `\%`、`_` → `\_`；顺序不可颠倒，否则转义出的 `\\` 会被后续规则再转义。
fn escape_like_pattern(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '%' => out.push_str(r"\%"),
            '_' => out.push_str(r"\_"),
            _ => out.push(ch),
        }
    }
    out
}

/// 分页会话列表（started_at 倒序）；多条件查询：adapter / 关键词 / 时间范围
pub(crate) fn list_sessions(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let offset = args
        .get("offset")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .filter(|l| *l > 0 && *l <= 200)
        .unwrap_or(PAGE_SIZE);
    let adapter = args.get("adapter").and_then(|v| v.as_str()).unwrap_or("");
    let q = args
        .get("q")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let from = args.get("from").and_then(|v| v.as_i64());
    let to = args.get("to").and_then(|v| v.as_i64());

    // 动态 WHERE：占位符按参数数组顺序编号（宿主按 1-based 顺序绑定）
    let mut clauses: Vec<String> = vec![];
    let mut params: Vec<serde_json::Value> = vec![];
    if !adapter.is_empty() {
        clauses.push("adapter = ?".to_string());
        params.push(serde_json::Value::String(adapter.to_string()));
    }
    if !q.is_empty() {
        // LIKE 通配符转义：用户输入的 `%`（任意长串）与 `_`（单字符）本意是
        // 字面量。不转义时输入一个 `%` 就匹配全部会话，输入 `_` 则逐字符匹配
        // （参数化绑定，无注入风险，纯粹是搜索语义错）。SQLite 默认没有转义字符，
        // 需在模式串里自行声明 ESCAPE '\'。
        let like = format!("%{}%", escape_like_pattern(&q));
        clauses.push(
            "(title LIKE ? ESCAPE '\\' OR project LIKE ? ESCAPE '\\' \
             OR cli_session_id LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        params.push(serde_json::Value::String(like.clone()));
        params.push(serde_json::Value::String(like.clone()));
        params.push(serde_json::Value::String(like));
    }
    if let Some(f) = from {
        clauses.push("started_at >= ?".to_string());
        params.push(serde_json::json!(f));
    }
    if let Some(t) = to {
        clauses.push("started_at <= ?".to_string());
        params.push(serde_json::json!(t));
    }
    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    // LIST 的 LIMIT/OFFSET 占位符排在 WHERE 参数之后
    let mut list_params = params.clone();
    list_params.push(serde_json::json!(limit));
    list_params.push(serde_json::json!(offset));
    let mut rows = h
        .plugin_db_query_params(
            &format!(
                "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                        duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                        tokens_cache_write, tokens_reasoning, cost_total \
                 FROM usage_session{where_sql} ORDER BY started_at DESC LIMIT ? OFFSET ?"
            ),
            &list_params,
        )
        .ok()
        .flatten()
        .unwrap_or(json!([]));

    // 正在使用的项目会话标记（扫描时存 state，查询时按适配器+会话 id 注入）
    mark_active_rows(&mut rows, &read_state(h)["activeSessions"]);
    let total = h
        .plugin_db_query_params(
            &format!("SELECT COUNT(*) AS n FROM usage_session{where_sql}"),
            &params,
        )
        .ok()
        .flatten()
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|row| row.get("n").and_then(|n| n.as_i64()))
        .unwrap_or(0);
    Ok(json!({ "sessions": rows, "total": total, "offset": offset, "limit": limit }))
}
// ==================== 会话日志视图（事件流 + 原始行） ====================

/// 打开单个会话（票据 06；票 07 新增 opencode 分支）
///
/// JSONL 源：按 `usage_session.source_path` 读源文件，用同一适配器解析为
/// 归一事件流 + 原始行（一次读盘双消费）。
/// **opencode 是 SQLite 源**：库里没有「本会话的源文件」，改走
/// [`crate::usage_sqlite::session_events`] 现查 message/part 联表
/// （原始行视图对它天然不存在，返回空数组）。
pub(crate) fn read_session(h: &WasmHost, args: &Value) -> anyhow::Result<Value> {
    ensure_schema(h)?;
    let id = args
        .get("id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("read-session: id required"))?;
    let row = h
        .plugin_db_query_params(
            "SELECT id, adapter, cli_session_id, project, title, started_at, ended_at, \
                    duration_ms, model, tokens_in, tokens_out, tokens_cache_read, \
                    tokens_cache_write, tokens_reasoning, cost_total, source_path \
             FROM usage_session WHERE id = ?1",
            &sql_params![id],
        )
        .map_err(|e| anyhow::anyhow!("read-session: query failed: {e}"))?
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .ok_or_else(|| anyhow::anyhow!("read-session: session {id} not found"))?;
    let adapter = row
        .get("adapter")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let source_path = row
        .get("source_path")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let cli_session_id = row
        .get("cli_session_id")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    if adapter == OPENCODE_ADAPTER {
        return read_opencode_session(h, &row, &cli_session_id);
    }

    // 源文件可能已被外部清理：会话元数据仍在（列表可展示），事件流报错呈现
    let content = h
        .fs_read(&source_path)
        .map_err(|e| anyhow::anyhow!("read-session: source unreadable: {e}"))?
        .unwrap_or_default();

    let parsed = parse_by_adapter(&adapter, &content)
        .ok_or_else(|| anyhow::anyhow!("read-session: unknown adapter {adapter}"))?;

    // 原始行视图：与事件流同源，上限独立计（超出截断并标注）
    let raw_truncated = content.lines().count() > RAW_LINE_CAP;
    let raw: Vec<&str> = content.lines().take(RAW_LINE_CAP).collect();

    let events: Vec<Value> = parsed.events.iter().map(event_to_json).collect();
    Ok(json!({
        "session": row,
        "events": events,
        "eventsTruncated": parsed.events_truncated,
        "raw": raw,
        "rawTruncated": raw_truncated,
        "skippedLines": parsed.skipped_lines,
    }))
}

/// opencode 会话详情：现查 message/part 联表（票 07）
///
/// 失败**显性报错**（不返回空事件流）：「查不出数据」有三种完全不同的成因
/// ——sqlite3 缺失 / 库被删 / 会话真没了——静默返回空数组会把它们压成同一个
/// 「该会话无记录」，与 spec §8 fail-visible 相悖。原始行视图对 SQLite 源
/// 天然不存在（没有「原始 JSONL」这个概念），恒空且不标截断。
fn read_opencode_session(h: &WasmHost, row: &Value, cli_session_id: &str) -> anyhow::Result<Value> {
    let home = HOME
        .get()
        .ok_or_else(|| anyhow::anyhow!("read-session: home unavailable"))?;
    let db = crate::usage_sqlite::db_path(home);
    let (events, truncated) = crate::usage_sqlite::session_events(h, &db, cli_session_id)
        .map_err(|e| anyhow::anyhow!("read-session: opencode query failed: {}", e.code()))?;
    let events: Vec<Value> = events.iter().map(event_to_json).collect();
    Ok(json!({
        "session": row,
        "events": events,
        "eventsTruncated": truncated,
        "raw": Vec::<String>::new(),
        "rawTruncated": false,
        "skippedLines": 0,
    }))
}

fn event_to_json(e: &NormalizedEvent) -> Value {
    json!({
        "ts": e.ts,
        "role": e.role,
        "text": e.text,
        "model": e.model,
        "tokens": e.tokens.map(|t| json!({
            "input": t.input, "output": t.output, "cacheRead": t.cache_read,
            "cacheWrite": t.cache_write, "reasoning": t.reasoning,
        })),
    })
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// LIKE 模式串转义契约：反斜杠自身先转义（否则会被后续规则二次转义），
    /// `%` / `_` 转为字面量，普通字符与多字节字符原样透传。
    #[test]
    fn escape_like_pattern_makes_wildcards_literal() {
        // 正例：普通关键词不变（无多余转义，索引仍可命中）
        assert_eq!(escape_like_pattern("agent hub"), "agent hub");
        assert_eq!(escape_like_pattern("会话日志"), "会话日志");
        // 反例：单个 % 不再是「任意长串」通配符
        assert_eq!(escape_like_pattern("%"), r"\%");
        // 反例：_ 不再是「任意单字符」通配符
        assert_eq!(escape_like_pattern("_"), r"\_");
        // 边界：通配符与转义字符混合，顺序不得互相污染
        assert_eq!(escape_like_pattern(r"a\%b"), r"a\\\%b");
        // 边界：空串
        assert_eq!(escape_like_pattern(""), "");
    }

    /// 回归见证：未转义时 `%` 会匹配全表（正向可证明转义确有必要）。
    /// 若哪天把转义去掉，本例会先红。
    #[test]
    fn escape_like_pattern_guards_against_unescaped_wildcard() {
        let user_input = "%";
        assert_ne!(
            escape_like_pattern(user_input),
            user_input,
            "未转义的 % 仍与输入相同，LIKE 会把它当通配符匹配全部会话",
        );
    }

    /// 事件 → wire 形状：token 明细 camelCase、ts 透传
    #[test]
    fn event_wire_shape() {
        let e = NormalizedEvent {
            ts: Some(1),
            role: "assistant",
            text: "t".to_string(),
            model: Some("m".to_string()),
            tokens: Some(crate::usage_parse::TokenUsage {
                input: 1,
                output: 2,
                cache_read: 3,
                cache_write: 4,
                reasoning: 5,
            }),
        };
        let v = event_to_json(&e);
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["tokens"]["cacheRead"], 3);
        assert_eq!(v["tokens"]["reasoning"], 5);
        let no_tokens = event_to_json(&NormalizedEvent {
            ts: None,
            role: "user",
            text: "t".to_string(),
            model: None,
            tokens: None,
        });
        assert!(no_tokens["tokens"].is_null());
    }
}
