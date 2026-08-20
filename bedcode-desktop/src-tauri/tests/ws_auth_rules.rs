//! WS 连接级认证规则集成测试（ticket 02，spec §4.3「首消息认证 + 10s 超时 + 拒绝对称」）
//!
//! 覆盖四条认证 gate 规则：
//! 1. 未认证连接的首条消息必须走 Auth 分派——发业务消息（SessionControl）→
//!    回 `AUTH_REQUIRED` 错误后服务端关闭连接（拒绝对称）
//! 2. JWT 首消息认证失败（无效 token）→ 回 `AUTH_FAILED` 后关闭连接
//! 3. 有效 token 认证后，业务消息不再校验 token（HTTP 配对拿 token →
//!    /ws/event JWT 认证 → 免 token 发 SessionControl 收 echo）
//! 4. 连接建立后 10s 内未完成认证 → 服务端主动关闭（spec §4.3 超时窗口）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端（/ws/event 单事件通道 + HTTP /api/auth/* 配对）→
//! 首条消息直接验证服务端 gate 行为。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景 1–4 严格串行，且共享全局
//! WsSessionRegistry 单例——并行测试互相 clear_all 会闪失败，见 registry.rs
//! 单测注释里的 #lesson）。tests/ 下每个文件是独立测试二进制 → 与其他文件
//! 进程隔离，全局单例天然不冲突。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_lib::db::Database;
use bedcode_lib::events::DesktopSyncEvent;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::plugin::PluginHost;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::server::message::{AuthPayload, AuthStage, Message, SessionControlAction};
use bedcode_lib::server::services::pairing_service::PairingService;
use bedcode_lib::server::ws::WebSocketManager;
use bedcode_lib::session::{SessionConfigManager, SessionManager};
use bedcode_lib::system::app_context::AppContextBuilder;
use bedcode_lib::system::constants::network::SYNC_EVENT_BROADCAST_CAPACITY;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::utils::auth::QrTokenManager;
use bedcode_lib::AppConfig;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::WebSocketStream;

/// WS 接收流（split 后的读半部）
type WsRecv = futures_util::stream::SplitStream<WebSocketStream<TcpStream>>;
/// WS 发送流（split 后的写半部）
type WsSend = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMsg>;

// ==================== 基建（与 04 broadcast_shutdown 同款模式） ====================

