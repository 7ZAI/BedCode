//! WS 配对与认证链路集成测试（spec L1 场景 3–5，ticket 02）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端模拟移动端 → 走完整配对链路（ws actor →
//! pairing_service → auth_service → JWT 签发 → registry 注册），断言端到端行为。
//!
//! AppContext 组装：app_handle(None) 无头模式——`tauri::test::mock_app()`
//! 只能产出 MockRuntime 句柄，与 Wry 的 `Arc<AppHandle>` 类型不兼容；
//! 其余字段全部真实实现 + 内存 SQLite（:memory:）。配对码在无头模式下
//! 不经过前端事件投递（生产走 pairing-code-generated event），测试直接从
//! PairingService 单例读取——该实例与 WS handler 经 AppContext 共享，
//! 读到的就是服务器为本次请求生成的真实配对码。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）。
//! tests/ 下每个文件是独立测试二进制 → 与 01 文件进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry 等全局单例天然互不冲突。
//! 等待异步事件统一 `tokio::time::sleep + yield_now`（current_thread
//! runtime 禁止 std::thread::sleep），每处 await 都有 timeout 防 CI 卡死。

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
use bedcode_lib::server::ws::registry::WsSessionRegistry;
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

/// 探测空闲端口：绑定 127.0.0.1:0 由 OS 分配，立即释放后交给服务器绑定
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port failed");
    listener.local_addr().expect("read probed port failed").port()
}

