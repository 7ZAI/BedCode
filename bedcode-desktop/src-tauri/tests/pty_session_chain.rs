//! PTY 会话链路集成测试（spec L1 场景 6，ticket 03）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端模拟移动端 → 已认证客户端驱动**内核执行端**
//! `SessionManager::create_session_from_spec` 创建真实 PTY
//! 会话（portable-pty 原生实现，Windows 走 ConPTY）→ 写入 echo 命令 →
//! 经「PtyReader 线程 → GlobalOutputManager → 订阅者通道 → forward_loop →
//! WS 帧」真实链路收到输出 → 关闭会话并核对状态一致，未认证客户端创建
//! 会话被拒（与 02 票 AUTH_REQUIRED 行为衔接）。
//!
//! 环境依赖（PTY 断言失败时先区分测试环境问题与链路缺陷）：
//! - Windows：真实 PTY 需 spawn powershell.exe（`-NoExit -Command`，UTF-8 输出编码
//!   由 `test_launch_config` 的 argv 决定——2026-09-23 PTY 解耦后宿主不再包装）。
//!   PATH 缺 powershell / 系统禁 ConPTY 属环境问题；
//! - Linux/macOS：走 ExecutionEnvironment::Linux 原生 bash（`bash -lic`，尾部
//!   `exec bash` 保驻留）。两种环境下 StartSession 失败都会回
//!   SESSION_CONTROL_ERROR，panic 消息带原始错误
//! - 输出编码：启动脚本已强制 UTF-8，断言用 ASCII marker，失败时断言消息
//!   附带已收集的原始文本（可见是否收到启动横幅等半程输出）辅助判别
//!
//! 票 13 口径（创建不再经 WS SessionControl → 插件编排）：v21 起 `StartSession`
//! 的编排（配置真源 → launch spec → 命名唯一化）整体在会话中心插件，读的是**插件
//! 私有库**；集成测试（tests/ 是独立 crate）无法注入 `plugin_db_root`（私有字段，
//! 仅 crate 内 `#[cfg(test)]` 可写）→ 无头上下文私有库不可达，插件编排必然失败。
//! 故本 target 直接驱动内核执行端（等价于插件经 host-session `create-with-spec`
//! 到达的同一入口），WS 控制通道仍覆盖 ListSessions / StopSession / RemoveSession
//! 与未认证拒绝；令牌经插件驱动的 HTTP 配对换取（夹具语义与迁移前一致）。
//!
//! 串行化：本文件只含一个 `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`（场景子步骤严格串行）。
//! tests/ 下每个文件是独立测试二进制 → 与 01/02 文件进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry / GlobalOutputManager 等
//! 全局单例天然互不冲突。等待异步事件统一 `tokio::time::sleep + yield_now`
//! （current_thread runtime 禁止 std::thread::sleep），每处 await 都有
//! timeout 防 CI 卡死；PTY 输出时序非确定 → 轮询 + 宽容超时断言
//! （真实往返断言：WS → 会话管理器 → openpty → 子进程 → 输出回传，非恒真）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use base64::Engine as _;
use bedcode_lib::db::Database;
use bedcode_lib::enums::{ExecutionEnvironment, SessionLaunchConfig, WindowsShell};
use bedcode_lib::events::DesktopSyncEvent;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::wasm_core::PluginHost;
use bedcode_lib::server::core::app::start_http_server;
use bedcode_lib::enums::{AuthPayload, AuthStage, SessionControlAction, SessionControlPayload};
use bedcode_lib::server::websocket::message::Message;
use bedcode_lib::session::{SessionConfigManager, SessionManager};
use bedcode_lib::system::app_context::AppContext;
use bedcode_lib::system::app_context::AppContextBuilder;
use bedcode_lib::system::constants::SYNC_EVENT_BROADCAST_CAPACITY;
use bedcode_lib::system::info::SystemInfo;
use bedcode_lib::AppConfig;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tokio_tungstenite::WebSocketStream;

/// WS 接收流（split 后的读半部）
type WsRecv = futures_util::stream::SplitStream<WebSocketStream<TcpStream>>;
/// WS 发送流（split 后的写半部）
type WsSend = futures_util::stream::SplitSink<WebSocketStream<TcpStream>, WsMsg>;

/// 会话中心插件 id（认证端点编排的权威实现方）
const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";

/// 随包插件产物目录（`cargo test` 前须重建产物，见 AGENTS §3）
///
/// 产物缺失时**显性失败**而非跳过：本 target 的令牌换取链没有别的驱动方式，
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

