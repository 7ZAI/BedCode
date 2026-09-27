//! claude JSONL 适配器：聚合 + 事件流
//!
//! 去重语义：同一 assistant message（message.id）按内容块拆多行且每行重复
//! 完整 usage，按 id 首现计入（无 usage / 无 id 的行只计事件）。token 字段
//! snake_case（`message.usage.input_tokens` 等）；`type=cost-state` 行含真实
//! `totalCostUSD`（最后一条为准，有则存不估算）。

use super::common::{as_i64, extract_text, parse_line, push_event, truncate_text};
use super::time::parse_iso8601_ms;
use super::types::{NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_TOOL, ROLE_USER};
use serde_json::Value;
use std::collections::HashSet;

// ==================== claude 适配器 ====================

/// claude JSONL 适配器：聚合 + 事件流
///
/// 去 重语义：同一 assistant message（message.id）按内容块拆多行且每行
/// 重复完整 usage，按 id 首现计入（无 usage / 无 id 的行只计事件）。
pub(crate) fn parse_claude_session(content: &str) -> ParsedSession {
    let mut session = ParsedSession::default();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut title: Option<String> = None;
    let mut project: Option<String> = None;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(v) = parse_line(line) else {
            session.skipped_lines += 1;
            continue;
        };
        let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso8601_ms);
        if project.is_none() {
            project = v.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
        }
        if session.cli_session_id.is_empty() {
            session.cli_session_id = v
                .get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
        }
        if let Some(t) = ts {
            session.started_at = Some(session.started_at.map_or(t, |cur| cur.min(t)));
            session.ended_at = Some(session.ended_at.map_or(t, |cur| cur.max(t)));
        }

        match kind {
            "assistant" => {
                let msg = v.get("message").cloned().unwrap_or(Value::Null);
                let model = msg
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let usage_val = msg.get("usage");
                let has_usage = usage_val.map(|u| u.is_object()).unwrap_or(false);
                let msg_id = msg
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string());
                let usage = TokenUsage {
                    input: as_i64(usage_val.and_then(|u| u.get("input_tokens"))),
                    output: as_i64(usage_val.and_then(|u| u.get("output_tokens"))),
                    cache_read: as_i64(usage_val.and_then(|u| u.get("cache_read_input_tokens"))),
                    cache_write: as_i64(
                        usage_val.and_then(|u| u.get("cache_creation_input_tokens")),
                    ),
                    reasoning: as_i64(
                        usage_val
                            .and_then(|u| u.get("output_tokens_details"))
                            .and_then(|d| d.get("thinking_tokens")),
                    ),
                };
                // 首现计入（HashSet::insert 新插入返回 true → !insert 即重复）
                let duplicated = has_usage
                    && msg_id
                        .as_deref()
                        .map(|id| !seen_ids.insert(id.to_string()))
                        .unwrap_or(false);
                if has_usage && !duplicated {
                    session.record_assistant_usage(&model, &usage);
                }
                push_event(
                    &mut session.events,
                    &mut session.events_truncated,
                    NormalizedEvent {
                        ts,
                        role: ROLE_ASSISTANT,
                        text: assistant_display_text(msg.get("content")),
                        model: Some(model),
                        tokens: has_usage.then_some(usage),
                    },
                );
            }
            "user" => {
                let msg = v.get("message").cloned().unwrap_or(Value::Null);
                let is_meta = v.get("isMeta").and_then(|m| m.as_bool()).unwrap_or(false);
                if msg.get("content").map(|c| c.is_array()).unwrap_or(false) {
                    // tool_result 数组块：归一为工具事件
                    for block in msg["content"].as_array().unwrap() {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                            let text = extract_text(block.get("content").unwrap_or(&Value::Null))
                                .unwrap_or_default();
                            push_event(
                                &mut session.events,
                                &mut session.events_truncated,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_TOOL,
                                    text: format!("tool_result · {}", truncate_text(&text, 400)),
                                    model: None,
                                    tokens: None,
                                },
                            );
                        }
                    }
                } else if let Some(text) = extract_text(msg.get("content").unwrap_or(&Value::Null))
                {
                    // isMeta（local-command 包裹等）作系统事件，不作会话标题
                    let role = if is_meta { ROLE_SYSTEM } else { ROLE_USER };
                    if role == ROLE_USER && title.is_none() {
                        title = Some(truncate_text(&text, 120));
                    }
                    push_event(
                        &mut session.events,
                        &mut session.events_truncated,
                        NormalizedEvent {
                            ts,
                            role,
                            text,
                            model: None,
                            tokens: None,
                        },
                    );
                }
            }
            "system" => {
                let text = extract_text(v.get("content").unwrap_or(&Value::Null))
                    .or_else(|| {
                        v.get("subtype")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();
                push_event(
                    &mut session.events,
                    &mut session.events_truncated,
                    NormalizedEvent {
                        ts,
                        role: ROLE_SYSTEM,
                        text,
                        model: None,
                        tokens: None,
                    },
                );
            }
            "cost-state" => {
                if let Some(cost) = v.get("totalCostUSD").and_then(|c| c.as_f64()) {
                    session.cost_total = Some(cost);
                }
            }
            // summary / attachment / mode / file-history-* 等元数据行不进事件流
            _ => {}
        }
    }

    session.title = title;
    session.project = project;
    session
}

