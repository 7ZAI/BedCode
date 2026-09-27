//! 值提取与事件流公共工具（纯函数）
//!
//! `as_i64` / `extract_text` / `parse_line` 是各家适配器共用的健壮取值；
//! `truncate_text`（字符边界截断 + …）/ `push_event`（事件流上限截断）
//! 保证任何异常巨大的会话都不会撑爆 WATM 边界序列化。

use super::types::NormalizedEvent;
use serde_json::Value;

use super::MAX_EVENTS;

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
pub(super) fn push_event(
    events: &mut Vec<NormalizedEvent>,
    truncated: &mut bool,
    event: NormalizedEvent,
) {
    if events.len() < MAX_EVENTS {
        events.push(event);
    } else {
        *truncated = true;
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
}
