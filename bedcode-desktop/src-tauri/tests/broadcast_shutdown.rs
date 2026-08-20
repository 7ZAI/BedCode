//! 多客户端广播与停机集成测试（spec L1 场景 7–8，ticket 04）
//!
//! 广播送达语义：`WsSessionRegistry::broadcast` 只投递到 Event 通道（见
//! registry.rs ChannelType）。ticket 02 的 /ws/event 已落地，发送方 A 与接收方 B
//! 均走事件通道（旧 /ws/terminal 终端通道已随兼容路由删除），发送端由来源地址排除，
//! 排除语义与 channel 过滤在此处全链路验证。
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 两个真实
//! tokio-tungstenite 客户端完成配对认证 → 一端发 WS 会话控制消息触发
//! 服务端广播（SessionControl → SessionManager → sync_tx 事件总线 →
//! SyncEventHandler → WsSessionRegistry 广播，与生产 lib.rs 同一组装路径）
//! → 另一端收到、发送端被排除；断开后注册表清理生效；停机后端口释放。
//!
//! 广播触发路径选择（ticket 验收要求"优先 WS 消息驱动"）：
//! `RemoveSession` 是纯 WS 可触发的广播语义操作——认证客户端发送
//! `SessionControl::RemoveSession`，服务端经 `remove_session_with_source`
//! 发布 `DesktopSyncEvent::SessionRemoved { source_device: Some(设备名) }`，
//! SyncEventHandler 以发送者设备名为 exclude 广播（exclude 语义正是
//! "发送端不收到"）。对不存在的会话该操作无副作用（各注册表 remove 均
//! 容忍缺项）且仍发布广播事件，故无需预建会话/PTY，链路最短且确定。
//! 场景 2 的对照组（断开后广播仍可达在线端）使用 WebSocketManager 广播
//! API——断开方已无法从外部观察，断言改为注册表层（send_to_client 报
//! not found + 全员广播只达在线端）。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）。
//! tests/ 下每个文件是独立测试二进制 → 与 01/02 文件进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry / global_matcher 等全局单例
//! 天然互不冲突。等待异步事件统一 `tokio::time::sleep + yield_now`
//! （current_thread runtime 禁止 std::thread::sleep），每处 await 有 timeout。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_lib::db::Database;
use bedcode_lib::enums::SyncPayload;
use bedcode_lib::events::{global_matcher, DesktopSyncEvent, SyncEventHandler};
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::plugin::PluginHost;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::server::message::{AuthPayload, AuthStage, Message, SessionControlAction};
use bedcode_lib::server::services::pairing_service::PairingService;
use bedcode_lib::server::ws::registry::WsSessionRegistry;
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

// ==================== ERROR 级日志计数（场景 4 验收：停机无 error） ====================

static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// 统计 ERROR 级日志事件数的 tracing layer
///
/// 停机/清理路径的异常（孤儿客户端清理失败、广播失败等）都以 error! 输出，
/// 计数为 0 是"无异常"的可观测判据，比仅断言注册表为空更贴近 ticket 验收
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

// ==================== 基建（与 02 ws_pairing_auth 同款模式） ====================

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
/// 与 02 的差异：额外完成 sync_tx 接线 + SyncEventHandler 注册——这是本票
/// 广播链路的必要半程，严格复刻生产 lib.rs 的组装顺序：
/// 1. session_manager/config_manager set_sync_tx（事件发布侧）
/// 2. global_matcher register_source + register SyncEventHandler（消费侧）
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
        let session_manager = Arc::new(SessionManager::from_database(session_db, Arc::new(PathBuf::from("."))));
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
            .sync_tx(sync_tx.clone())
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();

        // 与生产 lib.rs 同款：会话管理器/配置管理器接入事件总线
        session_manager.set_sync_tx(sync_tx.clone()).await;
        config_manager.set_sync_tx(sync_tx.clone()).await;

        // 与生产同款：注册同步事件处理器（broadcast 消费侧）
        let ws_manager = WebSocketManager::global();
        ws_manager.init().await.expect("init WebSocketManager failed");
        global_matcher()
            .register_source::<DesktopSyncEvent>(sync_tx.clone())
            .await;
        let sync_handler: Arc<dyn bedcode_lib::events::EventHandler<DesktopSyncEvent>> = Arc::new(
            SyncEventHandler::new(session_manager.clone(), config_manager.clone(), ws_manager),
        );
        global_matcher().register::<DesktopSyncEvent>(sync_handler).await;
        let _ = INIT.set(());
    }
}

