//! 使用统计解析层（票据 06）—— 纯函数，无宿主调用
//!
//! claude 与 pi 两家 JSONL 适配器：逐行解析为归一事件流，同时聚合成
//! 会话级使用记录（tokens / cost / 起止 / 主导模型）。看板与会话日志
//! 共用本层（§4.6「归一事件是使用记录的超集」）：扫描取聚合输出，
//! 日志打开单会话取事件流输出。
//!
//! 实机格式事实（2026-09-13 核验，spec §9）：
//! - claude `~/.claude/projects/<cwd→->/<uuid>.jsonl`：同一 assistant
//!   message 按内容块拆多行且**每行携带完整 usage**（220 行 / 97 个
//!   message.id）→ 聚合必须按 `message.id` 去重；token 字段 snake_case
//!   （`message.usage.input_tokens` 等）；`type=cost-state` 行含真实
//!   `totalCostUSD`（最后一条为准，有则存不估算）。
//! - pi `~/.pi/agent/sessions/<cwd桶>/<ts>_<uuid>.jsonl`：首行
//!   `type=session` 携带 id/cwd/timestamp；`type=message` 行 role ∈
//!   user/assistant/toolResult，token 字段 camelCase
//!   （`message.usage.input/output/cacheRead/cacheWrite/reasoning`）+
//!   `usage.cost.total`。
//!
//! 水位不可得 mtime（WIT 无 stat 原语）：JSONL 为 append-only 语义，
//! 以 size（字节长）为水位即可保证幂等（见 usage.rs）。

use serde_json::Value;
use std::collections::HashSet;

/// 事件角色（wire 形状小写，与前端 NormalizedEvent.role 对应）
pub(crate) const ROLE_USER: &str = "user";
pub(crate) const ROLE_ASSISTANT: &str = "assistant";
pub(crate) const ROLE_TOOL: &str = "tool";
pub(crate) const ROLE_SYSTEM: &str = "system";

/// 事件流上限：防御异常巨大的会话文件拖垮 WATM 边界序列化，超出截断
pub(crate) const MAX_EVENTS: usize = 5000;
/// 单文件解析上限（字节）：超过视为异常数据跳过（正常会话 < 30MB）
pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

// ==================== 归一数据结构 ====================

/// 消息级 token 明细（按 message.id 去重后的一条助手用量）
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TokenUsage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
}

impl TokenUsage {
    fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.reasoning += other.reasoning;
    }
}

/// 归一事件（会话日志视图的行；token 字段仅助手事件携带）
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NormalizedEvent {
    /// 事件时间（epoch ms；不可得为 None）
    pub ts: Option<i64>,
    /// user / assistant / tool / system
    pub role: &'static str,
    /// 展示文本（工具事件为「名称 + 参数/结果摘要」）
    pub text: String,
    /// 助手事件附带的模型
    pub model: Option<String>,
    /// 助手事件附带的 token 明细
    pub tokens: Option<TokenUsage>,
}

/// 单模型用量（主导模型判定与按模型聚合的明细）
#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModelUsage {
    pub model: String,
    /// 出现次数（去重后的助手消息数）
    pub messages: u32,
    pub tokens: TokenUsage,
}

/// 会话解析结果：聚合记录 + 事件流（一次解析两处消费）
#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedSession {
    pub cli_session_id: String,
    pub project: Option<String>,
    pub title: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub models: Vec<ModelUsage>,
    pub tokens: TokenUsage,
    /// 有则存（claude totalCostUSD / pi usage.cost.total），null 不估算
    pub cost_total: Option<f64>,
    pub events: Vec<NormalizedEvent>,
    /// 事件流被截断（超 MAX_EVENTS）
    pub events_truncated: bool,
    /// 解析过程中被跳过的损坏行数（截断行 / 非法 JSON）
    pub skipped_lines: u32,
}

impl ParsedSession {
    /// 主导模型：按输出 token 量最大者（无任何助手消息时 None）
    pub fn dominant_model(&self) -> Option<&str> {
        self.models
            .iter()
            .max_by_key(|m| (m.tokens.output, m.tokens.input, m.messages))
            .map(|m| m.model.as_str())
    }