/// 无头集成测试的会话中心插件私有库根（v24 认证记录下沉后配对/历史真源在
/// 私有库；无头上下文无 AppHandle，经 `set_plugin_db_root` 注入，activate 前设置）
fn session_plugin_db_root() -> &'static PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("bedcode-ptychain-pluginroot-{}", std::process::id()));
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
///
/// 服务器 future 必须保活（drop 会触发停机），spawn 到测试 runtime 上持续轮询；
/// actix worker 运行在各自线程的独立 runtime，不受 current_thread 测试 runtime 限制
async fn spawn_test_server(port: u16) -> std::io::Result<(ServerHandle, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let config = AppConfig::default().network;
    let (handle, server) = start_http_server(port, &config).await?;
    let server_task = tokio::spawn(server);
    Ok((handle, server_task))
}

/// 测试会话的 config_id：内核执行端不再读配置表（`config_id` 只是会话记录上的
/// 标识字段，真源与映射都在插件侧）
const TEST_CONFIG_ID: &str = "itest-pty";

/// 内核执行端入参（= 插件 `session-create` 算出的 launch spec 形状）
///
/// 2026-09-23 PTY 解耦票：宿主不再做 shell 包装，命令以 **argv 形态**给出——这里
/// 手写与插件 `launch.rs::build_argv` 同形的产物（Linux `bash -lic` / Windows
/// PowerShell `-NoExit -Command`）。shell 必须**驻留**：场景 2/5 要往会话里写
/// `echo` 并观察回显，而 `bash -lic "<只跑一次的脚本>"` 会在脚本结束后立即退出
/// （实测退出码 0，slave 关闭后续输入无回显），故在脚本尾部 `exec bash` 换成交互
/// shell（产物内 `cd … && pwd && <用户命令>` 的用户命令通常本身就是常驻 shell）。
fn test_launch_config() -> SessionLaunchConfig {
    let workdir = std::env::temp_dir().to_string_lossy().into_owned();
    let startup_marker_cmd = "echo BEDCODE_PTY_STARTUP_MARKER";
    let (environment, command_args) = if cfg!(target_os = "windows") {
        (
            ExecutionEnvironment::Windows {
                shell: WindowsShell::PowerShell,
            },
            vec![
                "powershell.exe".to_string(),
                "-NoLogo".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                startup_marker_cmd.to_string(),
            ],
        )
    } else {
        (
            ExecutionEnvironment::Linux,
            vec![
                "bash".to_string(),
                "-lic".to_string(),
                format!("cd '{}' && pwd && {}; exec bash", workdir, startup_marker_cmd),
            ],
        )
    };
    SessionLaunchConfig {
        name: "itest-pty".to_string(),
        environment,
        working_dir: workdir,
        // 启动命令输出固定 marker（区分「环境就绪但输入链路断」与「PTY 起不来」）；
        // 诊断字段，宿主不解释（实际 exec 的是 command_args）
        command: startup_marker_cmd.to_string(),
        command_args,
        env_vars: HashMap::new(),
        cols: 120,
        rows: 40,
    }
}

/// 组装真实服务 AppContext（app_handle=None 无头模式），每个测试进程只 init 一次
///
/// 与 02 基建一致：全部服务用真实实现 + 内存 SQLite，插件宿主走真实
/// `PluginHost::new`（wasmtime 引擎初始化 + 空插件目录扫描），仅 Tauri
/// 前端事件能力降级。
async fn init_test_app_context() {
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

        // 票 13：/api/auth/*（配对码换取 JWT）的编排已整体下沉会话中心插件——宿主
        // 无实现、网关按 PluginRequired 拒转发，故测试改用**真实插件产物**驱动配对。
        let plugins_dir = bundled_plugins_dir();
        // 用户插件目录：独立空目录（复用 plugins_dir 会让随包插件被标成
        // UserInstalled 来源，激活时撞审批门禁）
        let user_plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-userplugins-{}", std::process::id()));
        std::fs::create_dir_all(&user_plugins_dir).expect("create temp user plugins dir failed");

        // v21 起 SessionManager 无库依赖（会话配置真源归插件私有库，内核只按 spec 执行）
        let session_manager = Arc::new(SessionManager::new());
        let config_manager = Arc::new(SessionConfigManager::new(db.clone()));
        let plugin_host = Arc::new(
            PluginHost::new(
                db.clone(),
                &plugins_dir,
                &user_plugins_dir, // 用户插件目录：独立空目录（见上方来源标注说明）
                session_manager.clone(),
                config_manager.clone(),
                None, // 无头/测试上下文无 AppHandle
            )
            .await,
        );
        // 两阶段初始化：注入消息总线 dispatcher（与 lib.rs 生产路径一致）
        plugin_host.init_message_bus().await;
        // v24 认证记录下沉：配对/历史真源 = 认证中心私有库。无头上下文无
        // AppHandle，必须在 activation 前注入私有库根（activate 建表走
        // host-plugin-database；配对/信任链路无私有库即不可用）
        plugin_host
            .wasm_host_ctx()
            .set_plugin_db_root(Some(session_plugin_db_root().clone()));
        // 激活会话中心（随包 FileScan 来源 → 免审批门禁）：/api/auth/* 转发的前置
        plugin_host
            .activate_plugin(SESSION_PLUGIN_ID, false)
            .await
            .expect("activate com.bedcode.terminal-session (bundled artifact)");

        let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(MdnsAdvertiser::new()));
        let (sync_tx, _) = tokio::sync::broadcast::channel::<DesktopSyncEvent>(SYNC_EVENT_BROADCAST_CAPACITY);
        let system_info = Arc::new(SystemInfo::collect());

        AppContextBuilder::new()
            .db(db.clone())
            .session_manager(session_manager.clone())
            .config_manager(config_manager.clone())
            .plugin_host(plugin_host.clone())
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
/// 票 13：配对编排（配对码签发 / 验签 / 记录）已整体下沉会话中心插件，宿主
/// /api/auth/* 无实现（网关 PluginRequired）→ 本助手现在真实驱动插件端点，
/// 夹具语义与迁移前一致（含设备名：广播/注册表按 claims 恢复设备名）。
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

