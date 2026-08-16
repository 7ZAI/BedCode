//! PTY 会话链路集成测试（spec L1 场景 6，ticket 03）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端模拟移动端 → 已认证客户端经 WS 创建真实 PTY
//! 会话（portable-pty 原生实现，Windows 走 ConPTY）→ 写入 echo 命令 →
//! 经「PtyReader 线程 → GlobalOutputManager → 订阅者通道 → forward_loop →
//! WS 帧」真实链路收到输出 → 关闭会话并核对状态一致，未认证客户端创建
//! 会话被拒（与 02 票 AUTH_REQUIRED 行为衔接）。
//!
//! 环境依赖（PTY 断言失败时先区分测试环境问题与链路缺陷）：
//! - 真实 PTY 需 spawn powershell.exe（非 "wsl2" 环境 → WindowsShell::PowerShell，
//!   build_command 已注入 chcp 65001 + UTF-8 OutputEncoding）。PATH 缺
//!   powershell / 系统禁 ConPTY 属环境问题：StartSession 会回
//!   SESSION_CONTROL_ERROR，panic 消息会带上原始错误便于判别
//! - 输出编码：启动脚本已强制 UTF-8，断言用 ASCII marker，失败时断言消息
//!   附带已收集的原始文本（可见是否收到启动横幅等半程输出）辅助判别
//!
//! 串行化：本文件只含一个 `#[tokio::test]`（场景子步骤严格串行）。
//! tests/ 下每个文件是独立测试二进制 → 与 01/02 文件进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry / GlobalOutputManager 等
//! 全局单例天然互不冲突。等待异步事件统一 `tokio::time::sleep + yield_now`
//! （current_thread runtime 禁止 std::thread::sleep），每处 await 都有
//! timeout 防 CI 卡死；PTY 输出时序非确定 → 轮询 + 宽容超时断言
//! （真实往返断言：WS → 会话管理器 → openpty → 子进程 → 输出回传，非恒真）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use base64::Engine as _;
use bedcode_lib::db::{Database, SessionConfig};
use bedcode_lib::events::DesktopSyncEvent;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::plugin::PluginHost;
use bedcode_lib::server::app::start_http_server;
use bedcode_lib::server::message::{
    AuthPayload, AuthStage, Message, SessionControlAction, SessionControlPayload, TerminalAction,
};
use bedcode_lib::server::services::pairing_service::PairingService;
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

/// 组装真实服务 AppContext（app_handle=None 无头模式）+ 预置会话配置，
/// 每个测试进程只 init 一次，返回预置的会话配置 ID（StartSession 依赖）
///
/// 与 02 基建一致：全部服务用真实实现 + 内存 SQLite，插件宿主走真实
/// `PluginHost::new`（wasmtime 引擎初始化 + 空插件目录扫描），仅 Tauri
/// 前端事件能力降级。差异：会话配置直接写入 session_manager 自己的存储库
/// ——生产路径 session_manager 与 config_manager 共享同一 DB，02 为隔离
/// 配对记录拆成两个内存库，故此处向 session_db 预置配置供 create_session 读取
async fn init_test_app_context() -> String {
    static INIT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(config_id) = INIT.get() {
        return config_id.clone();
    }
    let config_id = {
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

        // 预置会话配置：环境取 "windows"（非 "wsl2" → PowerShell），启动命令输出
        // 固定 marker（区分「环境就绪但输入链路断」与「PTY 起不来」两种失败形态）
        let config = SessionConfig::new(
            "itest-pty".to_string(),
            "windows".to_string(),
            std::env::temp_dir().to_string_lossy().into_owned(),
            "echo BEDCODE_PTY_STARTUP_MARKER".to_string(),
        );
        let config_id = config.id.clone();
        session_db.create_session_config(&config).expect("insert session config failed");

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

        config_id
    };
    let _ = INIT.set(config_id.clone());
    config_id
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

/// 读取下一条业务消息（跳过 Ping/Pong），带超时；超时返回 None
async fn recv_message_timeout(stream: &mut WsRecv, timeout: Duration) -> Option<Message> {
    loop {
        let frame = tokio::time::timeout(timeout, stream.next()).await.ok()??;
        let frame = frame.expect("WS frame error");

        match frame {
            WsMsg::Text(text) => {
                return Some(Message::from_json(&text).expect("server message must be valid protocol JSON"));
            }
            WsMsg::Ping(_) | WsMsg::Pong(_) => continue,
            _ => continue,
        }
    }
}

/// 读取下一条业务消息（跳过 Ping/Pong），5s 超时后 panic
async fn recv_message(stream: &mut WsRecv) -> Message {
    recv_message_timeout(stream, Duration::from_secs(5))
        .await
        .expect("timed out waiting for WS message")
}

/// 轮询读取直到谓词命中的消息（跳过其余帧，如配对后推送的文件服务快照、
/// 输出帧、心跳），整体超时后 panic
async fn recv_until<F>(stream: &mut WsRecv, mut pred: F, timeout: Duration) -> Message
where
    F: FnMut(&Message) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("recv_until timed out ({timeout:?}) waiting for matching message");
        }
        let msg = recv_message_timeout(stream, remaining)
            .await
            .expect("timed out waiting for WS message");
        if pred(&msg) {
            return msg;
        }
    }
}

