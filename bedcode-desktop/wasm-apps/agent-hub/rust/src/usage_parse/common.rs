//! 值提取与事件流公共工具（纯函数）
//!
//! `as_i64` / `extract_text` / `parse_line` 是各家适配器共用的健壮取值；
//! `truncate_text`（字符边界截断 + …）/ `push_event`（事件流上限截断）
//! 保证任何异常巨大的会话都不会撑爆 WATM 边界序列化。

use super::types::{NormalizedEvent, ParsedSession};
use serde_json::Value;

use super::{MAX_ATTACHMENT_EVENTS, MAX_EVENTS};

// ==================== 值提取辅助 ====================

pub(super) fn as_i64(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64()).unwrap_or(0)
}

/// 内容块文本抽取：string 直取；数组取 text 块拼接；单块对象（pi user
/// content 形态 `{type:"text",text}`）直取 text；其余 None
pub(super) fn extract_text(content: &Value) -> Option<String> {
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
pub(super) fn truncate_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// 工具调用参数摘要：对象/数组 → 紧凑 JSON；字符串 → 原文；其余空。
/// 空对象 / 空数组视为无参（不显示 `{}`）；超出 `cap` 字符截断防撑爆序列化。
/// （pi toolCall 的 `arguments` 与 claude tool_use 的 `input` 都是对象；
///  README 审计 A1/A3：只显示工具名不显示参数的缺口靠它补齐。）
pub(super) fn tool_args_summary(v: Option<&Value>, cap: usize) -> String {
    let Some(v) = v else {
        return String::new();
    };
    let raw = match v {
        Value::Object(_) | Value::Array(_) => serde_json::to_string(v).unwrap_or_default(),
        Value::String(s) => s.trim().to_string(),
        _ => return String::new(),
    };
    if raw.is_empty() || raw == "{}" || raw == "[]" {
        String::new()
    } else {
        truncate_text(&raw, cap)
    }
}

/// tool_result 内容文本（claude user 行 / pi toolResult 共用）。
/// text 块拼接；非 text 块（图片/二进制）给「按类型名 + 可查提示」占位**而非
/// 静默过滤**（README 审计 A4 ③：图片块消失 → 用户看不到结果全貌，只能猜）。
/// Object 形态（单个块对象而非数组包裹，如 `{"type":"text","text":…}`）走与数组块
/// 完全同一套判定：此前直接落到 `_ => String::new()` 被静默置空，事件文本会变成
/// `tool_result · 名称 · `（尾随空段），与 A4 ③ 的目标相悖。
pub(super) fn tool_result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.trim().to_string(),
        Value::Array(blocks) => result_blocks_text(blocks.iter()),
        Value::Object(_) => result_blocks_text(std::iter::once(content)),
        _ => String::new(),
    }
}

/** 非文本块占位的**机器 token**（kind 为空时的退化形态） */
pub(super) const NON_TEXT_TOKEN_GENERIC: &str = "[non-text]";
/// 非文本块占位 token 前缀（前端镜像见 `utils/format.ts::NON_TEXT_TOKEN_RE`）
pub(super) const NON_TEXT_TOKEN_PREFIX: &str = "[non-text:";

/// kind 合法形态：`[A-Za-z0-9_.-]{1,40}`（真实块类型都是简单 ASCII 词）
const NON_TEXT_KIND_OK: fn(char) -> bool =
    |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');

/// 非文本块占位：由**块类型**生成机器 token，而不是直接写可读文案
///
/// 形态：`[non-text:<kind>]`；kind **严格校验**（见 [`NON_TEXT_KIND_OK`]），不合规
/// （含空格 / `]` / 换行 / 非 ASCII / 超 40 字符）一律退化为 `[non-text]`，前端走
/// 「未知类型」文案。前端拿到 token 后按 kind 查 i18n 表——故用户可见文案全部落在
/// 前端语言包里，guest 不产出任何文案。
///
/// **为什么不能直接写中文**：wire 上只有裸字符串（无 i18n key 字段），guest 写死文案
/// 就只能单语，英文界面会出现中文占位（AGENTS §6：用户可见文案一律走 i18n）。
///
/// **kind 来自日志内容（不可信输入）**，两条约束：
/// ① token 形状——否则 `kind` 里带 `]` / 换行能伪造出第二个 token；
/// ② 语义诚实——「过滤非法字符」会把 `x][non-text:evil` 变成看起来像类型名的
///    `non-textevil`，宁可退化成无 kind 形态也不谎报类型。
fn non_text_placeholder(kind: &str) -> String {
    if kind.is_empty() || kind.len() > 40 || !kind.chars().all(NON_TEXT_KIND_OK) {
        return NON_TEXT_TOKEN_GENERIC.to_string();
    }
    format!("{NON_TEXT_TOKEN_PREFIX}{kind}]")
}