    fn record_assistant_usage(&mut self, model: &str, usage: &TokenUsage) {
        self.tokens.add(usage);
        match self.models.iter_mut().find(|m| m.model == model) {
            Some(m) => {
                m.messages += 1;
                m.tokens.add(usage);
            }
            None => self.models.push(ModelUsage {
                model: model.to_string(),
                messages: 1,
                tokens: *usage,
            }),
        }
    }
}

// ==================== 时间戳（ISO8601 → epoch ms） ====================

/// 解析 ISO8601 / RFC3339 时间戳为 epoch ms
///
/// 支持形态：`2026-09-04T01:17:52Z`、`2026-09-04T01:17:52.455Z`、
/// 带时区偏移 `2026-09-04T01:17:52+08:00`；日期时间以 `T`/空格分隔。
/// 儒略日换算用 Hinnant 算法（civil_from_days 逆运算），手工实现以
/// 免引入 chrono/time 依赖（插件 crate 依赖保持最小）。
pub(crate) fn parse_iso8601_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    // 形如 2026-09-04T01:17:52(.fff)?(Z|±HH:MM)?，最短 16 字符（无秒时区）
    if bytes.len() < 16 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let sep = bytes[10];
    if sep != b'T' && sep != b't' && sep != b' ' {
        return None;
    }
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    if bytes[13] != b':' {
        return None;
    }
    let minute: i64 = s.get(14..16)?.parse().ok()?;
    let (second, rest) = if bytes.len() > 16 && bytes[16] == b':' {
        (s.get(17..19)?.parse::<i64>().ok()?, &s[19..])
    } else {
        (0, &s[16..])
    };

    // 时区：必须显式声明（Z 或 ±HH:MM）——裸时间戳按本地时未知语义拒绝，
    // 避免误当 UTC 造成统计偏移
    let mut offset_minutes: i64 = 0;
    let mut frac_ms: i64 = 0;
    let mut chars = rest.chars();
    match chars.next() {
        Some('Z') | Some('z') => {}
        None => return None,
        Some('.') => {
            let frac: String = chars.by_ref().take_while(|c| c.is_ascii_digit()).collect();
            let digits = frac.len();
            if digits == 0 {
                return None;
            }
            // 毫秒截断（更多小数位四舍五入到 ms 粒度内截断即可）
            let mut ms: i64 = frac[..digits.min(3)].parse().ok()?;
            for _ in 0..3usize.saturating_sub(digits) {
                ms *= 10;
            }
            frac_ms = ms;
            let tail = chars.as_str();
            offset_minutes = match tail {
                "" | "Z" | "z" => 0,
                _ => parse_offset_minutes(tail)?,
            };
        }
        Some(_) => {
            offset_minutes = parse_offset_minutes(rest)?;
        }
    }

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60;
    Some(secs * 1000 + frac_ms)
}

/// 解析 `Z` 之外的时区偏移（`+08:00` / `-0530`）；空串（无偏移）拒绝
fn parse_offset_minutes(tail: &str) -> Option<i64> {
    let t = tail.trim();
    if t.is_empty() {
        return None;
    }
    let sign = match t.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = t[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 2 || digits.len() > 4 {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = if digits.len() >= 4 {
        digits[2..4].parse().ok()?
    } else {
        0
    };
    Some(sign * (hours * 60 + minutes))
}

/// Hinnant days_from_civil：civil 日期 → 自 1970-01-01 的天数
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]：3 月 = 0
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

// ==================== 值提取辅助 ====================

fn as_i64(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64()).unwrap_or(0)
}

