//! 连接清理与停机鲁棒性集成测试（ticket 04；websocket 业务下沉票 08 改造为插件端点）
//!
//! websocket 业务下沉票 08 起宿主不再有事件通道与 `SyncData` 广播面（插件事件走
//! bus/emit，宿主 WS 只转原始帧）——旧「多客户端同步广播 + 排除发送者」链路已随
//! `/ws/event` 与 `Message::SyncData` 退役。本 target 保留的验收面是**通用
//! transport 的连接清理与优雅停机**（spec §9.1 / §2.3 生命周期）：
//!
//! - 多客户端经插件端点接入（真实 JWT 认证）→ 注册表就位；
//! - 断开一方 → 注册表摘除（stopping 清理），对已摘除 client_id 的定向发送显性失败；
//! - 停机（`ServerHandle::stop(true)`）→ 新连接被拒（ConnectionRefused）、端口释放
//!   可重绑、注册表无孤儿残留、全程无 ERROR 级日志。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）。tests/ 下每个
//! 文件是独立测试二进制 → 与其它文件进程隔离，AppContext（OnceLock）/
//! WsSessionRegistry 等全局单例天然互不冲突。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_lib::db::Database;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::server::core::app::start_http_server;
use bedcode_lib::server::websocket::registry::WsSessionRegistry;
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

// ==================== ERROR 级日志计数（停机无 error 验收） ====================

static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// 统计 ERROR 级日志事件数的 tracing layer
struct ErrorCounter;

