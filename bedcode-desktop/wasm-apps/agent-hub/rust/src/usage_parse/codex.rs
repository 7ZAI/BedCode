//! codex 适配器（官方 rollout 格式，预留骨架）
//!
//! 每行 `{timestamp, type, payload}`；计费数据在 `event_msg.payload.type ==
//! "token_count"` 的 `info.last_token_usage`（**该块含缓存部分**，须拆出
//! `cached_input_tokens`，codex 无缓存写入恒 0）。模型名来自最近一次
//! `turn_context`/`session_meta` 的 `payload.model`——token_count 事件自身
//! 不带模型，故解析是有状态的。本机未初始化过 codex 会话，格式依据官方
//! rollout 文档实现（实机初始化后校准，见母 spec §9 / §10 待办 3）。

use super::common::{as_i64, parse_line, push_event, truncate_text};
use super::time::parse_iso8601_ms;
use super::types::{NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_TOOL, ROLE_USER};
use serde_json::Value;

// ==================== codex 适配器（官方 rollout 格式） ====================

/// codex 助手事件的工具/推理摘要上限（官方 `function_call_output` 逐字回传
/// 命令输出，可达数万字符）
const CODEX_TOOL_TEXT_CAP: usize = 1000;

/// codex rollout JSONL 适配器：聚合 + 事件流（**预留骨架**）
///
/// 官方格式（`~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`）每行
/// `{"timestamp": RFC3339, "type": <RolloutItem 变体>, "payload": {...}}`：
/// - `session_meta`：`payload.id`（线程 UUID）、`payload.cwd`、`payload.timestamp`；
/// - `turn_context`：`payload.model`、`payload.cwd`——**模型名的唯一来源**
///   （`token_count` 事件自身不带模型，故 `current_model` 为有状态游标）；
/// - `event_msg.payload.type == "token_count"`：`info.last_token_usage` 是本轮
///   增量（`total_token_usage` 是累计，**不取**，否则重复计数）。其
///   `input_tokens` **含缓存部分**，须减出 `cached_input_tokens` 归入
///   `cache_read`；codex 无缓存写入（恒 0）。`info` 可为 null（本轮首次事件）。
/// - `event_msg.payload.type` ∈ `user_message` / `agent_message`：TUI 面向的
///   权威消息面（`response_item` 的 user 消息含环境上下文等合成注入，优先取此处）。
/// - `response_item.payload.type` ∈ `message` / `function_call(_output)` /
///   `custom_tool_call(_output)`：回退取消息与工具事件。
///
/// 未知变体一律跳过（Codex 每版本都在加新事件），坏行计入 `skipped_lines`。
pub(crate) fn parse_codex_session(content: &str) -> ParsedSession {
    let mut session = ParsedSession::default();
    let mut current_model: Option<String> = None;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(v) = parse_line(line) else {
            session.skipped_lines += 1;
            continue;
        };
        let ts = v
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso8601_ms);
        if let Some(t) = ts {
            session.started_at = Some(session.started_at.map_or(t, |cur| cur.min(t)));
            session.ended_at = Some(session.ended_at.map_or(t, |cur| cur.max(t)));
        }
        let payload = v.get("payload").unwrap_or(&Value::Null);

        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "session_meta" => {
                if let Some(id) = payload.get("id").and_then(|i| i.as_str()) {
                    session.cli_session_id = id.to_string();
                }
                if let Some(cwd) = payload.get("cwd").and_then(|c| c.as_str()) {
                    session.project = Some(cwd.to_string());
                }
                if let Some(m) = payload.get("model").and_then(|m| m.as_str()) {
                    current_model = Some(m.to_string());
                }
            }
            // turn_context 携带当前轮模型（token_count 靠它归模型）
            "turn_context" => {
                if let Some(m) = payload.get("model").and_then(|m| m.as_str()) {
                    current_model = Some(m.to_string());
                }
                if session.project.is_none() {
                    session.project = payload
                        .get("cwd")
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string());
                }
            }
            "event_msg" => {
                match payload.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "token_count" => {
                        if let Some(usage) = codex_last_token_usage(payload) {
                            let model = current_model.clone().unwrap_or_else(|| "unknown".into());
                            session.record_assistant_usage(&model, &usage);
                        }
                    }
                    // TUI 面向的权威用户消息面（response_item 的 user 消息含
                    // 环境上下文等合成注入，不作会话标题来源）
                    "user_message" => {
                        if let Some(text) = payload
                            .get("message")
                            .and_then(|m| m.as_str())
                            .or_else(|| payload.get("text").and_then(|t| t.as_str()))
                        {
                            if session.title.is_none() {
                                session.title = Some(truncate_text(text.trim(), 120));
                            }
                            push_event(
                                &mut session,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_USER,
                                    text: truncate_text(text.trim(), 2000),
                                    model: None,
                                    tokens: None,
                                    error: false,
                                    tool_use_id: None,
                                },
                            );
                        }
                    }
                    "agent_message" => {
                        if let Some(text) = payload
                            .get("message")
                            .and_then(|m| m.as_str())
                            .or_else(|| payload.get("text").and_then(|t| t.as_str()))
                        {
                            push_event(
                                &mut session,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_ASSISTANT,
                                    text: truncate_text(text.trim(), 2000),
                                    model: current_model.clone(),
                                    tokens: None,
                                    error: false,
                                    tool_use_id: None,
                                },
                            );
                        }
                    }
                    "exec_command_end" => {
                        let cmd = payload
                            .get("command")
                            .and_then(|c| c.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default();
                        let out = payload
                            .get("aggregated_output")
                            .and_then(|o| o.as_str())
                            .or_else(|| payload.get("stdout").and_then(|o| o.as_str()))
                            .unwrap_or("");
                        push_event(
                            &mut session,
                            NormalizedEvent {
                                ts,
                                role: ROLE_TOOL,
                                text: format!(
                                    "exec · {cmd} · {}",
                                    truncate_text(out.trim(), CODEX_TOOL_TEXT_CAP)
                                ),
                                model: None,
                                tokens: None,
                                error: false,
                                tool_use_id: None,
                            },
                        );
                    }
                    // 未知 event 变体（Codex 每版本新增）静默跳过
                    _ => {}
                }
            }
            // response_item 是回退面：event_msg 已有权威消息时它会被去重
            "response_item" => {
                if let Some((role, text, model, tool_use_id)) = codex_response_item_event(payload) {
                    push_event(
                        &mut session,
                        NormalizedEvent {
                            ts,
                            role,
                            text,
                            model,
                            tokens: None,
                            error: false,
                            tool_use_id,
                        },
                    );
                }
            }
            // session_state / compacted：结构事件，不进事件流
            _ => {}
        }
    }

    // 标题兜底：无 user_message 事件时取首条 assistant 正文
    if session.title.is_none() {
        session.title = session
            .events
            .iter()
            .find(|e| e.role == ROLE_USER || e.role == ROLE_ASSISTANT)
            .map(|e| truncate_text(e.text.trim(), 120))
            .filter(|t| !t.is_empty());
    }
    session
}

