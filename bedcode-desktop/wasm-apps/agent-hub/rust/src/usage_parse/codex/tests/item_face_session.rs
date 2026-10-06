//! codex 适配器测试（`usage_parse::codex::tests::*`）
//!
//! 用例分两组：
//! - 本文件：实机 rollout 的**权威面**（`event_msg.payload.type ==
//!   "item_completed"`）行为契约；
//! - [`fallback_face_dedup`]：回退面（`response_item`）去重与老格式兼容。
//!
//! 夹具形状取自本机 14 个真实 rollout（2026-10-04，994 行），字段名逐字照抄
//! （`item.type` 是 **PascalCase**、`Reasoning.summary_text` 是**字符串数组**、
//! `CommandExecution.id === function_call.call_id` 都是实机事实，改动前请回看
//! 模块头的双面说明。

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
    let u = match codex_last_token_usage(&payload) {
        Some(u) => u,
        None => panic!("last_token_usage 缺失时不得归一"),
    };
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
    let a = s
        .models
        .iter()
        .find(|m| m.model == "gpt-a")
        .unwrap_or_else(|| panic!("模型 gpt-a 的用量缺失"));
    let b = s
        .models
        .iter()
        .find(|m| m.model == "gpt-b")
        .unwrap_or_else(|| panic!("模型 gpt-b 的用量缺失"));
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

// ==================== 权威面（item_completed）行为契约 ====================

/// 事件文本列表（便于断言「哪几条进了流」）
fn texts(s: &ParsedSession) -> Vec<&str> {
    s.events.iter().map(|e| e.text.as_str()).collect()
}

/// 一份最小可用的当前版本 rollout 头（session_meta + turn_context）
fn current_head() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.000Z", "ordinal": 0,
            "type": "session_meta",
            "payload": { "id": "01a1052b-9390-7c92-b2a8-cf340072b91a",
                         "session_id": "01a1052b-9390-7c92-b2a8-cf340072b91a",
                         "cwd": "/home/u/oss/toolpath", "cli_version": "0.47.0",
                         "model_provider": "tokenplan",
                         "base_instructions": "<系统提示词全文……>" } }),
        serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.050Z", "ordinal": 5,
            "type": "turn_context",
            "payload": { "turn_id": "turn-1", "model": "deepseek-v4-pro-0813",
                         "cwd": "/home/u/oss/toolpath", "effort": "medium" } }),
    ]
}

/// 契约 I-1（正例）：权威面 `UserMessage` → user 事件 + 会话标题
///
/// 变异探针：删掉 `item_completed` 分支 → 事件列表空转红。
#[test]
fn item_user_message_becomes_user_event_and_title() {
    let mut lines = current_head();
    lines.push(item_completed(json!({ "type": "UserMessage", "id": "u1",
        "client_id": "3e5c55dc",
        "content": [{ "type": "text", "text": "查看codex 是否存在 前一个会话",
                      "text_elements": [] }] })));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(s.events.len(), 1);
    assert_eq!(s.events[0].role, ROLE_USER);
    assert_eq!(s.events[0].text, "查看codex 是否存在 前一个会话");
    assert_eq!(
        s.title.as_deref(),
        Some("查看codex 是否存在 前一个会话"),
        "标题取权威面真实提问"
    );
    // session_meta 的 id / cwd 照常取材（base_instructions 不得漏进标题）
    assert_eq!(s.cli_session_id, "01a1052b-9390-7c92-b2a8-cf340072b91a");
    assert_eq!(s.project.as_deref(), Some("/home/u/oss/toolpath"));
    assert!(
        !s.title.as_deref().unwrap_or("").contains("系统提示词"),
        "base_instructions 不是会话标题"
    );
}

/// 契约 I-2（反例）：合成注入（developer 技能表 / user AGENTS.md /
/// user 环境上下文）只存在于回退面，权威面在场时**整批不入流**
///
/// 回归见证（2026-10-06 实机 bug）：此前只读回退面，本机 14 个 codex 会话
/// 的标题**全部**是 `# AGENTS.md instructions for …`，真实提问在第三条。
#[test]
fn synthetic_injections_never_reach_events_or_title() {
    let mut lines = current_head();
    lines.extend(synthetic_injections());
    lines.push(item_completed(json!({ "type": "UserMessage", "id": "u1",
        "content": [{ "type": "text", "text": "你好" }] })));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(s.events.len(), 1, "合成注入不入流：{:?}", texts(&s));
    assert_eq!(s.events[0].text, "你好");
    assert_eq!(s.title.as_deref(), Some("你好"));
}

