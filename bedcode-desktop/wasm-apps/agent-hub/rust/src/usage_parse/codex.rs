//! codex 适配器（官方 rollout 格式）
//!
//! 每行 `{timestamp, ordinal, type, payload}`；计费数据在 `event_msg.payload.
//! type == "token_count"` 的 `info.last_token_usage`（**该块含缓存部分**，须
//! 拆出 `cached_input_tokens`，codex 无缓存写入恒 0）。模型名来自最近一次
//! `turn_context`/`session_meta` 的 `payload.model`——token_count 事件自身
//! 不带模型，故解析是有状态的。
//!
//! **两个并存的消息面（2026-10-06 实机校准，14 个 rollout / 994 行）**：
//! codex 现在把同一段对话写两遍，两面的语义**不同**，本适配器必须分工：
//!
//! | 面 | 记录形态 | 语义 |
//! | --- | --- | --- |
//! | **权威面** | `event_msg.payload.type == "item_completed"`，`payload.item.type` ∈ `UserMessage` / `AgentMessage` / `CommandExecution` / `Reasoning`（**PascalCase**） | 只写真实对话：真实用户提问、助手答复（含 `phase`）、命令执行（含 `aggregated_output` / `exit_code`）、推理摘要（`summary_text` 字符串数组） |
//! | 回退面 | `response_item.payload.type` ∈ `message` / `function_call(_output)` / `reasoning` | 权威面的**超集投影**：真实对话逐字重复，**外加每会话三条合成注入**（developer `<skills_instructions>`、user `# AGENTS.md instructions for …`、user `<environment_context>`） |
//!
//! 早期 rollout（无 `item_completed`）另有更老的一层 `event_msg.payload.type`
//! ∈ `user_message` / `agent_message` / `exec_command_end`，仍按原逻辑解析。
//!
//! 为什么必须分工而不是「两���都收」：权威面缺席时（本条记录的最初写法），
//! 会话标题与首屏全被合成注入占满——`# AGENTS.md instructions for …` 既是
//! 标题又是第一条「用户消息」，真实提问被挤到第三屏；且推理摘要整段丢失
//! （回退面的 `reasoning` 只有 `encrypted_content`，密文渲染成乱码）。
//!
//! 未知变体一律跳过（Codex 每版本都在加新事件），坏行计入 `skipped_lines`。
//! `token_usage_record` 是 `token_count` 的同源记账投影（实机 1:1），**不取**
//! ——两处都取会双倍计费；`world_state` 是结构快照，无展示价值。

use super::common::{as_i64, parse_line, push_event, truncate_text};
use super::time::parse_iso8601_ms;
use super::types::{NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_TOOL, ROLE_USER};
use serde_json::Value;
use std::collections::HashSet;

// ==================== codex 适配器（官方 rollout 格式） ====================

/// codex 助手事件的工具/推理摘要上限（官方 `function_call_output` 逐字回传
/// 命令输出，可达数万字符）
const CODEX_TOOL_TEXT_CAP: usize = 1000;

