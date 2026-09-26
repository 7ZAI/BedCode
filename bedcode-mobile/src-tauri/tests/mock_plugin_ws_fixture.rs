//! 假插件端点夹具自检（专项票 01 P0：基线与假插件端点夹具）
//!
//! 本文件只测夹具本身（启停 / 认证门接受·拒绝 / 帧形状逐字段锁 / 客户端数观测 /
//! 事件广播 / 终端控制帧与二进制分片 / 未知端点 / 端口释放），不经任何移动端业务
//! 代码——属单测性质，本票内可跑（`cargo test mock_plugin_ws`）。驱动移动端真实
//! 链路的集成测试由票 03/05 编写、票 07 统一运行。
//!
//! 帧形状断言按 `serde_json::Value` 语义（键序无关），键名与桌面 wire 逐字锁定
//! （`wasm-apps/terminal-session/rust/src/ws_{control,terminal}.rs`）；旧 TB v3 的
//! `from_offset` / `history_end` / per-frame offset 为禁令键，出现在任一帧即红。

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[path = "support/mock_plugin_ws.rs"]
mod mock_plugin_ws;

use mock_plugin_ws::*;

/// 单次帧接收超时（轮询等待防卡死）
const RECV_TIMEOUT: Duration = Duration::from_secs(2);

type TestWs = WebSocketStream<MaybeTlsStream<TcpStream>>;

// ==================== 测试助手 ====================

/// 建立 WS 连接（夹具自身的客户端视角）
async fn connect(url: &str) -> TestWs {
    let (ws, _resp) = connect_async(url).await.expect("ws connect");
    ws
}

/// 收下一条帧（超时 panic）
async fn recv(ws: &mut TestWs) -> WsMsg {
    tokio::time::timeout(RECV_TIMEOUT, ws.next())
        .await
        .expect("recv timeout")
        .expect("stream ended")
        .expect("ws frame error")
}

/// 发送一条 JSON 文本帧
async fn send_json(ws: &mut TestWs, v: serde_json::Value) {
    ws.send(WsMsg::Text(v.to_string().into())).await.unwrap();
}

/// 从帧取关闭码（非 Close 帧返回 None）
fn close_code(msg: &WsMsg) -> Option<u16> {
    match msg {
        WsMsg::Close(Some(cf)) => Some(cf.code.into()),
        _ => None,
    }
}

/// 把收到的文本帧解析为 Value
fn as_json(msg: &WsMsg) -> serde_json::Value {
    let WsMsg::Text(t) = msg else {
        panic!("expected text frame, got {msg:?}");
    };
    serde_json::from_str(t).expect("text frame must be JSON")
}

