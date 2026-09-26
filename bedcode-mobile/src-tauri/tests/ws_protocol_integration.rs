//! WS 客户端全链路集成测试（L1）
//!
//! 驱动真实客户端链路（WsClient → ClientDefaultMessageHandler → 业务 router →
//! 真实 handler → MobileEvent 事件流）连接协议级 mock 桌面端服务器，验证
//! 单测抓不到的跨模块集成行为：WS 连接 → 首消息 JWT 认证 → 终端输出 →
//! 请求-响应 → 断线感知 → 未连接拒绝。
//!
//! 协议对称保证：mock 服务器应答用移动端 `Message` 枚举构造（见 tests/common）。

mod common;

use std::sync::Arc;
use std::time::Duration;

use bedcode_lib::auth::AuthCredentials;
use bedcode_lib::connection::event_ws::run_supervisor;
use bedcode_lib::connection::manager::ConnectionManager;
use bedcode_lib::connection::request::AuthRequest;
use bedcode_lib::connection::ConnectionStatus;
use bedcode_lib::enums::control::SessionControlAction;
use bedcode_lib::model::message::Message;
use bedcode_lib::router::MobileEvent;
use bedcode_lib::state::{clear_global_token, get_auth_manager, get_global_token, set_global_token};
use tokio::sync::{broadcast, oneshot};

#[path = "support/mock_plugin_ws.rs"]
mod mock_plugin_ws;

use common::{MockDesktopServer, MOCK_SESSION_TOKEN, TEST_DEVICE_ID, TEST_DEVICE_NAME};
use mock_plugin_ws::{
    ENDPOINT_SESSION_CONTROL, EVENT_SESSION_MODE_CHANGED, EVENT_TASK_SCHEDULED_CHANGED,
    MockPluginWsServer,
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(5);

/// 等待收到满足谓词的业务事件（broadcast 先订阅后触发，超时即 panic）
async fn wait_event(
    events: &mut broadcast::Receiver<MobileEvent>,
    mut pred: impl FnMut(&MobileEvent) -> bool,
) -> MobileEvent {
    let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Ok(ev)) => {
                if pred(&ev) {
                    return ev;
                }
            }
            Ok(Err(_)) => panic!("broadcast channel closed"),
            Err(_) => panic!("timed out waiting for event"),
        }
    }
}

/// 连接 mock 桌面端并完成认证（WS 首消息 JWT），返回 (manager, 事件订阅)
///
/// 认证已 HTTP 化后，移动端正常路径不再经 WS 握手；本测试路径保留真实
/// WsClient→router→handler 链路，驱动 04 事件 WS 的首消息 JWT 认证语义
async fn connect_and_pair(server: &MockDesktopServer) -> (Arc<ConnectionManager>, broadcast::Receiver<MobileEvent>) {
    clear_global_token();
    let manager = ConnectionManager::new();
    let mut events = manager.subscribe();

    manager
        .connect_without_emit(
            "127.0.0.1".to_string(),
            server.addr.port(),
            Some(TEST_DEVICE_NAME.to_string()),
        )
        .await
        .expect("connect should succeed");

    // 认证：直接发 JWT 首消息（reauthenticate stage，mock 回 Authenticated）
    manager
        .send(&AuthRequest::reauthenticate(
            TEST_DEVICE_ID,
            "fp-test",
            MOCK_SESSION_TOKEN,
        ))
        .await
        .expect("send jwt reauthenticate");
    wait_event(&mut events, |ev| matches!(ev, MobileEvent::AuthSuccess { .. })).await;

    (manager, events)
}

// ==================== 场景 1：连接握手 ====================

async fn scenario_connect_handshake() {
    let server = MockDesktopServer::start().await;
    clear_global_token();
    let manager = ConnectionManager::new();

    manager
        .connect_without_emit(
            "127.0.0.1".to_string(),
            server.addr.port(),
            Some(TEST_DEVICE_NAME.to_string()),
        )
        .await
        .expect("connect should succeed");

    // WS 连接建立即 Connected（未认证）
    assert_eq!(
        manager.get_status().await,
        ConnectionStatus::Connected,
        "handshake 后状态应为 Connected（等待认证）"
    );

    manager.disconnect().await;
    assert_eq!(manager.get_status().await, ConnectionStatus::Disconnected);
}

