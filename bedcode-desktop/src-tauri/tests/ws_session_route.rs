//! 每会话终端路由集成测试（spec §5.1/§5.3，ticket 06）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端模拟移动端 → 直连 `/ws/terminal/session/{id}`，
//! 覆盖：未认证拒绝对称、JWT 认证 + 会话存在性校验（SESSION_NOT_FOUND）、
//! 快照订阅（subscribe_ok → history_end → 实时 TB v2 二进制帧）、
//! 会话停止通知（session_stopped 帧）。
//!
//! 会话存在性用 GlobalOutputManager::register_session 注册假会话（不启动
//! 真实 PTY）：订阅 → 空历史 history_end → 手动 on_output 推帧 → 断言
//! TB v2 帧头（magic/version/seq/len/data 与 flags 事件数编码）。
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）。
//! tests/ 下每个文件是独立测试二进制 → 与其余集成测试进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry 等全局单例天然互不冲突。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use actix_web::dev::ServerHandle;
use bedcode_lib::db::Database;
use bedcode_lib::events::DesktopSyncEvent;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::plugin::PluginHost;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::server::services::pairing_service::PairingService;
use bedcode_lib::session::{GlobalOutputManager, OutputEvent, SessionConfigManager, SessionManager};
use bedcode_lib::system::app_context::{AppContext, AppContextBuilder};
use bedcode_lib::system::constants::network::SYNC_EVENT_BROADCAST_CAPACITY;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::utils::auth::jwt::JwtService;
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
async fn spawn_test_server(port: u16) -> std::io::Result<(ServerHandle, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 组装真实服务 AppContext（app_handle=None 无头模式），每个测试进程只 init 一次
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
            .sync_tx(sync_tx)
            .resource_dir(Arc::new(PathBuf::from(".")))
            .system_info(system_info)
            .build_and_init();
        let _ = INIT.set(());
    }
}

// PairingService 类型别名（与 ws_pairing_auth.rs 相同组装，避免直接依赖）

/// 建立新路由 WS 连接：TCP（记录本地地址 = 服务端看到的 peer addr）→ 升级
async fn connect_session_ws(
    port: u16,
    session_id: &str,
) -> (WsSend, WsRecv, std::net::SocketAddr) {
    let tcp = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("tcp connect to test server failed");
    let local_addr = tcp.local_addr().expect("read local addr failed");
    let url = format!("ws://127.0.0.1:{port}/ws/terminal/session/{session_id}");
    let (ws, _) = tokio_tungstenite::client_async(url, tcp)
        .await
        .expect("ws upgrade failed");
    let (send, recv) = ws.split();
    (send, recv, local_addr)
}

/// 等待 WS 消息（带超时），超时即 panic——测试夹具统一收口
async fn recv_msg(recv: &mut WsRecv, what: &str) -> WsMsg {
    tokio::time::timeout(Duration::from_secs(5), recv.next())
        .await
        .unwrap_or_else(|_| panic!("timeout waiting for {what}"))
        .expect("ws stream ended")
        .expect("ws recv error")
}

/// 解析文本消息为 JSON Value（非文本消息 → panic）
async fn recv_text_json(recv: &mut WsRecv, what: &str) -> serde_json::Value {
    match recv_msg(recv, what).await {
        WsMsg::Text(t) => serde_json::from_str(&t).unwrap_or_else(|e| panic!("{what} not valid JSON: {e}: {t}")),
        other => panic!("{what}: expected text message, got {other:?}"),
    }
}

/// 等待连接关闭（服务端 ctx.stop() 后客户端可能收到 Close 帧或流直接结束；
/// 并行测试负载下时序不保证，两种形态都视为关闭）
async fn expect_ws_close(recv: &mut WsRecv, what: &str) {
    match tokio::time::timeout(Duration::from_secs(5), recv.next()).await {
        Ok(Some(Ok(WsMsg::Close(_)))) => {}
        Ok(Some(Err(_))) | Ok(None) => {}
        Ok(Some(Ok(other))) => panic!("{what}: expected connection close, got {other:?}"),
        Err(_) => panic!("{what}: timeout waiting for connection close"),
    }
}

