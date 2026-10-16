//! claude JSONL 适配器：聚合 + 事件流
//!
//! 去重语义：同一 assistant message（message.id）按内容块拆多行且每行重复
//! 完整 usage，按 id 首现计入（无 usage / 无 id 的行只计事件）。token 字段
//! snake_case（`message.usage.input_tokens` 等）；`type=cost-state` 行含真实
//! `totalCostUSD`（最后一条为准，有则存不估算）。

use super::common::{
    as_i64, extract_text, parse_line, push_attachment_event, push_event, tool_args_summary,
    tool_result_text, truncate_text,
};
use super::time::parse_iso8601_ms;
use super::types::{NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_TOOL, ROLE_USER};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

/// 工具调用参数/输入摘要的字符上限（README 审计 A1/A3：≤120–200，取 120）
const TOOL_ARGS_CAP: usize = 120;

/// 未配对 tool_use 的在途上限（内存护栏）
///
/// 会话被中断 / 日志被截断时 tool_result 不会回来，这些 id 会一直留在配对表里。
/// 上限只影响工具名展示（丢最旧的不改变任何事件的正确性），但保证内存有界。
const MAX_PENDING_TOOLS: usize = 256;

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
    // tool_use → tool_result 配对表（按 tool_use_id 消费）
    //
    // 用 HashMap 而非 Vec：会话被中断 / 日志被截断时，未配对的 tool_use 会**永久残留**
    // （无回收点），Vec 的 `iter().any` 去重 + `position()+remove()` 配对都退化为 O(n)，
    // n = 未配对调用数 → 整体 O(n²)，在 wasm guest 里对长会话是实打实的 CPU 占用。
    // 额外加上限：超过 `MAX_PENDING_TOOLS` 时丢弃最旧的未配对项（工具名只是展示用
    // 修饰，丢最旧的不影响正确性，但保证内存有界）。
    let mut pending_tools: HashMap<String, String> = HashMap::new();
    let mut pending_order: VecDeque<String> = VecDeque::new();

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
                // 收集本消息的 tool_use id → 名称（供后续 tool_result 配对）
                if let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) {
                    for b in blocks {
                        if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            if let Some(id) = b.get("id").and_then(|i| i.as_str()) {
                                if !id.is_empty() && !pending_tools.contains_key(id) {
                                    let name = b
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("tool")
                                        .to_string();
                                    pending_tools.insert(id.to_string(), name);
                                    pending_order.push_back(id.to_string());
                                    // 有界：淘汰最旧的未配对项
                                    while pending_order.len() > MAX_PENDING_TOOLS {
                                        if let Some(oldest) = pending_order.pop_front() {
                                            pending_tools.remove(&oldest);
                                        }
                                    }
                                }
                            }
                        }
                    }
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
                let msg = v.get("message").cloned().unwrap_or(Value::Null);
                let is_meta = v.get("isMeta").and_then(|m| m.as_bool()).unwrap_or(false);
                if msg.get("content").map(|c| c.is_array()).unwrap_or(false) {
                    // tool_result 数组块：归一为工具事件（is_error / 配对 / 非 text 占位）
                    for block in msg["content"].as_array().unwrap() {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                            let text =
                                tool_result_text(block.get("content").unwrap_or(&Value::Null));
                            let tid = block
                                .get("tool_use_id")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            // 配对：命中同 id 的 tool_use 取回工具名
                            let tool_name = if tid.is_empty() {
                                None
                            } else {
                                pending_tools.remove(&tid)
                            };
                            let err = block
                                .get("is_error")
                                .and_then(|e| e.as_bool())
                                .unwrap_or(false);
                            let text = match tool_name {
                                Some(name) => {
                                    format!("tool_result · {name} · {}", truncate_text(&text, 1000))
                                }
                                None => format!("tool_result · {}", truncate_text(&text, 1000)),
                            };
                            push_event(
                                &mut session,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_TOOL,
                                    text,
                                    model: None,
                                    tokens: None,
                                    error: err,
                                    tool_use_id: (!tid.is_empty()).then_some(tid),
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
                        &mut session,
                        NormalizedEvent {
                            ts,
                            role,
                            // A7：user 正文与 assistant 同口径截断（超长 user 消息
                            // 直入事件流有撑爆 WATM 序列化风险；标题仍取原文前 120）
                            text: truncate_text(&text, 2000),
                            model: None,
                            tokens: None,
                            error: false,
                            tool_use_id: None,
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
            "cost-state" => {
                if let Some(cost) = v.get("totalCostUSD").and_then(|c| c.as_f64()) {
                    session.cost_total = Some(cost);
                }
            }
            // attachment（A5）：读过/注入的文件、工具清单、提醒等——归一为 system
            // 条目（文件名/类型），不进标题与 token 聚合；text 只取短摘要，防撑爆
            "attachment" => {
                if let Some(att) = v.get("attachment") {
                    let kind = att.get("type").and_then(|t| t.as_str()).unwrap_or("file");
                    let note = att
                        .get("filename")
                        .and_then(|f| f.as_str())
                        .or_else(|| att.get("prompt").and_then(|p| p.as_str()))
                        .or_else(|| att.get("newDate").and_then(|d| d.as_str()));
                    let text = match note {
                        Some(n) => format!("attachment · {kind} · {}", truncate_text(n, 120)),
                        None => format!("attachment · {kind}"),
                    };
                    // 走**独立**附件上限：附件噪音不得挤掉后续实质消息
                    push_attachment_event(
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
            }
            // summary / mode / file-history-* 等元数据行不进事件流
            _ => {}
        }
    }

    session.title = title;
    session.project = project;
    session
}

/// 助手事件展示文本：text 块拼接；tool_use / toolCall 块给「工具名 + 参数摘要」。
/// 实机事实（README 审计 A1/A3）：claude 拼写 `tool_use`（input 对象）；pi 拼写
/// `toolCall`（arguments 对象）——两个令牌都命中，参数摘要 ≤120 字符不撑爆。
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
                    // claude: tool_use.input 对象；pi: toolCall.arguments 对象
                    Some(kind @ ("tool_use" | "toolCall")) => {
                        let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                        let args = if kind == "tool_use" {
                            tool_args_summary(b.get("input"), TOOL_ARGS_CAP)
                        } else {
                            tool_args_summary(b.get("arguments"), TOOL_ARGS_CAP)
                        };
                        if args.is_empty() {
                            parts.push(format!("tool_use · {name}"));
                        } else {
                            parts.push(format!("tool_use · {name} · {args}"));
                        }
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

    /// 接线契约：claude 的 attachment 行必须走**独立**上限入口
    ///
    /// 上面 `common::attachment_events_do_not_evict_substantive_events` 只锁 helper
    /// 本身；若 claude 适配器把 attachment 行改回走 `push_event`（共享额度），helper
    /// 测试照样绿——故这里走完整解析路径锁接线。
    /// 变异探针：`push_attachment_event` → `push_event` → 末尾实质消息消失 → 转红。
    #[test]
    fn claude_attachment_lines_do_not_evict_later_messages() {
        use crate::usage_parse::MAX_ATTACHMENT_EVENTS;
        let mut content = String::new();
        for i in 0..(MAX_ATTACHMENT_EVENTS + 20) {
            content.push_str(
                &json!({
                    "type": "attachment",
                    "timestamp": "2026-09-04T01:10:00Z",
                    "cwd": "/home/u/proj",
                    "sessionId": "sess-1",
                    "attachment": { "type": "file", "filename": format!("f{i}.rs") }
                })
                .to_string(),
            );
            content.push('\n');
        }
        // 附件之后仍有实质 user 消息（必须是它，而不是附件，占住尾部）
        content.push_str(
            &json!({
                "type": "user",
                "timestamp": "2026-09-04T01:20:00Z",
                "cwd": "/home/u/proj",
                "sessionId": "sess-1",
                "message": { "content": "收尾的实质消息" }
            })
            .to_string(),
        );

        let s = parse_claude_session(&content);

        let last = s.events.last().expect("事件流不得为空");
        assert_eq!(
            last.text, "收尾的实质消息",
            "附件塞满上限后，紧随其后的实质消息必须仍在事件流尾部"
        );
        assert_eq!(
            s.attachment_events, MAX_ATTACHMENT_EVENTS as u32,
            "附件入流数必须受独立上限约束"
        );
    }

    /// 未配对 tool_use 的在途上限：内存护栏，且只丢最旧的（工具名只是展示修饰）
    ///
    /// 会话被中断 / 日志截断时 tool_result 不会回来，这些 id 永不回收。
    /// 变异探针：去掉上限 → 最早的 id 仍能配出工具名 → 本用例转红。
    #[test]
    fn unmatched_tool_use_queue_is_bounded_and_evicts_oldest() {
        let mut content = String::new();
        for i in 0..(MAX_PENDING_TOOLS + 8) {
            content.push_str(&json!({
                "type": "assistant",
                "timestamp": "2026-09-04T01:10:00Z",
                "cwd": "/home/u/proj",
                "sessionId": "sess-1",
                "message": {
                    "id": format!("m{i}"),
                    "model": "claude-x",
                    "content": [{ "type": "tool_use", "id": format!("call_{i}"), "name": format!("T{i}") }],
                    "usage": { "input_tokens": 1, "output_tokens": 1 }
                }
            }).to_string());
            content.push('\n');
        }
        // 最旧的 call_0 早已被淘汰 → 结果里配不出工具名（只剩类型前缀）
        content.push_str(&json!({
            "type": "user",
            "timestamp": "2026-09-04T01:11:00Z",
            "sessionId": "sess-1",
            "message": {
                "content": [{ "type": "tool_result", "tool_use_id": "call_0", "content": [{ "type": "text", "text": "旧结果" }] }]
            }
        }).to_string());
        content.push('\n');
        // 在途内的最新一个仍可配对
        let newest = MAX_PENDING_TOOLS + 7;
        content.push_str(&json!({
            "type": "user",
            "timestamp": "2026-09-04T01:12:00Z",
            "sessionId": "sess-1",
            "message": {
                "content": [{ "type": "tool_result", "tool_use_id": format!("call_{newest}"), "content": [{ "type": "text", "text": "新结果" }] }]
            }
        }).to_string());

        let s = parse_claude_session(&content);
        let oldest = s
            .events
            .iter()
            .find(|e| e.text.contains("旧结果"))
            .expect("旧结果事件");
        assert!(
            oldest.text.starts_with("tool_result · "),
            "被淘汰的 id 不得配出工具名，实际={}",
            oldest.text
        );
        assert!(
            !oldest.text.contains(&format!("T0 ·")),
            "上限内的最早项不该还在表里"
        );
        let fresh = s
            .events
            .iter()
            .find(|e| e.text.contains("新结果"))
            .expect("新结果事件");
        assert!(
            fresh.text.contains(&format!("T{newest} ·")),
            "在途上限内的 tool_use 必须仍能配出工具名，实际={}",
            fresh.text
        );
    }

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

    // ==================== 解析层缺陷修复夹具（审计 2026-10-03 A1/A3/A4/A5/A7） ====================

    /// A3：tool_use 参数（input）必须进事件文本——只显示工具名等于丢信息
    #[test]
    fn claude_tool_use_shows_args_summary() {
        let content = json!({
            "type": "assistant",
            "timestamp": "2026-09-04T01:10:00Z",
            "sessionId": "sess-1",
            "message": {
                "id": "m1",
                "model": "claude-x",
                "content": [{
                    "type": "tool_use", "id": "call_a", "name": "Bash",
                    "input": { "command": "cargo check", "description": "check warnings" }
                }]
            }
        })
        .to_string();
        let s = parse_claude_session(&content);
        assert_eq!(s.events.len(), 1);
        assert!(
            s.events[0].text.contains("tool_use · Bash"),
            "事件文本应含工具名: {}",
            s.events[0].text
        );
        assert!(
            s.events[0].text.contains("cargo check"),
            "input 关键字段应可见: {}",
            s.events[0].text
        );
    }

    /// A3 边界：超长 input 被截断护栏夹住，不撑爆序列化
    #[test]
    fn claude_tool_use_truncates_oversized_args() {
        let long_cmd = "x".repeat(500);
        let content = json!({
            "type": "assistant",
            "timestamp": "2026-09-04T01:10:00Z",
            "sessionId": "sess-1",
            "message": {
                "id": "m1", "model": "claude-x",
                "content": [{
                    "type": "tool_use", "id": "call_a", "name": "Bash",
                    "input": { "command": long_cmd }
                }]
            }
        })
        .to_string();
        let s = parse_claude_session(&content);
        assert!(
            s.events[0].text.contains("tool_use · Bash"),
            "{}。超长参数仍应显示工具名",
            s.events[0].text
        );
        // 摘要段被截断护栏夹住（120 字符 + 省略号），不撑爆序列化
        let args_part = s.events[0]
            .text
            .split("tool_use · Bash · ")
            .nth(1)
            .unwrap_or("");
        assert!(
            args_part.chars().count() <= 121,
            "参数摘要超上限: {} chars",
            args_part.chars().count()
        );
        assert!(args_part.ends_with('…'));
    }

    /// A4①④：is_error 标记 + 非 text 块占位 + tool_result 配对工具名 + tool_use_id
    #[test]
    fn claude_tool_result_marks_error_keeps_id_and_placeholder() {
        let content = format!(
            "{}\n{}\n",
            json!({
                "type": "assistant",
                "timestamp": "2026-09-04T01:10:00Z",
                "sessionId": "sess-1",
                "message": {
                    "id": "m1", "model": "claude-x",
                    "content": [{
                        "type": "tool_use", "id": "call_a", "name": "Bash",
                        "input": { "command": "ls" }
                    }]
                }
            })
            .to_string(),
            json!({
                "type": "user",
                "timestamp": "2026-09-04T01:11:00Z",
                "sessionId": "sess-1",
                "message": {
                    "role": "user",
                    "content": [{
                        "type": "tool_result", "tool_use_id": "call_a", "is_error": true,
                        "content": [
                            { "type": "text", "text": "exit 2" },
                            { "type": "image", "source": { "type": "base64", "data": "AAAA" } }
                        ]
                    }]
                }
            })
            .to_string()
        );
        let s = parse_claude_session(&content);
        assert_eq!(s.events.len(), 2);
        // 配对：tool_result 事件文本带工具名（先后序栈），id 保留
        assert_eq!(s.events[1].role, "tool");
        assert!(
            s.events[1].text.starts_with("tool_result · Bash ·"),
            "结果应带有配对工具名: {}",
            s.events[1].text
        );
        assert!(s.events[1].text.contains("exit 2"));
        // 图片块不消失，有占位
        assert!(
            s.events[1].text.contains("[non-text:image]"),
            "非 text 块应给占位: {}",
            s.events[1].text
        );
        // is_error → error 标记
        assert!(s.events[1].error, "is_error=true 应标记 error");
        assert_eq!(s.events[1].tool_use_id.as_deref(), Some("call_a"));
        // 正常结果的 error 为 false（对照）
        let ok_content = json!({
            "type": "user",
            "timestamp": "2026-09-04T01:12:00Z",
            "sessionId": "sess-1",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result", "tool_use_id": "call_unknown",
                    "content": [{ "type": "text", "text": "ok" }]
                }]
            }
        })
        .to_string();
        // 未配对 id（无对应 tool_use）：不炸、文本仍带 tool_result 前缀
        let s2 = parse_claude_session(&ok_content);
        assert!(!s2.events[0].error);
        assert!(s2.events[0].text.starts_with("tool_result"));
        assert_eq!(s2.events[0].tool_use_id.as_deref(), Some("call_unknown"));
    }

    /// A5：attachment 行归一为 system 附件条目，不进标题与聚合
    #[test]
    fn claude_attachment_lines_become_system_entries() {
        let content = format!(
            "{}\n{}\n{}",
            json!({
                "type": "attachment", "timestamp": "2026-09-04T01:00:00Z",
                "sessionId": "sess-1",
                "attachment": { "type": "edited_text_file", "filename": "/p/biometric.rs" }
            })
            .to_string(),
            json!({
                "type": "user", "timestamp": "2026-09-04T01:01:00Z",
                "sessionId": "sess-1",
                "message": { "role": "user", "content": "修复编译警告" }
            })
            .to_string(),
            json!({
                "type": "attachment", "timestamp": "2026-09-04T01:02:00Z",
                "sessionId": "sess-1",
                "attachment": { "type": "skill_listing" }
            })
            .to_string()
        );
        let s = parse_claude_session(&content);
        // 两行 attachment → 两条 system 事件
        let systems: Vec<_> = s.events.iter().filter(|e| e.role == ROLE_SYSTEM).collect();
        assert_eq!(systems.len(), 2);
        assert!(systems[0]
            .text
            .contains("attachment · edited_text_file · /p/biometric.rs"));
        assert!(systems[1].text.contains("attachment · skill_listing"));
        // 标题仍来自真实 user 消息（attachment 不参与）
        assert_eq!(s.title.as_deref(), Some("修复编译警告"));
        // 统计口径不受污染：无 token / 无模型条目
        assert_eq!(s.tokens, TokenUsage::default());
        assert!(s.models.is_empty());
    }

    /// A7：超长 user 正文截断到 2000（与 assistant 同口径），标题仍取原文前 120
    #[test]
    fn claude_long_user_text_truncated_title_kept() {
        let long = "长".repeat(3000);
        let content = json!({
            "type": "user", "timestamp": "2026-09-04T01:00:00Z",
            "sessionId": "sess-1",
            "message": { "role": "user", "content": long }
        })
        .to_string();
        let s = parse_claude_session(&content);
        assert_eq!(s.events.len(), 1);
        assert_eq!(s.events[0].role, ROLE_USER);
        // 事件文本被截断（2000 字符 + 省略号），不整包进流
        assert_eq!(s.events[0].text.chars().count(), 2000 + 1);
        assert!(s.events[0].text.ends_with('…'));
        // 标题仍取截断前原文前 120（既有 truncate 语义：120 字符 + 省略号）
        let title = s.title.expect("标题应存在");
        assert_eq!(title.chars().count(), 121);
        assert!(title.starts_with("长".repeat(120).as_str()));
        assert!(title.ends_with('…'));
    }

    /// A1：pi 的 toolCall 块（assistant_display_text 复用到 pi）也命中
    #[test]
    fn claude_shared_display_handles_pi_tool_call_blocks() {
        let content = serde_json::json!([
            { "type": "text", "text": "先看路径" },
            { "type": "toolCall", "id": "call_1", "name": "read",
              "arguments": { "path": "bedcode-mobile/docs/code-map.md" } }
        ]);
        let text = assistant_display_text(Some(&content));
        assert!(
            text.contains("tool_use · read ·") && text.contains("code-map.md"),
            "toolCall 块应显示工具名+参数: {}",
            text
        );
    }
}
