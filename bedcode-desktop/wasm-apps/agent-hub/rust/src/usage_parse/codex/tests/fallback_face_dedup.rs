//! 回退面（`response_item`）去重与老格式兼容的行为契约
//!
//! 权威面与回退面在当前 codex 版本里**写的是同一段对话**：不处理就会既重复
//! 又被合成注入污染（见 [`item_face_session`]）。这里锁三条规则：
//! 1. 同一调用（`CommandExecution.id === function_call.call_id`）不重复入流；
//! 2. 权威面**没**覆盖到的工具调用（apply_patch / write_stdin 等）不能整段丢；
//! 3. 权威面缺席（更老版本 rollout）时回退面全量回填，行为不得退化。

use super::*;
use serde_json::json;

/// 事件文本列表（便于断言“哪几条进了流”）
fn texts(s: &ParsedSession) -> Vec<&str> {
    s.events.iter().map(|e| e.text.as_str()).collect()
}

/// 一轮完整对话：权威面有 UserMessage → Reasoning → CommandExecution → AgentMessage，
/// 回退面有对应的重复投影（function_call / function_call_output / message）
fn one_turn(call_id: &str) -> Vec<serde_json::Value> {
    vec![
        item_completed(json!({ "type": "UserMessage", "id": "u1",
            "content": [{ "type": "text", "text": "改一下 README", "text_elements": [] }] })),
        response_item(json!({ "type": "function_call", "name": "exec_command",
            "arguments": "{\"cmd\":\"sed -i 1s/a/b/ README.md\"}", "call_id": call_id })),
        item_completed(json!({ "type": "CommandExecution", "id": call_id,
            "command": ["/bin/bash", "-lc", "sed -i 1s/a/b/ README.md"],
            "cwd": "file:///home/u/oss/toolpath", "source": "unified_exec_startup",
            "status": "completed", "stdout": "", "stderr": "",
            "aggregated_output": "b\n", "exit_code": 0,
            "duration": { "secs": 0, "nanos": 5511 } })),
        response_item(json!({ "type": "function_call_output", "call_id": call_id,
            "output": "Chunk ID: d0982d\nb\n" })),
        item_completed(json!({ "type": "AgentMessage", "id": "a1",
            "phase": "final_answer", "content": [{ "type": "Text", "text": "改好了。" }] })),
    ]
}

// ==================== 契约 1：同一调用不重复 ====================

/// 权威面在场时，同一 call_id 的 `function_call` / `function_call_output`
/// 是同一件事的重复投影 → 整对丢弃，只留权威面的 `exec` 卡。
/// 会话标题取真实提问，不得被合成注入（AGENTS.md 开头）占位。
///
/// 变异探针：删掉 `item_call_ids` 过滤 → 本例事件数 3→5 转红；
/// 把 `is_title_source` 的标题来源改成回退面首条 → 标题变 AGENTS.md 开头转红。
#[test]
fn item_face_suppresses_duplicate_tool_projection() {
    let mut lines = vec![serde_json::json!({
        "timestamp": "2026-10-04T04:30:39.000Z", "type": "session_meta",
        "payload": { "id": "t1", "cwd": "/home/u/oss/toolpath" } })];
    lines.extend(synthetic_injections());
    lines.extend(one_turn("call_9907ff0d09f21d35"));
    let s = parse_codex_session(&rollout(&lines));

    let roles: Vec<&str> = s.events.iter().map(|e| e.role).collect();
    assert_eq!(
        roles,
        vec![ROLE_USER, ROLE_TOOL, ROLE_ASSISTANT],
        "权威面一条 user + 一条 exec + 一条 assistant，重复投影与合成注入都不许进来：{:?}",
        texts(&s)
    );
    assert_eq!(s.events[0].text, "改一下 README");
    assert_eq!(s.events[2].text, "改好了。");
    // 标题必须来自真实提问，而不是回退面首条的合成注入
    assert_eq!(
        s.title.as_deref(),
        Some("改一下 README"),
        "标题取权威面真实提问，不得被 AGENTS.md / skills / environment 注入占位"
    );
}

/// 重复投影的两条具体文本（参数 JSON 与 Chunk ID 输出）一条都不许残留
#[test]
fn item_face_event_stream_has_no_duplicate_projections() {
    let mut lines = vec![serde_json::json!({
        "timestamp": "2026-10-04T04:30:39.000Z", "type": "session_meta",
        "payload": { "id": "t2", "cwd": "/w" } })];
    lines.extend(synthetic_injections());
    lines.extend(one_turn("c1"));
    let s = parse_codex_session(&rollout(&lines));

    assert!(
        !texts(&s)
            .iter()
            .any(|t| t.contains("sed -i 1s/a/b/ README.md\"")),
        "function_call 的 JSON 参数投影应被丢弃：{:?}",
        texts(&s)
    );
    assert!(
        !texts(&s).iter().any(|t| t.contains("Chunk ID")),
        "function_call_output 的重复投影应被丢弃：{:?}",
        texts(&s)
    );
    assert!(
        !texts(&s).iter().any(|t| t.contains("AGENTS.md")
            || t.contains("skills_instructions")
            || t.contains("environment_context")),
        "合成注入不入事件流：{:?}",
        texts(&s)
    );
}

