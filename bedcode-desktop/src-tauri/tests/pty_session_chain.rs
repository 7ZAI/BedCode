//! PTY 会话链路集成测试（ticket 03；websocket 业务下沉票 08 改造为插件端点协议）
//!
//! 原理：进程内真实启动 Actix HTTP+WS 服务器（OS 分配端口）→ 真实
//! tokio-tungstenite 客户端模拟移动端 → 经**插件 WS 端点**驱动真实 PTY 闭环：
//! `/ws/plugin/com.bedcode.terminal-session/session-control`（会话控制：
//! start/list/stop，宿主只转原始帧，协议归插件 `ws_control`）→
//! `host-pty.spawn` 真实 bash → `/ws/plugin/.../terminal`（终端流：JWT 认证 →
//! subscribe → 输入 → ring-fetch 二进制输出帧 → `session_stopped` 停止帧）
//! → HTTP 历史快照（插件 `session-history` 互调 api，宿主不再直读会话输出环）。
//!
//! 环境依赖（PTY 断言失败时先区分测试环境问题与链路缺陷）：Linux/macOS 用
//! 原生 bash；Windows 需 powershell（见迁移前的构建约定）。输出编码强制 UTF-8，
//! 断言用 ASCII marker。
//!
//! 串行化：本文件只含一个 `#[tokio::test(flavor = "multi_thread")]`（场景子步骤
//! 严格串行）。tests/ 下每个文件是独立测试二进制 → 与 01/02 文件进程隔离，
//! AppContext（OnceLock）/ WsSessionRegistry 等全局单例天然互不冲突。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::dev::ServerHandle;
use bedcode_lib::db::Database;
use bedcode_lib::mdns::advertiser::MdnsAdvertiser;
use bedcode_lib::server::core::app::start_http_server;
use bedcode_lib::system::app_context::AppContext;
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

/// 会话中心插件 id
const SESSION_PLUGIN_ID: &str = "com.bedcode.terminal-session";
/// 会话控制端点挂载路径（host 注入命名空间段，manifest 声明后缀 `session-control`）
const SESSION_CONTROL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/session-control";
/// 终端流端点挂载路径（manifest 声明后缀 `terminal`）
const TERMINAL_PATH: &str = "/ws/plugin/com.bedcode.terminal-session/terminal";

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
        let dir = std::env::temp_dir().join(format!("bedcode-ptychain-pluginroot-{}", std::process::id()));
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
/// 与生产组装一致：插件宿主 + 会话中心插件激活（随包产物）；
/// websocket 业务下沉票 08 起无 `sync_tx`/SyncEventHandler 装配。
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
        // 激活会话中心：/api/auth/* 与 ws 端点编排在插件；激活期宿主自动从
        // manifest `contributes.wsEndpoints` 登记端点
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
        let _ = INIT.set(());
    }
}

/// 建立 WS 连接：先建 TCP（记录本地地址 = 服务端看到的 peer addr，
/// 即 registry 的 client_id），再升级为 WebSocket；`path` 指定插件端点路由
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

/// 监听响应帧（跳过二进制输出帧与 Ping/Pong），5s 超时后 panic；返回解析后的 JSON
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