/// 校验 TB v2 帧头（spec §5.3：magic "TB" + version=2 + flags + seq(8 LE) + len(4 LE) + data）
/// 返回 (seq, event_count, is_waiting, data)
fn check_tb_v2_frame<'a>(frame: &'a [u8], what: &str) -> (u64, usize, bool, &'a [u8]) {
    assert!(frame.len() >= 16, "{what}: frame too short: {} bytes", frame.len());
    assert_eq!(&frame[0..2], b"TB", "{what}: bad magic");
    assert_eq!(frame[2], 2, "{what}: bad version");
    let flags = frame[3];
    let is_waiting = flags & 0x01 != 0;
    let event_count = ((flags >> 1) as usize) + 1;
    let seq = u64::from_le_bytes(frame[4..12].try_into().unwrap());
    let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
    assert_eq!(len, frame.len() - 16, "{what}: len field mismatch");
    (seq, event_count, is_waiting, &frame[16..])
}

/// 签发测试 JWT（JwtService 内部固定密钥，验证端同一把密钥）
fn make_test_token(device_id: &str) -> String {
    JwtService::new()
        .generate_token(device_id.to_string(), Some("test-device".to_string()), Some("fp-test-1".to_string()))
        .expect("generate test token failed")
}

// ==================== 集成测试 ====================

