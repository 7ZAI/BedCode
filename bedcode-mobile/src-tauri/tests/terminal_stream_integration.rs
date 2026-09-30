//! 终端流新协议闭环集成测试（专项票 05 P4 用例登记 / 票 07 P6 统一运行）
//!
//! 驱动真实链路（`TerminalLinkManager` → `TerminalLink` → `link_io` → 真实
//! WS 客户端 → 票 01 假插件端点 `MockPluginWsServer` 的 terminal 端点），验证
//! 协议闭环：认证 → fresh subscribe（回放环窗口）→ 回放/实时裸字节 → ack 水位
//! → `ring_resync` 重锚 → `session_stopped`；输入 text/binary 双形态。
//!
//! 链路 → 前端事件经 `TerminalEventSink` trait 注入记录替身（票 07 emitter
//! trait 化——集成测试无法构造真实 `AppHandle`：`tauri::test::mock_app` 是
//! `MockRuntime`，与终端命令的 Wry 泛型不匹配；生产路径 `AppHandle` impl 行为
//! 等价，同样 emit `terminal-state` / `terminal-resync`）。
//!
//! 全局态注意：`terminal_link_manager` 与 `state::get_connection_manager` 都是
//! 进程级单例（target / token / 链路表跨用例共享），本二进制内全部用例必须
//! **串行**——单入口 suite 按序驱动，每场景尾部清理 target / token / 链路残留。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bedcode_mobile_lib::state::{clear_global_token, get_connection_manager, set_global_token};
use bedcode_mobile_lib::terminal_link::{
    terminal_ack_rendered, terminal_send_input, terminal_unsubscribe_all, terminal_link_manager,
    TerminalEventSink,
};
use tauri::ipc::{Channel, InvokeResponseBody};

#[path = "support/mock_plugin_ws.rs"]
mod mock_plugin_ws;

use mock_plugin_ws::{ENDPOINT_TERMINAL, MockPluginWsServer};

const SESSION_ID: &str = "s1";
const MOCK_TOKEN: &str = "mock-jwt-token";
const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

/// 事件记录替身（`TerminalEventSink`）：记录 (event, payload)，供断言链路状态流转
#[derive(Default)]
struct RecorderSink {
    events: Mutex<Vec<(String, serde_json::Value)>>,
}

impl TerminalEventSink for RecorderSink {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.events.lock().unwrap().push((event.to_string(), payload));
        Ok(())
    }
}

impl RecorderSink {
    fn snapshot(&self) -> Vec<(String, serde_json::Value)> {
        self.events.lock().unwrap().clone()
    }

