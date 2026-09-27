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
//!
//! 票据 07 新增两家：
//! - opencode `~/.local/share/opencode/opencode.db`（**SQLite**）：不逐行
//!   JSONL 解析，而是 `usage_sqlite.rs` 用 `sqlite3` 只读查表后，把行交给
//!   本层的 [`parse_opencode_session_row`] / [`parse_opencode_events`] 归一。
//!   实机事实（2026-09-27，54 会话 / 22 表）：
//!   `session` 表是扁平列，与 `usage_session` 一对一；**时间戳是 epoch 毫秒**
//!   （`time_created`）而非 ISO8601Z，故不复用 [`parse_iso8601_ms`]；
//!   **`model` 列是 JSON 串** `{"id":…,"providerID":…,"variant":…}` 而非模型名；
//!   **`cost` 全为 0.0**（54 行无一 > 0）→ 按 spec §4.5「有则存、null 不估算」
//!   落 NULL，让看板显示为空而非 `$0.00`；9 个会话 token 全零（空会话），
//!   正常入库、参与计数，不触发除零。
//! - codex `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`：
//!   官方 rollout 格式，每行 `{timestamp, type, payload}`；`type` ∈
//!   session_meta / turn_context / response_item / event_msg / session_state /
//!   compacted。计费数据在 `event_msg.payload.type == "token_count"` 的
//!   `info.last_token_usage`（**该块含缓存部分**，须拆出 `cached_input_tokens`，
//!   codex 无缓存写入恒 0）。模型名来自最近一次 `turn_context`/`session_meta`
//!   的 `payload.model`——token_count 事件自身不带模型，故解析是有状态的。
//!   本机未初始化过 codex 会话，格式依据官方 rollout 文档实现（预留骨架，
//!   实机初始化后校准，见母 spec §9 / §10 待办 3）。

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

// ==================== opencode 适配器（SQLite 行 → 归一） ====================

/// 单条工具事件的展示文本上限（opencode `part.data` 的 `state.output` 可达
/// 数十 KB——整段塞进事件流会撑爆 WATM 边界序列化）
const OPENCODE_TOOL_TEXT_CAP: usize = 400;

/// opencode `session` 表行 → 归一会话（聚合层，与 JSONL 适配器同构）
///
/// 入参是 `sqlite3 -json` 输出的一行对象，字段名与库列同名。
/// 事实与取舍（2026-09-27 实机）：
/// - 时间戳 `time_created` / `time_updated` **已是 epoch 毫秒**，直接取用；
/// - `model` 列是 **JSON 串**（`{"id":"…","providerID":"…","variant":"…"}`），
///   取 `id`（缺失时退回 `providerID`）作为模型名；解析失败按「unknown」处理，
///   不丢会话；
/// - `cost` 列虽为 `NOT NULL DEFAULT 0`，但本机 54 行**全为 0.0**——0 不是
///   「已知花费为零」而是「未上报」，按 spec §4.5「有则存、null 不估算」落
///   `None`，看板展示为空而非 `$0.00`；
/// - token 全零的会话（空会话）照常入库并计入会话数，仅各 token 列为 0；
/// - `directory` 是项目绝对路径（等价 claude 的 `cwd`），空串按无项目处理。
pub(crate) fn parse_opencode_session_row(row: &Value) -> ParsedSession {
    let mut session = ParsedSession::default();
    let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        return session;
    }
    session.cli_session_id = id.to_string();
    session.project = row
        .get("directory")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    session.title = row
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| truncate_text(s, 120));

    let started = row.get("time_created").and_then(|v| v.as_i64());
    let updated = row.get("time_updated").and_then(|v| v.as_i64());
    session.started_at = started;
    session.ended_at = updated.or(started);

    let model = opencode_model_name(row.get("model"));
    let tokens = TokenUsage {
        input: as_i64(row.get("tokens_input")),
        output: as_i64(row.get("tokens_output")),
        cache_read: as_i64(row.get("tokens_cache_read")),
        cache_write: as_i64(row.get("tokens_cache_write")),
        reasoning: as_i64(row.get("tokens_reasoning")),
    };
    // 零 token 会话不入 models（否则按模型汇总会多出一堆 0 消息的空模型）
    if tokens.input != 0 || tokens.output != 0 {
        session.tokens = tokens;
        session.models.push(ModelUsage {
            model,
            messages: 0,
            tokens,
        });
    }
    // 0.0 视为未上报（见函数注释），不写库
    session.cost_total = row
        .get("cost")
        .and_then(|v| v.as_f64())
        .filter(|c| *c > 0.0);
    session
}