/// 主场景流（单一 #[tokio::test]，子步骤严格串行）：
/// 1. 未认证发 subscribe → error(AUTH_REQUIRED) + 关闭（spec §4.3 拒绝对称）
/// 2. 认证 + 会话不存在 → error(SESSION_NOT_FOUND) + 关闭（spec §5.1）
/// 3. 认证 + 会话存在 → auth_ok → subscribe → subscribe_ok + history_end →
///    手动 on_output → TB v2 实时帧（seq/事件数/字节校验）→ 空历史边界
/// 4. 会话停止 → session_stopped 帧推送
#[tokio::test]
async fn session_terminal_route_full_flow() {
    let port = pick_free_port();
    init_test_app_context().await;
    let (_server_handle, _server_task) = spawn_test_server(port)
        .await
        .expect("start test server failed");

    // 会话存在性：直接注册假会话（不启动真实 PTY，聚焦路由/协议行为）
    let global_manager = GlobalOutputManager::global();
    let session_id = "itest-session-1";
    let output_manager = global_manager.register_session(session_id).await;

    let token = make_test_token("device-itest-1");

    // ---------- 场景 1：未认证发业务帧 → AUTH_REQUIRED + 关闭 ----------
    {
        let (mut send, mut recv, _addr) = connect_session_ws(port, session_id).await;
        send.send(WsMsg::Text(r#"{"type":"subscribe"}"#.into()))
            .await
            .expect("send subscribe failed");

        let msg = recv_text_json(&mut recv, "auth required error").await;
        assert_eq!(msg["type"], "error");
        assert_eq!(msg["code"], "AUTH_REQUIRED");

        // 拒绝对称：错误后服务端关闭连接（Close 帧或流结束）
        expect_ws_close(&mut recv, "after AUTH_REQUIRED").await;
    }

    // ---------- 场景 2：认证通过但会话不存在 → SESSION_NOT_FOUND + 关闭 ----------
    {
        let (mut send, mut recv, _addr) = connect_session_ws(port, "itest-no-such-session").await;
        send.send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
            .await
            .expect("send auth failed");

        let msg = recv_text_json(&mut recv, "session not found error").await;
        assert_eq!(msg["type"], "error");
        assert_eq!(msg["code"], "SESSION_NOT_FOUND");
        assert!(
            msg["message"].as_str().unwrap_or_default().contains("itest-no-such-session"),
            "error message should carry session id"
        );

        expect_ws_close(&mut recv, "after SESSION_NOT_FOUND").await;
    }

    // ---------- 场景 3：完整订阅流 ----------
    {
        let (mut send, mut recv, _addr) = connect_session_ws(port, session_id).await;

        // 认证 → auth_ok
        send.send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
            .await
            .expect("send auth failed");
        let msg = recv_text_json(&mut recv, "auth_ok").await;
        assert_eq!(msg["type"], "auth_ok");

        // 无效 token → AUTH_FAILED + 关闭（另一连接验证拒绝路径）
        {
            let (mut bad_send, mut bad_recv, _) = connect_session_ws(port, session_id).await;
            bad_send
                .send(WsMsg::Text(r#"{"type":"auth","token":"invalid-jwt"}"#.into()))
                .await
                .expect("send bad auth failed");
            let msg = recv_text_json(&mut bad_recv, "auth failed error").await;
            assert_eq!(msg["type"], "error");
            assert_eq!(msg["code"], "AUTH_FAILED");
            expect_ws_close(&mut bad_recv, "after AUTH_FAILED").await;
        }

        // 订阅 → subscribe_ok（快照元数据）+ history_end（空历史）
        send.send(WsMsg::Text(r#"{"type":"subscribe"}"#.into()))
            .await
            .expect("send subscribe failed");
        let msg = recv_text_json(&mut recv, "subscribe_ok").await;
        assert_eq!(msg["type"], "subscribe_ok");
        let snapshot_seq = msg["snapshot_seq"].as_u64().expect("snapshot_seq");
        assert_eq!(msg["min_seq"], snapshot_seq, "empty history: min == snapshot");
        assert_eq!(msg["history_count"], 0);

        let msg = recv_text_json(&mut recv, "history_end").await;
        assert_eq!(msg["type"], "history_end");
        assert_eq!(msg["snapshot_seq"], snapshot_seq);

        // 实时输出：手动推两条事件 → TB v2 二进制帧（seq = 首事件 index）
        output_manager
            .on_output(OutputEvent {
                session_id: session_id.to_string(),
                data: b"hello ".to_vec(),
                index: snapshot_seq + 1,
                timestamp: 0,
                is_waiting: false,
            })
            .await;
        output_manager
            .on_output(OutputEvent {
                session_id: session_id.to_string(),
                data: b"world".to_vec(),
                index: snapshot_seq + 2,
                timestamp: 0,
                is_waiting: true,
            })
            .await;

        let frame = match recv_msg(&mut recv, "TB v2 output frame").await {
            WsMsg::Binary(b) => b,
            other => panic!("expected binary output frame, got {other:?}"),
        };
        let (seq, event_count, is_waiting, data) = check_tb_v2_frame(&frame, "merged live frame");
        assert_eq!(seq, snapshot_seq + 1, "seq = first event index");
        // 两条小事件可能合并（30ms 窗）或拆分（直通）——事件数 ∈ [1,2]，
        // 且字节内容 = 按序拼接
        assert!(event_count <= 2, "at most 2 events merged: {event_count}");
        let expected: Vec<u8> = if event_count == 2 {
            b"hello world".to_vec()
        } else {
            b"hello ".to_vec()
        };
        assert_eq!(data, expected.as_slice());
        assert_eq!(is_waiting, event_count == 2, "is_waiting = last event's flag");
    }

    // ---------- 场景 4：会话停止 → session_stopped 帧 ----------
    {
        let (mut send, mut recv, _addr) = connect_session_ws(port, session_id).await;
        send.send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
            .await
            .expect("send auth failed");
        let msg = recv_text_json(&mut recv, "auth_ok").await;
        assert_eq!(msg["type"], "auth_ok");

        // 触发会话停止事件（SessionStatusEvent → status broadcast）
        let session_manager = AppContext::global().session_manager();
        session_manager
            .status_tx()
            .send(bedcode_lib::session::SessionStatusEvent {
                session_id: session_id.to_string(),
                old_status: Some(bedcode_lib::session::SessionStatus::Running),
                new_status: bedcode_lib::session::SessionStatus::Stopped,
                session_name: "itest".to_string(),
            })
            .expect("broadcast session stopped");

        let msg = recv_text_json(&mut recv, "session_stopped").await;
        assert_eq!(msg["type"], "session_stopped");
        assert_eq!(msg["session_id"], session_id);
    }

    // 清理：注销测试会话（防御性；tests/ 下每个文件是独立二进制）
    global_manager.unregister_session(session_id).await;
}