/// 契约 I-1（顺序边界）：**助手轮先于用户轮**时，标题仍取真实提问
///
/// 变异探针：把 `is_title_source` 条件删掉（退化为“首个 item 事件”）→
/// 标题变助手正文转红——这条是 `is_title_source` 参数存在意义的唯一见证。
#[test]
fn agent_message_before_user_message_still_yields_user_title() {
    let mut lines = current_head();
    lines.push(item_completed(json!({ "type": "AgentMessage", "id": "a0",
        "content": [{ "type": "Text", "text": "先来一句助手问候" }] })));
    lines.push(item_completed(json!({ "type": "UserMessage", "id": "u0",
        "content": [{ "type": "text", "text": "真正的提问" }] })));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(
        s.title.as_deref(),
        Some("真正的提问"),
        "助手轮先到也不能抢走标题"
    );
    assert_eq!(s.events[0].role, ROLE_ASSISTANT);
}

/// 契约 I-3：权威面 `AgentMessage`（内容块是 `Text` 而非 `text`）→ 助手
/// 事件，带 `turn_context` 的模型名
///
/// 变异探针：只认小写 `text` 块 → 助手事件消失转红。
#[test]
fn item_agent_message_carries_turn_context_model() {
    let mut lines = current_head();
    lines.push(item_completed(json!({ "type": "AgentMessage", "id": "a1",
        "phase": "final_answer",
        "content": [{ "type": "Text", "text": "你好！有什么可以帮你的吗？" }] })));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(s.events.len(), 1);
    assert_eq!(s.events[0].role, ROLE_ASSISTANT);
    assert_eq!(s.events[0].text, "你好！有什么可以帮你的吗？");
    assert_eq!(
        s.events[0].model.as_deref(),
        Some("deepseek-v4-pro-0813"),
        "助手事件挂当前轮模型"
    );
    // 无用户轮次时走既有标题兜底（恢复会话/首轮报错场景）：取首条正文
    assert_eq!(
        s.title.as_deref(),
        Some("你好！有什么可以帮你的吗？"),
        "无真实提问时兜底取首条助手正文（既有行为，勿改）"
    );
}

/// 契约 I-4：权威面 `Reasoning.summary_text`（**字符串数组**）→ system 事件
///
/// 回归见证：回退面的 `reasoning` 只有 `encrypted_content`（密文渲染成乱码），
/// 此前实机 96 条推理摘要全部丢失。
#[test]
fn item_reasoning_summary_text_becomes_system_event() {
    let mut lines = current_head();
    lines.push(item_completed(json!({ "type": "Reasoning", "id": "r1",
        "summary_text": [
            "The user is asking me to reply with exactly \"OK\".",
            "This is a simple acknowledgment request."
        ],
        "raw_content": [] })));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(s.events.len(), 1);
    assert_eq!(s.events[0].role, ROLE_SYSTEM);
    assert_eq!(
        s.events[0].text,
        "reasoning · The user is asking me to reply with exactly \"OK\".\n\
         This is a simple acknowledgment request."
    );
}

/// 契约 I-4（边界 / 异常）：`summary_text` 为空数组（纯密文形态）时**不**
/// 产生事件——渲染空行或密文都只会是噪音
///
/// 变异探针：去掉 `.filter(|s| !s.trim().is_empty())` → 本例事件数 1 转红。
#[test]
fn item_reasoning_with_empty_summary_emits_nothing() {
    let mut lines = current_head();
    lines.push(item_completed(json!({ "type": "Reasoning", "id": "r1",
        "summary_text": [], "raw_content": [] })));
    // 未知 item 变体（Codex 每版本新增）同样静默跳过
    lines.push(item_completed(
        json!({ "type": "SomeFutureItem", "id": "x1" }),
    ));
    let s = parse_codex_session(&rollout(&lines));

    assert!(
        s.events.is_empty(),
        "空推理摘要与未知变体都不产生事件：{:?}",
        texts(&s)
    );
}

/// 契约 I-5：`CommandExecution` → `exec · 命令 · 输出` 工具卡，
/// `tool_use_id` 取 `item.id`，`exit_code != 0` 标错误
#[test]
fn item_command_execution_renders_exec_card_and_error_flag() {
    let ok_payload = json!({ "type": "CommandExecution", "id": "call_1",
        "command": ["/bin/bash", "-lc", "cat hello.txt"],
        "cwd": "file:///home/u/oss/toolpath", "status": "completed",
        "stdout": "hello codex\n", "stderr": "", "aggregated_output": "hello codex\n",
        "exit_code": 0 });
    let mut lines = current_head();
    lines.push(item_completed(ok_payload));
    let s = parse_codex_session(&rollout(&lines));

    assert_eq!(s.events.len(), 1);
    assert_eq!(s.events[0].role, ROLE_TOOL);
    assert_eq!(
        s.events[0].text,
        "exec · /bin/bash -lc cat hello.txt · hello codex"
    );
    assert_eq!(s.events[0].tool_use_id.as_deref(), Some("call_1"));
    assert!(!s.events[0].error, "退出码 0 不是失败");

    // 边界：退出码非零 → error = true（展示层据此加失败标记）
    let fail_payload = json!({ "type": "CommandExecution", "id": "call_2",
        "command": ["/bin/bash", "-lc", "cat nope.txt"],
        "aggregated_output": "nope.txt: No such file or directory",
        "exit_code": 1 });
    let mut lines = current_head();
    lines.push(item_completed(fail_payload));
    let s = parse_codex_session(&rollout(&lines));
    assert!(
        s.events[0].error,
        "退出码非零必须标 error，否则失败命令看起来和成功命令一样"
    );
}