/// 建立 WS 连接：先建 TCP（记录本地地址 = 服务端看到的 peer addr，即
/// registry 的 client_id），再升级为 WebSocket
///
/// `path` 指定路由：/ws/event（事件通道，广播接收方）。旧终端通道 /ws/terminal 已删除
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

    // 注意：JWT 重连路径从 token claims 恢复 device_name（HTTP verify 签发），
    // 与移动端重连行为一致；广播排除语义依赖 device_name 正确携带
    token
}

/// 完整认证一个客户端（HTTP 配对 + JWT）：返回 session_token 供 WS 首消息认证
///
/// 客户端串行认证（每次请求配对都会轮换当前码，两个客户端必须先后完成）
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
///
/// 服务端从 token claims 恢复 device_name/fingerprint 并写入 registry，
/// 这是移动端持会话令牌重连的标准行为，也是广播排除语义的前提
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

/// 轮询等待流中出现满足条件的业务消息（5s 超时）
///
/// 认证成功后服务端还会补发文件服务快照（push_file_service_snapshot，
/// 无插件时为 FileService(Withdraw)），必须先于 echo 到达客户端——
/// 等待目标消息时必须跳过这些无关推送，不能假设下一帧就是响应
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

/// 轮询等待流中出现满足条件的 SyncData 消息（5s 超时，心跳帧自动跳过）
///
/// 广播经 SyncEventHandler 的 tokio::spawn 异步执行，必须轮询而非假定时序；
/// 期间可能出现其他业务帧（认证响应已被消费，此处只关注 SyncData）
/// 断言时间窗内不出现任何 SyncData 消息
///
/// 用于"发送端不收到"排除语义验证：广播若错误送达会以 Text 帧出现，
/// 心跳控制帧不算；窗口内无帧或仅控制帧即通过
async fn assert_no_sync_data(stream: &mut WsRecv, window: Duration, ctx: &str) {
    let deadline = Instant::now() + window;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        let frame = tokio::time::timeout(remaining, stream.next()).await;
        match frame {
            Ok(Some(Ok(WsMsg::Text(text)))) => {
                if let Ok(Message::SyncData { .. }) = Message::from_json(&text) {
                    panic!("{ctx}: client must NOT receive sync broadcast, got: {text}");
                }
            }
            Ok(Some(Ok(WsMsg::Ping(_))) | Some(Ok(WsMsg::Pong(_)))) => continue,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => panic!("WS error while asserting no sync data: {e}"),
            Ok(None) => panic!("{ctx}: stream closed unexpectedly"),
            Err(_) => return,
        }
    }
}

/// 轮询等待流中出现 SyncData 广播（5s 超时）
///
/// 广播经 SyncEventHandler 的 tokio::spawn 异步执行，必须轮询而非假定时序
async fn wait_for_sync_data(stream: &mut WsRecv, ctx: &str) -> Message {
    let msg = wait_for_message(stream, |m| matches!(m, Message::SyncData { .. })).await;
    assert!(matches!(msg, Message::SyncData { .. }), "{ctx}: expected SyncData");
    msg
}

/// 显式关闭 WS 连接（Close 帧让服务端 actor 走 stopping() 注销注册表）
async fn close_ws(sink: &mut WsSend, stream: &mut WsRecv) {
    let _ = sink.send(WsMsg::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stream.next()).await;
    let _ = sink.close().await;
}