/// opencode `model` 列（JSON 串）→ 模型名；非 JSON / 缺字段回退 "unknown"
fn opencode_model_name(raw: Option<&Value>) -> String {
    let fallback = "unknown".to_string();
    let Some(text) = raw.and_then(|v| v.as_str()) else {
        return fallback;
    };
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        // 未来版本若改成裸模型名，仍能显示（整串当模型名而非丢弃）
        return if text.is_empty() {
            fallback
        } else {
            text.to_string()
        };
    };
    v.get("id")
        .and_then(|i| i.as_str())
        .or_else(|| v.get("providerID").and_then(|i| i.as_str()))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or(fallback)
}

/// opencode 单会话的 `message` × `part` 联表行 → 归一事件流
///
/// 入参行来自 [`crate::usage_sqlite`] 的联表查询，字段已在 **SQL 侧用
/// `json_extract` 展平并截断**（每个 part 最多带回 600 字符 output）——
/// 原始 `part.data` 单条可达 150KB（工具输出逐字回传），整块跨 WATM 边界
/// 会撑爆序列化，且会挤掉同一会话的其它事件。
///
/// 归一规则：
/// - 消息级去重按 `mid`：同一 message 的多个 part 归入**一条**事件（与 claude
///   适配器「按 message.id 去重」同语义）；
/// - `role` ∈ user/assistant 决定事件角色，其余（含缺省）归系统；
/// - 事件文本按 part 逐块拼：`text` → 正文，`reasoning` → 附在正文后，
///   `tool` → 「tool · 名称 (状态) · 输出摘要」；`step-*` / `patch` /
///   `compaction` 等纯结构 part 不进事件流；
/// - `t_*` 合成列仅在 `tokens` 块存在时有值（`json_extract` 对缺失路径返回
///   NULL）——user 消息不带 token，故 user 事件恒 `tokens: None`。
pub(crate) fn parse_opencode_events(rows: &[Value]) -> Vec<NormalizedEvent> {
    let mut events: Vec<NormalizedEvent> = Vec::new();
    let mut truncated = false;
    let mut i = 0usize;
    while i < rows.len() {
        let mid = rows[i].get("mid").and_then(|v| v.as_str()).unwrap_or("");
        if mid.is_empty() {
            i += 1;
            continue;
        }
        // 收集同 mid 的所有 part（查询已按 (mid, pid) 有序排列）
        let mut j = i;
        while j < rows.len() && rows[j].get("mid").and_then(|v| v.as_str()) == Some(mid) {
            j += 1;
        }
        let head = &rows[i];
        let role = head.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let tokens = opencode_flat_tokens(head);

        let mut body = String::new();
        let mut extra: Vec<String> = Vec::new();
        for row in &rows[i..j] {
            match row.get("ptype").and_then(|t| t.as_str()).unwrap_or("") {
                "text" | "reasoning" => {
                    if let Some(t) = row
                        .get("ptext")
                        .and_then(|t| t.as_str())
                        .filter(|s| !s.trim().is_empty())
                    {
                        if row.get("ptype").and_then(|t| t.as_str()) == Some("text") {
                            if !body.is_empty() {
                                body.push('\n');
                            }
                            body.push_str(t);
                        } else {
                            extra.push(format!(
                                "reasoning · {}",
                                truncate_text(t.trim(), OPENCODE_TOOL_TEXT_CAP)
                            ));
                        }
                    }
                }
                "tool" => {
                    let name = row.get("ptool").and_then(|t| t.as_str()).unwrap_or("tool");
                    let status = row.get("pstatus").and_then(|t| t.as_str()).unwrap_or("");
                    let output = row.get("poutput").and_then(|t| t.as_str()).unwrap_or("");
                    let status_suffix = if status.is_empty() {
                        String::new()
                    } else {
                        format!(" ({status})")
                    };
                    extra.push(format!(
                        "tool · {name}{status_suffix} · {}",
                        truncate_text(output.trim(), OPENCODE_TOOL_TEXT_CAP)
                    ));
                }
                // step-start / step-finish / patch / compaction：结构 part，无正文
                _ => {}
            }
        }

        if !extra.is_empty() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&extra.join("\n"));
        }
        if body.trim().is_empty() {
            i = j;
            continue;
        }

        let (event_role, keep_tokens) = match role {
            "user" => (ROLE_USER, false),
            "assistant" => (ROLE_ASSISTANT, true),
            _ => (ROLE_SYSTEM, false),
        };
        push_event(
            &mut events,
            &mut truncated,
            NormalizedEvent {
                ts: head.get("mts").and_then(|v| v.as_i64()),
                role: event_role,
                text: truncate_text(body.trim(), 2000),
                model: if event_role == ROLE_ASSISTANT {
                    head.get("model")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                } else {
                    None
                },
                tokens: if keep_tokens { tokens } else { None },
            },
        );
        i = j;
    }
    events
}