/// 探测空闲端口：绑定 127.0.0.1:0 由 OS 分配，立即释放后交给服务器绑定
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 启动测试服务器：真实 `start_http_server` + 默认网络配置（端口显式传入）
async fn spawn_test_server(port: u16) -> std::io::Result<(ServerHandle, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 组装真实服务 AppContext（app_handle=None 无头模式），每个测试进程只 init 一次
///
/// 与 04 同款完整组装（session/config 接入 sync_tx + SyncEventHandler 注册）：
/// 场景 3 的免 token 业务消息 echo 走 session_control → remove_session_with_source
/// → 广播发布链路，sync 接线保证链路完整（不含断言广播，但保持生产组装一致性）
async fn init_test_app_context() {
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if INIT.get().is_some() {
        return;
    }
    {
        let db = Arc::new(tokio::sync::Mutex::new(
            Database::new(Path::new(":memory:")).expect("create in-memory db failed"),
        ));
        db.lock().await.init_schema().expect("init db schema failed");

        let plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-plugins-{}", std::process::id()));
        std::fs::create_dir_all(&plugins_dir).expect("create temp plugins dir failed");

        let session_db = Database::new(Path::new(":memory:")).expect("create session db failed");
        session_db.init_schema().expect("init session db schema failed");
        let session_manager = Arc::new(SessionManager::from_database(session_db, Arc::new(PathBuf::from("."))));
        let config_manager = Arc::new(SessionConfigManager::new(db.clone()));
        let plugin_host = Arc::new(
            PluginHost::new(
                db.clone(),
                &plugins_dir,
                session_manager.clone(),
                config_manager.clone(),
                None,
            )
            .await,
        );
        plugin_host.init_message_bus().await;

        let pairing_service = Arc::new(PairingService::new());
        let qr_manager = Arc::new(QrTokenManager::new());
        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let (sync_tx, _) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(SYNC_EVENT_BROADCAST_CAPACITY);
        let system_info = Arc::new(SystemInfo::collect());

        AppContextBuilder::new()
            .db(db.clone())
            .session_manager(session_manager.clone())
            .config_manager(config_manager.clone())
            .plugin_host(plugin_host.clone())
            .file_service(plugin_host.file_service().clone())
            .pairing_service(pairing_service.clone())
            .qr_manager(qr_manager.clone())
            .mdns_advertiser(mdns_advertiser.clone())
            .app_handle(None)
            .sync_tx(sync_tx.clone())
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();

        session_manager.set_sync_tx(sync_tx.clone()).await;
        config_manager.set_sync_tx(sync_tx.clone()).await;

        let ws_manager = WebSocketManager::global();
        ws_manager.init().await.expect("init WebSocketManager failed");
        bedcode_lib::events::global_matcher()
            .register_source::<DesktopSyncEvent>(sync_tx.clone())
            .await;
        let _ = INIT.set(());
    }
}

/// 建立 WS 连接：先建 TCP（记录本地地址 = 服务端看到的 peer addr），再升级为 WebSocket
async fn connect_ws(port: u16, path: &str) -> (WsSend, WsRecv, std::net::SocketAddr) {
    let tcp = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("tcp connect to test server failed");
    let local_addr = tcp.local_addr().expect("read local addr failed");
    let url = format!("ws://127.0.0.1:{port}{path}");
    let (ws, _resp) = tokio_tungstenite::client_async(&url, tcp)
        .await
        .expect("ws handshake failed");
    let (sink, stream) = ws.split();
    (sink, stream, local_addr)
}

/// 读取下一条业务消息（跳过 Ping/Pong），解析为协议 Message
///
/// 服务端心跳帧与业务帧可交错，必须跳过而非误判为响应
async fn recv_message(stream: &mut WsRecv) -> Message {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("timed out waiting for WS message")
            .expect("WS stream closed unexpectedly")
            .expect("WS frame error");
        match frame {
            WsMsg::Text(text) => {
                return Message::from_json(&text).expect("server message must be valid protocol JSON");
            }
            WsMsg::Ping(_) | WsMsg::Pong(_) => continue,
            _ => continue,
        }
    }
}

/// 轮询等待流中出现满足条件的业务消息（5s 超时）
async fn wait_for_message(stream: &mut WsRecv, is_match: impl FnMut(&Message) -> bool) -> Message {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut is_match = is_match;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for matching WS message");
        let frame = tokio::time::timeout(remaining, stream.next()).await;
        match frame {
            Ok(Some(Ok(WsMsg::Text(text)))) => {
                let msg = Message::from_json(&text).expect("server message must be valid protocol JSON");
                if is_match(&msg) {
                    return msg;
                }
            }
            Ok(Some(Ok(WsMsg::Ping(_))) | Some(Ok(WsMsg::Pong(_)))) => continue,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("WS error while waiting for message: {e}"),
            Ok(None) => panic!("WS stream closed while waiting for message"),
            Err(_) => panic!("timed out waiting for matching WS message"),
        }
    }
}

/// 轮询等待 WS 流被关闭（跳过收发控制帧/业务帧），超时返回 false
///
/// 服务端 ctx.stop() 后发送 Close 帧；客户端读到 None/Err/Close 均视为关闭。
/// 期间到达的 Ping/Pong/Text 一律跳过，只关心「流是否已终止」
async fn wait_for_close(stream: &mut WsRecv, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let frame = tokio::time::timeout(remaining, stream.next()).await;
        match frame {
            Ok(Some(Ok(WsMsg::Ping(_))) | Some(Ok(WsMsg::Pong(_)))) => continue,
            Ok(Some(Ok(WsMsg::Close(_)))) => return true,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) => return true,
            Ok(None) | Err(_) => return true,
        }
    }
}

