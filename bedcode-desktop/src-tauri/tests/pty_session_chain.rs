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
use bedcode_lib::server::message::{AuthPayload, AuthStage, Message, SessionControlAction, SessionControlPayload};
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
        session_db
            .create_session_config(&config)
            .expect("insert session config failed");

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
/// registry 的 client_id），再升级为 WebSocket；`path` 指定路由（/ws/event
/// 控制通道或 /ws/terminal/session/{id} 终端通道）
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

/// HTTP 配对换取 JWT session_token（POST /api/auth/pairing → verify）
///
/// 旧 WS 配对（RequestPairing/VerifyCode）已随 /ws/terminal 兼容路由删除，
/// 配对统一走 HTTP /api/auth/*；WS 首消息仅接受 JWT（Authenticated/Reauthenticate）
async fn http_pair_and_get_token(port: u16, tag: &str) -> String {
    let base = format!("http://127.0.0.1:{port}");
    let device_id = format!("itest-device-{tag}");
    let device_name = format!("ITest {tag}");
    let fingerprint = format!("fp-{tag}");
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
        .expect("pairing code must be present")
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
        .expect("session_token must be present")
        .to_string();
    assert!(!token.is_empty(), "session_token must be non-empty");
    token
}

/// 以 JWT session_token 认证 /ws/event 控制通道（Message::Auth 快速路径）
async fn authenticate_with_jwt(sink: &mut WsSend, stream: &mut WsRecv, session_token: &str, tag: &str) {
    let request = Message::Auth {
        message_id: format!("itest-jwt-{tag}"),
        expect_response: false,
        timestamp: 0,
        session_id: None,
        token: String::new(),
        payload: AuthPayload {
            stage: AuthStage::Authenticated,
            device_id: None,
            device_name: Some(format!("ITest {tag}")),
            device_fingerprint: Some(format!("fp-{tag}")),
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
        }
        other => panic!("expected Auth(Authenticated) from JWT re-auth, got: {other:?}"),
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
    let request_id = msg
        .message_id()
        .expect("control message must have message_id")
        .to_string();
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

/// 读取下一条 JSON 控制帧（新路由控制帧协议，跳过二进制输出帧与 Ping/Pong），
/// 5s 超时后 panic；返回解析后的 JSON
async fn recv_frame_json(stream: &mut WsRecv) -> serde_json::Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("timed out waiting for control frame")
            .expect("WS stream closed unexpectedly")
            .expect("WS frame error");
        match frame {
            WsMsg::Text(text) => {
                return serde_json::from_str(&text).expect("control frame must be valid JSON");
            }
            WsMsg::Ping(_) | WsMsg::Pong(_) | WsMsg::Binary(_) | _ => continue,
        }
    }
}