/// 助手事件展示文本：text 块拼接；tool_use 块给「工具名 + 摘要」
pub(super) fn assistant_display_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    match content {
        Value::Array(blocks) => {
            let mut parts: Vec<String> = vec![];
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                        parts.push(format!("tool_use · {name}"));
                    }
                    Some("thinking") => {}
                    _ => {}
                }
            }
            truncate_text(&parts.join("\n"), 2000)
        }
        _ => extract_text(content)
            .map(|t| truncate_text(&t, 2000))
            .unwrap_or_default(),
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claude_assistant_line(id: &str, input: i64, output: i64, ts: &str) -> String {
        json!({
            "type": "assistant",
            "timestamp": ts,
            "cwd": "/home/u/proj",
            "sessionId": "sess-1",
            "message": {
                "id": id,
                "model": "claude-x",
                "content": [{ "type": "text", "text": "回复" }],
                "usage": {
                    "input_tokens": input,
                    "output_tokens": output,
                    "cache_read_input_tokens": 10,
                    "cache_creation_input_tokens": 20,
                    "output_tokens_details": { "thinking_tokens": 5 }
                }
            }
        })
        .to_string()
    }

    #[test]
    fn claude_dedups_repeated_message_usage() {
        // 同一 message.id 三行（不同内容块）：usage 只计一次
        let mut content = String::new();
        for text in ["a", "b", "c"] {
            content.push_str(
                &json!({
                    "type": "assistant",
                    "timestamp": "2026-09-04T01:17:52.000Z",
                    "sessionId": "sess-1",
                    "message": {
                        "id": "msg-1",
                        "model": "claude-x",
                        "content": [{ "type": "text", "text": text }],
                        "usage": { "input_tokens": 100, "output_tokens": 7 }
                    }
                })
                .to_string(),
            );
            content.push('\n');
        }
        let s = parse_claude_session(&content);
        assert_eq!(s.tokens.input, 100);
        assert_eq!(s.tokens.output, 7);
        assert_eq!(s.models.len(), 1);
        assert_eq!(s.models[0].messages, 1);
        // 事件流仍保留三行（内容块都展示）
        assert_eq!(s.events.len(), 3);
    }

    #[test]
    fn claude_aggregates_multiple_messages_and_models() {
        let content = format!(
            "{}\n{}\n{}\n",
            claude_assistant_line("msg-1", 100, 10, "2026-09-04T01:00:00Z"),
            claude_assistant_line("msg-2", 200, 20, "2026-09-04T01:05:00Z"),
            json!({
                "type": "assistant",
                "timestamp": "2026-09-04T01:10:00Z",
                "sessionId": "sess-1",
                "message": {
                    "id": "msg-3",
                    "model": "claude-y",
                    "content": [{ "type": "tool_use", "name": "Read", "input": {} }],
                    "usage": { "input_tokens": 1, "output_tokens": 2 }
                }
            })
            .to_string()
        );
        let s = parse_claude_session(&content);
        assert_eq!(s.tokens.input, 301);
        assert_eq!(s.tokens.output, 32);
        assert_eq!(s.tokens.cache_read, 20); // msg-1/2 各 10
        assert_eq!(s.tokens.cache_write, 40);
        assert_eq!(s.tokens.reasoning, 10); // msg-1/2 各 5
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.models[0].messages, 2);
        assert_eq!(s.models[1].messages, 1);
        assert_eq!(s.started_at, Some(1_788_483_600_000));
        assert_eq!(s.ended_at, parse_iso8601_ms("2026-09-04T01:10:00Z"));
    }

    #[test]
    fn claude_empty_usage_counts_nothing() {
        let content = json!({
            "type": "assistant",
            "timestamp": "2026-09-04T01:17:52Z",
            "sessionId": "sess-1",
            "message": { "id": "msg-1", "model": "claude-x", "content": [] }
        })
        .to_string();
        let s = parse_claude_session(&content);
        assert_eq!(s.tokens, TokenUsage::default());
        assert!(s.models.is_empty());
        // 无 usage 的助手事件仍进流（不携带 token 明细）
        assert_eq!(s.events.len(), 1);
        assert!(s.events[0].tokens.is_none());
    }

    #[test]
    fn claude_truncated_and_invalid_lines_skipped() {
        let mut content = claude_assistant_line("msg-1", 10, 5, "2026-09-04T01:00:00Z");
        content.push('\n');
        content.push_str(&claude_assistant_line(
            "msg-2",
            20,
            6,
            "2026-09-04T01:01:00Z",
        ));
        content.push_str("\n{\"type\":\"assistant\",\"timestamp\":\"2026-09-04T01:02"); // 截断
        content.push_str("\nnot json at all\n"); // 非法
        let s = parse_claude_session(&content);
        assert_eq!(s.tokens.input, 30);
        assert_eq!(s.skipped_lines, 2);
    }

    #[test]
    fn claude_cost_state_last_wins() {
        let content = format!(
            "{}\n{}\n{}\n",
            claude_assistant_line("msg-1", 10, 5, "2026-09-04T01:00:00Z"),
            json!({"type":"cost-state","sessionId":"sess-1","totalCostUSD":1.25}).to_string(),
            json!({"type":"cost-state","sessionId":"sess-1","totalCostUSD":3.5}).to_string()
        );
        let s = parse_claude_session(&content);
        assert_eq!(s.cost_total, Some(3.5));
    }

    #[test]
    fn claude_meta_user_is_system_and_title_from_real_user() {
        let content = format!(
            "{}\n{}\n{}\n",
            json!({
                "type": "user",
                "isMeta": true,
                "timestamp": "2026-09-04T01:00:00Z",
                "sessionId": "sess-1",
                "cwd": "/home/u/proj",
                "message": { "role": "user", "content": "<local-command-caveat>…</local-command-caveat>" }
            })
            .to_string(),
            json!({
                "type": "user",
                "timestamp": "2026-09-04T01:01:00Z",
                "sessionId": "sess-1",
                "message": { "role": "user", "content": "修复启动崩溃" }
            })
            .to_string(),
            json!({
                "type": "system",
                "subtype": "local_command",
                "timestamp": "2026-09-04T01:02:00Z",
                "sessionId": "sess-1",
                "content": "ok"
            })
            .to_string()
        );
        let s = parse_claude_session(&content);
        assert_eq!(s.title.as_deref(), Some("修复启动崩溃"));
        assert_eq!(s.project.as_deref(), Some("/home/u/proj"));
        let roles: Vec<&str> = s.events.iter().map(|e| e.role).collect();
        assert_eq!(roles, vec!["system", "user", "system"]);
    }

    #[test]
    fn claude_tool_result_user_lines_become_tool_events() {
        let content = json!({
            "type": "user",
            "timestamp": "2026-09-04T01:02:00Z",
            "sessionId": "sess-1",
            "message": {
                "role": "user",
                "content": [
                    { "type": "tool_result", "tool_use_id": "t1", "content": [{ "type": "text", "text": "42 行" }] }
                ]
            }
        })
        .to_string();
        let s = parse_claude_session(&content);
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].role, "tool");
        assert!(s.events[0].text.starts_with("tool_result"));
    }
}