// ==================== 契约 2：未覆盖的工具调用不丢 ====================

/// 权威面只投影 `CommandExecution`；`apply_patch` / `write_stdin` 这类调用在
/// 权威面**没有**对应项 → 回退面的 call / output 必须保留
///
/// 变异探针：把过滤写成“回退面全丢” → 本例事件数 0 转红。
#[test]
fn fallback_keeps_tool_calls_the_item_face_never_covers() {
    let mut lines = vec![serde_json::json!({
        "timestamp": "2026-10-04T04:30:39.000Z", "type": "session_meta",
        "payload": { "id": "t3", "cwd": "/w" } })];
    lines.extend(synthetic_injections());
    lines.extend(one_turn("cmd-1"));
    // apply_patch：权威面无 CommandExecution 对应项
    lines.push(response_item(
        json!({ "type": "function_call", "name": "apply_patch",
        "arguments": "*** Begin Patch", "call_id": "patch-1" }),
    ));
    lines.push(response_item(json!({ "type": "function_call_output",
        "call_id": "patch-1", "output": "Success. Updated the following files:\nM a.rs" })));
    let s = parse_codex_session(&rollout(&lines));

    let kept: Vec<&str> = texts(&s)
        .into_iter()
        .filter(|t| t.contains("apply_patch") || t.contains("Success. Updated"))
        .collect();
    assert_eq!(
        kept.len(),
        2,
        "未被权威面覆盖的 apply_patch 调用与结果都该保留：{:?}",
        texts(&s)
    );
    assert!(kept[0].starts_with("tool · apply_patch · "), "{}", kept[0]);
    assert!(kept[1].starts_with("tool_result · "), "{}", kept[1]);
}

/// 边界：call_id 为空串的调用不得被误判为「已覆盖」而丢弃
/// （`call_id` 过滤是 `""` 时不能命中任何 CommandExecution id）
#[test]
fn fallback_keeps_call_without_id() {
    let content = concat!(
        r#"{"timestamp":"2026-10-04T04:30:39.000Z","type":"session_meta","payload":{"id":"t4","cwd":"/w"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.133Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.182Z","type":"event_msg","payload":{"type":"item_completed","completed_at_ms":1791088239182,"item":{"type":"UserMessage","id":"u","content":[{"type":"text","text":"真的提问"}]}}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:40.000Z","type":"response_item","payload":{"type":"function_call","name":"mcp_tool","arguments":"{}","call_id":""}}"#,
        "\n",
    );
    let s = parse_codex_session(content);
    assert!(
        texts(&s).iter().any(|t| t.contains("mcp_tool")),
        "空 call_id 的调用不应被权威面去重规则吃掉：{:?}",
        texts(&s)
    );
}

// ==================== 契约 3：老格式回退面全量回填 ====================

/// 权威面缺席（更老版本 rollout）→ 回退面是唯一消息面，必须全量回填，
/// 且标题取自回填进来的首条正文
///
/// 变异探针：把 `if item_face_seen` 反过来（无条件过滤）→ 本例事件数转红。
#[test]
fn legacy_rollout_without_item_face_fills_from_fallback() {
    let content = concat!(
        r#"{"timestamp":"2026-10-04T04:30:39.000Z","type":"session_meta","payload":{"id":"t5","cwd":"/w"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.133Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"老格式提问"}]}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.200Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"ls\"}","call_id":"x1"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.300Z","type":"response_item","payload":{"type":"function_call_output","call_id":"x1","output":"README.md"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:39.400Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"老格式答复"}]}}"#,
        "\n",
    );
    let s = parse_codex_session(content);
    assert_eq!(
        texts(&s),
        vec![
            "老格式提问",
            "tool · exec_command · {\"cmd\":\"ls\"}",
            "tool_result · README.md",
            "老格式答复",
        ]
    );
    assert_eq!(s.title.as_deref(), Some("老格式提问"));
}

/// 老格式 + 更老的权威面变体（`user_message` / `agent_message`）同时在场：
/// 两者都要保留（既有行为，本次改动不得动它）
#[test]
fn legacy_authoritative_variants_still_emit() {
    let content = concat!(
        r#"{"timestamp":"2026-10-04T04:30:39.000Z","type":"session_meta","payload":{"id":"t6","cwd":"/w"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:40.000Z","type":"event_msg","payload":{"type":"user_message","message":"开工"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:41.000Z","type":"event_msg","payload":{"type":"agent_message","message":"好的"}}"#,
        "\n",
        r#"{"timestamp":"2026-10-04T04:30:42.000Z","type":"event_msg","payload":{"type":"exec_command_end","command":["bash","-lc","ls"],"aggregated_output":"README.md"}}"#,
        "\n",
    );
    let s = parse_codex_session(content);
    assert_eq!(s.events.len(), 3);
    assert_eq!(s.events[2].text, "exec · bash -lc ls · README.md");
    assert_eq!(s.title.as_deref(), Some("开工"));
}