/// codex rollout JSONL 适配器：聚合 + 事件流
///
/// 官方格式（`~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`）每行
/// `{"timestamp": RFC3339, "ordinal": n, "type": <顶层变体>, "payload": {...}}`：
/// - `session_meta`：`payload.id`（线程 UUID）、`payload.cwd`、
///   `payload.timestamp`（**`base_instructions` 是整份系统提示词，实机每行
///   ~21KB**，故原始行视图首行即巨块——见前端逐行折叠）；
/// - `turn_context`：`payload.model`、`payload.cwd`——**模型名的唯一来源**
///   （`token_count` 事件自身不带模型，故 `current_model` 为有状态游标）；
/// - `event_msg.payload.type == "token_count"`：`info.last_token_usage` 是本轮
///   增量（`total_token_usage` 是累计，**不取**，否则重复计数）。其
///   `input_tokens` **含缓存部分**，须减出 `cached_input_tokens` 归入
///   `cache_read`；codex 无缓存写入（恒 0）。`info` 可为 null（本轮首次事件）。
/// - `event_msg.payload.type == "item_completed"`：**权威面**（见模块头），
///   也是会话标题的唯一可靠来源。
/// - `event_msg.payload.type` ∈ `user_message` / `agent_message` /
///   `exec_command_end`：更老一层的权威面。
/// - `response_item.payload.type` ∈ `message` / `function_call(_output)` /
///   `custom_tool_call(_output)` / `reasoning`：回退面，与权威面**去重**后
///   入流（见回退面结算）。
///
/// 未知变体一律跳过（Codex 每版本都在加新事件），坏行计入 `skipped_lines`。
pub(crate) fn parse_codex_session(content: &str) -> ParsedSession {
    let mut session = ParsedSession::default();
    let mut current_model: Option<String> = None;
    // 权威面覆盖到的调用 id：回退面里同 id 的 function_call / function_call_output
    // 是同一件事的重复投影（实机 CommandExecution.id === function_call.call_id）
    let mut item_call_ids: HashSet<String> = HashSet::new();
    let mut item_face_seen = false;
    // 回退面事件缓冲：权威面在场与否会改变它的语义（见模块头），文件读完
    // 才能定，故先缓冲、末尾结算
    let mut fallback: Vec<NormalizedEvent> = Vec::new();

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
                    // 权威面：真实对话本体（item.type 为 PascalCase，见模块头）
                    "item_completed" => {
                        if let Some((event, is_title_source, call_id)) =
                            codex_item_event(payload, current_model.as_deref())
                        {
                            item_face_seen = true;
                            if let Some(id) = call_id {
                                item_call_ids.insert(id);
                            }
                            // 标题只取**真实用户提问**（`UserMessage`）；助手正文
                            // 不作标题，否则用户还没说话就先看到一句助手问候
                            if is_title_source && session.title.is_none() {
                                session.title = Some(truncate_text(event.text.trim(), 120));
                            }
                            push_event(&mut session, event);
                        }
                    }
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
            // response_item 是回退面：先进缓冲，末尾按权威面在场与否结算
            "response_item" => {
                if let Some((role, text, model, tool_use_id, _)) =
                    codex_response_item_event(payload)
                {
                    fallback.push(NormalizedEvent {
                        ts,
                        role,
                        text,
                        model,
                        tokens: None,
                        error: false,
                        tool_use_id,
                    });
                }
            }
            // session_state / compacted / world_state / token_usage_record：
            // 结构事件，不进事件流（token_usage_record 与 token_count 同源，
            // 两处都取会双倍计费——见模块头）
            _ => {}
        }
    }

    // ==================== 回退面结算 ====================
    //
    // 权威面在场（当前 codex 版本）时，回退面只保留权威面没覆盖到的东西：
    // - `message` 变体整批丢：它与权威面逐字重复，且混着每会话三条合成注入
    //   （developer `<skills_instructions>` / user `# AGENTS.md instructions` /
    //   user `<environment_context>`）——不丢的话会话标题就是 AGENTS.md 开头。
    // - `function_call(_output)` 变体：call_id 命中权威面同一调用的丢弃；
    //   未命中的保留（apply_patch / write_stdin 等没有 CommandExecution 对应项，
    //   丢了整段工具调用就消失）。
    // - `reasoning` 变体保留（实机只有密文、本就不产生事件；保留是为了
    //   `summary` 有明文的形态不丢内容）。
    //
    // 权威面缺席（更老版本 rollout）时全部回填——那是当时唯一的消息面。
    if item_face_seen {
        fallback.retain(|e| {
            e.role != ROLE_USER
                && e.role != ROLE_ASSISTANT
                && e.role != ROLE_SYSTEM
                && !e
                    .tool_use_id
                    .as_deref()
                    .is_some_and(|id| item_call_ids.contains(id))
        });
    }
    session.events.append(&mut fallback);

    // 标题兜底：无权威 user 事件时取首条 assistant 正文（必须在回退面回填
    // 之后——老格式的标题只能取自回填进来的正文）
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