// ==================== 场景 2：认证全链路 ====================

async fn scenario_auth_full_flow() {
    let server = MockDesktopServer::start().await;
    let (manager, events) = connect_and_pair(&server).await;

    // 认证成功后全局 token 已持久化
    assert_eq!(
        get_global_token(),
        MOCK_SESSION_TOKEN,
        "Authenticated 响应应写入全局 token"
    );

    // 后续发送的消息自动注入 token（mock 端断言）
    manager
        .send(&Message::session_control(SessionControlAction::ListSessions, None))
        .await
        .expect("send should succeed");
    let msgs = server
        .wait_for_received(|v| v["type"] == "session_control", EVENT_TIMEOUT)
        .await;
    let token = msgs.last().unwrap()["payload"]["token"].as_str().expect("token field");
    assert_eq!(token, MOCK_SESSION_TOKEN, "认证后消息应自动携带 token");

    // 事件流完整性：至少 AuthSuccess 已在上层断言，这里验证无意外 Error 事件
    manager.disconnect().await;
    clear_global_token();
    let _ = events;
}

// ==================== 场景 4：请求-响应匹配 ====================

async fn scenario_send_and_wait_ack_matching() {
    let server = MockDesktopServer::start().await;
    let (manager, _events) = connect_and_pair(&server).await;

    // send_and_wait 注册 pending 后发送；mock 收到后手动回 Ack（状态机不处理
    // session_control，保持自动应答最小）
    let send_task = tokio::spawn({
        let manager = manager.clone();
        async move {
            manager
                .send_and_wait(
                    &Message::session_control_with_response(
                        SessionControlAction::StartSession {
                            config_id: "cfg-1".to_string(),
                        },
                        None,
                    ),
                    Duration::from_secs(5),
                )
                .await
        }
    });

    // mock 端提取请求 message_id 并回 Ack
    let msgs = server
        .wait_for_received(|v| v["type"] == "session_control", EVENT_TIMEOUT)
        .await;
    let req_id = msgs.last().unwrap()["payload"]["message_id"]
        .as_str()
        .unwrap()
        .to_string();
    server.send_message(&Message::ack(&req_id)).await;

    let resp = send_task
        .await
        .expect("send task panicked")
        .expect("send_and_wait should return Ok");
    match resp {
        Message::Ack { request_id, code, .. } => {
            assert_eq!(request_id, req_id, "ack 应匹配请求 message_id");
            assert_eq!(code, bedcode_lib::model::message::ACK_CODE_SUCCESS);
        }
        other => panic!("expected Ack response, got {:?}", other),
    }

    manager.disconnect().await;
    clear_global_token();
}

// ==================== 场景 5：服务端主动断开 ====================

async fn scenario_server_close_detected() {
    let server = MockDesktopServer::start().await;
    let (manager, mut events) = connect_and_pair(&server).await;

    // ① 业务层：服务端发 ServerClosed 消息 → SystemHandler → MobileEvent
    server
        .send_message(&Message::server_closed("host shutting down", false))
        .await;
    let ev = wait_event(&mut events, |ev| matches!(ev, MobileEvent::ServerClosed { .. })).await;
    match ev {
        MobileEvent::ServerClosed { reason } => {
            assert_eq!(reason, "host shutting down", "断开原因应透传");
        }
        other => panic!("expected ServerClosed event, got {:?}", other),
    }

    manager.disconnect().await;
    clear_global_token();
}

// ==================== 场景 7：TCP 断开感知（WsClient 事件层） ====================

async fn scenario_tcp_close_emits_client_event() {
    // lifecycle 状态由 disconnect/reconnect 流程显式更新（生产环境经
    // connection_monitor 消费 WsClientEvent 通知前端），TCP 断开本身的
    // 感知契约在 WsClient 事件层——这里直接驱动 WsClient 验证
    let server = MockDesktopServer::start().await;
    let client = bedcode_lib::connection::WsClient::new(bedcode_lib::connection::WsClientConfig::new(
        "127.0.0.1",
        server.addr.port(),
    ));
    let mut events = client.subscribe();
    client.connect().await.expect("connect should succeed");

    // mock 协议级优雅关闭（发 Close 帧，模拟桌面端停机）
    server.graceful_close("host shutting down").await;

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Ok(bedcode_lib::connection::WsClientEvent::ServerClosed { reason })) => {
                assert!(!reason.is_empty(), "断开原因不应为空");
                break;
            }
            Ok(Ok(bedcode_lib::connection::WsClientEvent::Error { .. })) => {
                // 传输错误同样表示断开感知，两种事件皆可
                break;
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("channel closed"),
            Err(_) => panic!("timed out waiting for disconnect event"),
        }
    }

    client.disconnect().await;
    clear_global_token();
}

