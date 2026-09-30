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
//! | C-005 | 插件 `session-stop` + `pty:exit` | 移动端 `stop_session` → 桌面真停 → 移动端链路收到 `session_stopped` 并回落 idle | 正例（终态） |
//! | C-006 | 插件 `session-remove` | 移动端 `remove_session` → 列表不再含该会话 | 正例（清理语义） |
//!
//! 记录替身（`EventRecorder` / `OutputRecorder`）只承接**已真实发生**的数据：
//! 输出字节必须来自桌面 PTY，状态事件必须来自桌面 WS 帧。

mod common;

use std::sync::Arc;

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::terminal_link_manager;

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

    // ==================== C-005 停止 → 移动端链路收到终态 ====================
    sessions
        .stop_session(&base, &session_id)
        .await
        .expect("C-005 停止会话必须成功");
    mobile_ctx::wait_event(&recorder, "terminal-state detail=\"stopped\"", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("stopped"))
    })
    .await;
    let stop_state = recorder.last_state_payload("stopped").expect("C-005 停止事件");
    assert_eq!(
        stop_state["session_id"],
        session_id.as_str(),
        "C-005 停止事件必须携带同一会话 id（防止张冠李戴的恒真断言）"
    );
    assert_eq!(stop_state["phase"], "idle", "C-005 终态后链路相位应回落 idle");

    // ==================== C-006 移除 → 列表不再含该会话 ====================
    sessions
        .remove_session(&base, &session_id)
        .await
        .expect("C-006 移除会话必须成功");
    let after = sessions.list_sessions(&base).await.expect("C-006 移除后可读列表");
    assert!(
        !after.iter().any(|s| s["id"] == session_id),
        "C-006 移除后会话不得仍在列表里（list={after:#?}）"
    );

    // ==================== 收尾 ====================
    terminal_link_manager().remove(&session_id);
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