/// HTTP 配对换取 JWT session_token（POST /api/auth/pairing → verify）
///
/// 旧 WS 配对（RequestPairing/VerifyCode）已随 /ws/terminal 兼容路由删除，
/// 配对统一走 HTTP /api/auth/*；WS 首消息仅接受 JWT（Authenticated/Reauthenticate）
async fn http_pair_and_get_token(port: u16, device_id: &str, device_name: &str, fingerprint: &str) -> String {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // 1) 请求配对码（HTTP DTO 为 camelCase，见 dtos/auth_dto.rs `rename_all`）
    let resp: serde_json::Value = client
        .post(format!("{base}/api/auth/pairing"))
        .json(&serde_json::json!({
            "deviceId": device_id,
            "deviceName": device_name,
            "fingerprint": fingerprint,
        }))
        .send()
        .await
        .expect("pairing request failed")
        .json()
        .await
        .expect("pairing response parse failed");
    let pairing_code = resp["data"]["pairingCode"]
        .as_str()
        .expect("pairingCode missing in response")
        .to_string();
    assert!(!pairing_code.is_empty(), "pairing code must be non-empty");

    // 2) 验证配对码 → JWT session_token
    let resp: serde_json::Value = client
        .post(format!("{base}/api/auth/verify"))
        .json(&serde_json::json!({
            "deviceId": device_id,
            "deviceName": device_name,
            "fingerprint": fingerprint,
            "pairingCode": pairing_code,
            "address": format!("127.0.0.1:{port}"),
        }))
        .send()
        .await
        .expect("verify request failed")
        .json()
        .await
        .expect("verify response parse failed");
    let token = resp["data"]["token"]
        .as_str()
        .expect("token missing in response")
        .to_string();
    assert!(!token.is_empty(), "session_token must be non-empty");
    token
}

/// 完整配对一个客户端（HTTP 配对）：获取 JWT session_token 供 WS 首消息认证
async fn pair_and_get_token(
    port: u16,
    device_id: &str,
    device_name: &str,
    fingerprint: &str,
    _message_tag: &str,
) -> String {
    http_pair_and_get_token(port, device_id, device_name, fingerprint).await
}

/// 以 JWT session_token 重连认证（AuthStage::Authenticated 快速路径）
async fn authenticate_with_jwt(
    sink: &mut WsSend,
    stream: &mut WsRecv,
    session_token: &str,
    device_name: &str,
    fingerprint: &str,
    message_tag: &str,
) {
    let request = Message::Auth {
        message_id: format!("itest-jwt-{message_tag}"),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::Authenticated,
            device_id: None,
            device_name: Some(device_name.to_string()),
            device_fingerprint: Some(fingerprint.to_string()),
            session_token: Some(session_token.to_string()),
            ..Default::default()
        },
    };
    sink.send(WsMsg::Text(
        request.to_json().expect("serialize jwt auth failed").into(),
    ))
    .await
    .expect("send jwt auth failed");

    let resp = recv_message(stream).await;
    match resp {
        Message::Auth { payload, .. } => {
            assert_eq!(payload.stage, AuthStage::Authenticated, "JWT re-auth must succeed");
            assert_eq!(
                payload.device_name.as_deref(),
                Some(device_name),
                "JWT re-auth response must carry device_name from claims"
            );
        }
        other => panic!("expected Auth(Authenticated) from JWT re-auth, got: {other:?}"),
    }
}