/// 内容块文本抽取：string 直取；数组取 text 块拼接；单块对象（pi user
/// content 形态 `{type:"text",text}`）直取 text；其余 None
fn extract_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_string())
        }
        Value::Array(blocks) => {
            let mut text = String::new();
            for b in blocks {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
            let t = text.trim().to_string();
            (!t.is_empty()).then_some(t)
        }
        Value::Object(_) => {
            if content.get("type").and_then(|t| t.as_str()) == Some("text") {
                content
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 单行安全解析：非法/截断 JSON 返回 None（由调用方计数 skipped_lines）
pub(crate) fn parse_line(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim()).ok()
}

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
        match ts {
            Some(t) => {
                session.started_at = Some(session.started_at.map_or(t, |cur| cur.min(t)));
                session.ended_at = Some(session.ended_at.map_or(t, |cur| cur.max(t)));
            }
            None => {}
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
    if session.cli_session_id.is_empty() {
        // 无 sessionId 行的极端数据：以 0 占位由调用方决定丢弃
        session.cli_session_id = String::new();
    }
    session
}

/// 助手事件展示文本：text 块拼接；tool_use 块给「工具名 + 摘要」
fn assistant_display_text(content: Option<&Value>) -> String {
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

fn truncate_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

fn push_event(events: &mut Vec<NormalizedEvent>, truncated: &mut bool, event: NormalizedEvent) {
    if events.len() < MAX_EVENTS {
        events.push(event);
    } else {
        *truncated = true;
    }
}

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
        match ts {
            Some(t) => {
                session.started_at = Some(session.started_at.map_or(t, |cur| cur.min(t)));
                session.ended_at = Some(session.ended_at.map_or(t, |cur| cur.max(t)));
            }
            None => {}
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
                        if let Some(text) = extract_text(msg.get("content").unwrap_or(&Value::Null))
                        {
                            if session.title.is_none() {
                                session.title = Some(truncate_text(&text, 120));
                            }
                            push_event(
                                &mut session.events,
                                &mut session.events_truncated,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_USER,
                                    text,
                                    model: None,
                                    tokens: None,
                                },
                            );
                        }
                    }
                    "toolResult" => {
                        let tool = msg
                            .get("toolName")
                            .and_then(|n| n.as_str())
                            .unwrap_or("tool");
                        let text = extract_text(msg.get("content").unwrap_or(&Value::Null))
                            .unwrap_or_default();
                        let err = msg
                            .get("isError")
                            .and_then(|e| e.as_bool())
                            .unwrap_or(false);
                        push_event(
                            &mut session.events,
                            &mut session.events_truncated,
                            NormalizedEvent {
                                ts,
                                role: ROLE_TOOL,
                                text: format!(
                                    "{tool}{} · {}",
                                    if err { " (error)" } else { "" },
                                    truncate_text(&text, 400)
                                ),
                                model: None,
                                tokens: None,
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
            // custom / thinking_level_change 等元数据行不进事件流
            _ => {}
        }
    }

    session
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==================== 时间戳 ====================

    #[test]
    fn iso8601_z_with_fraction() {
        // 常数与权威实现互证（Python datetime）：2026-09-04T00:00:00Z =
        // 1_788_480_000_000；实机 cost-state startTime 1788484654355 ≈
        // 01:17:34.355Z 同日锚定
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52.455Z"),
            Some(1_788_484_672_455)
        );
    }

    #[test]
    fn iso8601_z_without_fraction() {
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52Z"),
            Some(1_788_484_672_000)
        );
    }

    #[test]
    fn iso8601_offset_timezone() {
        // +08:00 → UTC 减 8 小时
        assert_eq!(
            parse_iso8601_ms("2026-09-04T09:17:52+08:00"),
            parse_iso8601_ms("2026-09-04T01:17:52Z")
        );
        assert_eq!(
            parse_iso8601_ms("2026-09-04T01:17:52-02:00"),
            Some(1_788_484_672_000 + 2 * 3600 * 1000)
        );
    }

    #[test]
    fn iso8601_space_separator() {
        assert_eq!(
            parse_iso8601_ms("2026-09-04 01:17:52Z"),
            Some(1_788_484_672_000)
        );
    }

    #[test]
    fn iso8601_invalid_inputs() {
        assert_eq!(parse_iso8601_ms(""), None);
        assert_eq!(parse_iso8601_ms("not-a-date"), None);
        assert_eq!(parse_iso8601_ms("2026-13-04T01:17:52Z"), None);
        assert_eq!(parse_iso8601_ms("2026-09-32T01:17:52Z"), None);
        assert_eq!(parse_iso8601_ms("2026-09-04T01:17:52"), None); // 无时区 → 拒绝
        assert_eq!(parse_iso8601_ms("2026-09-04X01:17:52Z"), None);
    }

    #[test]
    fn iso8601_epoch_known_value() {
        // 2026-01-01T00:00:00Z = 1767225600
        assert_eq!(
            parse_iso8601_ms("2026-01-01T00:00:00Z"),
            Some(1_767_225_600_000)
        );
    }

    // ==================== claude 适配器 ====================

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

    // ==================== pi 适配器 ====================

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

    #[test]
    fn line_parse_rejects_garbage() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
        assert!(parse_line("{\"a\":1}").is_some());
    }
}
