//! pi JSONL 适配器：聚合 + 事件流
//!
//! 首行 `type=session` 携带 id/cwd；助手 usage 为 camelCase +
//! `usage.cost.total`（逐消息上报累计量，最后一条为准）；无 session 头的
//! 文件以消息行兜底推进（id 缺省由调用方按文件名补足）。

use super::claude::assistant_display_text;
use super::common::{
    as_i64, extract_text, parse_line, push_event, tool_result_text, truncate_text,
};
use super::time::parse_iso8601_ms;
use super::types::{NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_TOOL, ROLE_USER};
use serde_json::Value;

// ==================== pi 适配器 ====================

/// pi JSONL 适配器：聚合 + 事件流
///
/// 首行 `type=session` 携带 id/cwd；助手 usage 为 camelCase +
/// `usage.cost.total`；无 session 头的文件以消息行兜底推进（id 缺省
/// 由调用方按文件名补足）。
pub(crate) fn parse_pi_session(content: &str) -> ParsedSession {
    let mut session = ParsedSession::default();

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
        if let Some(t) = ts {
            session.started_at = Some(session.started_at.map_or(t, |cur| cur.min(t)));
            session.ended_at = Some(session.ended_at.map_or(t, |cur| cur.max(t)));
        }

        match kind {
            "session" => {
                session.cli_session_id = v
                    .get("id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                session.project = v.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
            }
            "message" => {
                let msg = v.get("message").cloned().unwrap_or(Value::Null);
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                match role {
                    "assistant" => {
                        let model = msg
                            .get("model")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let usage_val = msg.get("usage");
                        let has_usage = usage_val.map(|u| u.is_object()).unwrap_or(false);
                        let usage = TokenUsage {
                            input: as_i64(usage_val.and_then(|u| u.get("input"))),
                            output: as_i64(usage_val.and_then(|u| u.get("output"))),
                            cache_read: as_i64(usage_val.and_then(|u| u.get("cacheRead"))),
                            cache_write: as_i64(usage_val.and_then(|u| u.get("cacheWrite"))),
                            reasoning: as_i64(usage_val.and_then(|u| u.get("reasoning"))),
                        };
                        if has_usage {
                            session.record_assistant_usage(&model, &usage);
                        }
                        if let Some(cost) = usage_val
                            .and_then(|u| u.get("cost"))
                            .and_then(|c| c.get("total"))
                            .and_then(|c| c.as_f64())
                        {
                            // 同会话多模型时取累计（cost 逐消息上报累计量）
                            session.cost_total = Some(cost);
                        }
                        push_event(
                            &mut session,
                            NormalizedEvent {
                                ts,
                                role: ROLE_ASSISTANT,
                                text: assistant_display_text(msg.get("content")),
                                model: Some(model),
                                tokens: has_usage.then_some(usage),
                                error: false,
                                tool_use_id: None,
                            },
                        );
                    }
                    "user" => {
                        if let Some(text) = extract_text(msg.get("content").unwrap_or(&Value::Null))
                        {
                            if session.title.is_none() {
                                session.title = Some(truncate_text(&text, 120));
                            }
                            push_event(
                                &mut session,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_USER,
                                    // A7 对称补齐：user 正文与 assistant 同口径截断
                                    text: truncate_text(&text, 2000),
                                    model: None,
                                    tokens: None,
                                    error: false,
                                    tool_use_id: None,
                                },
                            );
                        }
                    }
                    "toolResult" => {
                        let tool = msg
                            .get("toolName")
                            .and_then(|n| n.as_str())
                            .unwrap_or("tool");
                        // A4 ③：非 text 块（图片/二进制）给占位而非静默丢弃
                        let text = tool_result_text(msg.get("content").unwrap_or(&Value::Null));
                        let err = msg
                            .get("isError")
                            .and_then(|e| e.as_bool())
                            .unwrap_or(false);
                        let tid = msg
                            .get("toolCallId")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        push_event(
                            &mut session,
                            NormalizedEvent {
                                ts,
                                role: ROLE_TOOL,
                                text: format!(
                                    "{tool}{} · {}",
                                    if err { " (error)" } else { "" },
                                    truncate_text(&text, 1000)
                                ),
                                model: None,
                                tokens: None,
                                error: err,
                                tool_use_id: (!tid.is_empty()).then_some(tid),
                            },
                        );
                    }
                    _ => {}
                }
            }
            "model_change" => {
                let text = v
                    .get("modelId")
                    .and_then(|m| m.as_str())
                    .map(|m| format!("model_change · {m}"))
                    .unwrap_or_else(|| "model_change".to_string());
                push_event(
                    &mut session,
                    NormalizedEvent {
                        ts,
                        role: ROLE_SYSTEM,
                        text,
                        model: None,
                        tokens: None,
                        error: false,
                        tool_use_id: None,
                    },
                );
            }
            // custom / thinking_level_change 等元数据行不进事件流
            _ => {}
        }
    }

    session
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_parse::MAX_EVENTS;
    use serde_json::{json, Value};

    fn pi_session_header() -> String {
        json!({
            "type": "session",
            "version": 3,
            "id": "pi-sess-1",
            "timestamp": "2026-09-07T20:54:02.063Z",
            "cwd": "/home/binblink"
        })
        .to_string()
    }

    fn pi_assistant_line(model: &str, usage: Value, cost_total: f64, ts: &str) -> String {
        let mut usage_obj = usage;
        usage_obj["cost"] = json!({ "total": cost_total });
        json!({
            "type": "message",
            "id": "entry-1",
            "timestamp": ts,
            "message": {
                "role": "assistant",
                "model": model,
                "provider": "sensenova",
                "content": [{ "type": "text", "text": "回答" }],
                "usage": usage_obj
            }
        })
        .to_string()
    }

    #[test]
    fn pi_header_and_camel_case_usage() {
        let content = format!(
            "{}\n{}\n{}\n",
            pi_session_header(),
            pi_assistant_line(
                "deepseek-v4-flash",
                json!({ "input": 100, "output": 50, "cacheRead": 200, "cacheWrite": 0, "reasoning": 30 }),
                0.0,
                "2026-09-07T20:54:10.000Z"
            ),
            pi_assistant_line(
                "deepseek-v4-flash",
                json!({ "input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 }),
                1.5,
                "2026-09-07T20:55:10.000Z"
            )
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.cli_session_id, "pi-sess-1");
        assert_eq!(s.project.as_deref(), Some("/home/binblink"));
        assert_eq!(s.tokens.input, 110);
        assert_eq!(s.tokens.output, 55);
        assert_eq!(s.tokens.cache_read, 200);
        assert_eq!(s.tokens.reasoning, 30);
        assert_eq!(s.models[0].messages, 2);
        assert_eq!(s.cost_total, Some(1.5)); // 逐消息累计量，最后一条为准
        assert_eq!(s.started_at, Some(1_788_814_442_063));
    }

    #[test]
    fn pi_tool_result_and_user_events() {
        let content = format!(
            "{}\n{}\n{}\n",
            pi_session_header(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:05.000Z",
                "message": { "role": "user", "content": { "type": "text", "text": "修复 bug" } }
            })
            .to_string(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:30.000Z",
                "message": {
                    "role": "toolResult",
                    "toolName": "bash",
                    "toolCallId": "c1",
                    "isError": false,
                    "content": [{ "type": "text", "text": "done" }]
                }
            })
            .to_string()
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.title.as_deref(), Some("修复 bug"));
        let roles: Vec<&str> = s.events.iter().map(|e| e.role).collect();
        assert_eq!(roles, vec!["user", "tool"]);
        assert!(s.events[1].text.starts_with("bash ·"));
        // A4：非错误结果不标记 error，toolCallId 保留为配对键
        assert!(!s.events[1].error);
        assert_eq!(s.events[1].tool_use_id.as_deref(), Some("c1"));
    }

    /// A1：pi assistant toolCall 块不再丢——事件文本含工具名与参数摘要
    #[test]
    fn pi_tool_call_blocks_shown_with_args() {
        let content = format!(
            "{}\n{}\n",
            pi_session_header(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:10.000Z",
                "message": {
                    "role": "assistant",
                    "model": "deepseek-v4-flash",
                    "content": [
                        { "type": "thinking", "thinking": "看看文件在哪里" },
                        { "type": "toolCall", "id": "call_aa", "name": "read",
                          "arguments": { "path": "bedcode-mobile/docs/code-map.md" } }
                    ]
                }
            })
            .to_string()
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.events.len(), 1);
        assert!(
            s.events[0].text.contains("tool_use · read"),
            "toolCall 应出现（A1 丢失修复）: {}",
            s.events[0].text
        );
        assert!(
            s.events[0].text.contains("code-map.md"),
            "参数摘要应可见: {}",
            s.events[0].text
        );
    }

    /// A4①：pi toolResult isError=true → error 标记；图片块占位不丢
    #[test]
    fn pi_tool_result_error_flagged_and_image_placeholder() {
        let content = format!(
            "{}\n{}\n",
            pi_session_header(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:30.000Z",
                "message": {
                    "role": "toolResult",
                    "toolName": "bash",
                    "toolCallId": "c2",
                    "isError": true,
                    "content": [
                        { "type": "text", "text": "command not found" },
                        { "type": "image", "format": "png", "source": "data:image/png;base64,AAA=" }
                    ]
                }
            })
            .to_string()
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.events.len(), 1);
        assert!(s.events[0].error, "isError=true 应标记 error");
        assert!(
            s.events[0].text.contains("[non-text:image]"),
            "非 text 块占位: {}",
            s.events[0].text
        );
        assert!(s.events[0].text.contains("command not found"));
    }

    /// A7 对称：pi user 超长正文截断（与 claude 同口径）
    #[test]
    fn pi_long_user_text_truncated() {
        let long = "很".repeat(2500);
        let content = format!(
            "{}\n{}\n",
            pi_session_header(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:10.000Z",
                "message": { "role": "user", "content": { "type": "text", "text": long } }
            })
            .to_string()
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].text.chars().count(), 2000 + 1);
        assert!(s.events[0].text.ends_with('…'));
    }

    #[test]
    fn pi_missing_usage_is_tolerated() {
        let content = format!(
            "{}\n{}\n",
            pi_session_header(),
            json!({
                "type": "message",
                "timestamp": "2026-09-07T20:54:10.000Z",
                "message": {
                    "role": "assistant",
                    "model": "m",
                    "content": [{ "type": "text", "text": "hi" }]
                }
            })
            .to_string()
        );
        let s = parse_pi_session(&content);
        assert_eq!(s.tokens, TokenUsage::default());
        assert!(s.models.is_empty());
        // 无 usage 的助手事件不携带 token 明细
        assert_eq!(s.events.len(), 1);
        assert!(s.events[0].tokens.is_none());
    }

    // ==================== 通用：截断 / 大量行 / 主导模型 ====================

    #[test]
    fn pi_truncated_last_line_skipped() {
        let mut content = pi_session_header();
        content.push_str("\n{\"type\":\"message\",\"message\":{\"role\":\"user\","); // 截断
        let s = parse_pi_session(&content);
        assert_eq!(s.cli_session_id, "pi-sess-1");
        assert_eq!(s.skipped_lines, 1);
    }
    #[test]
    fn large_session_streams_within_event_cap() {
        // 6000 轮助手消息：聚合完整，事件流截断到 MAX_EVENTS
        let mut content = format!("{}\n", pi_session_header());
        for i in 0..6000 {
            content.push_str(&pi_assistant_line(
                "deepseek-v4-flash",
                json!({ "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 }),
                0.0,
                "2026-09-07T20:54:10.000Z",
            ));
            content.push('\n');
            let _ = i;
        }
        let s = parse_pi_session(&content);
        assert_eq!(s.tokens.input, 6000);
        assert_eq!(s.tokens.output, 6000);
        assert_eq!(s.models[0].messages, 6000);
        assert_eq!(s.events.len(), MAX_EVENTS);
        assert!(s.events_truncated);
    }
    #[test]
    fn dominant_model_prefers_highest_output() {
        let content = format!(
            "{}\n{}\n",
            pi_assistant_line(
                "m-input-heavy",
                json!({ "input": 5000, "output": 1 }),
                0.0,
                "2026-09-07T20:54:10.000Z"
            ),
            pi_assistant_line(
                "deepseek-v4-flash",
                json!({ "input": 1, "output": 999 }),
                0.0,
                "2026-09-07T20:54:11.000Z"
            )
        );
        let s = parse_pi_session(&content);
        // 两个模型各一条；输出量大的为主导（models 保持首次出现序）
        assert_eq!(s.models.len(), 2);
        assert_eq!(s.dominant_model(), Some("deepseek-v4-flash"));
    }
}