/// 插件端点首消息 JWT 认证（`{"type":"auth","token":...}`，auth:jwt 的唯一凭证）
async fn authenticate_endpoint(sink: &mut WsSend, stream: &mut WsRecv, token: &str) {
    sink.send(WsMsg::Text(format!(r#"{{"type":"auth","token":"{token}"}}"#).into()))
        .await
        .expect("send auth frame failed");
    // 端点协议不定义 auth_ok 回帧：首条业务帧可达即认证生效（无显式等待，
    // 调用方随后直接发业务帧即可；宿主侧认证为同步完成）
}

/// 向 session-control 端点发送动作帧并等待响应（跳过其余帧），带超时
async fn send_control(
    sink: &mut WsSend,
    stream: &mut WsRecv,
    action: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut frame = serde_json::json!({ "type": action });
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            frame[k] = v.clone();
        }
    }
    sink.send(WsMsg::Text(frame.to_string().into()))
        .await
        .expect("send control frame failed");
    recv_frame_json(stream).await
}

/// 宿主 → 插件互调（JSON-RPC 约定，与 `auth_center::call_api` 同 wire）
async fn plugin_api_call(api: &str, params: serde_json::Value) -> serde_json::Value {
    let host_ctx = AppContext::global().plugin_host().wasm_host_ctx().clone();
    let topic = format!("bedcode.api.{api}");
    let method = api.rsplit_once('.').map(|(_, m)| m).unwrap_or(api);
    let id = format!(
        "itest-req-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
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

/// HTTP 配对换取 JWT session_token（POST /api/auth/pairing → verify）
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

/// 轮询收集终端端点二进制输出（累积解码后的文本）直到出现 marker 或超时。
/// 输出为插件原始字节帧（不 JSON 化），文本帧（控制）+ Ping/Pong 跳过。
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
                text.push_str(&String::from_utf8_lossy(&bytes));
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
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    // AppContext 必须先于任何 WS 连接初始化：actor 的 stopping()/认证 handler
    // 都会调用 AppContext::global()（未初始化即 panic）
    init_test_app_context().await;

    let port = pick_free_port();
    let (handle, server_task) = spawn_test_server(port).await.expect("test server must start");

    // ==================== 场景 1：插件端点背书的全链路：配对 → 会话创建 ====================

    // HTTP 配对（插件端点）拿 JWT，连 session-control 端点首消息 JWT 认证
    let token = http_pair_and_get_token(port, "pty-001").await;
    let (mut control_sink, mut control_stream, _addr_a) = connect_ws(port, SESSION_CONTROL_PATH).await;
    authenticate_endpoint(&mut control_sink, &mut control_stream, &token).await;

    // 1a. 播种插件私有库配置（互调 config-upsert；无头集成测试无前端命令通道，
    // 这是唯一种子路径）
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

    // 1b. start_session 动作帧 → 插件 ws_control → 插件 session-create →
    // host-pty.spawn（真实 bash）。回包带 session_id（创建同步完成）。
    let resp = send_control(
        &mut control_sink,
        &mut control_stream,
        "start_session",
        serde_json::json!({ "config_id": config_id.clone() }),
    )
    .await;
    assert_eq!(resp["type"], "start_session", "start 回包类型, got: {resp}");
    assert_eq!(resp["config_id"], config_id, "start 动作回显 config_id, got: {resp}");
    let session_id = resp["session_id"]
        .as_str()
        .expect("created session must carry session_id")
        .to_string();
    assert_eq!(session_id.len(), 36, "插件自产 UUID");

    // 1c. list_sessions（插件登记域真源）确认会话已注册且状态 running
    let resp = send_control(
        &mut control_sink,
        &mut control_stream,
        "list_sessions",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(resp["type"], "session_list", "list 回包类型, got: {resp}");
    let sessions = resp["sessions"].as_array().expect("sessions 数组");
    let entry = sessions
        .iter()
        .find(|s| s["id"] == session_id)
        .expect("created session must appear in session list");
    assert_eq!(entry["status"], "running", "newly created session must be running");
    assert_eq!(
        entry["config_id"].as_str(),
        Some(config_id.as_str()),
        "config_id 透传（插件登记域会话记录标真源配置）"
    );

    // ==================== 场景 2：终端流端点（插件自持游标/合帧，宿主只转帧） ====================

    // 移动端直连 `/ws/plugin/.../terminal`：JWT 认证 → subscribe → 写输入 →
    // 收二进制输出帧（插件经 host-pty.ring-fetch 按每连接游标拉取）
    let (mut term_sink, mut term_stream, _addr_t) = connect_ws(port, TERMINAL_PATH).await;
    authenticate_endpoint(&mut term_sink, &mut term_stream, &token).await;

    // 2a. subscribe → subscribed 回包（插件协议：sessionId + mode 缺省 live）
    term_sink
        .send(WsMsg::Text(
            format!(r#"{{"type":"subscribe","sessionId":"{session_id}"}}"#).into(),
        ))
        .await
        .expect("send subscribe failed");
    let sub_ok = recv_frame_json(&mut term_stream).await;
    assert_eq!(
        sub_ok["type"], "subscribed",
        "插件会话订阅应回 subscribed（宿主不解释终端帧）, got: {sub_ok}"
    );
    assert_eq!(sub_ok["sessionId"], session_id, "subscribed 携带会话 id");

    // 2b. 写 echo marker（输入帧 UTF-8 明文，无控制字符；控制字节走二进制帧）→ 收输出
    let marker = "BEDCODE_MOBILE_OUTPUT_MARKER_061";
    let echo_cmd = format!("echo {marker}\n");
    term_sink
        .send(WsMsg::Text(format!(r#"{{"type":"input","data":{echo_cmd:?}}}"#).into()))
        .await
        .expect("send input frame failed");
    let collected = collect_terminal_output_until(&mut term_stream, marker, Duration::from_secs(10)).await;
    assert!(
        collected.contains(marker),
        "插件会话输出必须能通过终端端点收到（ring-fetch 闭环）：collected={collected:?}"
    );

    // 2c. HTTP 一次性历史：GET /api/sessions/{id}/history → 插件 session-history
    // 互调 api（宿主不再持有会话输出环直读映射，spec §2.4 / §4.3）
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
    assert_eq!(
        hist_resp["code"], 0,
        "历史应可取（插件 session-history）, got: {hist_resp}"
    );
    let data = hist_resp["data"].clone();
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        data["dataBase64"].as_str().expect("dataBase64"),
    )
    .expect("dataBase64 decode");
    let decoded_text = String::from_utf8_lossy(&decoded);
    assert!(
        decoded_text.contains(marker),
        "HTTP 历史必须包含 echo 输出（插件环拉取）：decoded={decoded_text:?}"
    );
    // 元数据自洽：min ≤ snapshot，history_bytes = snapshot - min
    assert_eq!(
        data["snapshotOffset"].as_u64().unwrap() as i64 - data["minOffset"].as_u64().unwrap() as i64,
        data["historyBytes"].as_u64().unwrap() as i64,
        "historyBytes = snapshotOffset - minOffset（插件环驻留语义）"
    );

    // 终端通道保持打开：场景 3 停止会话后断言 session_stopped 帧（终态收尾）

    // ==================== 场景 3：停止会话 → 终态（pty:exit → session_stopped） ====================

    // 3a. stop_session 动作帧 → 插件 session-close（登记 Stopping + host-pty.kill，
    // 终态由 pty:exit 事件收尾）→ 回包回显 session_id
    let resp = send_control(
        &mut control_sink,
        &mut control_stream,
        "stop_session",
        serde_json::json!({ "session_id": session_id }),
    )
    .await;
    assert_eq!(resp["type"], "stop_session", "stop 回包类型, got: {resp}");
    assert_eq!(resp["session_id"], session_id, "stop 回包回显会话 id, got: {resp}");

    // 3b. pty:exit 事件驱动终态（异步收尾）→ 轮询 list 直到该会话报告 stopped
    let mut entry_seen = false;
    for _ in 0..20 {
        let resp = send_control(
            &mut control_sink,
            &mut control_stream,
            "list_sessions",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(resp["type"], "session_list", "list 回包类型, got: {resp}");
        let sessions = resp["sessions"].as_array().expect("sessions 数组");
        if let Some(entry) = sessions.iter().find(|s| s["id"] == session_id) {
            if entry["status"] == "stopped" {
                entry_seen = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        entry_seen,
        "session must report stopped after stop_session (pty:exit 终态异步收尾)"
    );

    // 3c. 终态收尾：插件在 pty:exit 后向该会话全部订阅连接下发 session_stopped
    // 停止帧（尾帧在先）。断言收到停止帧且携带会话 id。
    let stopped_frame = recv_frame_json(&mut term_stream).await;
    assert_eq!(
        stopped_frame["type"], "session_stopped",
        "插件会话停止后终端端点应收到 session_stopped（终态收尾）, got: {stopped_frame}"
    );
    assert_eq!(stopped_frame["sessionId"], session_id, "session_stopped 应携带会话 id");

    // ==================== 场景 4：未注册端点路径 → 404（旧路由不存在，无 fallback） ====================
    let unregistered = format!("ws://127.0.0.1:{port}/ws/plugin/{SESSION_PLUGIN_ID}/no-such-endpoint");
    let err = tokio_tungstenite::connect_async(&unregistered).await;
    assert!(err.is_err(), "未注册端点必须拒绝（404 无升级），不得静默放行: {err:?}");

    // ==================== 收尾：显式关闭连接 + 优雅停机 + 清理 ====================
    for (mut sink, mut stream) in [(control_sink, control_stream), (term_sink, term_stream)] {
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
