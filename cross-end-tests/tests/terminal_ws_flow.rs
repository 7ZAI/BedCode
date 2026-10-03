//! 场景 4：终端流 WS 闭环（最致命盲区——真实 PTY 输出到达移动端）
//!
//! 移动端既有测试的对面是 `tests/support/mock_plugin_ws.rs`（假插件端点），
//! 桌面端既有测试的对面是通用 `tokio-tungstenite` 客户端。两套 mock 都能自洽，
//! 但**「桌面插件的 ring-fetch 输出 → 移动端 TerminalLink 的 ingest 门控 →
//! 页面 Channel」这条真实链路**从未被两端同时跑过。
//!
//! 本文件驱动的是：移动端真实 `SessionHttpClient`（会话控制 HTTP 面）+ 移动端
//! 真实 `TerminalLinkManager`（终端 WS 链路）→ 桌面真实插件 + **真实 bash PTY**。
//!
//! ## 行为契约
//!
//! | 契约 | 来源 | 行为 | 场景 |
//! |---|---|---|---|
//! | C-001 | `SessionHttpClient::start_session` + 插件 `start_session` | 移动端发 `{configId,cols,rows}` → 桌面建真会话 → 返回非空 sessionId | 正例 |
//! | C-002 | 插件 `list_sessions` | 移动端列表里能查到该会话且 `status == "running"` | 正例（登记域真源） |
//! | C-003 | 插件 `ws_terminal` | 移动端链路首帧 auth + subscribe → 桌面回 `subscribed` → 移动端 `terminal-state` detail=`subscribed` 且 phase=`live` | 正例（跨端时序） |
//! | C-004 | 插件 `session-input` + `host-pty` + 插件 ring-fetch | 移动端 HTTP 写入 `echo MARKER` → **真实 PTY 输出字节经 WS 到达移动端页面 Channel** | 正例（核心断言） |
//! | C-005 | 移动端 `terminal_send_input`（**WS 输入面，UI 实际走的那条**）| 「命令文本 + Enter」一次调用 → 命令**被执行**，marker 在输出里出现两次（回显 + 执行）| 正例（**回归锁**：文本曾被静默丢弃，只剩裸回车）|
//! | C-006 | 移动端 `terminal_send_input` 特殊键面 | 键帧单独投递 → `ctrl+u` 真的清掉整行，随后回车执行的是**空行**（被清命令不得执行）| 正例（键帧丢失即变异）|
//! | C-007 | 插件 `session-stop` + `pty:exit` | 移动端 `stop_session` → 桌面真停 → 移动端链路收到 `session_stopped` 并回落 idle | 正例（终态） |
//! | C-008 | 插件 `session-remove` | 移动端 `remove_session` → 列表不再含该会话 | 正例（清理语义） |
//!
//! 记录替身（`EventRecorder` / `OutputRecorder`）只承接**已真实发生**的数据：
//! 输出字节必须来自桌面 PTY，状态事件必须来自桌面 WS 帧。

mod common;

use std::sync::Arc;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::{terminal_link_manager, terminal_send_input};