/// 契约 I-6：`token_usage_record` 与 `token_count` 是**同源记账**（实机 1:1），
/// 只取后者——两处都取会让每个会话的 token 翻倍
///
/// 变异探针：把 `token_usage_record` 也接进聚合 → input 从 100 变 200 转红。
#[test]
fn token_usage_record_does_not_double_count() {
    let usage = json!({ "input_tokens": 120, "cached_input_tokens": 20,
                        "cache_write_input_tokens": 0, "output_tokens": 10,
                        "reasoning_output_tokens": 4, "total_tokens": 130 });
    let content = format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.000Z", "type": "session_meta",
            "payload": { "id": "tk", "cwd": "/w" } }))
        .unwrap(),
        serde_json::to_string(&event_msg(json!({ "type": "token_count",
            "info": { "last_token_usage": usage } })))
        .unwrap(),
        serde_json::to_string(&serde_json::json!({
            "timestamp": "2026-10-04T04:30:39.200Z", "ordinal": 12,
            "type": "token_usage_record",
            "payload": { "thread_id": "tk", "turn_id": "t1",
                         "usage": usage, "turn_token_usage": usage,
                         "thread_token_usage": usage } }))
        .unwrap(),
    );
    let s = parse_codex_session(&content);
    // input_tokens 含缓存 20 → 非缓存 100
    assert_eq!(s.tokens.input, 100, "只计一次 token_count");
    assert_eq!(s.tokens.cache_read, 20);
    assert_eq!(s.tokens.output, 10);
    assert_eq!(s.tokens.reasoning, 4);
}

/// 契约 I-7（边界）：权威面事件也受 `MAX_EVENTS` 上限约束——
/// 防御异常巨大的会话撑爆 WATM 边界序列化
#[test]
fn item_face_events_respect_event_cap() {
    use crate::usage_parse::MAX_EVENTS;
    let mut lines = current_head();
    for i in 0..(MAX_EVENTS + 10) {
        lines.push(item_completed(json!({ "type": "UserMessage",
            "id": format!("u{i}"),
            "content": [{ "type": "text", "text": format!("第 {i} 句") }] })));
    }
    let s = parse_codex_session(&rollout(&lines));
    assert_eq!(s.events.len(), MAX_EVENTS);
    assert!(
        s.events_truncated,
        "超上限必须置 truncated 标志（前端据此提示）"
    );
}

/// 契约 I-5（取值优先级）：命令输出的取值链
/// `aggregated_output` → `formatted_output` → `stdout`，首个**非空**者胜出
///
/// 变异探针：删掉 `formatted_output` 一级 → 反例用例转红（实机
/// `unified_exec_startup` 来源的命令只填 `formatted_output`）。
#[test]
fn command_execution_output_falls_back_through_sources() {
    let mut lines = current_head();
    // 反例：只有 formatted_output 有值（stdout 为空串）
    lines.push(item_completed(
        json!({ "type": "CommandExecution", "id": "c1",
        "command": ["bash"], "stdout": "", "stderr": "",
        "formatted_output": "只填了 formatted_output", "exit_code": 0 }),
    ));
    // 正例：aggregated_output 优先
    lines.push(item_completed(
        json!({ "type": "CommandExecution", "id": "c2",
        "command": ["bash"], "stdout": "stdout 值",
        "aggregated_output": "aggregate 值", "formatted_output": "formatted 值",
        "exit_code": 0 }),
    ));
    // 边界：三者皆空 → 命令段收尾，不留悬空分隔符
    lines.push(item_completed(
        json!({ "type": "CommandExecution", "id": "c3",
        "command": ["bash"], "exit_code": 0 }),
    ));
    let s = parse_codex_session(&rollout(&lines));

    let t: Vec<&str> = texts(&s);
    assert_eq!(t[0], "exec · bash · 只填了 formatted_output");
    assert_eq!(t[1], "exec · bash · aggregate 值");
    assert_eq!(t[2], "exec · bash · ");
}