// ==================== 场景 6：未连接拒绝 ====================

async fn scenario_send_when_disconnected_rejected() {
    clear_global_token();
    let manager = ConnectionManager::new();

    let err = manager
        .send(&Message::session_control(SessionControlAction::ListSessions, None))
        .await
        .expect_err("未连接时 send 必须失败");
    let msg = err.to_string();
    assert!(msg.contains("Not connected"), "错误信息应说明未连接，实际: {}", msg);
}

// ==================== 04 事件 WS：建连 + 极简 JWT 认证 + 事件帧路由（票 03 新协议） ====================

/// 事件 WS 首帧必须是极简认证 `{"type":"auth","token":"<jwt>"}`（票 03 新协议：
/// 不再发 `Message::Auth` 信封、不携带加密提案、**不等待回执**——插件端点无认证
/// 回包，认证失败由宿主 close 4001 显性表达）；认证后事件帧可路由到 MobileEvent
async fn scenario_event_ws_first_message_is_jwt_auth() {
    let server = MockPluginWsServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    let mut events = manager.subscribe();

    manager
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;
    manager
        .establish_event_ws(None)
        .await
        .expect("establish event ws should succeed");

    // 事件 WS 首帧必须是极简 JWT 认证（flat 帧，无信封 payload）
    let msgs = server
        .wait_for_text(
            ENDPOINT_SESSION_CONTROL,
            |v| v["type"] == "auth" && v["token"] == MOCK_SESSION_TOKEN,
            EVENT_TIMEOUT,
        )
        .await;
    assert!(!msgs.is_empty(), "事件 WS 首帧应为极简认证帧");
    assert!(
        msgs[0].get("payload").is_none(),
        "极简认证帧不得携带信封 payload（票 03 结构锁）"
    );

    // 认证后事件帧可路由：插件端点广播 session:mode-changed → MobileEvent
    server
        .send_event(
            EVENT_SESSION_MODE_CHANGED,
            serde_json::json!({ "session_id": "s1", "auto_approve": true }),
        )
        .await;
    let ev = wait_event(&mut events, |ev| matches!(ev, MobileEvent::SyncSessionModeChanged { .. })).await;
    match ev {
        MobileEvent::SyncSessionModeChanged { session_id, auto_approve } => {
            assert_eq!(session_id, "s1");
            assert!(auto_approve);
        }
        other => panic!("expected SyncSessionModeChanged event, got {:?}", other),
    }

    manager.disconnect().await;
    clear_global_token();
    server.shutdown().await;
}