/// codex `event_msg.payload.info.last_token_usage` → 归一 token 明细。
/// `info` 为 null（本轮首次事件）或全零（限流重发）时返回 None——不计入聚合。
fn codex_last_token_usage(payload: &Value) -> Option<TokenUsage> {
    let last = payload.get("info")?.get("last_token_usage")?;
    if !last.is_object() {
        return None;
    }
    let input = as_i64(last.get("input_tokens"));
    let cached = as_i64(last.get("cached_input_tokens")).max(0);
    let usage = TokenUsage {
        // input_tokens 含缓存，减出后归入 cache_read（与 OpenAI 口径一致）
        input: (input - cached).max(0),
        output: as_i64(last.get("output_tokens")),
        cache_read: cached,
        // codex 无缓存写入
        cache_write: 0,
        reasoning: as_i64(last.get("reasoning_output_tokens")),
    };
    (usage.input != 0 || usage.output != 0 || usage.cache_read != 0 || usage.reasoning != 0)
        .then_some(usage)
}

/// codex `response_item.payload` → (role, text, model, tool_use_id)；无正文返回 None
fn codex_response_item_event(
    payload: &Value,
) -> Option<(&'static str, String, Option<String>, Option<String>)> {
    let call_id = payload
        .get("call_id")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string());
    match payload.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "message" => {
            let role = match payload.get("role").and_then(|r| r.as_str()).unwrap_or("") {
                "user" => ROLE_USER,
                "assistant" => ROLE_ASSISTANT,
                "developer" => ROLE_SYSTEM,
                _ => return None,
            };
            let text = codex_content_text(payload.get("content"))?;
            Some((role, truncate_text(text.trim(), 2000), None, None))
        }
        "function_call" | "custom_tool_call" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            // arguments / input 是**字符串**（模型原文），不可当 JSON 解析
            let args = payload
                .get("arguments")
                .or_else(|| payload.get("input"))
                .and_then(|a| a.as_str())
                .unwrap_or("");
            Some((
                ROLE_TOOL,
                format!(
                    "tool · {name} · {}",
                    truncate_text(args.trim(), CODEX_TOOL_TEXT_CAP)
                ),
                None,
                call_id,
            ))
        }
        "function_call_output" | "custom_tool_call_output" => {
            let out = payload.get("output").and_then(|o| o.as_str()).unwrap_or("");
            Some((
                ROLE_TOOL,
                format!(
                    "tool_result · {}",
                    truncate_text(out.trim(), CODEX_TOOL_TEXT_CAP)
                ),
                None,
                call_id,
            ))
        }
        // reasoning：加密密文不展示（渲染成乱码），summary 文本才有意义
        "reasoning" => {
            let text = codex_content_text(payload.get("summary"))
                .or_else(|| codex_content_text(payload.get("content")))?;
            Some((
                ROLE_SYSTEM,
                format!(
                    "reasoning · {}",
                    truncate_text(text.trim(), CODEX_TOOL_TEXT_CAP)
                ),
                None,
                None,
            ))
        }
        _ => None,
    }
}

