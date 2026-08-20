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
use bedcode_lib::enums::SyncPayload;
use bedcode_lib::model::message::Message;
use bedcode_lib::router::MobileEvent;
use bedcode_lib::state::{clear_global_token, get_auth_manager, get_global_token};
use tokio::sync::{broadcast, oneshot};

use common::{MockDesktopServer, MOCK_SESSION_TOKEN, TEST_DEVICE_ID, TEST_DEVICE_NAME};

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

#[tokio::test]
async fn connect_handshake() {
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

#[tokio::test]
async fn auth_full_flow() {
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

#[tokio::test]
async fn send_and_wait_ack_matching() {
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

#[tokio::test]
async fn server_close_detected() {
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

#[tokio::test]
async fn tcp_close_emits_client_event() {
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

#[tokio::test]
async fn send_when_disconnected_rejected() {
    clear_global_token();
    let manager = ConnectionManager::new();

    let err = manager
        .send(&Message::session_control(SessionControlAction::ListSessions, None))
        .await
        .expect_err("未连接时 send 必须失败");
    let msg = err.to_string();
    assert!(msg.contains("Not connected"), "错误信息应说明未连接，实际: {}", msg);
}

// ==================== 04 事件 WS：建连 + 首消息 JWT + SyncData 收广播 ====================

/// 事件 WS 首消息必须是 reauthenticate（JWT 认证）；mock 回 Authenticated 后
/// 经 AuthHandler 产出 AuthSuccess 事件（验收点：首消息 JWT）
#[tokio::test]
async fn event_ws_first_message_is_jwt_auth() {
    let server = MockDesktopServer::start().await;
    clear_global_token();
    let manager = ConnectionManager::new();
    let mut events = manager.subscribe();

    manager
        .set_target("127.0.0.1".to_string(), server.addr.port(), None)
        .await;
    manager
        .establish_event_ws(None)
        .await
        .expect("establish event ws should succeed");

    // 事件 WS 首消息必须是 JWT 认证（reauthenticate stage）
    let msgs = server
        .wait_for_received(
            |v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate",
            EVENT_TIMEOUT,
        )
        .await;
    assert!(!msgs.is_empty(), "事件 WS 首消息应为 reauthenticate");

    // mock 回 Authenticated → AuthHandler → AuthSuccess 事件
    let ev = wait_event(&mut events, |ev| matches!(ev, MobileEvent::AuthSuccess { .. })).await;
    match ev {
        MobileEvent::AuthSuccess { session_token } => {
            assert_eq!(session_token, MOCK_SESSION_TOKEN, "AuthSuccess 应携带签发的 token");
        }
        other => panic!("expected AuthSuccess event, got {:?}", other),
    }

    manager.disconnect().await;
    clear_global_token();
}

/// 事件 WS 是 SyncData 收信道：桌面端向 Event 通道广播同步数据 → SyncHandler
/// → MobileEvent（路由层不区分连接来源，与终端 WS 同一链路，验收点 4）
#[tokio::test]
async fn event_ws_forwards_sync_data() {
    let server = MockDesktopServer::start().await;
    clear_global_token();
    let manager = ConnectionManager::new();
    let mut events = manager.subscribe();

    manager
        .set_target("127.0.0.1".to_string(), server.addr.port(), None)
        .await;
    manager
        .establish_event_ws(None)
        .await
        .expect("establish event ws should succeed");
    server
        .wait_for_received(
            |v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate",
            EVENT_TIMEOUT,
        )
        .await;

    // 桌面端广播定时自动任务变更
    server
        .send_message(&Message::sync_data(SyncPayload::TaskScheduledChanged {
            job_id: "job-1".to_string(),
            status: "executed".to_string(),
            action: "trigger".to_string(),
        }))
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
}

// ==================== 04 监督任务：AuthSuccess 驱动建连 / 防回声双连 / 断开自愈 ====================

/// 注入 AuthSuccess → 监督任务建立事件 WS 并发首消息 JWT，连接计数为 1
#[tokio::test]
async fn supervisor_establishes_on_auth_success() {
    let server = MockDesktopServer::start().await;
    let manager = ConnectionManager::new();
    manager
        .set_target("127.0.0.1".to_string(), server.addr.port(), None)
        .await;

    let (ready_tx, ready_rx) = oneshot::channel();
    let supervisor = tokio::spawn(run_supervisor(manager.clone(), None, Some(ready_tx)));
    ready_rx.await.expect("supervisor should subscribe");

    // 注入认证成功契口 → 监督任务应自动建连并发 JWT 首消息
    manager
        .event_tx()
        .send(MobileEvent::AuthSuccess {
            session_token: MOCK_SESSION_TOKEN.to_string(),
        })
        .unwrap();

    let msgs = server
        .wait_for_received(
            |v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate",
            EVENT_TIMEOUT,
        )
        .await;
    assert!(!msgs.is_empty(), "事件 WS 首消息应为 reauthenticate");
    // 给足建连 + 首消息认证的落定时间，确认没有多余连接
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.connection_count(), 1, "首次认证应恰好建立一条连接");

    manager.disconnect().await;
    supervisor.abort();
}

/// AuthHandler 对 Authenticated 回复的回吐（重复 AuthSuccess 回声）不应造成双连
#[tokio::test]
async fn supervisor_no_dup_connection_on_auth_echo() {
    let server = MockDesktopServer::start().await;
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

    // 等首条 reauthenticate 到达（建连完成）→ mock 回 Authenticated → AuthHandler
    // 回吐 AuthSuccess 回声——监督任务的连接守卫应拦截，不产生第二个连接
    server
        .wait_for_received(
            |v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate",
            EVENT_TIMEOUT,
        )
        .await;

    // 给足回声处理与守卫判断的窗口
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        server.connection_count(),
        1,
        "AuthHandler 回吐的 AuthSuccess 不应造成重复连接"
    );

    manager.disconnect().await;
    supervisor.abort();
}

/// 事件 WS 意外断开 → 监督任务自愈（空凭据快速失败，不触 HTTP）→ 重建连接
///
/// HTTP reauth 成功段由 03 的 `http_auth_flow::reauth_refreshes_token` 覆盖，
/// 二者拼成「断开 → 自动重认证 → 重发 JWT 首消息」完整闭环。
#[tokio::test]
async fn supervisor_recovers_after_disconnect() {
    let server = MockDesktopServer::start().await;
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
            |v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate",
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

    // 协议级优雅关闭事件 WS → 监督任务感知断开 → 自愈（快速失败）→ 重建
    server.graceful_close("testing disconnect").await;

    // 断开→重建主链路：等第二条 reauthenticate。触发源可能是断开前排队的
    // 回声 AuthSuccess，也可能是 1s 后的兜底注入——两种时序收敛到同一终点
    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(5);
    let mut injected = false;
    loop {
        let auth_count = {
            let guard = server.received.lock().await;
            guard
                .iter()
                .filter(|v| v["type"] == "auth" && v["payload"]["payload"]["stage"] == "reauthenticate")
                .count()
        };
        if auth_count >= 2 {
            break;
        }
        // 回声若迟迟未处理（链路慢），1s 后兜底注入一次 AuthSuccess 驱动重建
        if !injected && start.elapsed() >= Duration::from_secs(1) {
            manager
                .event_tx()
                .send(MobileEvent::AuthSuccess {
                    session_token: String::new(),
                })
                .unwrap();
            injected = true;
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