/// 轮询等待条件成立（连接数递减等异步收敛场景）
async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
    let deadline = std::time::Instant::now() + RECV_TIMEOUT;
    loop {
        if cond() {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// 等夹具记录到指定文本帧序列（连接任务异步处理帧——读侧必须先等，防竞态）
async fn wait_for_recorded_text(
    server: &MockPluginWsServer,
    endpoint: &str,
    expect: Vec<serde_json::Value>,
    what: &str,
) {
    let deadline = std::time::Instant::now() + RECV_TIMEOUT;
    loop {
        if server.received_text(endpoint).await == expect {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for recorded frames ({what}): expect {expect:?}");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ==================== 启停与端口 ====================

/// 启停：随机端口 + shutdown 后同一地址可重新绑定（端口确定性释放）
#[tokio::test]
async fn mock_plugin_ws_start_shutdown_releases_port() {
    let server = MockPluginWsServer::start().await;
    assert_ne!(server.addr().port(), 0, "必须使用 OS 分配端口，避免与真实桌面实例冲突");
    let addr = server.addr();
    server.shutdown().await;
    // listener 随 accept 任务终止而关闭：立即重新绑定同一地址应成功
    let rebound = TcpListener::bind(addr).await;
    assert!(rebound.is_ok(), "shutdown 后端口必须释放，可重新绑定 {addr}");
}

/// 启停：并发实例端口互不冲突（每次 start 都是独立随机端口）
#[tokio::test]
async fn mock_plugin_ws_separate_instances_get_distinct_ports() {
    let a = MockPluginWsServer::start().await;
    let b = MockPluginWsServer::start().await;
    assert_ne!(a.addr(), b.addr(), "两个实例必须分配不同端口");
    let (pa, pb) = (a.addr(), b.addr());
    a.shutdown().await;
    b.shutdown().await;
    assert!(TcpListener::bind(pa).await.is_ok());
    assert!(TcpListener::bind(pb).await.is_ok());
}

// ==================== 认证门（接受 / 拒绝） ====================

/// 默认策略（RequireAnyToken）：合法 auth 首帧通过，连接保持、事件可达；
/// 认证帧被夹具记录（「校验首帧」可观测）
#[tokio::test]
async fn mock_plugin_ws_auth_gate_accepts_default_policy() {
    let server = MockPluginWsServer::start().await;
    let mut client = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;

    send_json(&mut client, auth_frame("jwt-123")).await;

    // 注入事件帧：连接保持 + 广播可达
    let payload = serde_json::json!({ "session_id": "s1", "source_device": "phone" });
    server.send_event(EVENT_SESSION_CREATED, payload.clone()).await;
    let msg = recv(&mut client).await;
    assert_eq!(as_json(&msg), event_frame(EVENT_SESSION_CREATED, payload));

    // 夹具记录到 auth 首帧，且连接数=1（记录发生在连接任务内，先轮询等齐再断言）
    wait_for_recorded_text(
        &server,
        ENDPOINT_SESSION_CONTROL,
        vec![auth_frame("jwt-123")],
        "auth 首帧记录",
    )
    .await;
    assert_eq!(server.connected(ENDPOINT_SESSION_CONTROL), 1);
}

/// RequireToken：token 精确匹配才放行
#[tokio::test]
async fn mock_plugin_ws_auth_gate_accepts_expected_token() {
    let server = MockPluginWsServer::start().await;
    server.set_auth_policy(ENDPOINT_SESSION_CONTROL, AuthPolicy::RequireToken("secret".to_string()));
    let mut client = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;

    send_json(&mut client, auth_frame("secret")).await;
    server.send_event(EVENT_SESSION_CREATED, serde_json::json!({"session_id": "s1"})).await;
    let msg = recv(&mut client).await; // 未关闭 → 事件可达
    assert_eq!(as_json(&msg).get("type"), Some(&serde_json::Value::String("event".to_string())));
}

/// RequireToken 不匹配 / RejectAll：认证被拒 → close 4001（对齐桌面宿主坏 token 关闭码）
#[tokio::test]
async fn mock_plugin_ws_auth_gate_rejects_bad_token_and_reject_all() {
    let server = MockPluginWsServer::start().await;
    server.set_auth_policy(ENDPOINT_SESSION_CONTROL, AuthPolicy::RequireToken("correct".to_string()));
    let mut client = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;

    send_json(&mut client, auth_frame("wrong")).await;
    let msg = recv(&mut client).await;
    assert_eq!(
        close_code(&msg),
        Some(AUTH_REJECT_CLOSE_CODE),
        "坏 token 必须 close 4001；实际收到 {msg:?}"
    );
    // 被拒绝的帧仍被记录（帧先记录后判定）
    wait_for_recorded_text(
        &server,
        ENDPOINT_SESSION_CONTROL,
        vec![auth_frame("wrong")],
        "拒绝场景仍记录 auth 帧",
    )
    .await;

    // RejectAll：合法形状的 auth 同样拒绝
    server.set_auth_policy(ENDPOINT_SESSION_CONTROL, AuthPolicy::RejectAll);
    let mut client2 = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    send_json(&mut client2, auth_frame("any-token")).await;
    let msg = recv(&mut client2).await;
    assert_eq!(close_code(&msg), Some(AUTH_REJECT_CLOSE_CODE));
}

/// 非 auth 首帧（协议违规）：close 4001
#[tokio::test]
async fn mock_plugin_ws_auth_gate_requires_auth_as_first_frame() {
    let server = MockPluginWsServer::start().await;
    let mut client = connect(&server.url(ENDPOINT_TERMINAL)).await;

    // 上来就发 subscribe（未认证）：首帧必须是 auth
    send_json(&mut client, subscribe_frame("s1", Some("live"))).await;
    let msg = recv(&mut client).await;
    assert_eq!(close_code(&msg), Some(AUTH_REJECT_CLOSE_CODE), "非 auth 首帧必须拒绝");

    // 非 JSON 首帧同样拒绝
    let mut client2 = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    client2.send(WsMsg::Text("not json".to_string().into())).await.unwrap();
    let msg = recv(&mut client2).await;
    assert_eq!(close_code(&msg), Some(AUTH_REJECT_CLOSE_CODE));
}

// ==================== 客户端数观测 ====================

/// 连接数：按端点分别计数；断开后递减；累计握手数在重连后递增
#[tokio::test]
async fn mock_plugin_ws_client_count_observed_per_endpoint() {
    let server = MockPluginWsServer::start().await;
    assert_eq!(server.connected(ENDPOINT_SESSION_CONTROL), 0);
    assert_eq!(server.connected(ENDPOINT_TERMINAL), 0);

    let mut sc = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    send_json(&mut sc, auth_frame("t1")).await;
    assert_eq!(server.connected(ENDPOINT_SESSION_CONTROL), 1);
    assert_eq!(server.total_accepted(ENDPOINT_SESSION_CONTROL), 1);

    let mut term = connect(&server.url(ENDPOINT_TERMINAL)).await;
    send_json(&mut term, auth_frame("t2")).await;
    assert_eq!(server.connected(ENDPOINT_TERMINAL), 1);
    assert_eq!(server.connected(ENDPOINT_SESSION_CONTROL), 1, "两端点独立计数");

    // 断开 → 连接数递减（夹具在连接任务退出时摘除）
    drop(sc);
    wait_until(|| server.connected(ENDPOINT_SESSION_CONTROL) == 0, "session-control 连接数归零").await;

    // 重连 → 累计握手数递增（票 03 断线自愈断言会用到）
    let mut sc2 = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    send_json(&mut sc2, auth_frame("t3")).await;
    assert_eq!(server.total_accepted(ENDPOINT_SESSION_CONTROL), 2);
    assert_eq!(server.connected(ENDPOINT_SESSION_CONTROL), 1);
}

// ==================== 事件广播（session-control） ====================

/// 事件帧广播：多个客户端收到逐字节一致的 `{"type":"event",...}` 帧
#[tokio::test]
async fn mock_plugin_ws_event_broadcast_reaches_all_clients() {
    let server = MockPluginWsServer::start().await;
    let mut c1 = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    let mut c2 = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    send_json(&mut c1, auth_frame("t1")).await;
    send_json(&mut c2, auth_frame("t2")).await;

    let payload = serde_json::json!({
        "session_id": "s1",
        "task_status": "running",
        "task_reason": "approved",
    });
    server.send_event(EVENT_TASK_STATUS_CHANGED, payload.clone()).await;

    let expected = event_frame(EVENT_TASK_STATUS_CHANGED, payload);
    assert_eq!(as_json(&recv(&mut c1).await), expected);
    assert_eq!(as_json(&recv(&mut c2).await), expected);
}

/// 7 类业务事件均可经 send_event 注入（事件名与帧壳形状自检见帧形状用例）
#[tokio::test]
async fn mock_plugin_ws_all_seven_business_events_injectable() {
    let server = MockPluginWsServer::start().await;
    let mut client = connect(&server.url(ENDPOINT_SESSION_CONTROL)).await;
    send_json(&mut client, auth_frame("t")).await;

    for (i, name) in ALL_BUSINESS_EVENTS.iter().enumerate() {
        let payload = serde_json::json!({ "seq": i });
        server.send_event(name, payload.clone()).await;
        let msg = recv(&mut client).await;
        assert_eq!(as_json(&msg), event_frame(name, payload), "事件名必须逐字注入");
    }
}

// ==================== 终端端点（terminal） ====================

/// 终端文本帧记录 + 二进制分片按序回放 + 控制帧形状
#[tokio::test]
async fn mock_plugin_ws_terminal_frames_recorded_and_binary_fragments_replayed() {
    let server = MockPluginWsServer::start().await;
    let mut client = connect(&server.url(ENDPOINT_TERMINAL)).await;
    send_json(&mut client, auth_frame("t")).await;

    // 客户端 → 插件：订阅 / ack / resync / input / poll / unsubscribe 全记录
    send_json(&mut client, subscribe_frame("s1", Some("poll"))).await;
    send_json(&mut client, ack_frame(4096)).await;
    send_json(&mut client, resync_frame(8192)).await;
    send_json(&mut client, terminal_input_frame("ls -la")).await;
    send_json(&mut client, poll_frame()).await;
    send_json(&mut client, unsubscribe_frame()).await;

    let mut expect_frames = vec![auth_frame("t")]; // auth 首帧也在记录流中（首元素）
    expect_frames.extend([
        subscribe_frame("s1", Some("poll")),
        ack_frame(4096),
        resync_frame(8192),
        terminal_input_frame("ls -la"),
        poll_frame(),
        unsubscribe_frame(),
    ]);
    // 订阅/ack/resync/input/poll/unsubscribe 逐帧记录（键名不可漂移；记录异步，轮询等齐）
    wait_for_recorded_text(&server, ENDPOINT_TERMINAL, expect_frames, "终端 6 帧记录").await;

    // 二进制输入帧（原始控制字节）单独记录
    client.send(WsMsg::Binary(vec![0x03].into())).await.unwrap(); // Ctrl-C
    let binary = server.wait_for_binary(ENDPOINT_TERMINAL, |b| !b.is_empty(), RECV_TIMEOUT).await;
    assert_eq!(binary, vec![vec![0x03]], "binary 输入按到达序记录");

    // 插件 → 客户端：二进制输出分片（裸字节，无帧头/无 per-frame offset）+ 控制帧
    server.send_binary(ENDPOINT_TERMINAL, b"\x1b[31mred text").await;
    server.send_binary(ENDPOINT_TERMINAL, b" tail").await;
    server.send_text(ENDPOINT_TERMINAL, &subscribed_frame("s1", "poll")).await;
    server.send_text(ENDPOINT_TERMINAL, &ring_resync_frame(5000)).await;
    server.send_text(ENDPOINT_TERMINAL, &session_stopped_frame("s1", "stopped", Some(0))).await;

    let m1 = recv(&mut client).await;
    assert_eq!(m1, WsMsg::Binary(vec![0x1b, b'[', b'3', b'1', b'm', b'r', b'e', b'd', b' ', b't', b'e', b'x', b't'].into()), "输出=裸字节，逐字节一致");
    let m2 = recv(&mut client).await;
    assert_eq!(m2, WsMsg::Binary(b" tail".to_vec().into()), "分片按序到达");
    let m3 = recv(&mut client).await;
    assert_eq!(as_json(&m3), subscribed_frame("s1", "poll"));
    let m4 = recv(&mut client).await;
    assert_eq!(as_json(&m4), ring_resync_frame(5000));
    let m5 = recv(&mut client).await;
    assert_eq!(as_json(&m5), session_stopped_frame("s1", "stopped", Some(0)));

    // 畸形服务端帧可注入且不破坏连接（消费端丢弃+留痕行为属票 03 单测；此处只验证可注入）
    server.send_raw_text(ENDPOINT_TERMINAL, "not json at all").await;
    let raw = recv(&mut client).await;
    assert!(
        matches!(raw, WsMsg::Text(t) if t == "not json at all"),
        "畸形帧原样可达（夹具不做 JSON 校验拦截）"
    );
    server.send_text(ENDPOINT_TERMINAL, &terminal_error_frame("会话不存在")).await;
    let err = recv(&mut client).await;
    assert_eq!(as_json(&err), terminal_error_frame("会话不存在"), "error 帧形状锁死");
}

/// 停止帧：exitCode 缺省（None）时键不出现（桌面仅在可用时携带）
#[tokio::test]
async fn mock_plugin_ws_session_stopped_exit_code_optional() {
    let with_code = session_stopped_frame("s1", "killed", Some(137));
    assert_eq!(with_code["exitCode"], 137);
    assert_eq!(with_code["reason"], "killed");

    let without = session_stopped_frame("s1", "error", None);
    assert!(without.get("exitCode").is_none(), "exitCode 缺省即不出现键（不伪造空值）");
    assert_eq!(without["sessionId"], "s1");
}

// ==================== 未知端点 ====================

/// 未声明路径：握手成功但立即 close 1008（模拟路由 404 语义）
#[tokio::test]
async fn mock_plugin_ws_unknown_endpoint_closed_with_1008() {
    let server = MockPluginWsServer::start().await;
    let url = format!("ws://{}:{}/ws/plugin/{}/no-such-endpoint", server.addr().ip(), server.addr().port(), PLUGIN_ID);
    let mut client = connect(&url).await;
    let msg = recv(&mut client).await;
    assert_eq!(close_code(&msg), Some(UNKNOWN_ENDPOINT_CLOSE_CODE), "未知端点必须 close 1008");
}

// ==================== 帧形状自检（字段名漂移即测试红） ====================

/// 断言 JSON 对象键集合（不含任何其余键）
fn assert_keys(v: &serde_json::Value, expected: &[&str]) {
    let obj = v.as_object().expect("frame must be JSON object");
    let mut actual: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "帧键集合漂移：{v}");
}

/// 逐字段锁 spec §3.2/§3.3 帧表 + 禁令键（旧 TB v3 from_offset/history_end/per-frame offset）
///
/// 帧形状单一出处 = 本夹具构造函数；桌面 wire 一旦漂移（或夹具写错键名），本测试即红。
#[test]
fn mock_plugin_ws_frame_shapes_match_spec() {
    // ---- spec §3.2：认证 + 事件帧壳 ----
    assert_keys(&auth_frame("jwt"), &["type", "token"]);
    assert_eq!(auth_frame("jwt")["type"], "auth");
    assert_eq!(auth_frame("jwt")["token"], "jwt");
    assert_keys(&event_frame("x", serde_json::json!({})), &["type", "event", "payload"]);

    // 7 类业务事件名逐字（spec §3.1 表）
    let expected_names = [
        "session:created",
        "session:stopped",
        "session:removed",
        "task:status-changed",
        "session:mode-changed",
        "task:queue-changed",
        "task:scheduled-changed",
    ];
    assert_eq!(ALL_BUSINESS_EVENTS.as_slice(), &expected_names[..], "7 类事件名不可漂移");
    for name in ALL_BUSINESS_EVENTS {
        assert_keys(&event_frame(name, serde_json::json!({})), &["type", "event", "payload"]);
        assert_eq!(event_frame(name, serde_json::json!({}))["event"], name);
    }

    // ---- spec §3.3：终端控制帧（桌面 wire 字面量：sessionId/exitCode 为 camelCase） ----
    assert_keys(&subscribe_frame("s1", Some("live")), &["type", "sessionId", "mode"]);
    assert_keys(&subscribe_frame("s1", None), &["type", "sessionId"]); // mode 缺省即省略键
    assert_keys(&unsubscribe_frame(), &["type"]);
    assert_keys(&ack_frame(100), &["type", "offset"]);
    assert_keys(&resync_frame(100), &["type", "offset"]);
    assert_keys(&terminal_input_frame("hi"), &["type", "data"]);
    assert_keys(&poll_frame(), &["type"]);
    assert_keys(&subscribed_frame("s1", "live"), &["type", "sessionId", "mode"]);
    assert_keys(&unsubscribed_frame(), &["type"]);
    assert_keys(&ring_resync_frame(100), &["type", "offset"]);
    assert_keys(&session_stopped_frame("s1", "stopped", Some(0)), &["type", "sessionId", "reason", "exitCode"]);
    assert_keys(&session_stopped_frame("s1", "stopped", None), &["type", "sessionId", "reason"]);
    assert_keys(&terminal_error_frame("boom"), &["type", "message"]);

    // ---- 禁令键：任何帧不得带旧 TB v3 / 旧信封残留字段 ----
    let forbidden = ["from_offset", "history_end", "message_id", "stage", "payload.binary_offset"];
    let all_frames = [
        auth_frame("jwt"),
        event_frame("session:created", serde_json::json!({})),
        subscribe_frame("s1", Some("live")),
        ack_frame(1),
        subscribed_frame("s1", "live"),
        ring_resync_frame(1),
        session_stopped_frame("s1", "stopped", None),
        terminal_error_frame("e"),
    ];
    for frame in all_frames {
        let flat = frame.to_string();
        for key in &forbidden {
            assert!(
                !flat.contains(&format!("\"{key}\"")),
                "帧不得出现退役字段 {key:?}：{frame}"
            );
        }
    }
}

/// 二进制输出形状：夹具发送的字节 = 客户端原样收到（无帧头、无封装），
/// 且普通文本帧不得混入二进制帧（帧类型不串道）
#[tokio::test]
async fn mock_plugin_ws_binary_output_is_raw_bytes_without_header() {
    let server = MockPluginWsServer::start().await;
    let mut client = connect(&server.url(ENDPOINT_TERMINAL)).await;
    send_json(&mut client, auth_frame("t")).await;

    let arbitrary = b"\x00\x01\xff\x1b[2J\x00tail";
    server.send_binary(ENDPOINT_TERMINAL, arbitrary).await;
    let msg = recv(&mut client).await;
    match msg {
        WsMsg::Binary(bytes) => assert_eq!(bytes, arbitrary.to_vec(), "输出 = 逐字节裸数据，无 16B TB v3 头"),
        other => panic!("输出必须是二进制帧，实际 {other:?}"),
    }

    // 帧类型不串道：binary 之后注入的文本帧仍是文本帧
    server.send_text(ENDPOINT_TERMINAL, &subscribed_frame("s1", "live")).await;
    let msg = recv(&mut client).await;
    assert!(matches!(msg, WsMsg::Text(_)), "文本控制帧不得被包装成 binary");
}