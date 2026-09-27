//! opencode 适配器（SQLite 行 → 归一）
//!
//! 入参是 `sqlite3 -json` 输出的一行对象，字段名与库列同名（见
//! [`crate::usage_sqlite`] 的联表查询——字段已在 SQL 侧用 `json_extract`
//! 展平并截断，原始 `part.data` 单条可达 150KB，整块跨 WATM 边界会撑爆
//! 序列化）。消息级去重按 `mid`（同一 message 的多个 part 归入一条事件）。

use super::common::{as_i64, push_event, truncate_text};
use super::types::{ModelUsage, NormalizedEvent, ParsedSession, TokenUsage};
use super::{ROLE_ASSISTANT, ROLE_SYSTEM, ROLE_USER};
use serde_json::Value;

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

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