#[tokio::test]
async fn ws_auth_gate_rules() {
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：首条非 auth 消息被拒 + 关闭（spec §4.3 拒绝对称） ====================
    let (mut sink1, mut stream1, _addr1) = connect_ws(port, "/ws/event").await;

    let control = Message::session_control_with_response(
        SessionControlAction::RemoveSession {
            session_id: "ghost-auth-rules-1".to_string(),
        },
        None,
    );
    sink1
        .send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");

    // 1a. 未认证连接发业务消息 → AUTH_REQUIRED 错误
    let resp = recv_message(&mut stream1).await;
    match resp {
        Message::Error { code, message, .. } => {
            assert_eq!(
                code, "AUTH_REQUIRED",
                "unauthenticated business message must yield AUTH_REQUIRED"
            );
            assert!(!message.is_empty(), "error message must be non-empty");
        }
        other => panic!("expected Error(AUTH_REQUIRED), got: {other:?}"),
    }

    // 1b. 服务端随后关闭连接（拒绝对称：回错误 + 终止）
    assert!(
        wait_for_close(&mut stream1, Duration::from_secs(3)).await,
        "connection must be closed after AUTH_REQUIRED"
    );
    let _ = sink1.close().await;
    drop(sink1);
    drop(stream1);

    // ==================== 场景 2：无效 JWT token → AUTH_FAILED + 关闭 ====================
    // 认证门禁在 /ws/event（共享文本线）上验证：旧 /ws/terminal 已删除
    let (mut sink2, mut stream2, _addr2) = connect_ws(port, "/ws/event").await;

    let bad_auth = Message::Auth {
        message_id: "itest-bad-token".to_string(),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::Reauthenticate,
            device_id: None,
            device_fingerprint: Some("fp-auth-rules-2".to_string()),
            session_token: Some("garbage-token".to_string()),
            ..Default::default()
        },
    };
    sink2
        .send(WsMsg::Text(bad_auth.to_json().expect("serialize auth failed").into()))
        .await
        .expect("send bad-token auth failed");

    // 2a. 无效 token → AUTH_FAILED 错误（错误码/文案保持既有语义）
    let resp = recv_message(&mut stream2).await;
    match resp {
        Message::Error { code, .. } => assert_eq!(code, "AUTH_FAILED", "invalid JWT token must yield AUTH_FAILED"),
        other => panic!("expected Error(AUTH_FAILED), got: {other:?}"),
    }

    // 2b. 服务端随后关闭连接（JWT 认证失败 → 终止，不允许继续挂机）
    assert!(
        wait_for_close(&mut stream2, Duration::from_secs(3)).await,
        "connection must be closed after AUTH_FAILED"
    );
    let _ = sink2.close().await;
    drop(sink2);
    drop(stream2);

    // ==================== 场景 3：HTTP 配对拿 token → JWT 认证 → 免 token 业务 ====================
    // 旧 WS 配对（RequestPairing/VerifyCode）已随 /ws/terminal 兼容路由删除，配对统一走
    // HTTP /api/auth/*；拿到 token 后在 /ws/event 上 JWT 首消息认证，认证后的业务消息不再校验 token
    let token = pair_and_get_token(port, "itest-device-rules-3", "ITest R3", "fp-auth-rules-3", "rules-3").await;
    let (mut sink3, mut stream3, _addr3) = connect_ws(port, "/ws/event").await;

    // 3a. 首消息 JWT 认证 → Authenticated 回复
    authenticate_with_jwt(
        &mut sink3,
        &mut stream3,
        &token,
        "ITest R3",
        "fp-auth-rules-3",
        "rules-3",
    )
    .await;

    // 3b. 认证后发 SessionControl（不带 token 字段，与移动端原生消息形态一致）→
    // 收到 echo，证明业务消息放行不依赖 token（首消息认证即凭证）
    let control = Message::session_control_with_response(
        SessionControlAction::RemoveSession {
            session_id: "ghost-auth-rules-3".to_string(),
        },
        None,
    );
    sink3
        .send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control (tokenless) failed");

    let resp = wait_for_message(&mut stream3, |m| matches!(m, Message::SessionControl { .. })).await;
    match resp {
        Message::SessionControl { payload, .. } => match payload.action {
            SessionControlAction::RemoveSession { session_id } => {
                assert_eq!(session_id, "ghost-auth-rules-3", "echo must carry removed session id");
            }
            other => panic!("expected RemoveSession echo, got: {other:?}"),
        },
        other => panic!("expected SessionControl echo (tokenless business allowed), got: {other:?}"),
    }

    let _ = sink3.send(WsMsg::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stream3.next()).await;
    let _ = sink3.close().await;
    drop(sink3);
    drop(stream3);

    // ==================== 场景 4：10s 认证超时 → 服务端主动关闭 ====================
    let (mut sink4, mut stream4, _addr4) = connect_ws(port, "/ws/event").await;

    // 连接建立后保持静默：run_later 在 10s 后关闭未认证连接（spec §4.3 超时
    // 窗口）。心跳 5s 一次（客户端自动 Pong 或服务端宽松远程超时兜底），
    // 不干扰超时关断——assert 9s 下限防止过早关闭，14s 上限覆盖调度抖动
    let started = Instant::now();
    let closed = wait_for_close(&mut stream4, Duration::from_secs(14)).await;
    let elapsed = started.elapsed();
    assert!(
        closed,
        "idle unauthenticated connection must be closed by server auth timeout"
    );
    assert!(
        elapsed >= Duration::from_secs(9),
        "server must not close before the 10s auth window elapses, closed at {elapsed:?}"
    );
    let _ = sink4.close().await;
    drop(sink4);
    drop(stream4);

    // ==================== 收尾：停机释放 ====================
    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");

    // 清理临时插件目录
    let plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(plugins_dir);
}