/// 权威面 `event_msg.payload.item`（`item_completed` 载荷）→ 归一事件；
/// item 变体未知或无正文时返回 None。
///
/// 返回三元组 `(event, title_source, call_id)`：
/// - `title_source`：该事件是否可作会话标题（仅真实用户提问）；
/// - `call_id`：`CommandExecution.id`——实机等于回退面 `function_call.call_id`，
///   结算时用它识别同一调用的重复投影。
///
/// item 变体是 **PascalCase**（`UserMessage` / `AgentMessage` /
/// `CommandExecution` / `Reasoning`），与回退面的 snake_case 不是一套命名。
/// `Reasoning.summary_text` 是**字符串数组**（回退面的 `summary` 是内容块
/// 数组，形态不同）——密文（`raw_content` / `encrypted_content`）不展示。
fn codex_item_event(
    payload: &Value,
    model: Option<&str>,
) -> Option<(NormalizedEvent, bool, Option<String>)> {
    let ts = payload
        .get("completed_at_ms")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            payload
                .get("timestamp")
                .and_then(|t| t.as_str())
                .and_then(parse_iso8601_ms)
        });
    let item = payload.get("item")?;
    let text_of = |key: &str| codex_content_text(item.get(key));
    let event = match item.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "UserMessage" => NormalizedEvent {
            ts,
            role: ROLE_USER,
            text: truncate_text(text_of("content")?.trim(), 2000),
            model: None,
            tokens: None,
            error: false,
            tool_use_id: None,
        },
        "AgentMessage" => NormalizedEvent {
            ts,
            role: ROLE_ASSISTANT,
            text: truncate_text(text_of("content")?.trim(), 2000),
            model: model.map(str::to_string),
            tokens: None,
            error: false,
            tool_use_id: None,
        },
        "CommandExecution" => {
            let cmd = item
                .get("command")
                .and_then(|c| c.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let out = item
                .get("aggregated_output")
                .and_then(|o| o.as_str())
                .or_else(|| item.get("formatted_output").and_then(|o| o.as_str()))
                .or_else(|| item.get("stdout").and_then(|o| o.as_str()))
                .unwrap_or("");
            NormalizedEvent {
                ts,
                role: ROLE_TOOL,
                text: format!(
                    "exec · {cmd} · {}",
                    truncate_text(out.trim(), CODEX_TOOL_TEXT_CAP)
                ),
                model: None,
                tokens: None,
                error: item.get("exit_code").and_then(|c| c.as_i64()).unwrap_or(0) != 0,
                tool_use_id: item.get("id").and_then(|i| i.as_str()).map(str::to_string),
            }
        }
        "Reasoning" => {
            // summary_text 是字符串数组；空数组（密文形态）不产生事件
            let joined = item
                .get("summary_text")
                .and_then(|s| s.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .filter(|s| !s.trim().is_empty())?;
            NormalizedEvent {
                ts,
                role: ROLE_SYSTEM,
                text: format!(
                    "reasoning · {}",
                    truncate_text(joined.trim(), CODEX_TOOL_TEXT_CAP)
                ),
                model: None,
                tokens: None,
                error: false,
                tool_use_id: None,
            }
        }
        _ => return None,
    };
    let is_user = event.role == ROLE_USER;
    let call_id = event.tool_use_id.clone();
    Some((event, is_user, call_id))
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

/// 回退面事件的来源变体（决定权威面在场时能否保留，见解析函数内的结算段）
#[derive(PartialEq, Eq, Debug)]
enum FallbackVariant {
    /// `response_item` 的 `message`：权威面在场时**整批丢弃**——它与权威面
    /// 逐字重复，且混着合成注入（developer `<skills_instructions>` /
    /// user `# AGENTS.md instructions` / user `<environment_context>`）。
    Message,
    /// `reasoning` / `function_call(_output)`：按 call_id 去重后保留
    /// （apply_patch / write_stdin 等没有权威面 CommandExecution 对应项）。
    Detail,
}

/// 回退面事件的归一形状 `(role, text, model, tool_use_id, 变体)`
type FallbackEvent = (
    &'static str,
    String,
    Option<String>,
    Option<String>,
    FallbackVariant,
);

/// codex `response_item.payload` → 归一五元组；无正文返回 None
fn codex_response_item_event(payload: &Value) -> Option<FallbackEvent> {
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
            Some((
                role,
                truncate_text(text.trim(), 2000),
                None,
                None,
                FallbackVariant::Message,
            ))
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
                FallbackVariant::Detail,
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
                FallbackVariant::Detail,
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
                FallbackVariant::Detail,
            ))
        }
        _ => None,
    }
}