/// 发送 RequestPairing 并断言收到 VerifyCode 响应（配对请求半程，供认证复用）
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

/// 完成「配对 → 配对码验证 → 认证」全流程，返回 session_token
///
/// 与 02 场景 1 相同：配对码在无头模式下不经过前端事件投递，直接从
/// PairingService 单例读取（与 WS handler 经 AppContext 共享同一实例）
async fn pair_and_authenticate(sink: &mut WsSend, stream: &mut WsRecv, tag: &str) -> String {
    let device_id = format!("itest-device-{tag}");
    let device_name = format!("ITest {tag}");
    let fingerprint = format!("fp-{tag}");

    request_pairing_and_expect_verify_code(
        sink,
        stream,
        &format!("itest-pair-{tag}"),
        &device_id,
        &device_name,
    )
    .await;

    let code = bedcode_lib::AppContext::global()
        .pairing_service()
        .get_current_code()
        .await
        .expect("pairing code must exist after request_pairing")
        .code;
    assert!(!code.is_empty(), "pairing code must be non-empty");

    let verify = Message::Auth {
        message_id: format!("itest-verify-{tag}"),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::VerifyCode,
            pairing_code: Some(code),
            device_id: Some(device_id),
            device_name: Some(device_name),
            device_fingerprint: Some(fingerprint),
            ..Default::default()
        },
    };
    sink.send(WsMsg::Text(verify.to_json().expect("serialize verify failed").into()))
        .await
        .expect("send verify_code failed");

    let resp = recv_message(stream).await;
    match resp {
        Message::Auth { payload, .. } => {
            assert_eq!(
                payload.stage,
                AuthStage::Authenticated,
                "valid pairing code must yield Authenticated"
            );
            payload.session_token.expect("authenticated response must carry session_token")
        }
        other => panic!("expected Auth(Authenticated) response, got: {other:?}"),
    }
}

