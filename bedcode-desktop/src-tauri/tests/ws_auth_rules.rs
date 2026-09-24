//! WS 连接级认证规则集成测试（ticket 02，spec §4.3「首消息认证 + 10s 超时」；
//! websocket 业务下沉票 08 后改造为插件端点协议）
//!
//! 覆盖四条认证 gate 规则（宿主通用 transport 的认证边界，spec §9.1）：
//! 1. 未认证连接发业务帧 → 插件端点通道丢弃 + warn（不缓存），静默挂到 10s
//!    认证超时 → 服务端 close 4001（`auth:"jwt"` 端点的安全闭合）
//! 2. 首消息 JWT 认证失败（无效 token）→ close 4001
//! 3. 有效 token 认证后，业务帧正常投递（list_sessions 回包，免 token 复验）
//! 4. 连接建立后 10s 内未完成认证 → 服务端主动 close 4001（spec §4.3 超时窗口）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端连 `/ws/plugin/com.bedcode.terminal-session/session-control`
//! （manifest `contributes.wsEndpoints` 声明，auth=jwt）+ HTTP /api/auth/* 配对 →
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
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::server::core::app::start_http_server;
use bedcode_lib::server::websocket::WebSocketManager;
use bedcode_lib::system::app_context::AppContextBuilder;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::wasm_core::PluginHost;
use bedcode_lib::AppConfig;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::WebSocketStream;

/// WS 接收流（split 后的读半部）
type WsRecv = futures_util::stream::SplitStream<WebSocketStream<TcpStream>>;
/// WS 发送流（split 后的写半部）
type WsSend = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMsg>;

// ==================== 基建 ====================

/// 会话中心插件 id（认证端点编排的权威实现方）
const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";
/// 插件端点挂载路径（host 注入命名空间段，manifest 声明 `session-control` 后缀）
const SESSION_CONTROL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/session-control";

/// 随包插件产物目录（`cargo test` 前须重建产物，见 AGENTS §3）
///
/// 产物缺失时**显性失败**而非跳过：本 target 的认证换取链没有别的驱动方式，
/// 静默 `[skip]` 会把「未验证」伪装成「通过」。
fn bundled_plugins_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/plugins/desktop");
    assert!(
        dir.join("com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm")
            .exists(),
        "插件产物缺失：先跑 `node scripts/plugin-build.js --plugin com.bedcode.terminal-session`（workdir bedcode-desktop），目录 {}",
        dir.display()
    );
    dir
}

/// 无头集成测试的会话中心插件私有库根（认证记录下沉 v24 后配对/历史真源在
/// 私有库；无头上下文无 AppHandle，经 `set_plugin_db_root` 注入，activate 前设置）
fn session_plugin_db_root() -> &'static PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bedcode-wsar-pluginroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    })
}

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
/// 与生产组装对齐：插件宿主 + 会话中心插件激活（配对端点编排在插件）。
/// websocket 业务下沉票 08 起无 `sync_tx`/SyncEventHandler 装配（宿主导流
/// 事件通道已删，插件事件走 bus/emit）。
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

        let plugins_dir = bundled_plugins_dir();
        // 用户插件目录：独立空目录。复用 plugins_dir 会让随包插件被标成
        // UserInstalled 来源，激活时撞审批门禁（无批准记录 → NeedsApproval）
        let user_plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-userplugins-{}", std::process::id()));
        std::fs::create_dir_all(&user_plugins_dir).expect("create temp user plugins dir failed");

        let plugin_host = Arc::new(PluginHost::new(db.clone(), &plugins_dir, &user_plugins_dir, None).await);
        plugin_host.init_message_bus().await;
        // v24 认证记录下沉：配对/历史真源 = 认证中心私有库。无头上下文无
        // AppHandle，必须在 activation 前注入私有库根（activate 建表走
        // host-plugin-database；配对/信任链路无私有库即不可用）
        plugin_host
            .wasm_host_ctx()
            .set_plugin_db_root(Some(session_plugin_db_root().clone()));
        // 激活会话中心：配对码 / QR 的签发与验签执行在插件；激活期宿主自动从
        // manifest `contributes.wsEndpoints` 登记 `session-control` / `terminal` 端点
        plugin_host
            .activate_plugin(SESSION_PLUGIN_ID, false)
            .await
            .expect("activate com.bedcode.terminal-session (bundled artifact)");

        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let system_info = Arc::new(SystemInfo::collect());

        AppContextBuilder::new()
            .db(db.clone())
            .plugin_host(plugin_host.clone())
            .mdns_advertiser(mdns_advertiser.clone())
            .app_handle(None)
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();

        let ws_manager = WebSocketManager::global();
        ws_manager.init().await.expect("init WebSocketManager failed");
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