    /// terminal-state 的 detail 序列（可按序断言状态流转）
    fn state_details(&self) -> Vec<String> {
        self.snapshot()
            .iter()
            .filter(|(e, _)| e == "terminal-state")
            .filter_map(|(_, p)| p.get("detail").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    }

    fn terminal_resync_offsets(&self) -> Vec<u64> {
        self.snapshot()
            .iter()
            .filter(|(e, _)| e == "terminal-resync")
            .filter_map(|(_, p)| p.get("offset").and_then(|v| v.as_u64()))
            .collect()
    }
}

/// 轮询等待链路产生满足谓词的事件（超时 panic；防卡死）
async fn wait_sink(
    sink: &Arc<RecorderSink>,
    mut pred: impl FnMut(&[(String, serde_json::Value)]) -> bool,
    what: &str,
) {
    let deadline = std::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        if pred(&sink.snapshot()) {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for {what} (events: {:#?})", sink.snapshot());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 模拟终端页在场：`page_subscribe` 置 `frontend_subscribed=true`，输出进入
/// ingest 路径（pending_ack 推进 → 流控 ack 可发）；伪 channel 丢弃输出帧
/// （无真实 WebView，handler 恒 Ok）
fn page_attach(session_id: &str) {
    let channel = Channel::<InvokeResponseBody>::new(|_| Ok(()));
    terminal_link_manager().page_subscribe(session_id, channel);
}

/// 订阅会话并等待 mock 端收到 auth + subscribe 帧（链路已建立）
async fn subscribe_and_wait_ready(server: &MockPluginWsServer, sink: Arc<RecorderSink>) {
    bedcode_mobile_lib::terminal_link::terminal_link_manager().subscribe(sink, SESSION_ID.to_string());

    // 链路建连 → 认证 → 订阅；夹具按到达序记录（auth 先于 subscribe）
    server
        .wait_for_text(ENDPOINT_TERMINAL, |v| v.get("type") == Some(&serde_json::json!("auth")), WAIT_TIMEOUT)
        .await;
    server
        .wait_for_text(
            ENDPOINT_TERMINAL,
            |v| {
                v.get("type") == Some(&serde_json::json!("subscribe"))
                    && v.get("sessionId") == Some(&serde_json::json!(SESSION_ID))
            },
            WAIT_TIMEOUT,
        )
        .await;
}

/// 场景清理：摘除全部链路 + target 置不可达 + 清 token（避免污染下一场景）。
/// server 不显式 shutdown——`MockPluginWsServer` 的 Drop 兜底 abort 任务并释放端口。
async fn cleanup(_server: &MockPluginWsServer) {
    let _ = terminal_unsubscribe_all().await;
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), 0, None)
        .await;
    clear_global_token();
}

/// 场景 1：订阅 → 回放/实时 → subscribed 门控 → ack 水位
async fn scenario_subscribe_replay_live_ack() {
    let server = MockPluginWsServer::start().await;
    set_global_token(MOCK_TOKEN);
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let sink = Arc::new(RecorderSink::default());
    subscribe_and_wait_ready(&server, sink.clone()).await;
    // 终端页在场（ack 流控驱动条件：frontend_subscribed=true）
    page_attach(SESSION_ID);

    // 注入回放 + 实时分片（裸字节，无帧头）
    server.send_binary(ENDPOINT_TERMINAL, b"hello ").await;
    server.send_binary(ENDPOINT_TERMINAL, b"world").await;

    // subscribed 回包 → live 门控置位
    server
        .send_text(ENDPOINT_TERMINAL, &mock_plugin_ws::subscribed_frame(SESSION_ID, "live"))
        .await;
    wait_sink(
        &sink,
        |ev| ev.iter().any(|(e, p)| e == "terminal-state" && p["phase"] == "live"),
        "phase=live (subscribed)",
    )
    .await;
    let details = sink.state_details();
    assert!(
        details.iter().any(|d| d == "subscribed"),
        "订阅回包后应产出 terminal-state(detail=subscribed)，实际：{details:?}"
    );

    // ack 水位：前端提交已渲染字节 11 → 注入 ≥64KB（threshold 分支）触发首个
    // ack{offset=11}（last_ack_at=0 时空闲兜底不生效，需阈值触发）
    terminal_ack_rendered(SESSION_ID.to_string(), 11)
        .await
        .expect("ack rendered");
    server.send_binary(ENDPOINT_TERMINAL, &[b'x'; 70_000]).await;
    server
        .wait_for_text(
            ENDPOINT_TERMINAL,
            |v| v.get("type") == Some(&serde_json::json!("ack")) && v.get("offset") == Some(&serde_json::json!(11)),
            WAIT_TIMEOUT,
        )
        .await;

    cleanup(&server).await;
}

/// 场景 2：输入双形态 —— text（UTF-8 无转义损失）与 special_key（binary pty 字节）
async fn scenario_input_dual_mode() {
    let server = MockPluginWsServer::start().await;
    set_global_token(MOCK_TOKEN);
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let sink = Arc::new(RecorderSink::default());
    subscribe_and_wait_ready(&server, sink.clone()).await;
    server
        .send_text(ENDPOINT_TERMINAL, &mock_plugin_ws::subscribed_frame(SESSION_ID, "live"))
        .await;
    wait_sink(
        &sink,
        |ev| ev.iter().any(|(e, p)| e == "terminal-state" && p["phase"] == "live"),
        "live",
    )
    .await;

    // text：UTF-8 原文（含空格/引号转义）
    terminal_send_input(SESSION_ID.to_string(), "echo \"hi you\"".to_string(), None)
        .await
        .expect("send text input");
    server
        .wait_for_text(
            ENDPOINT_TERMINAL,
            |v| v.get("type") == Some(&serde_json::json!("input")) && v.get("data") == Some(&serde_json::json!("echo \"hi you\"")),
            WAIT_TIMEOUT,
        )
        .await;

    // special_key：ctrl+c → KeyCombo::to_pty_bytes → 二进制帧 [0x03]
    terminal_send_input(SESSION_ID.to_string(), String::new(), Some("ctrl+c".to_string()))
        .await
        .expect("send special key");
    server
        .wait_for_binary(ENDPOINT_TERMINAL, |b| b == &[0x03], WAIT_TIMEOUT)
        .await;
    // 帧序：text input 帧在前、binary 在后（按到达序断言）
    let bins = server.received_binary(ENDPOINT_TERMINAL).await;
    assert_eq!(bins, vec![vec![0x03]], "特殊键 binary 帧应只有一次 ctrl+c 的 pty 字节");

    cleanup(&server).await;
}

/// 场景 3：ring_resync 唯一重锚 —— 事件带环偏移 + 本地计数基准归零（后续 ack 照常）
async fn scenario_ring_resync_reanchors() {
    let server = MockPluginWsServer::start().await;
    set_global_token(MOCK_TOKEN);
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let sink = Arc::new(RecorderSink::default());
    subscribe_and_wait_ready(&server, sink.clone()).await;
    page_attach(SESSION_ID);
    server
        .send_text(ENDPOINT_TERMINAL, &mock_plugin_ws::subscribed_frame(SESSION_ID, "live"))
        .await;
    wait_sink(
        &sink,
        |ev| ev.iter().any(|(e, p)| e == "terminal-state" && p["phase"] == "live"),
        "live",
    )
    .await;

    // 进入 live 后收到 ring_resync（环淘汰）→ 发射 terminal-resync（offset 透传）
    server
        .send_text(ENDPOINT_TERMINAL, &mock_plugin_ws::ring_resync_frame(4096))
        .await;
    wait_sink(
        &sink,
        |ev| ev.iter().any(|(e, p)| e == "terminal-resync" && p["offset"] == 4096),
        "terminal-resync(offset=4096)",
    )
    .await;
    // pending_resync（subscribe 给新链路打标）会在 subscribed 回包时先发一次
    // offset=0 重锚；本帧（ring_resync）是**最后**一次重锚且 offset 透传
    let offsets = sink.terminal_resync_offsets();
    assert_eq!(
        offsets.last(),
        Some(&4096),
        "ring_resync 应是最后一次重锚且 offset=4096，实际：{offsets:?}"
    );

    // 重锚后 ack 以新基准照常推进：先提交 rendered=12，再注入 ≥64KB 触发阈值 ack
    terminal_ack_rendered(SESSION_ID.to_string(), 12)
        .await
        .expect("ack rendered after resync");
    server.send_binary(ENDPOINT_TERMINAL, &[b'y'; 70_000]).await;
    server
        .wait_for_text(
            ENDPOINT_TERMINAL,
            |v| v.get("type") == Some(&serde_json::json!("ack")) && v.get("offset") == Some(&serde_json::json!(12)),
            WAIT_TIMEOUT,
        )
        .await;

    cleanup(&server).await;
}

/// 场景 4：session_stopped —— 停止帧后链路停推（不再重连），状态事件 phase=idle/detail=stopped
async fn scenario_session_stopped_terminates() {
    let server = MockPluginWsServer::start().await;
    set_global_token(MOCK_TOKEN);
    get_connection_manager()
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let sink = Arc::new(RecorderSink::default());
    subscribe_and_wait_ready(&server, sink.clone()).await;
    server
        .send_text(ENDPOINT_TERMINAL, &mock_plugin_ws::subscribed_frame(SESSION_ID, "live"))
        .await;
    wait_sink(
        &sink,
        |ev| ev.iter().any(|(e, p)| e == "terminal-state" && p["phase"] == "live"),
        "live",
    )
    .await;

    // 尾帧先于停止帧按序到达并消费（帧序保证），随后停止帧
    server.send_binary(ENDPOINT_TERMINAL, b"tail").await;
    server
        .send_text(
            ENDPOINT_TERMINAL,
            &mock_plugin_ws::session_stopped_frame(SESSION_ID, "exited", Some(0)),
        )
        .await;

    wait_sink(
        &sink,
        |ev| {
            ev.iter()
                .any(|(e, p)| e == "terminal-state" && p["phase"] == "idle" && p["detail"] == "stopped")
        },
        "phase=idle/detail=stopped",
    )
    .await;

    // 停止后不再自动重连：等一个重连窗口，确认 mock 无新连接
    let accepted_before = server.total_accepted(ENDPOINT_TERMINAL);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        server.total_accepted(ENDPOINT_TERMINAL),
        accepted_before,
        "session_stopped 后链路不得自动重连"
    );

    cleanup(&server).await;
}

// ==================== 全文件串行入口 ====================

/// 全局 target / token / 链路单例跨用例共享：本二进制内场景必须串行
/// （单入口按序驱动，顺序即文件排列序；场景失败 panic 带文件行号定位）。
#[tokio::test]
async fn terminal_stream_full_suite() {
    scenario_subscribe_replay_live_ack().await;
    scenario_input_dual_mode().await;
    scenario_ring_resync_reanchors().await;
    scenario_session_stopped_terminates().await;
}