use common::desktop_ctx;
use common::mobile_ctx;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_pty_output_reaches_mobile_terminal_link() {
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    desktop_ctx::init_app_context().await;
    let (port, handle, server_task) = desktop_ctx::start_server().await;
    mobile_ctx::set_target(port).await;

    let base = mobile_ctx::base_url(port);
    let address = format!("127.0.0.1:{port}");

    // ==================== 认证（真实配对 → 真实 token → 移动端全局） ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "crossend-term-device",
        device_name: "CrossEnd Terminal",
        fingerprint: "crossend-term-fp",
        uid_hash: None,
    };
    let pairing = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect("配对码签发");
    let token = auth
        .verify_pairing_code(&base, ctx, &pairing.pairing_code, &address)
        .await
        .expect("配对码换 token")
        .token;
    // 终端链路首帧认证直接读全局 token（与生产路径同一入口）
    mobile_ctx::remember_token(&token);

    // ==================== C-001 移动端 HTTP 面启动真实会话 ====================
    let config_id = desktop_ctx::seed_shell_config("cross-end-terminal").await;
    let sessions = SessionHttpClient::new();
    let session_id = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("C-001 移动端必须能经桌面真实插件启动会话");
    assert!(!session_id.is_empty(), "C-001 sessionId 不得为空");

    // ==================== C-002 会话出现在移动端读到的列表里 ====================
    let list = sessions.list_sessions(&base).await.expect("C-002 会话列表可读");
    let entry = list
        .iter()
        .find(|s| s["id"] == session_id)
        .unwrap_or_else(|| panic!("C-002 启动的会话必须出现在列表里，list={list:#?}"));
    assert_eq!(entry["status"], "running", "C-002 新建会话应为 running");

    // ==================== C-003 移动端终端链路订阅（真实 WS） ====================
    let recorder = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs = Arc::new(mobile_ctx::OutputRecorder::default());
    // 页面级通道必须**先于**链路建立就位（ingest 门控要求段2 已订阅，
    // 否则输出被计入 dropped 而不会推给页面）
    terminal_link_manager().page_subscribe(&session_id, mobile_ctx::output_channel(outputs.clone()));
    terminal_link_manager().subscribe(recorder.clone(), session_id.clone());

    mobile_ctx::wait_event(&recorder, "terminal-state detail=\"subscribed\"", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;
    let phases = recorder.state_phases();
    assert!(
        phases.contains(&"live".to_string()),
        "C-003 收到 subscribed 后链路相位必须进入 live，实际相位序列={phases:?}"
    );
    assert!(
        terminal_link_manager().is_page_subscribed(&session_id),
        "C-003 页面订阅态应保持（否则 ingest 门控会把输出全丢）"
    );

    // ==================== C-004 真实 PTY 输出字节到达移动端 ====================
    // 选一个不依赖 shell 提示符的 marker：PTY 首次提示符 + 回显都可能带噪声，
    // 断言只看「真实进程执行这条命令后产出的字节确实到了移动端」。
    let marker = "CROSS_END_PTY_MARKER_7F3A";
    sessions
        .send_input(&base, &session_id, &format!("echo {marker}\n"), None)
        .await
        .expect("C-004 输入写入必须成功（桌面真实 PTY 收下）");
    mobile_ctx::wait_output(&outputs, marker).await;
    assert!(
        outputs.text().contains(marker),
        "C-004 移动端必须收到含 marker 的真实 PTY 输出"
    );
    assert!(
        outputs.frame_count() > 0,
        "C-004 输出必须以裸字节帧形态推达页面通道（frame_count=0 说明 ingest 门控未过）"
    );

    // ==================== C-005 WS 输入面：命令文本 + Enter 必须**都**送达 ====================
    //
    // C-004 走的是 HTTP `session-input`，而移动端输入栏 / 快捷命令面板实际走的是
    // WS 终端链路的 `terminal_send_input`（text 帧 + binary 帧）。两条面互不覆盖：
    // 历史缺陷只发生在 WS 面（`if 有键 … else if 有文本` 互斥 → 命令文本被丢弃，
    // 只剩一个裸回车），而 HTTP 面当时是绿的——这正是本文件补它的原因。
    //
    // 判据用「独占一行的 marker」（count_line_occurrences）而不是 marker 出现次数：
    // PTY 回显会在每次提示符重绘时把当前输入行整行重发，按出现次数判会把
    // 「回车丢了、命令根本没执行」误判成通过（变异探针已复现）。
    let ws_marker = "CROSS_END_WS_INPUT_4B2E";
    terminal_send_input(
        session_id.clone(),
        format!("echo {ws_marker}"),
        Some("enter".to_string()),
    )
    .await
    .expect("C-005 WS 输入面投递必须成功（文本帧 + 回车帧）");
    mobile_ctx::wait_until(
        "C-005 命令执行后 marker 独占一行出现在 PTY 输出里",
        || mobile_ctx::count_line_occurrences(&outputs.text(), ws_marker) >= 1,
    )
    .await;

    // ==================== C-006 WS 特殊键面：键帧必须独立生效 ====================
    //
    // 键帧走 binary 通道（`ctrl+u` = kill line）。判据是「被清掉的命令不得执行」：
    // 键入 `echo <marker>`（无回车）→ `ctrl+u` 清行 → 回车执行的是空行 ⇒
    // marker 在输出里**没有独占行**。
    //
    // **为什么不按「输出稳定一窗」再断言**（旧写法，已废）：那是个时间窗竞态——
    // 若ctrl+u 帧丢失，回车会真的执行 echo，其输出要经 PTY → 插件 → WS → 页面通道
    // 才回来，CI 负载下完全可能超过 500ms，于是断言在结果到达前就判「通过」，
    // 变异（ctrl+u 帧丢失）被误报为通过。
    //
    // 改用**哨兵命令做时序屏障**：回车后再发一条 `echo <sentinel>` 并等它**真的执行**。
    // 单 PTY 的字节是有序的，shell 串行执行——哨兵出现在输出里，就证明上一行已被
    // 提交且其输出（如有）已经写完。此时再断言「marker 无独占行」不依赖任何时间窗。
    //
    // 哨兵同时充当**反向自证**（必须有）：它走的是同一条 WS 输入面（文本帧 + 回车帧），
    // 能执行就排除了「全链路丢帧」让上面那条断言恒真的可能。
    let key_marker = "CROSS_END_WS_KEYCTRL_9C1D";
    terminal_send_input(session_id.clone(), format!("echo {key_marker}"), None)
        .await
        .expect("C-006 键入文本帧必须投递成功");
    mobile_ctx::wait_output(&outputs, key_marker).await;
    terminal_send_input(session_id.clone(), String::new(), Some("ctrl+u".to_string()))
        .await
        .expect("C-006 特殊键帧必须投递成功（ctrl+u）");
    terminal_send_input(session_id.clone(), String::new(), Some("enter".to_string()))
        .await
        .expect("C-006 回车帧必须投递成功");

    let sentinel_marker = "CROSS_END_WS_BARRIER_3B7F";
    terminal_send_input(
        session_id.clone(),
        format!("echo {sentinel_marker}"),
        Some("enter".to_string()),
    )
    .await
    .expect("C-006 哨兵命令（时序屏障 + 反向自证）必须投递成功");
    mobile_ctx::wait_until("C-006 哨兵命令必须真的执行（此时上一行的输出已写完）", || {
        mobile_ctx::count_line_occurrences(&outputs.text(), sentinel_marker) >= 1
    })
    .await;

    assert_eq!(
        mobile_ctx::count_line_occurrences(&outputs.text(), key_marker),
        0,
        "C-006 ctrl+u 之后被清掉的命令不得执行（marker 不得以独占行出现）"
    );

    // 反向自证：同一条 WS 输入面此刻必须仍能执行命令（排除「全链路丢帧」的恒真）
    let control_marker = "CROSS_END_WS_CONTROL_5E8A";
    terminal_send_input(
        session_id.clone(),
        format!("echo {control_marker}"),
        Some("enter".to_string()),
    )
    .await
    .expect("C-006 反向自证：WS 输入面必须仍可投递");
    mobile_ctx::wait_until("C-006 反向自证：对照命令必须真的执行", || {
        mobile_ctx::count_line_occurrences(&outputs.text(), control_marker) >= 1
    })
    .await;

    // ==================== C-007 停止 → 移动端链路收到终态 ====================
    sessions
        .stop_session(&base, &session_id)
        .await
        .expect("C-007 停止会话必须成功");
    mobile_ctx::wait_event(&recorder, "terminal-state detail=\"stopped\"", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("stopped"))
    })
    .await;
    let stop_state = recorder.last_state_payload("stopped").expect("C-007 停止事件");
    assert_eq!(
        stop_state["session_id"],
        session_id.as_str(),
        "C-007 停止事件必须携带同一会话 id（防止张冠李戴的恒真断言）"
    );
    assert_eq!(stop_state["phase"], "idle", "C-007 终态后链路相位应回落 idle");

    // ==================== C-008 移除 → 列表不再含该会话 ====================
    sessions
        .remove_session(&base, &session_id)
        .await
        .expect("C-008 移除会话必须成功");
    let after = sessions.list_sessions(&base).await.expect("C-008 移除后可读列表");
    assert!(
        !after.iter().any(|s| s["id"] == session_id),
        "C-008 移除后会话不得仍在列表里（list={after:#?}）"
    );

    // ==================== 收尾 ====================
    terminal_link_manager().remove(&session_id);
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