/// codex 内容块数组 → 文本拼接（`input_text` / `output_text` / `text` 三种键名）
fn codex_content_text(content: Option<&Value>) -> Option<String> {
    let items = content?.as_array()?;
    let mut out = String::new();
    for item in items {
        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t);
        }
    }
    let t = out.trim();
    (!t.is_empty()).then(|| t.to_string())
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// codex rollout：session_meta 取材、turn_context 供模型、token_count 计费
    #[test]
    fn codex_session_aggregates_token_count() {
        let content = concat!(
            r#"{"timestamp":"2026-04-20T16:44:37.772Z","type":"session_meta","payload":{"id":"019dabc6-8fef-7681-a054-b5bb75fcb97d","cwd":"/Users/ben/oss/toolpath"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:38.000Z","type":"turn_context","payload":{"model":"gpt-5.4","cwd":"/Users/ben/oss/toolpath"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:40.000Z","type":"event_msg","payload":{"type":"user_message","message":"开工"}}"#,
            "\n",
            // info 为 null（本轮首次 token_count）→ 不计费
            r#"{"timestamp":"2026-04-20T16:44:41.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#,
            "\n",
            // input_tokens 含缓存：11980 = 9728 缓存 + 2252 非缓存
            r#"{"timestamp":"2026-04-20T16:45:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":99999,"output_tokens":9999},"last_token_usage":{"input_tokens":11980,"cached_input_tokens":9728,"output_tokens":269,"reasoning_output_tokens":41}}}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:45:01.000Z","type":"event_msg","payload":{"type":"agent_message","message":"好的"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:45:02.000Z","type":"event_msg","payload":{"type":"some_future_event","x":1}}"#,
            "\n",
        );
        let s = parse_codex_session(content);
        assert_eq!(s.cli_session_id, "019dabc6-8fef-7681-a054-b5bb75fcb97d");
        assert_eq!(s.project.as_deref(), Some("/Users/ben/oss/toolpath"));
        // 只取 last_token_usage（total 是累计，取了会重复计数）
        assert_eq!(s.tokens.input, 2_252);
        assert_eq!(s.tokens.cache_read, 9_728);
        assert_eq!(s.tokens.output, 269);
        assert_eq!(s.tokens.reasoning, 41);
        assert_eq!(s.tokens.cache_write, 0, "codex 无缓存写入");
        assert_eq!(s.dominant_model(), Some("gpt-5.4"));
        // 标题取权威 user_message
        assert_eq!(s.title.as_deref(), Some("开工"));
        // user + agent 两条事件；未知 event 变体被跳过而不 panic
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].role, ROLE_USER);
        assert_eq!(s.events[1].role, ROLE_ASSISTANT);
    }

    /// 回归见证：若改成取 total_token_usage，input 会变成 99999——本例会红
    #[test]
    fn codex_uses_last_token_usage_not_total() {
        let payload = json!({
            "type": "token_count",
            "info": {
                "total_token_usage": { "input_tokens": 99999, "output_tokens": 9999 },
                "last_token_usage": { "input_tokens": 100, "cached_input_tokens": 40, "output_tokens": 10 },
            },
        });
        let u = codex_last_token_usage(&payload).expect("usage");
        assert_eq!(u.input, 60);
        assert_eq!(u.output, 10);
    }

    /// 边界：全零用量（限流重发）不入聚合
    #[test]
    fn codex_zero_usage_token_count_ignored() {
        let payload = json!({
            "type": "token_count",
            "info": { "last_token_usage": { "input_tokens": 0, "output_tokens": 0 } },
        });
        assert_eq!(codex_last_token_usage(&payload), None);
    }

    /// 边界：模型切换（turn_context 出现在会话中段）时后续 token 归新模型
    #[test]
    fn codex_model_switch_moves_following_usage() {
        let content = concat!(
            r#"{"timestamp":"2026-04-20T16:44:37.772Z","type":"session_meta","payload":{"id":"t1","cwd":"/w"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:38.000Z","type":"turn_context","payload":{"model":"gpt-a"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:39.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:40.000Z","type":"turn_context","payload":{"model":"gpt-b"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:41.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"output_tokens":2}}}}"#,
            "\n",
        );
        let s = parse_codex_session(content);
        assert_eq!(s.models.len(), 2);
        let a = s.models.iter().find(|m| m.model == "gpt-a").expect("a");
        let b = s.models.iter().find(|m| m.model == "gpt-b").expect("b");
        assert_eq!(a.tokens.input, 10);
        assert_eq!(b.tokens.input, 20);
    }

    /// response_item 回退面：工具调用/输出与 assistant 消息
    #[test]
    fn codex_response_items_become_events() {
        let content = concat!(
            r#"{"timestamp":"2026-04-20T16:44:37.772Z","type":"session_meta","payload":{"id":"t2","cwd":"/w"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:38.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"我先看看"}]}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:39.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pwd\"}","call_id":"c1"}}"#,
            "\n",
            r#"{"timestamp":"2026-04-20T16:44:40.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"/w\n"}}"#,
            "\n",
            // 加密推理：不展示密文
            r#"{"timestamp":"2026-04-20T16:44:41.000Z","type":"response_item","payload":{"type":"reasoning","summary":[],"content":null,"encrypted_content":"gAAAAABp5lf5"}}"#,
            "\n",
        );
        let s = parse_codex_session(content);
        // reasoning 密文不产生事件（渲染成乱码）
        assert_eq!(s.events.len(), 3);
        assert_eq!(s.events[0].role, ROLE_ASSISTANT);
        assert_eq!(s.events[0].text, "我先看看");
        assert!(
            s.events[1].text.contains("exec_command"),
            "{}",
            s.events[1].text
        );
        assert!(
            s.events[2].text.contains("tool_result"),
            "{}",
            s.events[2].text
        );
        // 无 user_message 事件时标题兜底为首条正文
        assert_eq!(s.title.as_deref(), Some("我先看看"));
    }

    /// 边界：坏行计入 skipped_lines 且不中断（官方格式随版本演进）
    #[test]
    fn codex_bad_line_counted_not_fatal() {
        let content = "not json\n{\"timestamp\":\"2026-04-20T16:44:37.772Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"t3\"}}\n{broken\n";
        let s = parse_codex_session(content);
        assert_eq!(s.skipped_lines, 2);
        assert_eq!(s.cli_session_id, "t3", "坏行不能中断整文件解析");
    }
}