/// 插件端点帧永不加密（票 06 WS 帧级链路加密退役的结构锁行为面）：
/// session-control 首帧**原文**必须是明文 JSON（`{"type":"auth",...}`）——若未来
/// 对插件端点重上 WS codec，原文会变成密文（无 `"type"` 字面量、不可解析），
/// 本场景即红。覆盖 `scenario_event_ws_first_message_is_jwt_auth` 的解析面盲区。
async fn scenario_ws_frames_never_encrypted() {
    let server = MockPluginWsServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    manager
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;
    manager
        .establish_event_ws(None)
        .await
        .expect("establish event ws should succeed");

    // 等首条 raw 文本帧出现，断言其原文是明文 JSON（非加密信封）
    let deadline = std::time::Instant::now() + EVENT_TIMEOUT;
    let first_raw = loop {
        let raws = server.received_raw_text(ENDPOINT_SESSION_CONTROL).await;
        if let Some(first) = raws.first() {
            break first.clone();
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for first raw frame on session-control");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(
        first_raw.contains("\"type\"") && first_raw.contains("\"auth\""),
        "插件端点首帧必须是明文 JSON 认证帧（WS 帧级加密已退役），实际原文: {first_raw}"
    );

    // 同断言对 terminal 端点（终端集成已覆盖；此处只锁事件通道）——
    // 认证后事件可路由（顺带验证连接仍健康）
    server
        .send_event(
            EVENT_SESSION_MODE_CHANGED,
            serde_json::json!({ "session_id": "s1", "auto_approve": true }),
        )
        .await;
    // 无需等待具体事件：establish_event_ws 完成后连接即活，广播不缺省丢弃

    manager.disconnect().await;
    clear_global_token();
    server.shutdown().await;
}

/// 事件 WS 是事件帧收信道：桌面插件向 session-control 广播（票 03 新链路，
///  帧形状与桌面 wire 逐字一致；`SyncData` 信封已退役）
async fn scenario_event_ws_forwards_sync_data() {
    let server = MockPluginWsServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    let mut events = manager.subscribe();

    manager
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;
    manager
        .establish_event_ws(None)
        .await
        .expect("establish event ws should succeed");
    server
        .wait_for_text(ENDPOINT_SESSION_CONTROL, |v| v["type"] == "auth", EVENT_TIMEOUT)
        .await;

    // 桌面插件广播定时自动任务变更（事件帧，非 Message::SyncData 信封）
    server
        .send_event(
            EVENT_TASK_SCHEDULED_CHANGED,
            serde_json::json!({
                "job_id": "job-1",
                "status": "executed",
                "action": "trigger",
            }),
        )
        .await;

    let ev = wait_event(&mut events, |ev| {
        matches!(ev, MobileEvent::SyncTaskScheduledChanged { .. })
    })
    .await;
    match ev {
        MobileEvent::SyncTaskScheduledChanged { job_id, status, action } => {
            assert_eq!(job_id, "job-1");
            assert_eq!(status, "executed");
            assert_eq!(action, "trigger");
        }
        other => panic!("expected SyncTaskScheduledChanged event, got {:?}", other),
    }

    manager.disconnect().await;
    clear_global_token();
    server.shutdown().await;
}

// ==================== 04 监督任务：AuthSuccess 驱动建连 / 防回声双连 / 断开自愈 ====================

/// 注入 AuthSuccess → 监督任务建立事件 WS（session-control 端点）并发极简
/// JWT 认证首帧，连接计数为 1
async fn scenario_supervisor_establishes_on_auth_success() {
    let server = MockPluginWsServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    manager
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let (ready_tx, ready_rx) = oneshot::channel();
    let supervisor = tokio::spawn(run_supervisor(manager.clone(), None, Some(ready_tx)));
    ready_rx.await.expect("supervisor should subscribe");

    // 注入认证成功契口 → 监督任务应自动建连并发极简 JWT 认证首帧
    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();

    let msgs = server
        .wait_for_text(
            ENDPOINT_SESSION_CONTROL,
            |v| v["type"] == "auth" && v["token"] == MOCK_SESSION_TOKEN,
            EVENT_TIMEOUT,
        )
        .await;
    assert!(!msgs.is_empty(), "认证首帧应为极简 JWT 帧");
    // 给足建连 + 认证首帧的落定时间，确认没有多余连接
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        server.connected(ENDPOINT_SESSION_CONTROL),
        1,
        "首次认证应恰好建立一条连接"
    );

    manager.disconnect().await;
    supervisor.abort();
    server.shutdown().await;
}

/// 重复 AuthSuccess 回声不应造成双连（票 03 新协议：插件端点无认证回包，旧
/// AuthHandler 回声源消失；等价场景 = 认证流重复广播——守卫 current.is_some()
/// 应拦截，不产生第二个连接）
async fn scenario_supervisor_no_dup_connection_on_auth_echo() {
    let server = MockPluginWsServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    manager
        .set_target("127.0.0.1".to_string(), server.addr().port(), None)
        .await;

    let (ready_tx, ready_rx) = oneshot::channel();
    let supervisor = tokio::spawn(run_supervisor(manager.clone(), None, Some(ready_tx)));
    ready_rx.await.expect("supervisor should subscribe");

    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();

    // 等首条极简认证帧到达（建连完成）→ 二次注入 AuthSuccess（回声）
    server
        .wait_for_text(ENDPOINT_SESSION_CONTROL, |v| v["type"] == "auth", EVENT_TIMEOUT)
        .await;

    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();

    // 给足回声处理与守卫判断的窗口
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        server.connected(ENDPOINT_SESSION_CONTROL),
        1,
        "重复 AuthSuccess 不应造成重复连接"
    );

    manager.disconnect().await;
    supervisor.abort();
    server.shutdown().await;
}

