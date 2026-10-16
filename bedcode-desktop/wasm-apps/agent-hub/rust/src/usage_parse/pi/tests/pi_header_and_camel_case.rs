//! general — crate 内单元测试（自 bedcode-desktop/wasm-apps/agent-hub/rust/src/usage_parse/pi.rs 迁出）

use super::*;

use serde_json::{json, Value};

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