/// 宿主 → 插件互调（JSON-RPC，复制 `auth_center::call_api` 的 wire 约定：
/// topic = `bedcode.api.<api>`、method = 短名、reply topic 由宿主 `call_plugin_api_host`
/// 内部订阅等待）。集成测试 crate 无法访问 lib 的 `pub(crate)` 桥接，故在此
/// 用公开面（`WasmHostContext::call_plugin_api_host`）复刻同一约定。
///
/// P1-b 用途：播种插件私有库配置（config-upsert）——WS StartSession 需要
/// 插件登记域的 config_id，而无头集成测试没有前端命令通道，这是唯一种子路径。
async fn plugin_api_call(api: &str, params: serde_json::Value) -> serde_json::Value {
    let host_ctx = AppContext::global().plugin_host().wasm_host_ctx().clone();
    let topic = format!("bedcode.api.{api}");
    let method = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(api);
    let id = format!("itest-req-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos());
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    let reply_json = host_ctx
        .call_plugin_api_host(&topic, &payload.to_string(), 5000)
        .unwrap_or_else(|e| panic!("plugin api '{api}' failed: {e}"));
    let reply: serde_json::Value = serde_json::from_str(&reply_json).expect("reply json");
    if let Some(err) = reply.get("error") {
        panic!("plugin api '{api}' error: {err}");
    }
    reply.get("result").cloned().expect("result")
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pty_session_chain_flow() {
    // 测试日志输出到 harness（失败时可查链路）；重复 init 静默跳过
    if tracing_subscriber::fmt().with_test_writer().with_max_level(tracing::Level::DEBUG).try_init().is_err() {
        tracing::debug!("tracing subscriber already initialized");
    }

    // AppContext 必须先于任何 WS 连接初始化：actor 的 stopping()/认证 handler
    // 都会调用 AppContext::global()（未初始化即 panic）
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：插件背书的会话创建（P1-b） ====================

    // HTTP 配对（插件端点）拿 JWT，/ws/event 控制通道 JWT 首消息认证
    let token = http_pair_and_get_token(port, "pty-001").await;
    let (mut sink_a, mut stream_a, _addr_a) = connect_ws(port, "/ws/event").await;
    authenticate_with_jwt(&mut sink_a, &mut stream_a, &token, "pty-001").await;

    // 1a. 播种插件私有库配置（互调 config-upsert；无头集成测试无前端命令通道，
    // 这是唯一种子路径——复制宿主桥接 wire 约定，见 plugin_api_call）
    let config = plugin_api_call(
        "com.bedcode.terminal-session.config-upsert",
        serde_json::json!({
            "name": "itest-chain",
            "environment": "linux",
            "workingDir": std::env::temp_dir().to_string_lossy(),
            "command": "bash",
        }),
    )
    .await;
    let config_id = config["id"].as_str().expect("config id").to_string();

    // 1b. WS StartSession 控制动作 → 宿主窄转发层 → 插件 session-create →
    // host-pty.spawn（真实 bash）。回执带 session_id（P1-b 创建同步完成）。
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
                    action: SessionControlAction::StartSession { config_id: act_cfg },
                },
            ..
        } => {
            assert_eq!(act_cfg, config_id, "start action must carry config_id");
            sid
        }
        Message::Error { code, message, .. } => panic!("StartSession rejected ({code}: {message})"),
        other => panic!("expected SessionControl(StartSession) response, got: {other:?}"),
    };
    assert!(!session_id.is_empty(), "created session must carry non-empty session_id");

    // 1c. ListSessions（插件登记域真源）确认会话已注册且状态 running
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
            assert_eq!(
                entry.config_id.as_deref(),
                Some(config_id.as_str()),
                "config_id 透传（插件登记域会话记录标真源配置）"
            );
        }
        other => panic!("expected SessionControl(SessionList) response, got: {other:?}"),
    }

    // ==================== 场景 2：WS 终端输出通道（P1-b 受损面恢复，票 06 红→绿） ====================
    //
    // 移动端 `/ws/terminal/session/{id}` 输出订阅原读宿主 `GlobalOutputManager`，而
    // P1-b 起生产会话（插件创建）的输出在宿主 PTY 引擎 `PtyRing`——票 06 起经票 05
    // 的广播声明（hostBroadcastSessionId）改直读同进程 `PtyRing`（零跨 WASM 边界）。
    // 此处断言恢复形态：auth_ok → subscribe_ok → 写 echo → 收输出帧 → HTTP 历史可取。
    let (mut sink_t, mut stream_t, _addr_t) = connect_ws(port, &format!("/ws/terminal/session/{session_id}")).await;

    // 2a. 首消息 JWT 认证 → 引擎广播声明存在 → auth_ok（不再是 SESSION_NOT_FOUND）
    sink_t
        .send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
        .await
        .expect("send auth frame failed");
    let auth_resp = recv_frame_json(&mut stream_t).await;
    assert_eq!(
        auth_resp["type"], "auth_ok",
        "P1-b 受损面已恢复：插件会话应 auth_ok（P3 形态 B 直读引擎环）, got: {auth_resp}"
    );

    // 2b. subscribe（无 from_offset = 全量回放）→ subscribe_ok 快照三件套 + history_end
    sink_t
        .send(WsMsg::Text(r#"{"type":"subscribe"}"#.into()))
        .await
        .expect("send subscribe failed");
    let sub_ok = recv_frame_json(&mut stream_t).await;
    assert_eq!(
        sub_ok["type"], "subscribe_ok",
        "插件会话订阅应 subscribe_ok（引擎环直读）, got: {sub_ok}"
    );
    assert_eq!(sub_ok["protocol"], 3, "TB v3 协议版本不变（老客户端零改动）");
    let snapshot_offset = sub_ok["snapshot_offset"].as_u64().expect("snapshot_offset");
    let min_offset = sub_ok["min_offset"].as_u64().expect("min_offset");
    assert!(
        snapshot_offset >= min_offset,
        "快照边界须 ≥ 驻留起点（min={min_offset}, snapshot={snapshot_offset})"
    );
    // history_end（空历史也必发，移动端历史拼接锚点）
    let hist_end = recv_frame_json(&mut stream_t).await;
    assert_eq!(hist_end["type"], "history_end", "订阅后应收到 history_end, got: {hist_end}");
    assert_eq!(
        hist_end["snapshot_offset"], snapshot_offset as u64,
        "history_end 边界与 subscribe_ok 快照一致"
    );

    // 2c. 写 echo marker（PTY 输入走控制帧 input，Base64 明文协议）→ 收输出帧
    let marker = "BEDCODE_MOBILE_OUTPUT_MARKER_061";
    let echo_cmd = format!("echo {marker}\n");
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, echo_cmd.as_bytes());
    sink_t
        .send(WsMsg::Text(format!(r#"{{"type":"input","data":"{b64}"}}"#).into()))
        .await
        .expect("send input frame failed");
    let collected = collect_terminal_output_until(&mut stream_t, marker, Duration::from_secs(10)).await;
    assert!(
        collected.contains(marker),
        "插件会话输出必须能通过 WS 终端通道收到（引擎环直读）：collected={collected:?}"
    );

    // 2d. HTTP 一次性历史：GET /api/sessions/{id}/history 改读引擎环快照（M7 恢复）
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let hist_resp: serde_json::Value = client
        .get(format!("{base}/api/sessions/{session_id}/history"))
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("from", "0")])
        .send()
        .await
        .expect("history request failed")
        .json()
        .await
        .expect("history response parse failed");
    assert_eq!(hist_resp["code"], 0, "历史应可取（引擎环快照）, got: {hist_resp}");
    let data = hist_resp["data"].clone();
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        data["dataBase64"].as_str().expect("dataBase64"),
    )
    .expect("dataBase64 decode");
    let decoded_text = String::from_utf8_lossy(&decoded);
    assert!(
        decoded_text.contains(marker),
        "HTTP 历史必须包含 echo 输出（引擎环快照）：decoded={decoded_text:?}"
    );
    // 元数据自洽：min ≤ snapshot，history_bytes = snapshot - min（引擎环驻留语义）
    assert_eq!(
        data["snapshotOffset"].as_u64().unwrap() as i64
            - data["minOffset"].as_u64().unwrap() as i64,
        data["historyBytes"].as_u64().unwrap() as i64,
        "historyBytes = snapshotOffset - minOffset（引擎环连续驻留语义）"
    );

    // 场景 2 的终端通道保持打开：场景 3 停止会话后断言 session_stopped 帧（终态收尾）

    // ==================== 场景 3：停止插件会话 → 状态一致（P1-b 插件背书） ====================

    // 3a. StopSession 经宿主窄转发层 → 插件 session-close（登记 Stopping +
    // host-pty.kill，终态由 pty:exit 事件收尾）→ 响应回显 session_id
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

    // 3b. pty:exit 事件驱动终态（异步收尾）→ 有限轮询重发 ListSessions 直到
    // 该会话报告 stopped（而非消失或仍 running）
    let mut entry_seen = false;
    for _ in 0..20 {
        let resp = send_control_and_wait(&mut sink_a, &mut stream_a, SessionControlAction::ListSessions, None).await;
        match resp {
            Message::SessionControl {
                payload:
                    SessionControlPayload {
                        action: SessionControlAction::SessionList { sessions },
                    },
                ..
            } => {
                if let Some(entry) = sessions.iter().find(|s| s.id == session_id) {
                    if entry.status == "stopped" {
                        entry_seen = true;
                        break;
                    }
                }
            }
            other => panic!("expected SessionControl(SessionList) response, got: {other:?}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        entry_seen,
        "session must report stopped after StopSession (pty:exit 终态异步收尾)"
    );

    // 3c. 终态收尾（票 06）：引擎订阅者在宽限排空后向终端通道发 session_stopped 帧。
    // 此刻场景 2 的终端 WS 仍打开（sink_t/stream_t 未 drop）——断言收到停止帧，
    // 且帧序在尾帧之后（宽限排空保证，见 engine_subscriber_loop）
    let stopped_frame = recv_frame_json(&mut stream_t).await;
    assert_eq!(
        stopped_frame["type"], "session_stopped",
        "插件会话停止后终端通道应收到 session_stopped（终态收尾）, got: {stopped_frame}"
    );
    assert_eq!(
        stopped_frame["session_id"], session_id,
        "session_stopped 应携带会话 id"
    );

    // ==================== 场景 4：未认证客户端创建会话被拒 ====================

    let (mut sink_b, mut stream_b, _addr_b) = connect_ws(port, "/ws/event").await;

    // 与 02 场景 3a 的拒绝行为衔接：业务消息在 actor 状态机层被拦（authenticated=false）
    let control = Message::session_control_with_response(
        SessionControlAction::StartSession {
            // 认证门在 actor 状态机层，先于任何编排（插件未激活也一样拒）
            config_id: TEST_CONFIG_ID.to_string(),
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

    // ==================== 场景 5：重启后输出链路（P1-b 记账） ====================
//
// 插件会话的 restart（remove + 同 id 重建 → host-pty.spawn）全链由 lib 侧 e2e
// （test_session_actions_closed_loop）覆盖；移动端输出通道的重启后恢复随 P3 形态 B
// （宿主直读 PtyRing）一并恢复，本场景不再重复内核直连回归（内核路径是 P1-b 后
// 仅测试存在的载体，其 restart 输出管理器注册是内核内部实现细节）。

// ==================== 收尾：显式关闭连接 + 优雅停机 + 清理 ====================

    // 先发 Close 让服务端 actor 走 stopping()（注销注册表 + 取消订阅），再优雅停机，
    // 避免 stop(true) 等待长连接（与 02 相同策略）
    for (mut sink, mut stream) in [(sink_a, stream_a), (sink_b, stream_b), (sink_t, stream_t)] {
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

    // 清理临时用户插件目录（随包产物目录 resources/plugins/desktop 不得触碰）
    let user_plugins_dir = std::env::temp_dir().join(format!("bedcode-itest-userplugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(user_plugins_dir);
    // 私有库根清理（含认证中心私有库测试数据）
    let _ = std::fs::remove_dir_all(session_plugin_db_root());
}