/// opencode 联表行的 `t_*` 合成列 → 归一 token 明细；全为 NULL 时返回 None
/// （`tokens` 块缺失的消息不该显示「0 用量」）
fn opencode_flat_tokens(row: &Value) -> Option<TokenUsage> {
    let get = |k: &str| row.get(k).and_then(|v| v.as_i64());
    if get("t_in").is_none() && get("t_out").is_none() && get("t_reason").is_none() {
        return None;
    }
    Some(TokenUsage {
        input: get("t_in").unwrap_or(0),
        output: get("t_out").unwrap_or(0),
        cache_read: get("t_cache_read").unwrap_or(0),
        cache_write: get("t_cache_write").unwrap_or(0),
        reasoning: get("t_reason").unwrap_or(0),
    })
}

// ==================== codex 适配器（官方 rollout 格式） ====================

/// codex 助手事件的工具/推理摘要上限（官方 `function_call_output` 逐字回传
/// 命令输出，可达数万字符）
const CODEX_TOOL_TEXT_CAP: usize = 400;

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
                                &mut session.events,
                                &mut session.events_truncated,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_USER,
                                    text: truncate_text(text.trim(), 2000),
                                    model: None,
                                    tokens: None,
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
                                &mut session.events,
                                &mut session.events_truncated,
                                NormalizedEvent {
                                    ts,
                                    role: ROLE_ASSISTANT,
                                    text: truncate_text(text.trim(), 2000),
                                    model: current_model.clone(),
                                    tokens: None,
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
                            &mut session.events,
                            &mut session.events_truncated,
                            NormalizedEvent {
                                ts,
                                role: ROLE_TOOL,
                                text: format!(
                                    "exec · {cmd} · {}",
                                    truncate_text(out.trim(), CODEX_TOOL_TEXT_CAP)
                                ),
                                model: None,
                                tokens: None,
                            },
                        );
                    }
                    // 未知 event 变体（Codex 每版本新增）静默跳过
                    _ => {}
                }
            }
            // response_item 是回退面：event_msg 已有权威消息时它会被去重
            "response_item" => {
                if let Some((role, text, model)) = codex_response_item_event(payload) {
                    push_event(
                        &mut session.events,
                        &mut session.events_truncated,
                        NormalizedEvent {
                            ts,
                            role,
                            text,
                            model,
                            tokens: None,
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

/// codex `response_item.payload` → (role, text, model)；无正文返回 None
fn codex_response_item_event(payload: &Value) -> Option<(&'static str, String, Option<String>)> {
    match payload.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "message" => {
            let role = match payload.get("role").and_then(|r| r.as_str()).unwrap_or("") {
                "user" => ROLE_USER,
                "assistant" => ROLE_ASSISTANT,
                "developer" => ROLE_SYSTEM,
                _ => return None,
            };
            let text = codex_content_text(payload.get("content"))?;
            Some((role, truncate_text(text.trim(), 2000), None))
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

    // ==================== opencode 适配器（票据 07） ====================

    /// 助手消息合成列（联表查询里随每 part 重复的 message 级字段）
    fn asst_meta() -> Option<serde_json::Map<String, Value>> {
        Some(
            json!({ "role": "assistant", "model": "space-bunny-free",
                    "t_in": 100, "t_out": 20, "t_reason": 5,
                    "t_cache_read": 10, "t_cache_write": 0 })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        )
    }

    /// opencode `session` 行归一：epoch ms 直取、model JSON 串解析、列全映射
    #[test]
    fn opencode_session_row_normalizes() {
        // 实机行形态（2026-09-27 opencode.db session 表）
        let row = json!({
            "id": "ses_f29b584f5ffeTe1R6nCihQHe1d",
            "directory": "/home/binblink/project/tauriProject/BedCode",
            "title": "http route 注册下沉",
            "cost": 0.0,
            "tokens_input": 493414,
            "tokens_output": 12345,
            "tokens_reasoning": 678,
            "tokens_cache_read": 11033,
            "tokens_cache_write": 7,
            "model": "{\"id\":\"space-bunny-free\",\"providerID\":\"opencode\",\"variant\":\"max\"}",
            "time_created": 1790301670571i64,
            "time_updated": 1790301700000i64,
        });
        let s = parse_opencode_session_row(&row);
        assert_eq!(s.cli_session_id, "ses_f29b584f5ffeTe1R6nCihQHe1d");
        assert_eq!(
            s.project.as_deref(),
            Some("/home/binblink/project/tauriProject/BedCode")
        );
        assert_eq!(s.title.as_deref(), Some("http route 注册下沉"));
        // epoch 毫秒直取（不是 ISO 串，不经 parse_iso8601_ms）
        assert_eq!(s.started_at, Some(1_790_301_670_571));
        assert_eq!(s.ended_at, Some(1_790_301_700_000));
        // model 列是 JSON 串 → 取 id
        assert_eq!(s.dominant_model(), Some("space-bunny-free"));
        assert_eq!(s.tokens.input, 493_414);
        assert_eq!(s.tokens.output, 12_345);
        assert_eq!(s.tokens.reasoning, 678);
        assert_eq!(s.tokens.cache_read, 11_033);
        assert_eq!(s.tokens.cache_write, 7);
        // cost 全 0 → 按「有则存、null 不估算」落 None（看板显示为空而非 $0.00）
        assert_eq!(s.cost_total, None);
    }

    /// 回归见证：cost 真的 > 0 时必须保留（否则「0 才落 None」写反了也测不出来）
    #[test]
    fn opencode_session_row_keeps_real_cost() {
        let row = json!({
            "id": "ses_1", "directory": "/p", "title": "t",
            "cost": 1.25, "tokens_input": 1, "tokens_output": 1,
            "tokens_reasoning": 0, "tokens_cache_read": 0, "tokens_cache_write": 0,
            "model": "{\"id\":\"m\"}", "time_created": 1i64, "time_updated": 2i64,
        });
        assert_eq!(parse_opencode_session_row(&row).cost_total, Some(1.25));
    }

    /// 零 token 会话（实机 9 个）：照常入库并带时间/项目，但不产生空模型条目
    #[test]
    fn opencode_session_row_handles_zero_token_session() {
        let row = json!({
            "id": "ses_zero", "directory": "/p", "title": "",
            "cost": 0.0, "tokens_input": 0, "tokens_output": 0,
            "tokens_reasoning": 0, "tokens_cache_read": 0, "tokens_cache_write": 0,
            "model": "{\"id\":\"m\"}", "time_created": 1000i64, "time_updated": 2000i64,
        });
        let s = parse_opencode_session_row(&row);
        // 有会话 id → 不被丢弃（能进列表与计数）
        assert_eq!(s.cli_session_id, "ses_zero");
        assert_eq!(s.started_at, Some(1000));
        // 空 title → None（不由调用方拿文件名兼底）
        assert_eq!(s.title, None);
        // 零 token 不入 models（否则按模型汇总多出 0 消息空模型）
        assert!(s.models.is_empty());
        assert_eq!(s.tokens.input, 0);
    }

    /// 边界：无 id 的行直接丢弃（调用方不会拿它当会话）
    #[test]
    fn opencode_session_row_without_id_is_empty() {
        let row = json!({ "id": "", "tokens_input": 99 });
        assert_eq!(parse_opencode_session_row(&row).cli_session_id, "");
    }

    /// model 列解析的四种形态：JSON 取 id / 只有 providerID / 裸串 / 非法 JSON
    #[test]
    fn opencode_model_name_variants() {
        assert_eq!(
            opencode_model_name(Some(&json!("{\"id\":\"a\",\"providerID\":\"p\"}"))),
            "a"
        );
        assert_eq!(
            opencode_model_name(Some(&json!("{\"providerID\":\"p\"}"))),
            "p"
        );
        // 未来版本改成裸模型名：整串当模型名而非丢成 unknown
        assert_eq!(opencode_model_name(Some(&json!("gpt-5.4"))), "gpt-5.4");
        // 非法 JSON：整串兜底；空串 → unknown
        assert_eq!(opencode_model_name(Some(&json!("{broken"))), "{broken");
        assert_eq!(opencode_model_name(Some(&json!(""))), "unknown");
        assert_eq!(opencode_model_name(None), "unknown");
        // JSON 对象但无 id/providerID
        assert_eq!(
            opencode_model_name(Some(&json!("{\"variant\":\"max\"}"))),
            "unknown"
        );
    }

    /// opencode 事件流：同 message 的多 part 归一条事件（message 级去重）
    #[test]
    fn opencode_events_group_parts_per_message() {
        // 助手消息的合成列在联表查询里随每行重复（json_extract 展平 message.data）
        let asst = |mut row: Value| {
            let (Some(o), Some(m)) = (row.as_object_mut(), asst_meta()) else {
                return row;
            };
            for (k, v) in m {
                o.insert(k.to_string(), v.clone());
            }
            row
        };
        let rows = vec![
            json!({ "mid": "msg_1", "mts": 1790301600538i64, "role": "user",
                    "ptext": "开工", "ptype": "text" }),
            asst(
                json!({ "mid": "msg_2", "mts": 1790301600557i64, "ptype": "reasoning",
                         "ptext": "先看 code-map" }),
            ),
            // 同 msg_2 的第二个 part（工具调用）——应并入上一条事件而非独立成条
            asst(
                json!({ "mid": "msg_2", "mts": 1790301600557i64, "ptype": "tool",
                         "ptool": "read", "pstatus": "completed", "poutput": "ok" }),
            ),
            json!({ "mid": "msg_3", "mts": 1790301600600i64, "role": "assistant",
                    "ptype": "text", "ptext": "已完成" }),
            // 纯结构 part（step-start）不产生事件
            json!({ "mid": "msg_3", "mts": 1790301600600i64, "role": "assistant",
                    "ptype": "step-start" }),
        ];
        let events = parse_opencode_events(&rows);
        // 3 个 message → 3 条事件（第 3 个只有 text part）
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].role, ROLE_USER);
        assert_eq!(events[0].text, "开工");
        assert_eq!(events[0].tokens, None, "user 消息不携带 token");
        // 助手：model + token 明细随事件；reasoning 与 tool 行并入同一条正文
        assert_eq!(events[1].role, ROLE_ASSISTANT);
        assert_eq!(events[1].model.as_deref(), Some("space-bunny-free"));
        let t = events[1].tokens.expect("assistant carries tokens");
        assert_eq!(t.input, 100);
        assert_eq!(t.reasoning, 5);
        assert_eq!(t.cache_read, 10);
        assert!(
            events[1].text.contains("先看 code-map"),
            "{}",
            events[1].text
        );
        assert!(
            events[1].text.contains("tool · read (completed) · ok"),
            "{}",
            events[1].text
        );
        // 纯结构 part 被跳过：msg_3 只有一条 text 事件
        assert_eq!(events[2].role, ROLE_ASSISTANT);
        assert_eq!(events[2].text, "已完成");
    }

    /// 边界：无 mid 的行跳过；全空 part 消息不产生空事件；工具行带在正文后
    #[test]
    fn opencode_events_skip_blank_and_unkeyed_rows() {
        let rows = vec![
            json!({ "mid": "", "role": "user", "ptext": "x", "ptype": "text" }),
            // 有 mid 但全空 part（损坏 / 未知 part 类型）→ 不产出空事件
            json!({ "mid": "msg_x", "mts": 1i64, "role": "assistant", "ptype": "step-finish" }),
            // 未知 role（JSON 里出现新值）→ 归系统而非丢事件
            json!({ "mid": "msg_z", "mts": 3i64, "role": "developer",
                    "ptype": "text", "ptext": "系统提示" }),
        ];
        let events = parse_opencode_events(&rows);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].role, ROLE_SYSTEM);
    }

    /// 回归见证：assistant 纯工具轮（正文空）不得丢事件——工具行兜底正文
    #[test]
    fn opencode_events_tool_only_turn_kept() {
        let rows = vec![json!({ "mid": "msg_1", "mts": 1i64, "role": "assistant",
            "ptype": "tool", "ptool": "bash", "pstatus": "completed", "poutput": "hi" })];
        let events = parse_opencode_events(&rows);
        assert_eq!(events.len(), 1, "纯工具轮不能被当成空消息丢掉");
        assert!(events[0].text.contains("bash"));
    }

    /// 边界：t_* 合成列全缺（tokens 块不存在）→ tokens None 而非全 0
    #[test]
    fn opencode_events_absent_token_columns_yield_none() {
        let rows = vec![json!({ "mid": "m", "mts": 1i64, "role": "assistant",
                                "ptype": "text", "ptext": "hi" })];
        let events = parse_opencode_events(&rows);
        assert_eq!(events[0].tokens, None);
    }

    // ==================== codex 适配器（票据 07 预留骨架） ====================

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