/// 启动测试服务器：真实 `start_http_server` + 默认网络配置（端口显式传入）
///
/// 服务器 future 必须保活（drop 会触发停机），spawn 到测试 runtime 上持续轮询；
/// actix worker 运行在各自线程的独立 runtime，不受 current_thread 测试 runtime 限制
async fn spawn_test_server(port: u16) -> std::io::Result<(ServerHandle, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 组装真实服务 AppContext（app_handle=None 无头模式），每个测试进程只 init 一次
///
/// 全部服务用真实实现 + 内存 SQLite；插件宿主走真实 `PluginHost::new`
/// （wasmtime 引擎初始化 + 空插件目录扫描，与生产同路径），仅 Tauri
/// 前端事件能力降级（与 wasm_runtime.rs 既有测试同一策略）
async fn init_test_app_context() {
    // OnceLock 守卫：本二进制只有一个 #[tokio::test]，但防未来新增测试重复组装
    static INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if INIT.get().is_some() {
        return;
    }
    {
        // AppContext.db：配对记录/连接历史落库
        let db = Arc::new(tokio::sync::Mutex::new(
            Database::new(Path::new(":memory:")).expect("create in-memory db failed"),
        ));
        db.lock().await.init_schema().expect("init db schema failed");

        // 插件目录：独立临时目录（空目录即可——扫描无插件，WASM 引擎仍走真实初始化）
        let plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-plugins-{}", std::process::id()));
        std::fs::create_dir_all(&plugins_dir).expect("create temp plugins dir failed");

        // 会话管理器用独立内存库（会话持久化与配对记录互不相干，避免共用连接）
        let session_db = Database::new(Path::new(":memory:")).expect("create session db failed");
        session_db.init_schema().expect("init session db schema failed");
        let session_manager = Arc::new(SessionManager::from_database(
            session_db,
            Arc::new(PathBuf::from(".")),
        ));
        let config_manager = Arc::new(SessionConfigManager::new(db.clone()));
        let plugin_host = Arc::new(
            PluginHost::new(
                db.clone(),
                &plugins_dir,
                session_manager.clone(),
                config_manager.clone(),
                None, // 无头/测试上下文无 AppHandle
            )
            .await,
        );
        // 两阶段初始化：注入消息总线 dispatcher（与 lib.rs 生产路径一致）
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
            .sync_tx(sync_tx)
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();
        let _ = INIT.set(());
    }
}

/// 建立 WS 连接：先建 TCP（记录本地地址 = 服务端看到的 peer addr，即
/// registry 的 client_id），再升级为 WebSocket
async fn connect_ws(port: u16) -> (WsSend, WsRecv, std::net::SocketAddr) {
    let tcp = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("tcp connect to test server failed");
    let local_addr = tcp.local_addr().expect("read local addr failed");
    let url = format!("ws://127.0.0.1:{port}/ws/terminal");
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

/// 轮询等待条件成立（25ms 间隔 + yield_now，5s 超时）
async fn wait_until<F, Fut>(mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if cond().await {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        tokio::task::yield_now().await;
    }
}

/// 从注册表查找客户端条目
async fn registry_entry(client_id: &str) -> Option<bedcode_lib::server::ws::registry::ClientSummary> {
    WsSessionRegistry::global()
        .list_clients()
        .await
        .into_iter()
        .find(|c| c.client_id == client_id)
}

/// 发送 RequestPairing 并断言收到 VerifyCode 响应（配对请求半程，供场景 1/4 复用）
async fn request_pairing_and_expect_verify_code(
    sink: &mut WsSend,
    stream: &mut WsRecv,
    message_id: &str,
    device_id: &str,
    device_name: &str,
) {
    let request = Message::Auth {
        message_id: message_id.to_string(),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::RequestPairing,
            device_id: Some(device_id.to_string()),
            device_name: Some(device_name.to_string()),
            ..Default::default()
        },
    };
    sink.send(WsMsg::Text(request.to_json().expect("serialize request failed").into()))
        .await
        .expect("send request_pairing failed");

    let resp = recv_message(stream).await;
    match resp {
        Message::Auth { payload, message_id: resp_id, .. } => {
            assert_eq!(
                payload.stage,
                AuthStage::VerifyCode,
                "server must respond with verify_code stage"
            );
            assert_eq!(resp_id, message_id, "response must echo request message_id");
        }
        other => panic!("expected Auth(VerifyCode) response, got: {other:?}"),
    }
}

#[tokio::test]
async fn ws_pairing_auth_flow() {
    // 测试日志输出到 harness（失败时可查链路）；重复 init 静默跳过
    if tracing_subscriber::fmt().with_test_writer().try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    // AppContext 必须先于任何 WS 连接初始化：actor 的 stopping()/认证 handler
    // 都会调用 AppContext::global()（未初始化即 panic）
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port)
        .await
        .expect("test server must start");

    // ==================== 场景 1：配对全链路 ====================
    // 请求配对 → VerifyCode 响应 + 配对码 → 验证配对码 → Authenticated + session_token

    let (mut sink_a, mut stream_a, addr_a) = connect_ws(port).await;
    let client_id_a = addr_a.to_string();

    // 1a. RequestPairing → VerifyCode 响应
    request_pairing_and_expect_verify_code(&mut sink_a, &mut stream_a, "itest-pair-001", "itest-device-001", "ITest Phone").await;

    // 1b. 配对码：生产路径经 pairing-code-generated 事件投递给桌面端前端；
    // 无头模式下直接读 PairingService 单例（与 WS handler 共享同一实例）
    let code = bedcode_lib::AppContext::global()
        .pairing_service()
        .get_current_code()
        .await
        .expect("pairing code must exist after request_pairing")
        .code;
    assert!(!code.is_empty(), "pairing code must be non-empty");

    // 1c. 用配对码 VerifyCode → Authenticated + 非空 session_token
    let verify = Message::Auth {
        message_id: "itest-verify-001".to_string(),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::VerifyCode,
            pairing_code: Some(code),
            device_id: Some("itest-device-001".to_string()),
            device_name: Some("ITest Phone".to_string()),
            device_fingerprint: Some("fp-itest-001".to_string()),
            ..Default::default()
        },
    };
    sink_a.send(WsMsg::Text(verify.to_json().expect("serialize verify failed").into()))
        .await
        .expect("send verify_code failed");

    let resp = recv_message(&mut stream_a).await;
    let session_token = match resp {
        Message::Auth { payload, message_id, .. } => {
            assert_eq!(
                payload.stage,
                AuthStage::Authenticated,
                "valid pairing code must yield Authenticated"
            );
            assert_eq!(message_id, "itest-verify-001", "response must echo request message_id");
            assert_eq!(
                payload.device_fingerprint.as_deref(),
                Some("fp-itest-001"),
                "authenticated response must carry device fingerprint"
            );
            payload.session_token.expect("authenticated response must carry session_token")
        }
        other => panic!("expected Auth(Authenticated) response, got: {other:?}"),
    };
    assert!(!session_token.is_empty(), "session_token must be non-empty");

    // ==================== 场景 2：认证后已注册且标记 authenticated ====================

    // registry 更新经 actix::spawn 异步落地，轮询等待
    assert!(
        wait_until(|| async {
            let entry = registry_entry(&client_id_a).await;
            matches!(entry, Some(e) if e.authenticated)
        })
        .await,
        "authenticated client must appear in WsSessionRegistry with authenticated=true"
    );

    let entry = registry_entry(&client_id_a)
        .await
        .expect("authenticated client must be registered");
    assert_eq!(entry.addr, addr_a.to_string(), "registry addr must match peer addr");
    assert_eq!(
        entry.fingerprint.as_deref(),
        Some("fp-itest-001"),
        "registry must record device fingerprint"
    );

    // ==================== 场景 3：未认证连接被拒绝 ====================

    let (mut sink_b, mut stream_b, addr_b) = connect_ws(port).await;
    let client_id_b = addr_b.to_string();

    // 3a. 未认证直接发业务消息（SessionControl）→ 被拒 + 明确错误
    let control = Message::session_control_with_response(SessionControlAction::ListSessions, None);
    sink_b.send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");

    let resp = recv_message(&mut stream_b).await;
    match resp {
        Message::Error { code, message, .. } => {
            assert_eq!(code, "AUTH_REQUIRED", "unauthenticated business message must be rejected");
            assert!(
                message.contains("authenticate"),
                "rejection must carry clear message, got: {message}"
            );
        }
        other => panic!("expected Error(AUTH_REQUIRED) response, got: {other:?}"),
    }

    // 3b. 该客户端不出现在已认证集合中（连接即注册，但 authenticated 必须为 false）
    let entry_b = registry_entry(&client_id_b)
        .await
        .expect("unauthenticated client must still be registered (connection-level)");
    assert!(
        !entry_b.authenticated,
        "unauthenticated client must not be marked authenticated"
    );
    let authenticated_ids: Vec<String> = WsSessionRegistry::global()
        .list_clients()
        .await
        .into_iter()
        .filter(|c| c.authenticated)
        .map(|c| c.client_id)
        .collect();
    assert!(
        !authenticated_ids.contains(&client_id_b),
        "unauthenticated client must not appear in authenticated client list"
    );

    // ==================== 场景 4：错误配对码被拒 ====================

    let (mut sink_c, mut stream_c, addr_c) = connect_ws(port).await;
    let client_id_c = addr_c.to_string();

    // 4a. 先请求新配对码（场景 1 的码已消耗，且必须用当前码验证）
    request_pairing_and_expect_verify_code(&mut sink_c, &mut stream_c, "itest-pair-002", "itest-device-002", "ITest Phone 2").await;

    // 4b. 提交错误配对码 → Failed + 明确错误
    let wrong_code = Message::Auth {
        message_id: "itest-verify-wrong".to_string(),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::VerifyCode,
            pairing_code: Some("000000".to_string()),
            device_id: Some("itest-device-002".to_string()),
            device_name: Some("ITest Phone 2".to_string()),
            device_fingerprint: Some("fp-itest-002".to_string()),
            ..Default::default()
        },
    };
    sink_c.send(WsMsg::Text(wrong_code.to_json().expect("serialize wrong code failed").into()))
        .await
        .expect("send wrong verify_code failed");

    let resp = recv_message(&mut stream_c).await;
    match resp {
        Message::Auth { payload, .. } => {
            assert_eq!(
                payload.stage,
                AuthStage::Failed,
                "wrong pairing code must yield Failed stage"
            );
            let err = payload.error.expect("failed auth must carry error message");
            assert!(
                err.contains("code"),
                "error must explain code issue, got: {err}"
            );
        }
        other => panic!("expected Auth(Failed) response, got: {other:?}"),
    }

    // 4c. 无已认证客户端产生（注册表中该连接未被标记）
    assert!(
        wait_until(|| async {
            match registry_entry(&client_id_c).await {
                Some(e) => !e.authenticated,
                None => false,
            }
        })
        .await,
        "wrong-code client must remain unauthenticated in registry"
    );

    // ==================== 收尾：显式关闭连接 + 优雅停机 + 清理 ====================

    // 先发 Close 让服务端 actor 走 stopping()（注销注册表），再优雅停机，
    // 避免 stop(true) 等待长连接
    for (mut sink, mut stream) in [
        (sink_a, stream_a),
        (sink_b, stream_b),
        (sink_c, stream_c),
    ] {
        let _ = sink.send(WsMsg::Close(None)).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
        drop(sink);
        drop(stream);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

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