/// 发送会话控制消息并等待 message_id 回显的响应（跳过中途的其他帧）
async fn send_control_and_wait(
    sink: &mut WsSend,
    stream: &mut WsRecv,
    action: SessionControlAction,
    session_id: Option<&str>,
) -> Message {
    let msg = Message::session_control_with_response(action, session_id);
    let request_id = msg.message_id().expect("control message must have message_id").to_string();
    sink.send(WsMsg::Text(msg.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");
    recv_until(
        stream,
        |m| m.message_id().is_some_and(|id| id == request_id.as_str()),
        Duration::from_secs(10),
    )
    .await
}

/// 轮询收集指定会话的终端输出（累积解码后的文本），直到出现 marker 或超时
///
/// 输出事件是 `Message::Terminal { action: Output, data: base64 }`（移动端
/// WS 通道形态，经 forward_loop 合并/直通编码），其余消息帧（心跳、
/// SubscribeResponse、文件服务推送等）跳过。PTY 输出时序非确定（PowerShell
/// 启动、chcp、回显均无保证），因此是「轮询到超时」而非「等 N 条帧」。
/// 返回已收集文本：断言失败时可借启动 marker 判断链路断在哪一段
async fn collect_output_until(
    stream: &mut WsRecv,
    session_id: &str,
    marker: &str,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    let mut text = String::new();
    while Instant::now() < deadline {
        let Some(msg) = recv_message_timeout(stream, Duration::from_millis(300)).await else {
            continue;
        };
        match msg {
            Message::Terminal { session_id: sid, payload, .. } if sid == session_id => {
                if let TerminalAction::Output { data, .. } = payload.action {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(&data)
                        .unwrap_or_else(|e| panic!("output base64 decode failed: {e}"));
                    text.push_str(&String::from_utf8_lossy(&bytes));
                    if text.contains(marker) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    text
}

#[tokio::test]
async fn pty_session_chain_flow() {
    // 测试日志输出到 harness（失败时可查链路）；重复 init 静默跳过
    if tracing_subscriber::fmt().with_test_writer().try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    // AppContext 必须先于任何 WS 连接初始化：actor 的 stopping()/认证 handler
    // 都会调用 AppContext::global()（未初始化即 panic）
    let config_id = init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port)
        .await
        .expect("test server must start");

    // ==================== 场景 1：已认证客户端创建会话 ====================

    let (mut sink_a, mut stream_a, _addr_a) = connect_ws(port).await;
    let _token = pair_and_authenticate(&mut sink_a, &mut stream_a, "pty-001").await;

    // 1a. StartSession → 返回会话标识（真实往返：WS → session_control service
    // → SessionManager → openpty + powershell spawn）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::StartSession { config_id: config_id.clone() },
        None,
    )
    .await;
    let session_id = match resp {
        Message::SessionControl {
            session_id: Some(sid),
            payload: SessionControlPayload { action: SessionControlAction::StartSession { config_id: cfg } },
            ..
        } => {
            assert_eq!(cfg, config_id, "response must echo requested config_id");
            sid
        }
        Message::Error { code, message, .. } => panic!(
            "StartSession rejected ({code}: {message})——若为 spawn 失败属测试环境问题 \
             （powershell.exe 不在 PATH / ConPTY 不可用），否则为链路缺陷"
        ),
        other => panic!("expected SessionControl(StartSession) response, got: {other:?}"),
    };
    assert!(!session_id.is_empty(), "created session must carry non-empty session_id");

    // 1b. ListSessions 确认会话已注册且状态 running（会话标识与注册表一致）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::ListSessions,
        None,
    )
    .await;
    match resp {
        Message::SessionControl {
            payload: SessionControlPayload { action: SessionControlAction::SessionList { sessions } },
            ..
        } => {
            let entry = sessions
                .iter()
                .find(|s| s.id == session_id)
                .expect("created session must appear in session list");
            assert_eq!(entry.status, "running", "newly created session must be running");
            assert_eq!(entry.config_id.as_deref(), Some(config_id.as_str()));
        }
        other => panic!("expected SessionControl(SessionList) response, got: {other:?}"),
    }

    // ==================== 场景 2：写入 echo → 收到包含预期输出的输出事件 ====================

    // 订阅输出（GlobalOutputManager 订阅者通道 + forward_loop → WS 帧；
    // start_seq=None → 全量重播，先于订阅到达的启动输出也会回放）
    let sub = Message::subscribe(&session_id, None);
    sink_a.send(WsMsg::Text(sub.to_json().expect("serialize subscribe failed").into()))
        .await
        .expect("send subscribe failed");

    // 写入简单命令：marker 带会话 ID 后缀保证本测试进程内唯一
    let marker = format!("BEDCODE_PTY_ECHO_{session_id}");
    let input = Message::input(&session_id, &format!("echo {marker}\r\n"), None);
    sink_a.send(WsMsg::Text(input.to_json().expect("serialize input failed").into()))
        .await
        .expect("send input failed");

    // 轮询 + 宽容超时：PowerShell 启动与回显时序非确定，断言「最终包含」而非即时到达
    let collected = collect_output_until(&mut stream_a, &session_id, &marker, Duration::from_secs(20)).await;
    assert!(
        collected.contains(&marker),
        "PTY echo output not observed within 20s; collected so far: {collected:?} \
         （空输出 = 环境问题（powershell 未启动/未读到输出）；有启动输出无 echo = 输入链路缺陷）"
    );

    // ==================== 场景 3：关闭会话 → 状态一致 ====================

    // 3a. StopSession 响应回显 session_id（kill_session 已 await，响应即状态已落库）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::StopSession { session_id: session_id.clone() },
        Some(&session_id),
    )
    .await;
    match resp {
        Message::SessionControl {
            session_id: Some(sid),
            payload: SessionControlPayload { action: SessionControlAction::StopSession { session_id: act_sid } },
            ..
        } => {
            assert_eq!(sid, session_id, "stop response must echo session_id");
            assert_eq!(act_sid, session_id, "stop action must carry session_id");
        }
        Message::Error { code, message, .. } => {
            panic!("StopSession rejected ({code}: {message})");
        }
        other => panic!("expected SessionControl(StopSession) response, got: {other:?}"),
    }

    // 3b. 后续操作状态一致：ListSessions 中该会话报告 stopped（而非消失或仍 running）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::ListSessions,
        None,
    )
    .await;
    match resp {
        Message::SessionControl {
            payload: SessionControlPayload { action: SessionControlAction::SessionList { sessions } },
            ..
        } => {
            let entry = sessions
                .iter()
                .find(|s| s.id == session_id)
                .expect("stopped session must still be listed");
            assert_eq!(
                entry.status, "stopped",
                "session must report stopped after StopSession, got: {}",
                entry.status
            );
        }
        other => panic!("expected SessionControl(SessionList) response, got: {other:?}"),
    }

    // ==================== 场景 4：未认证客户端创建会话被拒 ====================

    let (mut sink_b, mut stream_b, _addr_b) = connect_ws(port).await;

    // 与 02 场景 3a 的拒绝行为衔接：业务消息在 actor 状态机层被拦（authenticated=false）
    let control = Message::session_control_with_response(
        SessionControlAction::StartSession { config_id: config_id.clone() },
        None,
    );
    sink_b.send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
        .await
        .expect("send session_control failed");

    let resp = recv_message(&mut stream_b).await;
    match resp {
        Message::Error { code, message, .. } => {
            assert_eq!(code, "AUTH_REQUIRED", "unauthenticated StartSession must be rejected");
            assert!(
                message.contains("authenticate"),
                "rejection must carry clear message, got: {message}"
            );
        }
        other => panic!("expected Error(AUTH_REQUIRED) response, got: {other:?}"),
    }

    // ==================== 收尾：显式关闭连接 + 优雅停机 + 清理 ====================

    // 先发 Close 让服务端 actor 走 stopping()（注销注册表 + 取消订阅），再优雅停机，
    // 避免 stop(true) 等待长连接（与 02 相同策略）
    for (mut sink, mut stream) in [(sink_a, stream_a), (sink_b, stream_b)] {
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