impl<S> tracing_subscriber::Layer<S> for ErrorCounter
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        if *event.metadata().level() == tracing::Level::ERROR {
            ERROR_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ==================== 基建 ====================

/// 会话中心插件 id
const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";
/// 插件端点挂载路径（manifest `contributes.wsEndpoints` 声明 + 激活期登记）
const SESSION_CONTROL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/session-control";

/// 随包插件产物目录（`cargo test` 前须重建产物，见 AGENTS §3）
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

/// 无头集成测试的会话中心插件私有库根（配对/历史真源在私有库）
fn session_plugin_db_root() -> &'static PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("bedcode-bsdrop-pluginroot-{}", std::process::id()));
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
/// websocket 业务下沉票 08 起无 `sync_tx`/SyncEventHandler 装配（宿主导流事件
/// 通道已删，插件事件走 bus/emit）。
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
        // 用户插件目录：独立空目录（复用 plugins_dir 会让随包插件被标成
        // UserInstalled 来源，激活时撞审批门禁）
        let user_plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-userplugins-{}", std::process::id()));
        std::fs::create_dir_all(&user_plugins_dir).expect("create temp user plugins dir failed");

        let plugin_host = Arc::new(PluginHost::new(db.clone(), &plugins_dir, &user_plugins_dir, None).await);
        plugin_host.init_message_bus().await;
        plugin_host
            .wasm_host_ctx()
            .set_plugin_db_root(Some(session_plugin_db_root().clone()));
        // 激活会话中心：/api/auth/* 编排与 ws 端点登记在插件
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

/// HTTP 配对换取 JWT session_token（POST /api/auth/pairing → verify）
async fn http_pair_and_get_token(port: u16, device_id: &str, device_name: &str, fingerprint: &str) -> String {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(0)
        .build()
        .expect("reqwest client build failed");

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

/// 插件端点首消息 JWT 认证（`{"type":"auth","token":...}`）
async fn authenticate_endpoint(sink: &mut WsSend, stream: &mut WsRecv, token: &str) {
    sink.send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
        .await
        .expect("send auth frame failed");
    // 端点协议不定义 auth_ok 回帧：调用方随后直接发业务帧即可验证认证生效
    let _ = stream;
}

/// 显式关闭 WS 连接（Close 帧让服务端 actor 走 stopping() 注销注册表）
async fn close_ws(sink: &mut WsSend, stream: &mut WsRecv) {
    let _ = sink.send(WsMsg::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
    let _ = sink.close().await;
}

#[tokio::test]
async fn connection_cleanup_and_shutdown_flow() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with(tracing_subscriber::fmt::layer().with_test_writer())
        .with(ErrorCounter);
    if subscriber.try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：两客户端接入插件端点并认证 ====================
    let token_a = http_pair_and_get_token(port, "itest-device-a-04", "ITest A-04", "fp-itest-a-04").await;
    let (mut sink_a, mut stream_a, addr_a) = connect_ws(port, SESSION_CONTROL_PATH).await;
    let client_id_a = addr_a.to_string();
    authenticate_endpoint(&mut sink_a, &mut stream_a, &token_a).await;

    let token_b = http_pair_and_get_token(port, "itest-device-b-04", "ITest B-04", "fp-itest-b-04").await;
    let (mut sink_b, mut stream_b, addr_b) = connect_ws(port, SESSION_CONTROL_PATH).await;
    let client_id_b = addr_b.to_string();
    authenticate_endpoint(&mut sink_b, &mut stream_b, &token_b).await;

    // registry 更新经 actix::spawn 异步落地，轮询等待两个客户端都就位
    assert!(
        wait_until(|| async {
            let auth = WsSessionRegistry::global()
                .list_clients()
                .await
                .into_iter()
                .filter(|c| c.authenticated)
                .map(|c| c.client_id)
                .collect::<Vec<_>>();
            auth.contains(&client_id_a) && auth.contains(&client_id_b)
        })
        .await,
        "both authenticated clients must appear in WsSessionRegistry"
    );

    // 认证后业务帧可达（JWT 认证即凭证，免 token 复验）
    sink_a
        .send(WsMsg::Text(r#"{"type":"list_sessions"}"#.into()))
        .await
        .expect("send list_sessions failed");
    let reply = loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), stream_a.next())
            .await
            .expect("timed out waiting for reply")
            .expect("ws stream closed")
            .expect("ws frame error");
        match frame {
            WsMsg::Text(text) => {
                let v: serde_json::Value = serde_json::from_str(&text).expect("reply json");
                if v["type"] == "session_list" {
                    break v;
                }
            }
            WsMsg::Ping(_) | WsMsg::Pong(_) | WsMsg::Binary(_) | _ => continue,
        }
    };
    assert_eq!(reply["sessions"], serde_json::json!([]), "空登记域回包形状");

    // ==================== 场景 2：断开 B → 注册表摘除 + 定向发送显性失败 ====================

    close_ws(&mut sink_b, &mut stream_b).await;
    drop(sink_b);
    drop(stream_b);

    // 注销经 actor stopping() 异步落地，轮询等待注册表移除 B
    assert!(
        wait_until(|| async {
            WsSessionRegistry::global()
                .list_clients()
                .await
                .into_iter()
                .all(|c| c.client_id != client_id_b)
        })
        .await,
        "disconnected client must be unregistered from WsSessionRegistry"
    );

    // 注册表层断言：向已摘除 client_id 的定向发送必须失败（条目已清理，fail-visible）
    let direct_send = WsSessionRegistry::global()
        .send_to_client(&client_id_b, "probe-after-disconnect".to_string())
        .await;
    assert!(
        direct_send.is_err(),
        "send to disconnected client must fail after registry cleanup, got: {direct_send:?}"
    );

    // ==================== 场景 3：停机后新连接被拒 + 端口释放 ====================

    close_ws(&mut sink_a, &mut stream_a).await;
    drop(sink_a);
    drop(stream_a);
    tokio::time::sleep(Duration::from_millis(200)).await;

    tokio::time::timeout(Duration::from_secs(10), handle.stop(true))
        .await
        .expect("graceful stop must complete within timeout");
    server_task
        .await
        .expect("server task must not panic")
        .expect("server must exit Ok after graceful stop");

    // 3a. 新连接被拒：connect 最终失败且错误为 ConnectionRefused。
    // Windows TCP 栈对刚关闭的监听端口需要约 2s 才返回 RST（SYN 重传行为），
    // 故超时放宽到 10s——断言核心是连接最终被拒而非无限挂起
    let refused = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(("127.0.0.1", port))).await;
    match refused {
        Ok(Err(e)) => assert_eq!(
            e.kind(),
            std::io::ErrorKind::ConnectionRefused,
            "new connection must be refused after server stop, got: {e}"
        ),
        Ok(Ok(_)) => panic!("new connection must be rejected after server stop"),
        Err(_) => panic!("connect to stopped server must be refused, not hang forever"),
    }

    // 3b. 端口释放：同端口可重新绑定（OS 不再占用）
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))
        .expect("port must be released and rebindable after server stop");
    drop(listener);

    // ==================== 场景 4：停机无孤儿客户端残留 + 无 error 级日志 ====================

    // 4a. 停机后注册表为空：客户端先 Close（actor stopping() 注销）+ 停机兜底，
    // 两条路径共同保证不残留孤儿条目
    let clients = WsSessionRegistry::global().list_clients().await;
    assert!(
        clients.is_empty(),
        "no orphan clients must remain after shutdown, got: {clients:?}"
    );
    assert_eq!(WsSessionRegistry::global().client_count().await, 0);

    // 4b. 防御性清理路径：模拟生产 WebSocketManager::stop 的防御分支，验证幂等不报错
    WsSessionRegistry::global().clear_all().await;
    assert_eq!(WsSessionRegistry::global().client_count().await, 0);

    // 4c. 全程（含停机/清理）无 ERROR 级日志——计数 layer 从测试启动即挂载
    assert_eq!(
        ERROR_COUNT.load(Ordering::SeqCst),
        0,
        "no error-level logs are allowed during cleanup & shutdown flow"
    );

    // 清理临时用户插件目录（随包产物目录 resources/plugins/desktop 不得触碰）
    let user_plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-userplugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(user_plugins_dir);
    // 私有库根清理（含认证中心私有库测试数据）
    let _ = std::fs::remove_dir_all(session_plugin_db_root());
}