/// 内容块序列 → 文本（数组形态与单对象形态共用）
fn result_blocks_text<'a>(blocks: impl Iterator<Item = &'a Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for b in blocks {
        match b.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        parts.push(t.trim().to_string());
                    }
                }
            }
            Some(other) => parts.push(non_text_placeholder(other)),
            // 未知形状块：不制造占位（原始 JSONL 页签可查）
            None => {}
        }
    }
    parts.join("\n").trim().to_string()
}
pub(super) fn push_event(session: &mut ParsedSession, event: NormalizedEvent) {
    // 额度只算**实质**事件：附件已单独计数（`attachment_events`），
    // 否则附件噪音会挤掉后续 user / assistant / tool 事件
    let substantive = session
        .events
        .len()
        .saturating_sub(session.attachment_events as usize);
    if substantive < MAX_EVENTS {
        session.events.push(event);
    } else {
        session.events_truncated = true;
    }
}

/// 附件事件入流（独立子上限，见 `MAX_ATTACHMENT_EVENTS`）
pub(super) fn push_attachment_event(session: &mut ParsedSession, event: NormalizedEvent) {
    if (session.attachment_events as usize) < MAX_ATTACHMENT_EVENTS {
        session.attachment_events += 1;
        session.events.push(event);
    } else {
        session.events_truncated = true;
    }
}