/// 轮询收集新路由（/ws/terminal/session/{id}）终端输出（累积解码后的文本），
/// 直到出现 marker 或超时
///
/// 输出为 TB v2 二进制帧（16B 帧头：magic"TB"+version+flags+seq(8LE)+len(4LE)
/// + payload），JSON 控制帧（auth_ok/subscribe_ok/history_end）与 Ping/Pong 跳过。
/// PTY 输出时序非确定（PowerShell 启动、chcp、回显均无保证），因此是
/// 「轮询到超时」而非「等 N 条帧」。返回已收集文本：断言失败时可借启动
/// marker 判断链路断在哪一段
async fn collect_terminal_output_until(stream: &mut WsRecv, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut text = String::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let frame = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(frame))) => frame,
            _ => break, // 超时 / 流关闭
        };
        match frame {
            WsMsg::Binary(bytes) => {
                // TB v2 帧头：len 在第 12..16 字节（u32 LE）
                if bytes.len() < 16 {
                    continue;
                }
                let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
                if bytes.len() < 16 + len {
                    continue;
                }
                text.push_str(&String::from_utf8_lossy(&bytes[16..16 + len]));
                if text.contains(marker) {
                    break;
                }
            }
            WsMsg::Text(_) | WsMsg::Ping(_) | WsMsg::Pong(_) | WsMsg::Close(_) | _ => continue,
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
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：已认证客户端创建会话 ====================

    // HTTP 配对拿 JWT，/ws/event 控制通道 JWT 首消息认证（旧 WS 配对已删除）
    let token = http_pair_and_get_token(port, "pty-001").await;
    let (mut sink_a, mut stream_a, _addr_a) = connect_ws(port, "/ws/event").await;
    authenticate_with_jwt(&mut sink_a, &mut stream_a, &token, "pty-001").await;

    // 1a. StartSession → 返回会话标识（真实往返：WS → session_control service
    // → SessionManager → openpty + powershell spawn）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::StartSession {
            config_id: config_id.clone(),
        },
        None,
    )
    .await;
    let session_id = match resp {
        Message::SessionControl {
            session_id: Some(sid),
            payload:
                SessionControlPayload {
                    action: SessionControlAction::StartSession { config_id: cfg },
                },
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
    assert!(
        !session_id.is_empty(),
        "created session must carry non-empty session_id"
    );

    // 1b. ListSessions 确认会话已注册且状态 running（会话标识与注册表一致）
    let resp = send_control_and_wait(&mut sink_a, &mut stream_a, SessionControlAction::ListSessions, None).await;
    match resp {
        Message::SessionControl {
            payload:
                SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions },
                },
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

    // ==================== 场景 2：新路由订阅输出 + 写入 echo → 收到输出 ====================
    // 终端 I/O 走新路由 /ws/terminal/session/{id}（简化控制帧：auth → subscribe →
    // input，输出为 TB v2 二进制帧）。旧多会话 /ws/terminal 自订阅通道已删除
    let (mut sink_t, mut stream_t, _addr_t) = connect_ws(port, &format!("/ws/terminal/session/{session_id}")).await;

    // 2a. 首消息 JWT 认证 → auth_ok
    sink_t
        .send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
        .await
        .expect("send auth frame failed");
    let auth_ok = recv_frame_json(&mut stream_t).await;
    assert_eq!(auth_ok["type"], "auth_ok", "session route must auth with JWT");

    // 2b. subscribe → subscribe_ok（订阅即连接：快照回放 + 实时推送）
    sink_t
        .send(WsMsg::Text(r#"{"type":"subscribe"}"#.into()))
        .await
        .expect("send subscribe frame failed");
    let sub_ok = recv_frame_json(&mut stream_t).await;
    assert_eq!(sub_ok["type"], "subscribe_ok", "session route must subscribe");

    // 2c. 写入 echo 命令（input data 为 UTF-8 → base64，与 handle_session_input 编码约定一致）
    let marker = format!("BEDCODE_PTY_ECHO_{session_id}");
    let input_b64 = base64::engine::general_purpose::STANDARD.encode(format!("echo {marker}\r\n").as_bytes());
    sink_t
        .send(WsMsg::Text(
            format!(r#"{{"type":"input","data":"{input_b64}"}}"#).into(),
        ))
        .await
        .expect("send input frame failed");

    // 2d. 轮询 + 宽容超时：PowerShell 启动与回显时序非确定，断言「最终包含」而非即时到达
    let collected = collect_terminal_output_until(&mut stream_t, &marker, Duration::from_secs(20)).await;
    assert!(
        collected.contains(&marker),
        "PTY echo output not observed within 20s; collected so far: {collected:?} \
         （空输出 = 环境问题（powershell 未启动/未读到输出）；有启动输出无 echo = 输入链路缺陷）"
    );

    // 终端通道使命完成，断开
    let _ = sink_t.send(WsMsg::Close(None)).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), stream_t.next()).await;
    drop(sink_t);
    drop(stream_t);

    // ==================== 场景 3：关闭会话 → 状态一致 ====================

    // 3a. StopSession 响应回显 session_id（kill_session 已 await，响应即状态已落库）
    let resp = send_control_and_wait(
        &mut sink_a,
        &mut stream_a,
        SessionControlAction::StopSession {
            session_id: session_id.clone(),
        },
        Some(&session_id),
    )
    .await;
    match resp {
        Message::SessionControl {
            session_id: Some(sid),
            payload:
                SessionControlPayload {
                    action: SessionControlAction::StopSession { session_id: act_sid },
                },
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
    let resp = send_control_and_wait(&mut sink_a, &mut stream_a, SessionControlAction::ListSessions, None).await;
    match resp {
        Message::SessionControl {
            payload:
                SessionControlPayload {
                    action: SessionControlAction::SessionList { sessions },
                },
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

    let (mut sink_b, mut stream_b, _addr_b) = connect_ws(port, "/ws/event").await;

    // 与 02 场景 3a 的拒绝行为衔接：业务消息在 actor 状态机层被拦（authenticated=false）
    let control = Message::session_control_with_response(
        SessionControlAction::StartSession {
            config_id: config_id.clone(),
        },
        None,
    );
    sink_b
        .send(WsMsg::Text(control.to_json().expect("serialize control failed").into()))
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