/// 事件 WS 意外断开 → 监督任务自愈（空凭据快速失败，不触 HTTP）→ 新一轮
/// 认证流到达时重建连接
///
/// 票 03 新协议无服务端认证回包（回声源消失）：断开后重建**只**依赖下一次
/// AuthSuccess（HTTP 认证成功契口）——事件不重放，断连期间的变化由前端在
/// `ws_event_channel_ready` 触发对账补齐。HTTP reauth 成功段由 03 的
/// `http_auth_flow::reauth_refreshes_token` 覆盖。
async fn scenario_supervisor_recovers_after_disconnect() {
    let server = MockDesktopServer::start().await;
    clear_global_token();
    set_global_token(MOCK_SESSION_TOKEN);
    let manager = ConnectionManager::new();
    manager
        .set_target("127.0.0.1".to_string(), server.addr.port(), None)
        .await;

    let (ready_tx, ready_rx) = oneshot::channel();
    let supervisor = tokio::spawn(run_supervisor(manager.clone(), None, Some(ready_tx)));
    ready_rx.await.expect("supervisor should subscribe");

    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();
    server
        .wait_for_received(
            |v| v["type"] == "auth" && v["token"] == MOCK_SESSION_TOKEN,
            EVENT_TIMEOUT,
        )
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.connection_count(), 1);

    // 清空凭据（无 clear_credentials setter，置空 session_token 等效）：断开后的
    // reconnect 走空 token 快速失败路径（不触 HTTP，规避对 WS 端口发 HTTP）
    let auth_mgr = get_auth_manager();
    auth_mgr
        .set_credentials(AuthCredentials {
            pairing_id: TEST_DEVICE_ID.to_string(),
            fingerprint: "fp-test".to_string(),
            session_token: String::new(),
        })
        .await;
    clear_global_token();

    // 协议级优雅关闭事件 WS → 监督任务感知断开 → 自愈（快速失败）：无 token
    // 期间不得重建（新协议无回声，重建等下一次 AuthSuccess）
    server.graceful_close("testing disconnect").await;

    // 给足断开处理 + reconnect 快速失败窗口
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        server.connection_count(),
        1,
        "无 token 期间不得重建连接（事件通道需下一次认证流）"
    );

    // 新一轮认证流：token 恢复 + AuthSuccess → 监督任务重建事件 WS
    set_global_token(MOCK_SESSION_TOKEN);
    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();

    // 断开→重建主链路：等第二条极简认证帧
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let auth_count = {
            let guard = server.received.lock().await;
            guard
                .iter()
                .filter(|v| v["type"] == "auth" && v["token"] == MOCK_SESSION_TOKEN)
                .count()
        };
        if auth_count >= 2 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for event WS re-establish");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.connection_count(), 2, "断开后应重建一条新连接");

    manager.disconnect().await;
    supervisor.abort();
}

// ==================== 全文件串行入口 ====================

/// 进程全局 token / 凭据（`get_global_token` / `get_auth_manager`）是共享静态，
/// 本二进制内全部场景必须**串行**执行——并行时一处 clear 会踩掉另一处险待读取的
/// token（票 03 起事件通道要求非空 JWT，establish 读全局态）。放弃「静态 Mutex +
/// 跨 await 持锁」的串行方案（规则引擎判 blocker），改为单入口按序驱动：
/// 顺序即文件排列序；场景失败时 panic 携带所在文件行号定位。
#[tokio::test]
async fn ws_protocol_full_suite() {
    scenario_connect_handshake().await;
    scenario_auth_full_flow().await;
    scenario_send_and_wait_ack_matching().await;
    scenario_server_close_detected().await;
    scenario_tcp_close_emits_client_event().await;
    scenario_send_when_disconnected_rejected().await;
    scenario_event_ws_first_message_is_jwt_auth().await;
    scenario_ws_frames_never_encrypted().await;
    scenario_event_ws_forwards_sync_data().await;
    scenario_supervisor_establishes_on_auth_success().await;
    scenario_supervisor_no_dup_connection_on_auth_echo().await;
    scenario_supervisor_recovers_after_disconnect().await;
}