/// codex 内容块数组 → 文本拼接
///
/// 只看块的 `text` 字段，不校验 `type`：块类型名在不同 item 上大小写不一
/// （实机 `UserMessage` 用 `text`，`AgentMessage` 用 `Text`，回退面
/// `message` 用 `input_text` / `output_text`），按 `text` 取值对三者一致。
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
// 用例按功能拆至 `codex/tests/`（本内联模块的子模块路径由 rustc 自动解析到
// 该目录；模块树 `usage_parse::codex::tests::<文件>` 与内联形态等价，私有项
// 可见性不受影响）。
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 回退面去重与老格式兼容
    mod fallback_face_dedup;
    /// 真实 rollout 形状夹具（2026-10-06 实机脱敏，见 `item_face_session`）
    mod item_face_session;

    /// 跨分组共享的实机 rollout 夹具（子模块经 `use super::*` 可见）
    ///
    /// 字段名逐字照抄本机 2026-10-04 的 rollout：顶层 `timestamp`/`ordinal`/
    /// `type`/`payload`；权威面 `payload.item.type` 是 **PascalCase**；
    /// `AgentMessage` 内容块是 `{"type":"Text"}` 而 `UserMessage` 是
    /// `{"type":"text"}`；`CommandExecution.id === function_call.call_id`。
    /// 一行一个 JSON，换行即 JSONL。
    fn rollout(lines: &[serde_json::Value]) -> String {
        lines
            .iter()
            .map(|l| match serde_json::to_string(l) {
                Ok(s) => s,
                Err(e) => panic!("fixture line not serializable: {e}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `event_msg` 包一层（权威面与旧权威面都走这个顶层变体）
    fn event_msg(payload: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.182Z",
            "ordinal": 7,
            "type": "event_msg",
            "payload": payload,
        })
    }

    /// `item_completed` 包一层（`item` 即权威面记录）
    fn item_completed(item: serde_json::Value) -> serde_json::Value {
        event_msg(serde_json::json!({
            "type": "item_completed",
            "thread_id": "01a1052b-9390-7c92-b2a8-cf340072b91a",
            "turn_id": "01a1052d-a208-7d12-bb9e-37d6bcc856d6",
            "item": item,
            "started_at_ms": 1791088239182i64,
            "completed_at_ms": 1791088239182i64,
        }))
    }

    /// `response_item` 包一层（回退面）
    fn response_item(payload: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.133Z",
            "ordinal": 2,
            "type": "response_item",
            "payload": payload,
        })
    }

    /// 每会话必写的合成注入三件套（developer 技能表 + user AGENTS.md + user
    /// 环境上下文）——它们只出现在**回退面**，正是标题被污染的源头。
    fn synthetic_injections() -> Vec<serde_json::Value> {
        vec![
            response_item(json!({ "type": "message", "role": "developer",
                "content": [{ "type": "input_text", "text": "<skills_instructions>\n## Skills\n…</skills_instructions>" }] })),
            response_item(json!({ "type": "message", "role": "user",
                "content": [{ "type": "input_text", "text": "# AGENTS.md instructions for /home/u/oss/toolpath\n\n<INSTRUCTIONS>" }] })),
            response_item(json!({ "type": "message", "role": "user",
                "content": [{ "type": "input_text", "text": "<environment_context>\n  <cwd>/home/u/oss/toolpath</cwd>\n</environment_context>" }] })),
        ]
    }
}