#[tokio::test]
async fn broadcast_and_shutdown_flow() {
    // 测试日志输出到 harness（失败时可查链路）+ ERROR 级计数（场景 4 判据）
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_test_writer())
        .with(ErrorCounter);
    if subscriber.try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    // AppContext 必须先于任何 WS 连接初始化：actor 的 stopping()/认证 handler
    // 都会调用 AppContext::global()（未初始化即 panic）；sync 事件链路同步就位
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：两客户端认证 + WS 驱动广播 + 排除发送者 ====================
    // A（发送端）与 B（接收端）先配对拿 token，再 JWT 重连认证（重连路径
    // 正确携带 device_name，广播排除语义才生效，见 pair_and_get_token 注释）；
    // A 发 SessionControl::RemoveSession 触发服务端广播：B 收到
    // SyncData(SessionRemoved)，A 只收 echo 不收广播

    let token_a = pair_and_get_token(port, "itest-device-a-04", "ITest A-04", "fp-itest-a-04", "a-04").await;
    // A 现也走事件通道（旧 /ws/terminal 终端通道已删除）；发送端排除语义
    // 由源地址判定，与通道类型无关
    let (mut sink_a, mut stream_a, addr_a) = connect_ws(port, "/ws/event").await;
    let client_id_a = addr_a.to_string();
    authenticate_with_jwt(
        &mut sink_a,
        &mut stream_a,
        &token_a,
        "ITest A-04",
        "fp-itest-a-04",
        "a-04",
    )
    .await;

    let token_b = pair_and_get_token(port, "itest-device-b-04", "ITest B-04", "fp-itest-b-04", "b-04").await;
    // B 连接事件通道（/ws/event）：广播接收方必须是 Event 通道（ticket 02 语义）
    let (mut sink_b, mut stream_b, addr_b) = connect_ws(port, "/ws/event").await;
    let client_id_b = addr_b.to_string();
    authenticate_with_jwt(
        &mut sink_b,
        &mut stream_b,
        &token_b,
        "ITest B-04",
        "fp-itest-b-04",
        "b-04",
    )
    .await;

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

    // 1a. A 发送 RemoveSession（对不存在会话无副作用，仅触发广播链路）
    let ghost_session_1 = "itest-ghost-04-1";
    let control = Message::session_control_with_response(
        SessionControlAction::RemoveSession {
            session_id: ghost_session_1.to_string(),
        },
        None,
    );
    let control_id = control.message_id().expect("control message must carry id").to_string();
    sink_a
        .send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");

    // 1b. A 收到 echo 响应（message_id 回显 + 会话 ID 回显）；
    // 跳过认证后补发的文件服务快照等无关推送
    let resp = wait_for_message(&mut stream_a, |m| matches!(m, Message::SessionControl { .. })).await;
    match resp {
        Message::SessionControl {
            message_id, payload, ..
        } => {
            assert_eq!(message_id, control_id, "echo must carry original message_id");
            match payload.action {
                SessionControlAction::RemoveSession { session_id } => {
                    assert_eq!(session_id, ghost_session_1, "echo must carry removed session id");
                }
                other => panic!("expected RemoveSession echo, got: {other:?}"),
            }
        }
        other => panic!("expected SessionControl echo, got: {other:?}"),
    }

    // 1c. B 经事件通道收到 SyncData 广播（ticket 02 正式语义：广播只投递
    // Event 通道；接收方 B 连接 /ws/event，与过渡期断言反转的对应恢复）
    let sync = wait_for_sync_data(&mut stream_b, "event channel must receive sync broadcast").await;
    match sync {
        Message::SyncData { payload, .. } => match payload {
            SyncPayload::SessionRemoved { session_id, .. } => {
                assert_eq!(session_id, ghost_session_1, "broadcast must carry removed session id");
            }
            other => panic!("expected SessionRemoved sync payload, got: {other:?}"),
        },
        other => panic!("expected SyncData, got: {other:?}"),
    }

    // 1d. 排除语义：A（发送端）在广播发出后不应收到 SyncData
    assert_no_sync_data(&mut stream_a, Duration::from_millis(500), "sender exclusion").await;

    // ==================== 场景 2：断开 B 后广播不再送达 B（注册表清理生效） ====================

    close_ws(&mut sink_b, &mut stream_b).await;
    drop(sink_b);
    drop(stream_b);

    // 注销经 actor stopping() 异步落地，轮询等待注册表移除 B
    assert!(
        wait_until(|| async { registry_entry(&client_id_b).await.is_none() }).await,
        "disconnected client must be unregistered from WsSessionRegistry"
    );

    // 2a. 注册表层断言：直接向 B 的 client_id 发送必须失败（条目已清理）
    let direct_send = WsSessionRegistry::global()
        .send_to_client(&client_id_b, "probe-after-disconnect".to_string())
        .await;
    assert!(
        direct_send.is_err(),
        "send to disconnected client must fail after registry cleanup, got: {direct_send:?}"
    );

    // 2b. A 再次触发排除语义广播（此时除 A 外无其他已认证客户端 → 无送达目标）
    let ghost_session_2 = "itest-ghost-04-2";
    let control = Message::session_control_with_response(
        SessionControlAction::RemoveSession {
            session_id: ghost_session_2.to_string(),
        },
        None,
    );
    let control_id = control.message_id().expect("control message must carry id").to_string();
    sink_a
        .send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");
    let resp = wait_for_message(&mut stream_a, |m| matches!(m, Message::SessionControl { .. })).await;
    assert!(
        matches!(resp, Message::SessionControl { message_id, .. } if message_id == control_id),
        "A must still get echo after B disconnected"
    );
    assert_no_sync_data(&mut stream_a, Duration::from_millis(500), "no broadcast target").await;

    // 2c. 全员广播（WebSocketManager::broadcast）到达全部事件通道：A 现为
    // /ws/event 通道（旧 /ws/terminal 已删除，终端通道不再存在）→ A 应收到。
    // 旧“终端通道对广播不设投递”语义随兼容路由下线而废弃
    let ctrl = Message::sync_data(SyncPayload::SessionModeChanged {
        session_id: "itest-ctrl-04".to_string(),
        auto_approve: true,
    });
    WebSocketManager::global()
        .broadcast(&ctrl)
        .await
        .expect("full broadcast must succeed");
    let sync = wait_for_sync_data(&mut stream_a, "event channel must receive full broadcast").await;
    assert!(
        matches!(
            sync,
            Message::SyncData { payload: SyncPayload::SessionModeChanged { session_id, .. }, .. } if session_id == "itest-ctrl-04"
        ),
        "full broadcast must reach the remaining event client (A)"
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
    // Windows TCP 栈对刚关闭的监听端口需要约 2s 才返回 RST（SYN 重传行为，
    // 实测普通 Python socket 关闭后 connect 同样挂 2s 才拒绝），故超时放宽到
    // 10s——断言核心是连接最终被拒而非无限挂起，"立即失败"在 Windows 不成立
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

    // 4b. 防御性清理路径：模拟生产 WebSocketManager::stop 的防御分支
    // （残留时 warn + unsubscribe + clear_all），验证幂等不报错
    WsSessionRegistry::global().clear_all().await;
    assert_eq!(WsSessionRegistry::global().client_count().await, 0);

    // 4c. 全程（含停机/清理）无 ERROR 级日志——计数 layer 从测试启动即挂载
    assert_eq!(
        ERROR_COUNT.load(Ordering::SeqCst),
        0,
        "no error-level logs are allowed during broadcast & shutdown flow"
    );

    // 清理临时插件目录
    let plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(plugins_dir);
}
