//! 通用：截断 / 大量行 / 主导模型 — crate 内单元测试（自 bedcode-desktop/wasm-apps/agent-hub/rust/src/usage_parse/pi.rs 迁出）

use super::*;

use crate::usage_parse::MAX_EVENTS;
use serde_json::{json};

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
