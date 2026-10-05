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

// ==================== Tests ====================

// 用例按功能拆至 `pi/tests/`（本内联模块的子模块路径由 rustc
// 自动解析到该目录；模块树 `usage_parse::pi::tests::<文件>` 与内联形态等价，私有项可见性不受影响）。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_parse::MAX_EVENTS;
    use serde_json::{json, Value};
    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）

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
    mod pi_header_and_camel_case;
    mod pi_truncated_last_line;
}