/// 读取下一条 JSON 文本帧（跳过 Ping/Pong 与二进制帧）
async fn recv_json(stream: &mut WsRecv, timeout: Duration) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        let frame = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(frame))) => frame,
            _ => return None,
        };
        match frame {
            WsMsg::Text(text) => return Some(serde_json::from_str(&text).expect("frame must be JSON")),
            WsMsg::Ping(_) | WsMsg::Pong(_) | WsMsg::Binary(_) | _ => continue,
        }
    }
}

/// 轮询等待 WS 流被关闭（跳过收发控制帧/业务帧），超时返回 false
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
async fn http_pair_and_get_token(port: u16, device_id: &str, device_name: &str, fingerprint: &str) -> String {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

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

#[tokio::test]
async fn ws_auth_gate_rules() {
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：未认证业务帧被丢弃 + 10s 认证超时闭合 ====================
    // 插件端点 `auth:"jwt"`：未认证期业务帧丢弃 + warn（不缓存，spec §4.3），
    // 连接静默挂到认证超时 → 服务端 close 4001
    let (mut sink1, mut stream1, _addr1) = connect_ws(port, SESSION_CONTROL_PATH).await;

    sink1
        .send(WsMsg::Text(r#"{"type":"list_sessions"}"#.into()))
        .await
        .expect("send business frame failed");
    // 未认证业务帧不得产生回包（被丢弃）：短暂等待后仍无文本帧
    let early = recv_json(&mut stream1, Duration::from_millis(500)).await;
    assert!(early.is_none(), "未认证业务帧必须被丢弃，不得有回包, got: {early:?}");

    // 10s 认证超时随后关闭连接（spec §4.3 超时窗口，close 4001）
    let started = Instant::now();
    let closed = wait_for_close(&mut stream1, Duration::from_secs(14)).await;
    let elapsed = started.elapsed();
    assert!(closed, "未认证连接必须在认证超时后被服务端关闭");
    assert!(
        elapsed >= Duration::from_secs(9),
        "server must not close before the 10s auth window elapses, closed at {elapsed:?}"
    );
    let _ = sink1.close().await;
    drop(sink1);
    drop(stream1);

    // ==================== 场景 2：无效 JWT token → close 4001 ====================
    let (mut sink2, mut stream2, _addr2) = connect_ws(port, SESSION_CONTROL_PATH).await;

    sink2
        .send(WsMsg::Text(r#"{"type":"auth","token":"garbage-token"}"#.into()))
        .await
        .expect("send bad-token auth failed");

    // 认证失败 → 无 auth_ok，随后服务端 close 4001
    assert!(
        wait_for_close(&mut stream2, Duration::from_secs(3)).await,
        "connection must be closed after failed JWT auth"
    );
    let _ = sink2.close().await;
    drop(sink2);
    drop(stream2);

    // ==================== 场景 3：HTTP 配对拿 token → JWT 认证 → 业务帧放行 ====================
    let token = http_pair_and_get_token(port, "itest-device-rules-3", "ITest R3", "fp-auth-rules-3").await;
    let (mut sink3, mut stream3, _addr3) = connect_ws(port, SESSION_CONTROL_PATH).await;

    // 3a. 首消息 JWT 认证（插件端点固定形状 {"type":"auth","token":...}）
    sink3
        .send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
        .await
        .expect("send auth frame failed");

    // 3b. 认证后发业务帧（list_sessions）→ 插件 ws_control 回包（认证已放行）
    sink3
        .send(WsMsg::Text(r#"{"type":"list_sessions"}"#.into()))
        .await
        .expect("send list_sessions failed");
    let reply = recv_json(&mut stream3, Duration::from_secs(5))
        .await
        .expect("list reply");
    assert_eq!(reply["type"], "session_list", "认证后业务帧必须可达, got: {reply}");
    assert_eq!(reply["sessions"], serde_json::json!([]), "空登记域回包形状");

    let _ = sink3.send(WsMsg::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stream3.next()).await;
    let _ = sink3.close().await;
    drop(sink3);
    drop(stream3);

    // ==================== 场景 4：10s 认证超时（无任何首消息）→ 服务端主动关闭 ====================
    let (mut sink4, mut stream4, _addr4) = connect_ws(port, SESSION_CONTROL_PATH).await;

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

    // 清理临时插件目录与私有库根
    let plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(plugins_dir);
    let _ = std::fs::remove_dir_all(session_plugin_db_root());
}