// ==================== Tests（纯函数单测） ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_parse_rejects_garbage() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
        assert!(parse_line("{\"a\":1}").is_some());
    }

    // ==================== tool_result_text（数组 / 单对象 / 非内容形态） ====================

    #[test]
    fn tool_result_text_reads_single_object_block() {
        // 单块对象（非数组包裹）：此前落 `_ => String::new()` 被静默置空
        let v = serde_json::json!({"type": "text", "text": "42 行"});
        assert_eq!(tool_result_text(&v), "42 行");
    }

    #[test]
    fn tool_result_text_placeholder_for_single_object_non_text_block() {
        let v = serde_json::json!({"type": "image", "source": {"data": "…"}});
        assert_eq!(tool_result_text(&v), "[non-text:image]");
    }

    #[test]
    fn tool_result_text_array_behaviour_unchanged() {
        let v = serde_json::json!([
            {"type": "text", "text": "第一段"},
            {"type": "image"},
            {"type": "text", "text": "  "},
            {"type": "text", "text": "第二段"},
        ]);
        assert_eq!(tool_result_text(&v), "第一段\n[non-text:image]\n第二段");
    }

    /// 占位必须是**机器 token**（前端按 kind 查 i18n），guest 不产出任何可读文案
    #[test]
    fn non_text_placeholder_is_machine_token() {
        assert_eq!(non_text_placeholder("image"), "[non-text:image]");
        assert_eq!(
            non_text_placeholder("tool_use.serail"),
            "[non-text:tool_use.serail]"
        );
    }

    /// 反例：kind 来自日志内容（不可信）——消毒后为空 → 退化形态，不带 kind
    #[test]
    fn non_text_placeholder_sanitizes_untrusted_kind() {
        // 含 `]` / 空格 / 换行：全部不是 kind 允许字符 → 不带 kind
        assert_eq!(
            non_text_placeholder("] [non-text:evil"),
            NON_TEXT_TOKEN_GENERIC
        );
        assert_eq!(non_text_placeholder("中文类型"), NON_TEXT_TOKEN_GENERIC);
        assert_eq!(non_text_placeholder(""), NON_TEXT_TOKEN_GENERIC);
    }

    /// 边界：40 字符内的合法 kind 原样保留；超长 kind 不截断（截出来的名字是谎报）
    #[test]
    fn non_text_placeholder_keeps_valid_kind_but_rejects_overlong() {
        let at_limit = "a".repeat(40);
        assert_eq!(
            non_text_placeholder(&at_limit),
            format!("[non-text:{at_limit}]")
        );
        assert_eq!(
            non_text_placeholder(&"a".repeat(41)),
            NON_TEXT_TOKEN_GENERIC
        );
    }

    /// 不可信 kind 不能伪造出**第二个** token（注入防护）
    #[test]
    fn non_text_placeholder_cannot_forge_second_token() {
        let token = non_text_placeholder("x][non-text:evil");
        assert_eq!(
            token.matches("non-text").count(),
            1,
            "占位里只能有一个 token，实际={token}"
        );
        assert!(
            !token.contains("evil"),
            "被消毒掉的字符不得出现在 token 里，实际={token}"
        );
    }

    // ==================== 入流上限：实质 / 附件分开计（完整性回退防护） ====================

    /// 附件不得挤掉后续实质消息：附件有自己的子上限，实质事件始终能拿满 MAX_EVENTS
    ///
    /// 变异探针：把附件改回走 `push_event`（共享额度）→ 末尾的实质事件被挤掉 → 转红。
    #[test]
    fn attachment_events_do_not_evict_substantive_events() {
        use crate::usage_parse::MAX_ATTACHMENT_EVENTS;
        let mut session = ParsedSession::default();
        let att = || NormalizedEvent {
            ts: Some(1),
            role: super::super::ROLE_SYSTEM,
            text: "attachment · file · a.rs".to_string(),
            model: None,
            tokens: None,
            error: false,
            tool_use_id: None,
        };
        for _ in 0..(MAX_ATTACHMENT_EVENTS + 50) {
            push_attachment_event(&mut session, att());
        }
        // 附件超上限被丢弃，但已入流的附件数不超子上限
        assert_eq!(session.attachment_events as usize, MAX_ATTACHMENT_EVENTS);
        assert_eq!(session.events.len(), MAX_ATTACHMENT_EVENTS);
        assert!(
            session.events_truncated,
            "附件被截断必须置 truncated 标志（前端据此提示）"
        );

        // 关键：附件塞满之后，实质事件仍必须进得来（这正是共享额度时的回退点）
        push_event(
            &mut session,
            NormalizedEvent {
                ts: Some(2),
                role: super::super::ROLE_USER,
                text: "实质消息".to_string(),
                model: None,
                tokens: None,
                error: false,
                tool_use_id: None,
            },
        );
        assert!(
            session.events.last().map(|e| e.text.as_str()) == Some("实质消息"),
            "附件不得挤掉后续实质消息（实际最后一条={:?}）",
            session.events.last().map(|e| e.text.clone())
        );
    }

    /// 反例：实质事件达到 MAX_EVENTS 后才截断（附件不计入该额度）
    #[test]
    fn substantive_cap_still_applies_and_excludes_attachments() {
        use crate::usage_parse::MAX_EVENTS;
        let mut session = ParsedSession::default();
        push_attachment_event(
            &mut session,
            NormalizedEvent {
                ts: Some(0),
                role: super::super::ROLE_SYSTEM,
                text: "attachment".to_string(),
                model: None,
                tokens: None,
                error: false,
                tool_use_id: None,
            },
        );
        for _ in 0..MAX_EVENTS {
            push_event(
                &mut session,
                NormalizedEvent {
                    ts: Some(1),
                    role: super::super::ROLE_USER,
                    text: "m".to_string(),
                    model: None,
                    tokens: None,
                    error: false,
                    tool_use_id: None,
                },
            );
        }
        assert!(
            !session.events_truncated,
            "恰好 MAX_EVENTS 条实质事件不应算截断"
        );
        push_event(
            &mut session,
            NormalizedEvent {
                ts: Some(2),
                role: super::super::ROLE_USER,
                text: "overflow".to_string(),
                model: None,
                tokens: None,
                error: false,
                tool_use_id: None,
            },
        );
        assert!(
            session.events.len() == MAX_EVENTS + 1,
            "多出的那一条附件不计入实质额度（总额 = MAX_EVENTS + 附件数）"
        );
        assert!(
            session.events_truncated,
            "超 MAX_EVENTS 必须置 truncated 标志"
        );
    }

    #[test]
    fn tool_result_text_non_content_shapes_stay_empty() {
        // 数字 / 布尔 / null 不是内容块（此前与单对象同走 `_` 分支，行为不得变）
        assert_eq!(tool_result_text(&serde_json::json!(42)), "");
        assert_eq!(tool_result_text(&serde_json::json!(true)), "");
        assert_eq!(tool_result_text(&serde_json::Value::Null), "");
    }
}
