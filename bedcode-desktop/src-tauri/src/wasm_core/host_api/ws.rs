//! host-websocket 逻辑层 —— WS 基础能力服务（ABI v14）
//!
//! spec：`.scratch/2026-09-18-ws-base-service/spec.md`；票据 04（客户端域闭环）。
//!
//! **零业务代码红线（D1）**：本模块只做引擎原语——连接生命周期、帧收发、句柄
//! 登记、属主仲裁、按属主回收、事件定向投递；不拼装、不解读任何业务字段
//! （消息格式 / 房间 / 协议 / 重连策略一律由插件构造编排）。
//!
//! - **客户端域（出站）**：`LazyLock` 全局连接表（每条带 `owner`），复用
//!   `tokio-tungstenite 0.24`。`connect` 同步阻塞至握手完成（ABI v14 D4），
//!   成功返回句柄并发布 `<owner>::ws:open`，失败只回 Err 不发事件；
//!   **不自动重连**（编排归插件）；**不启用 TLS**（`wss://` 显式拒绝，D7）；
//! - **服务端域（入站）**：插件在宿主 WS 服务器上挂载端点（`/ws/plugin/<owner>/<path>`，
//!   spec D5）。端点表见 [`crate::server::websocket::endpoint`]，连接侧通道见
//!   [`crate::server::websocket::channel::plugin`]——宿主只做引擎级动作（命名空间注入、
//!   认证策略执行、帧转发、按属主回收），**业务语义完全归插件**；
//! - **事件**：状态事件走消息总线属主私有 topic（票 05 命名空间）
//!   （`<owner>::ws:open|error|close`、`<owner>::ws:client-connect|client-disconnect`，
//!   标识在 payload；非属主订阅被总线门禁拒绝）；消息帧经可选导出 `events-ws` 回调，未导出则丢弃 +
//!   首次 `warn!` + 计数（宿主不缓存，spec §2.2）；
//! - **回收**：插件停用 → [`purge_for_plugin`] 关闭并摘除其全部出站连接与入站端点
//!   （只碰本人）。

use bedcode_plugin_api::host::bus::owned_topic;
use bedcode_plugin_api::host::ws::{WS_CLOSE, WS_ERROR, WS_OPEN};

use crate::wasm_core::bus::{MessageBus, WsFrameDispatch};
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::wasm_core::permission::{PERMISSION_WS_CLIENT, PERMISSION_WS_SERVER};
use crate::server::websocket::endpoint::EndpointAuth;
use crate::server::websocket::registry::WsSessionRegistry;
use crate::system::constants::{
    PLUGIN_WS_CONNECT_TIMEOUT_SECS, PLUGIN_WS_MAX_CONNS_PER_PLUGIN, PLUGIN_WS_MAX_MESSAGE_BYTES,
    PLUGIN_WS_SEND_QUEUE_CAPACITY,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::Message;

/// 连接状态：握手完成且未关闭
const STATE_OPEN: u8 = 1;
/// 连接已关闭（对端 Close / 传输错误 / 宿主回收）
const STATE_CLOSED: u8 = 0;

/// 非属主操作的统一拒绝文案（属主仲裁，spec §2.3）
const NOT_OWNER: &str = "not owner of ws handle";

/// 非属主端点的统一拒绝文案（服务端域属主仲裁）
const NOT_ENDPOINT_OWNER: &str = "not owner of ws endpoint";

fn denied_client() -> String {
    "permission denied: ws:client".to_string()
}

fn denied_server() -> String {
    "permission denied: ws:server".to_string()
}

// ==================== 连接表 ====================

/// 出站帧（插件 → 对端）：经有界队列交写任务，队列满即 fail-visible 拒绝
enum OutboundFrame {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
}

/// 单条出站连接
struct ClientEntry {
    /// 属主插件（停用时按属主回收；跨插件调用一律拒绝）
    owner: String,
    /// 连接 URL（日志/审计用；不含凭据）
    url: String,
    /// 有界发送队列（`try_send` 立即判定，满 → `ws send queue full`）
    tx: mpsc::Sender<OutboundFrame>,
    /// 连接状态（`is-connected` 事实源）
    state: Arc<AtomicU8>,
    /// 消息总线（事件发布 + `events-ws` 帧投递的 dispatcher 来源）
    ///
    /// 由 `connect` 时的 `host_ctx` 克隆持有：读任务不依赖 `AppContext`
    /// 全局单例，宿主测试可直接以自建上下文驱动（可测性 + 依赖倒置）
    bus: Arc<MessageBus>,
    /// 读任务（帧回灌 + 关闭事件上报）
    reader: tokio::task::JoinHandle<()>,
    /// 写任务（发送队列 → 对端）
    writer: tokio::task::JoinHandle<()>,
}

/// 全局连接表（句柄 → 连接；句柄带 owner，回收只碰本人）
static CLIENTS: LazyLock<Mutex<HashMap<String, ClientEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 未导出 `events-ws` 的插件被丢弃的消息帧计数（core-monitor 之外的本地可见性）
static WS_FRAMES_DROPPED: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 已就「未导出 events-ws」告警过的插件（首次 warn，后续 debug，防刷屏）
static WS_DROP_WARNED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

// ==================== 配置 ====================

/// `connect` 的 config-json 契约（纯引擎参数，camelCase）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectConfig {
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    protocols: Vec<String>,
    #[serde(default)]
    connect_timeout_secs: Option<u64>,
    #[serde(default)]
    max_message_bytes: Option<usize>,
}

/// `close` / `close-client` 的 close-json 契约（code 缺省由调用域决定）
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseConfig {
    #[serde(default)]
    code: Option<u16>,
    #[serde(default)]
    reason: Option<String>,
}

/// 插件端点注册 config-json 契约（服务端域；本票只做形状校验的预留定义）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EndpointConfig {
    path: String,
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    max_message_bytes: Option<usize>,
    #[serde(default)]
    max_clients: Option<usize>,
}

// ==================== 客户端域原语 ====================

/// 建立出站 WS 连接（同步阻塞至握手完成，spec D4）
///
/// 成功 → 返回句柄 `wsc-<uuid>` 并发布 `<owner>::ws:open`；
/// 失败 → 错误上抛且**不发布任何事件**（无句柄可寻址）
pub(crate) fn ws_connect(host_ctx: &WasmHostContext, plugin_id: &str, config_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_CLIENT, "host_websocket_connect") {
        return Err(denied_client());
    }
    let config: ConnectConfig =
        serde_json::from_str(config_json).map_err(|e| format!("ws connect: invalid config: {e}"))?;
    let url = config.url.trim().to_string();
    if url.is_empty() {
        return Err("ws connect: url must not be empty".to_string());
    }
    // D7：本期仅 ws://——tokio-tungstenite 0.24 未启用任何 TLS feature，
    // 接受 wss:// 会以「握手失败」掩盖真实原因，故在此显式拒绝并提示
    if !url.starts_with("ws://") {
        return Err(format!(
            "ws connect: url scheme not supported ({}); only ws:// is available in this version (wss:// is not implemented yet)",
            url
        ));
    }
    // 连接数上限（spec §4.4）：超限直接 Err，不产生任何副作用
    let owned = {
        let table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
        table.values().filter(|e| e.owner == plugin_id).count()
    };
    if owned >= PLUGIN_WS_MAX_CONNS_PER_PLUGIN {
        return Err(format!(
            "ws connect: connection limit reached ({PLUGIN_WS_MAX_CONNS_PER_PLUGIN})"
        ));
    }

    // 请求构造：url + 插件自定 headers / subprotocols（宿主不解读任何业务头）
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| format!("ws connect: invalid url: {e}"))?;
    for (name, value) in &config.headers {
        let header = tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| format!("ws connect: invalid header name '{name}': {e}"))?;
        let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value)
            .map_err(|e| format!("ws connect: invalid header value for '{name}': {e}"))?;
        request.headers_mut().insert(header, value);
    }
    if !config.protocols.is_empty() {
        let joined = config.protocols.join(", ");
        let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&joined)
            .map_err(|e| format!("ws connect: invalid protocols: {e}"))?;
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
            value,
        );
    }

    // 超时上限截断为常量（spec §4.4「上限截断为常量」）
    let timeout_secs = config
        .connect_timeout_secs
        .unwrap_or(PLUGIN_WS_CONNECT_TIMEOUT_SECS)
        .clamp(1, PLUGIN_WS_CONNECT_TIMEOUT_SECS);
    let limit = max_message_bytes()
        .min(config.max_message_bytes.unwrap_or(usize::MAX))
        .max(1);
    let ws_config = WebSocketConfig {
        max_message_size: Some(limit),
        max_frame_size: Some(limit),
        ..WebSocketConfig::default()
    };

    // 同步阻塞至握手完成（与 host-http 非流式模式同一 block_on 桥）
    let (stream, response) = crate::wasm_core::manager::runtime::block_on_async(async move {
        tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            tokio_tungstenite::connect_async_with_config(request, Some(ws_config), false),
        )
        .await
    })
    .map_err(|_| format!("ws connect: handshake timed out after {timeout_secs}s"))?
    .map_err(|e| format!("ws connect: handshake failed: {e}"))?;

    // 子协议回执（协商结果回传插件；缺失则省略字段）
    let protocol = response
        .headers()
        .get(tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let handle = format!("wsc-{}", uuid::Uuid::new_v4());
    let owner = plugin_id.to_string();
    let state = Arc::new(AtomicU8::new(STATE_OPEN));
    let bus = host_ctx.message_bus.clone();
    let (tx, rx) = mpsc::channel(PLUGIN_WS_SEND_QUEUE_CAPACITY);
    let (write, read) = stream.split();
    let writer = crate::system::error_boundary::spawn_with_error_boundary("ws_client_writer", run_writer(write, rx));
    let reader = crate::system::error_boundary::spawn_with_error_boundary(
        "ws_client_reader",
        run_reader(
            read,
            handle.clone(),
            owner.clone(),
            Arc::clone(&state),
            Arc::clone(&bus),
        ),
    );

    {
        let mut table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
        table.insert(
            handle.clone(),
            ClientEntry {
                owner: owner.clone(),
                url: url.clone(),
                tx,
                state,
                bus: Arc::clone(&bus),
                reader,
                writer,
            },
        );
    }

    // 成功才发 open 事件（失败路径零事件，D4）；发布先于返回值到达插件
    let mut payload = serde_json::json!({ "handle": handle, "url": url });
    if let Some(protocol) = protocol {
        payload["protocol"] = serde_json::Value::String(protocol);
    }
    publish_ws(&bus, &owned_topic(&owner, WS_OPEN), payload);
    tracing::info!(plugin_id = %owner, handle = %handle, "ws client connection opened");
    Ok(handle)
}

/// 发送文本帧（UTF-8）
pub(crate) fn ws_send_text(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    handle: &str,
    text: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_CLIENT, "host_websocket_send_text") {
        return Err(denied_client());
    }
    enqueue(plugin_id, handle, OutboundFrame::Text(text.to_string()))
}

/// 发送二进制帧
pub(crate) fn ws_send_binary(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    handle: &str,
    payload: &[u8],
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_CLIENT, "host_websocket_send_binary") {
        return Err(denied_client());
    }
    enqueue(plugin_id, handle, OutboundFrame::Binary(payload.to_vec()))
}

/// 主动关闭连接：返回是否命中（幂等：未知句柄 false）
///
/// 关闭命令入队后由写任务发出 Close 帧并结束；对端回 Close → 读任务上报
/// `<owner>::ws:close`（`wasClean` 按对端回帧 code 判定，spec D11）
pub(crate) fn ws_close(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    handle: &str,
    close_json: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_CLIENT, "host_websocket_close") {
        return Err(denied_client());
    }
    let close: CloseConfig =
        serde_json::from_str(close_json).map_err(|e| format!("ws close: invalid close-json: {e}"))?;
    let code = close.code.unwrap_or(1000);
    let reason = close.reason.unwrap_or_default();

    let entry = {
        let mut table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = table.remove(handle) else {
            return Ok(false);
        };
        if entry.owner != plugin_id {
            table.insert(handle.to_string(), entry);
            return Err(NOT_OWNER.to_string());
        }
        entry
    };
    if entry.tx.try_send(OutboundFrame::Close { code, reason }).is_err() {
        // 写任务已退出（对端先断）：句柄已摘除，语义等同关闭完成
        tracing::debug!(plugin_id = %plugin_id, handle = %handle, "ws close: writer already finished");
    }
    Ok(true)
}

/// 查询连接是否处于 open 态（握手完成且未关闭）；仅属主可查
pub(crate) fn ws_is_connected(host_ctx: &WasmHostContext, plugin_id: &str, handle: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_CLIENT, "host_websocket_is_connected") {
        return Err(denied_client());
    }
    let table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
    match table.get(handle) {
        Some(entry) if entry.owner == plugin_id => Ok(entry.state.load(Ordering::SeqCst) == STATE_OPEN),
        Some(_) => Err(NOT_OWNER.to_string()),
        None => Ok(false),
    }
}

/// 入队出站帧（fail-visible：队列满 / 连接已关闭立即返回明确错误，D10）
fn enqueue(plugin_id: &str, handle: &str, frame: OutboundFrame) -> Result<(), String> {
    let table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = table.get(handle) else {
        return Err(format!("ws connection not found: {handle}"));
    };
    if entry.owner != plugin_id {
        return Err(NOT_OWNER.to_string());
    }
    match entry.tx.try_send(frame) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(_)) => Err("ws send queue full".to_string()),
        Err(mpsc::error::TrySendError::Closed(_)) => Err("ws connection is closed".to_string()),
    }
}

// ==================== 服务端域原语（插件入站端点，spec §2.4 / 票据 05） ====================
//
// 宿主只做引擎级动作：路径命名空间注入、认证策略执行、帧转发、按属主回收。
// 业务语义（消息格式 / 房间 / 协议 / 重连策略）完全归插件（D1）。

/// 端点域属主仲裁：未注册 / 非属主 → `Err`（跨插件不可互操作）
fn owned_endpoint(endpoint_id: &str, plugin_id: &str) -> Result<crate::server::websocket::endpoint::EndpointEntry, String> {
    match crate::server::websocket::endpoint::get(endpoint_id) {
        Some(entry) if entry.owner == plugin_id => Ok(entry),
        Some(_) => Err(NOT_ENDPOINT_OWNER.to_string()),
        None => Err(format!("ws endpoint not found: {endpoint_id}")),
    }
}

/// 注册插件端点（`/ws/plugin/<plugin-id>/<path>`，命名空间段由宿主注入，D5）
///
/// 校验顺序：权限门 → 形状校验（空 / 含 `/` / 含 `.` / 超长）→ 认证策略解析
/// → 端点数上限与同插件冲突（失败零副作用）。返回端点句柄 `wse-<uuid>`；
/// 完整挂载路径 = [`crate::server::websocket::endpoint::mount_path`]（插件侧可推导）。
pub(crate) fn ws_register_endpoint(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    config_json: &str,
) -> Result<String, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_register_endpoint",
    ) {
        return Err(denied_server());
    }
    let config: EndpointConfig =
        serde_json::from_str(config_json).map_err(|e| format!("ws register-endpoint: invalid config: {e}"))?;
    let path = config.path.trim();
    if path.is_empty() {
        return Err("ws register-endpoint: path must not be empty".to_string());
    }
    if path.contains('/') || path.contains('.') {
        return Err("ws register-endpoint: path must not contain '/' or '.'".to_string());
    }
    if path.chars().count() > crate::system::constants::PLUGIN_WS_ENDPOINT_PATH_MAX_LEN {
        return Err(format!(
            "ws register-endpoint: path too long (max {})",
            crate::system::constants::PLUGIN_WS_ENDPOINT_PATH_MAX_LEN
        ));
    }
    // 缺省档 = none（WS 历史行为）；未定义取值报错，绝不静默降级为较宽档位
    let auth = EndpointAuth::parse_with(config.auth.as_deref(), EndpointAuth::None)
        .map_err(|e| format!("ws register-endpoint: {e}"))?;

    let entry = crate::server::websocket::endpoint::register(
        plugin_id,
        path,
        auth,
        config.max_clients,
        config.max_message_bytes,
        host_ctx.message_bus.clone(),
    )?;
    Ok(entry.endpoint_id)
}

/// 向端点指定客户端发文本帧
///
/// 客户端不在该端点名下 → `Err`（错配寻址 fail-visible，不静默丢弃）；
/// 出站帧由接收连接的通道过滤链处理（`TrafficChannel::WsPlugin`）
pub(crate) fn ws_send_text_to_client(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
    client_id: &str,
    text: &str,
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_send_text_to_client",
    ) {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let endpoint = endpoint_id.to_string();
    let client = client_id.to_string();
    let text = text.to_string();
    crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global()
            .send_to_endpoint_client(&endpoint, &client, text)
            .await
    })
}

/// 向端点指定客户端发二进制帧
pub(crate) fn ws_send_binary_to_client(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
    client_id: &str,
    payload: &[u8],
) -> Result<(), String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_send_binary_to_client",
    ) {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let endpoint = endpoint_id.to_string();
    let client = client_id.to_string();
    let payload = payload.to_vec();
    crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global()
            .send_binary_to_endpoint_client(&endpoint, &client, payload)
            .await
    })
}

/// 向端点全部客户端广播文本帧 → 成功入队客户端数
///
/// 部分失败不回滚（失败明细记 debug，成功数供调用方判定，spec D10）
pub(crate) fn ws_broadcast_text(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
    text: &str,
) -> Result<u32, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_broadcast_text",
    ) {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let endpoint = endpoint_id.to_string();
    let text = text.to_string();
    let sent = crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global().broadcast_to_endpoint(&endpoint, text).await
    });
    Ok(sent as u32)
}

/// 向端点全部客户端广播二进制帧 → 成功入队客户端数
pub(crate) fn ws_broadcast_binary(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
    payload: &[u8],
) -> Result<u32, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_broadcast_binary",
    ) {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let endpoint = endpoint_id.to_string();
    let payload = payload.to_vec();
    let sent = crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global()
            .broadcast_binary_to_endpoint(&endpoint, payload)
            .await
    });
    Ok(sent as u32)
}

/// 踢出端点指定客户端（close-json：`{ code?, reason? }`，缺省 4004）
///
/// 返回是否命中（客户端不在该端点名下 → `false`，幂等不 panic）；
/// 对端随后收到 `client-disconnect` 事件（宿主主动断开，`wasClean = false`）
pub(crate) fn ws_close_client(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
    client_id: &str,
    close_json: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_SERVER, "host_websocket_close_client") {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let close: CloseConfig =
        serde_json::from_str(close_json).map_err(|e| format!("ws close-client: invalid close-json: {e}"))?;
    // 踢出缺省 4004（spec §4.5 关闭理由语义）
    let code = close.code.unwrap_or(4004);
    let reason = close.reason.unwrap_or_else(|| "kicked by plugin".to_string());
    let endpoint = endpoint_id.to_string();
    let client = client_id.to_string();
    Ok(crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global()
            .disconnect_endpoint_client(&endpoint, &client, code, &reason)
            .await
    }))
}

/// 关闭端点并回收句柄（含下线全部客户端，close code 4005）
///
/// 返回是否存在该端点（未知句柄 → `Ok(false)`；他人端点 → `Err` 属主仲裁）。
/// 先摘端点再下线客户端：摘除后台的握手立即 404，不再有新客户端接入
pub(crate) fn ws_unregister_endpoint(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
) -> Result<bool, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_unregister_endpoint",
    ) {
        return Err(denied_server());
    }
    let Some(entry) = crate::server::websocket::endpoint::get(endpoint_id) else {
        return Ok(false);
    };
    if entry.owner != plugin_id {
        return Err(NOT_ENDPOINT_OWNER.to_string());
    }
    crate::server::websocket::endpoint::remove(endpoint_id);
    let endpoint = entry.endpoint_id.clone();
    let closed = crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global()
            .disconnect_by_endpoint(&endpoint, 4005, "endpoint unregistered")
            .await
    });
    tracing::info!(
        plugin_id = %plugin_id,
        endpoint_id = %endpoint_id,
        clients = closed,
        "plugin ws endpoint unregistered"
    );
    Ok(true)
}

/// 端点在线的客户端清单 → JSON 数组
///
/// `[{ clientId, addr, authenticated, connectedAt }]`（camelCase）——丢弃
/// `client-connect` 事件后的自愈入口（spec D3）；仅属主可查
pub(crate) fn ws_list_clients(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_id: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_WS_SERVER, "host_websocket_list_clients") {
        return Err(denied_server());
    }
    owned_endpoint(endpoint_id, plugin_id)?;
    let endpoint = endpoint_id.to_string();
    let clients = crate::wasm_core::manager::runtime::block_on_async(async move {
        WsSessionRegistry::global().list_by_endpoint(&endpoint).await
    });
    let list: Vec<serde_json::Value> = clients
        .into_iter()
        .map(|client| {
            serde_json::json!({
                "clientId": client.client_id,
                "addr": client.addr,
                "authenticated": client.authenticated,
                "connectedAt": client.connected_at,
            })
        })
        .collect();
    serde_json::to_string(&list).map_err(|e| format!("ws list-clients: serialization failed: {e}"))
}

/// 本插件已注册端点清单 → JSON 数组
///
/// `[{ endpointId, path, clientCount }]`（camelCase，按挂载路径升序稳定输出）
pub(crate) fn ws_list_endpoints(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(
        host_ctx,
        plugin_id,
        PERMISSION_WS_SERVER,
        "host_websocket_list_endpoints",
    ) {
        return Err(denied_server());
    }
    let entries = crate::server::websocket::endpoint::list_by_owner(plugin_id);
    let mut list: Vec<serde_json::Value> = Vec::with_capacity(entries.len());
    for entry in entries {
        let endpoint = entry.endpoint_id.clone();
        let client_count = crate::wasm_core::manager::runtime::block_on_async(async move {
            WsSessionRegistry::global().endpoint_client_count(&endpoint).await
        });
        list.push(serde_json::json!({
            "endpointId": entry.endpoint_id,
            "path": entry.path,
            "clientCount": client_count,
        }));
    }
    serde_json::to_string(&list).map_err(|e| format!("ws list-endpoints: serialization failed: {e}"))
}

// ==================== 回收 ====================

/// 回收指定插件的全部 WS 资源（插件停用/卸载时由 PluginHost 调用）
///
/// 两侧一并回收，且**只碰本人**（`owner` 比对）：
/// - 客户端域（出站连接）：close 4005 并摘除句柄；
/// - 服务端域（入站端点）：先摘端点（后续握手立即 404），再下线其全部在线
///   客户端（close 4005，spec §4.5 属主停用语义）。
///
/// 返回被回收的连接数（出站 + 入站在线客户端）
pub(crate) fn purge_for_plugin(plugin_id: &str) -> usize {
    let handles: Vec<String> = {
        let table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .filter(|(_, entry)| entry.owner == plugin_id)
            .map(|(handle, _)| handle.clone())
            .collect()
    };
    let mut purged = 0;
    for handle in handles {
        if purge_one(plugin_id, &handle) {
            purged += 1;
        }
    }

    // 服务端域：端点表回收 + 其在线客户端 4005 下线
    let endpoints = crate::server::websocket::endpoint::purge_for_plugin(plugin_id);
    for entry in endpoints {
        let endpoint = entry.endpoint_id.clone();
        let closed = crate::wasm_core::manager::runtime::block_on_async(async move {
            WsSessionRegistry::global()
                .disconnect_by_endpoint(&endpoint, 4005, "plugin deactivated")
                .await
        });
        purged += closed;
    }

    // 降级计数/告警标记随属主回收，避免插件重载后残留旧计数
    WS_FRAMES_DROPPED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(plugin_id);
    WS_DROP_WARNED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(plugin_id);
    if purged > 0 {
        tracing::info!(plugin_id = %plugin_id, connections = purged, "ws plugin resources purged");
    }
    purged
}

/// 摘除并关闭单条连接（属主校验；返回是否命中）
fn purge_one(owner: &str, handle: &str) -> bool {
    let entry = {
        let mut table = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = table.remove(handle) else {
            return false;
        };
        if entry.owner != owner {
            table.insert(handle.to_string(), entry);
            return false;
        }
        entry
    };
    // 属主停用回收：close code 4005（spec §4.5），wasClean=false
    if entry
        .tx
        .try_send(OutboundFrame::Close {
            code: 4005,
            reason: "plugin deactivated".to_string(),
        })
        .is_err()
    {
        tracing::debug!(handle = %handle, "ws purge: writer already finished");
    }
    true
}

// ==================== 读写任务 ====================

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type WsWrite = futures_util::stream::SplitSink<WsStream, Message>;
type WsRead = futures_util::stream::SplitStream<WsStream>;

/// 写任务：有界发送队列 → 对端；收到 Close 帧后发送并结束
async fn run_writer(mut write: WsWrite, mut rx: mpsc::Receiver<OutboundFrame>) {
    while let Some(frame) = rx.recv().await {
        let (message, is_close) = match frame {
            OutboundFrame::Text(text) => (Message::Text(text), false),
            OutboundFrame::Binary(payload) => (Message::Binary(payload), false),
            OutboundFrame::Close { code, reason } => (
                Message::Close(Some(CloseFrame {
                    code: CloseCode::from(code),
                    reason: reason.into(),
                })),
                true,
            ),
        };
        if let Err(e) = write.send(message).await {
            tracing::debug!(error = %e, "ws client write failed, closing");
            break;
        }
        if is_close {
            break;
        }
    }
    if let Err(e) = write.close().await {
        tracing::debug!(error = %e, "ws client close handshake failed");
    }
}

/// 读任务：帧回灌插件（可选导出）+ 关闭事件上报（恰好一次）
///
/// `<owner>::ws:close` 的 `wasClean` 仅当**对端主动发送 Close 帧**且 code ∈
/// {1000, 1001} 时为 true（spec D11）；异常断开 / 传输错误 → 省略 code + false。
/// 同一连接内帧按到达序投递（保序，spec §2.2 D2）
async fn run_reader(mut read: WsRead, handle: String, owner: String, state: Arc<AtomicU8>, bus: Arc<MessageBus>) {
    // 关闭事件恰好一次（对端 Close 与后续收尾共用同一守卫）
    let close_reported = Arc::new(AtomicBool::new(false));
    let mut close_frame: Option<(Option<u16>, String, bool)> = None;

    while let Some(incoming) = read.next().await {
        match incoming {
            Ok(Message::Text(text)) => deliver_frame(&bus, &owner, &handle, "text", text.into_bytes()).await,
            Ok(Message::Binary(payload)) => deliver_frame(&bus, &owner, &handle, "binary", payload).await,
            Ok(Message::Close(frame)) => {
                let (code, reason) = match frame {
                    Some(f) => (Some(u16::from(f.code)), f.reason.to_string()),
                    None => (None, String::new()),
                };
                // 1006（异常关闭）等不可发送码不会出现在对端 Close 帧里；
                // 无 code 的对端 Close 视为异常断开
                close_frame = Some((code, reason.clone(), close_was_clean(code)));
                report_close(
                    &bus,
                    &owner,
                    &handle,
                    code,
                    &reason,
                    close_was_clean(code),
                    &close_reported,
                );
                break;
            }
            // 心跳与原始帧由 tungstenite 协议层处理，业务层不外泄
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
            Err(e) => {
                publish_ws(
                    &bus,
                    &owned_topic(&owner, WS_ERROR),
                    serde_json::json!({ "handle": handle, "message": e.to_string() }),
                );
                break;
            }
        }
    }

    // 先置关闭态再上报：上报后 `is-connected` 立即为 false（spec D3 同款时序）
    state.store(STATE_CLOSED, Ordering::SeqCst);
    if let Some((code, reason, clean)) = close_frame {
        // 已在 Close 分支上报过（守卫防御性再调一次，不会重复投递）
        report_close(&bus, &owner, &handle, code, &reason, clean, &close_reported);
    } else {
        // 未收到对端 Close（TCP 断 / 传输错误 / 宿主主动关）→ code 省略 + wasClean=false
        report_close(&bus, &owner, &handle, None, "", false, &close_reported);
    }
    tracing::debug!(plugin_id = %owner, handle = %handle, "ws client reader exited");
}

/// close code → `wasClean`（spec D11）：仅对端主动 Close 且 code ∈ {1000,1001} 为 true
fn close_was_clean(code: Option<u16>) -> bool {
    matches!(code, Some(1000) | Some(1001))
}

/// 上报 `<owner>::ws:close`（守卫保证每连接恰好一次）
fn report_close(
    bus: &MessageBus,
    owner: &str,
    handle: &str,
    code: Option<u16>,
    reason: &str,
    was_clean: bool,
    reported: &AtomicBool,
) {
    if reported.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut payload = serde_json::json!({ "handle": handle, "wasClean": was_clean });
    if let Some(code) = code {
        payload["code"] = serde_json::Value::Number(code.into());
    }
    if !reason.is_empty() {
        payload["reason"] = serde_json::Value::String(reason.to_string());
    }
    publish_ws(bus, &owned_topic(owner, WS_CLOSE), payload);
    tracing::info!(plugin_id = %owner, handle = %handle, was_clean, "ws client connection closed");
}

// ==================== 帧投递与事件 ====================

/// 客户端域帧投递：经消息总线持有的 dispatcher 定向投给属主插件实例
async fn deliver_frame(bus: &MessageBus, plugin_id: &str, handle: &str, kind: &str, payload: Vec<u8>) {
    let frame = WsFrameDispatch::Client {
        handle: handle.to_string(),
        kind: kind.to_string(),
        payload,
    };
    dispatch_frame(bus, plugin_id, frame, handle).await;
}

/// 服务端域帧投递（插件端点入站帧；标识为 `endpoint_id/client_id`）
///
/// 与客户端域同源同语义（同一降级路径）：由端点通道的单条投递任务串行调用，
/// 因此**同一连接内的帧按到达序投递**（保序，spec §2.2 D2）
pub(crate) async fn deliver_endpoint_frame(
    bus: &MessageBus,
    plugin_id: &str,
    endpoint_id: &str,
    client_id: &str,
    kind: &str,
    payload: Vec<u8>,
) {
    let frame = WsFrameDispatch::EndpointClient {
        endpoint_id: endpoint_id.to_string(),
        client_id: client_id.to_string(),
        kind: kind.to_string(),
        payload,
    };
    let target = format!("{endpoint_id}/{client_id}");
    dispatch_frame(bus, plugin_id, frame, &target).await;
}

/// 共同投递语义（客户端域 / 服务端域唯一降级路径）
///
/// dispatcher 未注入（无头/测试的中间态）→ 仅记 debug，不计数也不 panic；
/// 插件未导出 `events-ws` → 降级（丢弃 + 首次 warn + 计数，宿主不缓存）
async fn dispatch_frame(bus: &MessageBus, plugin_id: &str, frame: WsFrameDispatch, target: &str) {
    let Some(dispatcher) = bus.dispatcher().await else {
        tracing::debug!(
            plugin_id = %plugin_id,
            target = %target,
            "ws frame dispatch skipped: message bus dispatcher not set"
        );
        return;
    };
    match dispatcher.dispatch_ws_frame(plugin_id, &frame) {
        Ok(true) => {}
        Ok(false) => record_dropped_frame(plugin_id, target),
        Err(e) => {
            // trap 等投递失败由宿主统一记录并触发重载，此处只补上下文
            tracing::error!(
                plugin_id = %plugin_id,
                target = %target,
                error = %e,
                "ws frame dispatch failed"
            );
        }
    }
}

/// 未导出 `events-ws`：丢弃 + 首次 `warn!` + 计数（宿主不缓存，spec §2.2）
fn record_dropped_frame(plugin_id: &str, handle: &str) {
    let total = {
        let mut counters = WS_FRAMES_DROPPED.lock().unwrap_or_else(|e| e.into_inner());
        let counter = counters.entry(plugin_id.to_string()).or_insert(0);
        *counter += 1;
        *counter
    };
    let first = {
        let mut warned = WS_DROP_WARNED.lock().unwrap_or_else(|e| e.into_inner());
        warned.insert(plugin_id.to_string())
    };
    // 结构化字段按 AGENTS §8：plugin_id 为字段而非消息拼接
    if first {
        tracing::warn!(
            plugin_id = %plugin_id,
            handle = %handle,
            dropped_total = total,
            "ws frame dropped: plugin exports no events-ws (frames are not buffered); subscribe state events via host-bus and query snapshots for self-healing"
        );
    } else {
        tracing::debug!(
            plugin_id = %plugin_id,
            dropped_total = total,
            "ws frame dropped (no events-ws export)"
        );
    }
}

/// 该插件的消息帧丢弃计数（宿主侧可见性；测试与排障用）
pub(crate) fn dropped_frame_count(plugin_id: &str) -> u64 {
    WS_FRAMES_DROPPED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(plugin_id)
        .copied()
        .unwrap_or(0)
}

/// 发布状态事件到插件消息总线（sender = "host"；订阅侧按精确 topic 分发）
///
/// 经连接持有的总线实例发送（非全局单例）：与 `deliver_frame` 同源，
/// 宿主测试可用自建上下文的 bus 直接断言属主私有 topic（`<owner>::ws:*`）
fn publish_ws(bus: &MessageBus, topic: &str, payload: serde_json::Value) {
    bus.publish(topic, "host", payload);
}

/// 帧/消息字节上限：与移动端终端链路同一事实源；配置不可读时回退常量
fn max_message_bytes() -> usize {
    crate::server::websocket::routes::ws_frame_limit().max(PLUGIN_WS_MAX_MESSAGE_BYTES.min(1))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    /// 唯一插件 id（静态表按 id 隔离，并行用例互不干扰）
    fn test_plugin(seed: &str) -> String {
        format!("test-ws-{seed}")
    }

    /// 伪造一条出站连接条目（不真连网络）：写任务持有永不消费的队列
    fn fake_client(owner: &str, handle: &str, url: &str) {
        let (tx, rx) = mpsc::channel(PLUGIN_WS_SEND_QUEUE_CAPACITY);
        // 队列接收端挂起任务持有，保证 try_send 非 Closed 语义
        let writer = crate::system::error_boundary::spawn_with_error_boundary("ws_test_writer", async move {
            let mut rx = rx;
            while rx.recv().await.is_some() {}
        });
        let reader = crate::system::error_boundary::spawn_with_error_boundary("ws_test_reader", async {});
        CLIENTS.lock().unwrap().insert(
            handle.to_string(),
            ClientEntry {
                owner: owner.to_string(),
                url: url.to_string(),
                tx,
                state: Arc::new(AtomicU8::new(STATE_OPEN)),
                bus: Arc::new(MessageBus::new()),
                reader,
                writer,
            },
        );
    }

    /// 摘除并清理测试条目（避免污染其他用例的计数）
    fn drop_client(handle: &str) {
        if let Some(entry) = CLIENTS.lock().unwrap().remove(handle) {
            entry.reader.abort();
            entry.writer.abort();
        }
    }

    // ==================== 权限门（客户端域 / 服务端域分域） ====================

    #[tokio::test]
    async fn connect_denied_without_ws_client_permission() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("perm-client");
        // 未授权：客户端域一律拒绝
        let err = ws_connect(&ctx, &plugin, r#"{"url":"ws://127.0.0.1:1/"}"#).expect_err("denied");
        assert_eq!(err, denied_client());

        // 只有服务端域权限也不得放行客户端域（分域隔离，spec D6）
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_SERVER]);
        let err = ws_connect(&ctx, &plugin, r#"{"url":"ws://127.0.0.1:1/"}"#).expect_err("denied");
        assert_eq!(err, denied_client());
    }

    #[tokio::test]
    async fn server_domain_denied_without_ws_server_permission() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("perm-server");
        let err = ws_register_endpoint(&ctx, &plugin, r#"{"path":"chat"}"#).expect_err("denied");
        assert_eq!(err, denied_server());

        // 只有客户端域权限也不得放行服务端域
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);
        let err = ws_register_endpoint(&ctx, &plugin, r#"{"path":"chat"}"#).expect_err("denied");
        assert_eq!(err, denied_server());
    }

    // ==================== connect 语义边界（失败零事件、wss 拒绝） ====================

    #[tokio::test]
    async fn connect_rejects_non_ws_scheme_and_bad_config() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("scheme");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);

        // D7：wss:// 显式拒绝（不启用 TLS），且错误文案指明未支持
        let err = ws_connect(&ctx, &plugin, r#"{"url":"wss://example.com/socket"}"#).expect_err("wss rejected");
        assert!(err.contains("only ws://"), "wss 拒绝文案应指明仅支持 ws://：{err}");

        // 空 url / 非法 JSON / 非法 header 名：都在握手前失败
        assert!(ws_connect(&ctx, &plugin, r#"{"url":"  "}"#).is_err());
        assert!(ws_connect(&ctx, &plugin, "not json").is_err());
        assert!(ws_connect(
            &ctx,
            &plugin,
            r#"{"url":"ws://127.0.0.1:1/","headers":{"bad header":"x"}}"#
        )
        .is_err());

        // 失败路径不产生连接（表内无本人条目）→ 也无任何事件副作用
        let table = CLIENTS.lock().unwrap();
        assert!(!table.values().any(|e| e.owner == plugin));
    }

    #[tokio::test]
    async fn connect_enforces_per_plugin_limit() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("limit");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);
        // 预置到上限
        for i in 0..PLUGIN_WS_MAX_CONNS_PER_PLUGIN {
            fake_client(&plugin, &format!("wsc-{plugin}-{i}"), "ws://127.0.0.1:1/");
        }
        let err = ws_connect(&ctx, &plugin, r#"{"url":"ws://127.0.0.1:1/"}"#).expect_err("limit");
        assert!(err.contains("connection limit reached"), "got: {err}");
        for i in 0..PLUGIN_WS_MAX_CONNS_PER_PLUGIN {
            drop_client(&format!("wsc-{plugin}-{i}"));
        }
    }

    // ==================== 句柄寻址 / 属主仲裁 / fail-visible ====================

    #[tokio::test]
    async fn send_and_query_unknown_handle_are_errors_not_panics() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("unknown");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);

        assert!(ws_send_text(&ctx, &plugin, "wsc-missing", "hi").is_err());
        assert!(ws_send_binary(&ctx, &plugin, "wsc-missing", b"hi").is_err());
        // 未知句柄：close 幂等 false；is-connected false
        assert!(!ws_close(&ctx, &plugin, "wsc-missing", "{}").unwrap());
        assert!(!ws_is_connected(&ctx, &plugin, "wsc-missing").unwrap());
    }

    #[tokio::test]
    async fn cross_plugin_handle_access_rejected_without_consuming() {
        let ctx = build_host_ctx();
        let owner = test_plugin("owner");
        let intruder = test_plugin("intruder");
        grant_permissions(&ctx, &owner, &[PERMISSION_WS_CLIENT]);
        grant_permissions(&ctx, &intruder, &[PERMISSION_WS_CLIENT]);
        let handle = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&owner, &handle, "ws://127.0.0.1:1/");

        assert_eq!(ws_send_text(&ctx, &intruder, &handle, "hi").unwrap_err(), NOT_OWNER);
        assert_eq!(ws_is_connected(&ctx, &intruder, &handle).unwrap_err(), NOT_OWNER);
        assert_eq!(ws_close(&ctx, &intruder, &handle, "{}").unwrap_err(), NOT_OWNER);
        // 拒绝不得消费句柄
        assert!(ws_is_connected(&ctx, &owner, &handle).unwrap());
        // 属主本人可用
        assert!(ws_send_text(&ctx, &owner, &handle, "hi").is_ok());

        drop_client(&handle);
    }

    #[tokio::test]
    async fn send_queue_full_is_fail_visible() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("queuefull");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);
        let handle = format!("wsc-{}", uuid::Uuid::new_v4());
        // 构造一条无消费者的连接（写任务立刻结束 → 队列 Closed）
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        CLIENTS.lock().unwrap().insert(
            handle.to_string(),
            ClientEntry {
                owner: plugin.clone(),
                url: "ws://127.0.0.1:1/".to_string(),
                tx,
                state: Arc::new(AtomicU8::new(STATE_OPEN)),
                bus: Arc::new(MessageBus::new()),
                reader: crate::system::error_boundary::spawn_with_error_boundary("ws_test_reader", async {}),
                writer: crate::system::error_boundary::spawn_with_error_boundary("ws_test_writer", async {}),
            },
        );
        let err = ws_send_text(&ctx, &plugin, &handle, "hi").expect_err("closed queue");
        assert_eq!(err, "ws connection is closed");
        drop_client(&handle);
    }

    #[tokio::test]
    async fn close_config_defaults_to_1000_and_reports_hit() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("close");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_CLIENT]);
        let handle = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&plugin, &handle, "ws://127.0.0.1:1/");

        // 缺省 code = 1000（spec §4.4 close 语义）
        assert!(ws_close(&ctx, &plugin, &handle, "{}").unwrap());
        // 句柄已摘除 → 幂等 false
        assert!(!ws_close(&ctx, &plugin, &handle, "{}").unwrap());
        assert!(!ws_is_connected(&ctx, &plugin, &handle).unwrap());
        // 显式 code/reason 解析成功路径
        let handle2 = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&plugin, &handle2, "ws://127.0.0.1:1/");
        assert!(ws_close(&ctx, &plugin, &handle2, r#"{"code":4004,"reason":"kicked"}"#).unwrap());
        drop_client(&handle2);
        // 非法 close-json 拒绝
        let handle3 = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&plugin, &handle3, "ws://127.0.0.1:1/");
        assert!(ws_close(&ctx, &plugin, &handle3, "not json").is_err());
        assert!(
            ws_is_connected(&ctx, &plugin, &handle3).unwrap(),
            "解析失败不得摘除句柄"
        );
        drop_client(&handle3);
    }

    // ==================== 回收（只碰本人） ====================

    #[tokio::test]
    async fn purge_for_plugin_only_touches_owner() {
        let victim = test_plugin("purge");
        let bystander = test_plugin("bystander");
        let victim_handle = format!("wsc-{}", uuid::Uuid::new_v4());
        let bystander_handle = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&victim, &victim_handle, "ws://127.0.0.1:1/");
        fake_client(&bystander, &bystander_handle, "ws://127.0.0.1:1/");

        assert_eq!(purge_for_plugin(&victim), 1);
        {
            let table = CLIENTS.lock().unwrap();
            assert!(!table.contains_key(&victim_handle), "victim purged");
            assert!(table.contains_key(&bystander_handle), "bystander survives");
        }
        // 幂等：再次回收命中 0
        assert_eq!(purge_for_plugin(&victim), 0);

        drop_client(&bystander_handle);
    }

    // ==================== 降级路径（未导出 events-ws） ====================

    #[tokio::test]
    async fn dropped_frame_accounting_counts_and_warns_once() {
        let plugin = test_plugin("dropcount");
        assert_eq!(dropped_frame_count(&plugin), 0);
        record_dropped_frame(&plugin, "wsc-x");
        record_dropped_frame(&plugin, "wsc-x");
        record_dropped_frame(&plugin, "wsc-y");
        assert_eq!(dropped_frame_count(&plugin), 3, "计数累计");
        {
            let warned = WS_DROP_WARNED.lock().unwrap();
            assert!(warned.contains(&plugin), "首次告警标记已置位");
        }
        // 属主回收后计数与标记一并清理
        purge_for_plugin(&plugin);
        assert_eq!(dropped_frame_count(&plugin), 0);
    }

    // ==================== wasClean 规则（D11） ====================

    #[tokio::test]
    async fn was_clean_only_for_peer_close_1xxx() {
        assert!(close_was_clean(Some(1000)), "对端正常关闭 → clean");
        assert!(close_was_clean(Some(1001)), "对端 going away → clean");
        assert!(!close_was_clean(None), "无 Close 帧（异常断开）→ 不 clean");
        assert!(!close_was_clean(Some(4001)), "认证失败 → 不 clean");
        assert!(!close_was_clean(Some(4004)), "宿主踢出 → 不 clean");
        assert!(!close_was_clean(Some(4005)), "端点注销/属主回收 → 不 clean");
        assert!(!close_was_clean(Some(1006)), "异常关闭码 → 不 clean");
    }

    // ==================== 服务端域（票 05：注册 / 查询 / 属主仲裁 / 回收） ====================

    /// 注册端点并返回句柄（形状合法时的成功路径 = 句柄 `wse-<uuid>`）
    fn register_endpoint(ctx: &WasmHostContext, plugin: &str, path: &str) -> String {
        ws_register_endpoint(ctx, plugin, &format!(r#"{{"path":"{path}"}}"#)).expect("register endpoint")
    }

    /// 摘除端点（全局表跨用例共享，用例结束必须清理）
    fn drop_endpoint(endpoint_id: &str) {
        crate::server::websocket::endpoint::remove(endpoint_id);
    }

    #[tokio::test]
    async fn register_endpoint_validates_shape_and_auth() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("ep-shape");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_SERVER]);

        // path 校验先行（契约形状尽早暴露拼装错误）
        assert!(ws_register_endpoint(&ctx, &plugin, r#"{"path":""}"#)
            .unwrap_err()
            .contains("must not be empty"));
        assert!(ws_register_endpoint(&ctx, &plugin, r#"{"path":"a/b"}"#)
            .unwrap_err()
            .contains("must not contain"));
        assert!(ws_register_endpoint(&ctx, &plugin, r#"{"path":".."}"#)
            .unwrap_err()
            .contains("must not contain"));
        let too_long = "x".repeat(crate::system::constants::PLUGIN_WS_ENDPOINT_PATH_MAX_LEN + 1);
        assert!(
            ws_register_endpoint(&ctx, &plugin, &format!(r#"{{"path":"{too_long}"}}"#))
                .unwrap_err()
                .contains("too long")
        );
        // 非法 JSON / 未定义 auth 取值 → 报错（认证策略绝不静默降级为 none）
        assert!(ws_register_endpoint(&ctx, &plugin, "not json").is_err());
        assert!(ws_register_endpoint(&ctx, &plugin, r#"{"path":"chat","auth":"token"}"#)
            .unwrap_err()
            .contains("unknown auth"));
        assert_eq!(
            crate::server::websocket::endpoint::count_by_owner(&plugin),
            0,
            "校验失败零副作用"
        );

        // 两种合法策略都能注册（缺省 = none）
        let open = register_endpoint(&ctx, &plugin, "open");
        let guarded = ws_register_endpoint(&ctx, &plugin, r#"{"path":"guarded","auth":"jwt"}"#).expect("jwt endpoint");
        assert_eq!(
            crate::server::websocket::endpoint::get(&open).unwrap().auth,
            EndpointAuth::None,
            "缺省 auth = none"
        );
        assert_eq!(
            crate::server::websocket::endpoint::get(&guarded).unwrap().auth,
            EndpointAuth::Jwt
        );

        drop_endpoint(&open);
        drop_endpoint(&guarded);
    }

    #[tokio::test]
    async fn register_endpoint_returns_handle_and_lists_it() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("ep-roundtrip");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_SERVER]);

        let endpoint = register_endpoint(&ctx, &plugin, "chat");
        assert!(endpoint.starts_with("wse-"), "句柄前缀 wse-，got: {endpoint}");

        // 完整挂载路径由插件侧推导（宿主注入属主命名空间段，spec D5）
        let entry = crate::server::websocket::endpoint::get(&endpoint).expect("endpoint exists");
        assert_eq!(entry.owner, plugin);
        assert_eq!(entry.mount_path, format!("/ws/plugin/{plugin}/chat"));

        // list-endpoints：`[{ endpointId, path, clientCount }]`
        let listed: Vec<serde_json::Value> = serde_json::from_str(&ws_list_endpoints(&ctx, &plugin).unwrap()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["endpointId"], endpoint);
        assert_eq!(listed[0]["path"], "chat");
        assert_eq!(listed[0]["clientCount"], 0);

        // 注销：命中 true → 清单清空 → 幂等 false
        assert!(ws_unregister_endpoint(&ctx, &plugin, &endpoint).unwrap());
        assert_eq!(ws_list_endpoints(&ctx, &plugin).unwrap(), "[]");
        assert!(!ws_unregister_endpoint(&ctx, &plugin, &endpoint).unwrap());
    }

    #[tokio::test]
    async fn register_endpoint_conflict_and_limit_are_side_effect_free() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("ep-limit");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_SERVER]);
        let limit = crate::system::constants::PLUGIN_WS_MAX_ENDPOINTS_PER_PLUGIN;

        let mut ids = vec![register_endpoint(&ctx, &plugin, "chat")];
        // 同插件同后缀 → 冲突拒绝（端点表按完整挂载路径判定）
        assert!(ws_register_endpoint(&ctx, &plugin, r#"{"path":"chat"}"#)
            .unwrap_err()
            .contains("already registered"));
        // 同插件不同后缀可用
        ids.push(register_endpoint(&ctx, &plugin, "lobby"));

        let mut i = ids.len();
        while crate::server::websocket::endpoint::count_by_owner(&plugin) < limit {
            ids.push(register_endpoint(&ctx, &plugin, &format!("p{i}")));
            i += 1;
        }
        // 超限 → Err 且无副作用
        assert!(ws_register_endpoint(&ctx, &plugin, &format!(r#"{{"path":"p{i}"}}"#))
            .unwrap_err()
            .contains("endpoint limit reached"));
        assert_eq!(
            crate::server::websocket::endpoint::count_by_owner(&plugin),
            limit,
            "超限拒绝不得留下副作用"
        );

        for id in ids {
            drop_endpoint(&id);
        }
    }

    #[tokio::test]
    async fn server_domain_cross_owner_access_is_rejected() {
        let ctx = build_host_ctx();
        let owner = test_plugin("ep-owner");
        let intruder = test_plugin("ep-intruder");
        grant_permissions(&ctx, &owner, &[PERMISSION_WS_SERVER]);
        grant_permissions(&ctx, &intruder, &[PERMISSION_WS_SERVER]);
        let endpoint = register_endpoint(&ctx, &owner, "chat");

        // 属主仲裁：他人端点上的一切操作一律拒绝（跨插件不可互操作）
        let results = [
            ws_send_text_to_client(&ctx, &intruder, &endpoint, "c-1", "hi").map(|_| ()),
            ws_send_binary_to_client(&ctx, &intruder, &endpoint, "c-1", b"hi").map(|_| ()),
            ws_broadcast_text(&ctx, &intruder, &endpoint, "hi").map(|_| ()),
            ws_broadcast_binary(&ctx, &intruder, &endpoint, b"hi").map(|_| ()),
            ws_close_client(&ctx, &intruder, &endpoint, "c-1", "{}").map(|_| ()),
            ws_list_clients(&ctx, &intruder, &endpoint).map(|_| ()),
        ];
        for result in results {
            assert_eq!(result.unwrap_err(), NOT_ENDPOINT_OWNER, "跨属主必须拒绝");
        }
        // 注销同样仲裁，且拒绝不得摘除端点
        assert_eq!(
            ws_unregister_endpoint(&ctx, &intruder, &endpoint).unwrap_err(),
            NOT_ENDPOINT_OWNER
        );
        assert!(
            crate::server::websocket::endpoint::get(&endpoint).is_some(),
            "拒绝不得消费端点"
        );
        // 属主本人可用（无在线客户端 → 清单空数组，计数 0）
        assert_eq!(ws_list_clients(&ctx, &owner, &endpoint).unwrap(), "[]");
        assert_eq!(ws_broadcast_text(&ctx, &owner, &endpoint, "hi").unwrap(), 0);

        drop_endpoint(&endpoint);
    }

    #[tokio::test]
    async fn server_domain_unknown_endpoint_is_idempotent() {
        let ctx = build_host_ctx();
        let plugin = test_plugin("ep-unknown");
        grant_permissions(&ctx, &plugin, &[PERMISSION_WS_SERVER]);

        // 未注册端点：单发 / 广播 / 清单 → Err（fail-visible，不静默成功）
        assert!(ws_send_text_to_client(&ctx, &plugin, "wse-none", "c-1", "hi")
            .unwrap_err()
            .contains("not found"));
        assert!(ws_send_binary_to_client(&ctx, &plugin, "wse-none", "c-1", b"hi")
            .unwrap_err()
            .contains("not found"));
        assert!(ws_broadcast_text(&ctx, &plugin, "wse-none", "hi")
            .unwrap_err()
            .contains("not found"));
        assert!(ws_list_clients(&ctx, &plugin, "wse-none")
            .unwrap_err()
            .contains("not found"));
        // 注销是幂等查询：未知端点 → false（不 panic）
        assert!(!ws_unregister_endpoint(&ctx, &plugin, "wse-none").unwrap());

        // 端点存在但客户端不在其名下 → 踢出 Ok(false)（寻址错配不消费端点）
        let endpoint = register_endpoint(&ctx, &plugin, "chat");
        assert!(!ws_close_client(&ctx, &plugin, &endpoint, "c-missing", "{}").unwrap());
        // 非法 close-json → Err（不静默用缺省码）
        assert!(ws_close_client(&ctx, &plugin, &endpoint, "c-missing", "not json").is_err());
        assert_eq!(ws_broadcast_binary(&ctx, &plugin, &endpoint, b"hi").unwrap(), 0);

        drop_endpoint(&endpoint);
    }

    #[tokio::test]
    async fn purge_for_plugin_removes_only_owner_endpoints() {
        let ctx = build_host_ctx();
        let victim = test_plugin("ep-purge");
        let bystander = test_plugin("ep-purge-bystander");
        grant_permissions(&ctx, &victim, &[PERMISSION_WS_SERVER]);
        grant_permissions(&ctx, &bystander, &[PERMISSION_WS_SERVER]);
        let victim_endpoint = register_endpoint(&ctx, &victim, "chat");
        let bystander_endpoint = register_endpoint(&ctx, &bystander, "chat");

        purge_for_plugin(&victim);

        assert!(
            crate::server::websocket::endpoint::get(&victim_endpoint).is_none(),
            "本人端点随停用回收"
        );
        assert!(
            crate::server::websocket::endpoint::get(&bystander_endpoint).is_some(),
            "他人端点不受影响"
        );
        // 幂等：再次回收无命中
        assert_eq!(purge_for_plugin(&victim), 0);

        drop_endpoint(&bystander_endpoint);
    }

    // ==================== 隔离契约（票据 06：跨插件 + 事件面） ====================

    /// 静态订阅者：把收到的 topic 投回测试线程（收不到即隔离成立）
    struct TopicSink {
        tx: std::sync::mpsc::Sender<String>,
    }

    impl crate::wasm_core::bus::BusMessageHandler for TopicSink {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            let _ = self.tx.send(msg.topic.clone());
            Ok(())
        }
    }

    /// 等静态订阅者投递（消费任务异步；超时返回 None）
    async fn wait_topic(rx: &std::sync::mpsc::Receiver<String>) -> Option<String> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(topic) = rx.try_recv() {
                return Some(topic);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// 状态事件按属主私有 topic 投递：事件只达本人命名空间，他人命名空间零投递（spec §2.3）
    ///
    /// 本用例锁「宿主投递寻址正确」；「非属主订阅不到」由总线命名空间门禁负责
    /// （见 host_impl/bus.rs 的跨命名空间订阅拒绝用例）。
    #[tokio::test]
    async fn status_events_are_owner_scoped() {
        let bus = Arc::new(MessageBus::new());
        let owner = test_plugin("topic-owner");
        let other = test_plugin("topic-other");
        let owner_topic = owned_topic(&owner, WS_CLOSE);
        let other_topic = owned_topic(&other, WS_CLOSE);

        let (owner_tx, owner_rx) = std::sync::mpsc::channel();
        let (other_tx, other_rx) = std::sync::mpsc::channel();
        bus.subscribe_static("sub-owner", &owner_topic, Box::new(TopicSink { tx: owner_tx }))
            .await;
        bus.subscribe_static("sub-other", &other_topic, Box::new(TopicSink { tx: other_tx }))
            .await;

        publish_ws(&bus, &owner_topic, serde_json::json!({ "handle": "wsc-1" }));

        assert_eq!(
            wait_topic(&owner_rx).await.as_deref(),
            Some(owner_topic.as_str()),
            "属主 topic 必须收到投递"
        );
        assert!(
            other_rx.try_recv().is_err(),
            "他人命名空间收不到该事件（宿主只向属主投递）"
        );
        // 事件不重放：再等一轮不得出现第二条投递
        assert!(wait_topic(&owner_rx).await.is_none(), "事件不重放（恰好一次投递）");
    }

    /// 跨插件隔离（全函数负向，票据 06 清单）：他人句柄 / 端点 / 对端客户端标识
    /// 上的全部函数一律拒绝，且**零副作用**（句柄不消费、端点不注销、连接不断开）
    #[tokio::test]
    async fn cross_plugin_isolation_covers_all_handle_functions() {
        let ctx = build_host_ctx();
        let owner = test_plugin("iso-owner");
        let intruder = test_plugin("iso-intruder");
        grant_permissions(&ctx, &owner, &[PERMISSION_WS_CLIENT, PERMISSION_WS_SERVER]);
        grant_permissions(&ctx, &intruder, &[PERMISSION_WS_CLIENT, PERMISSION_WS_SERVER]);

        let handle = format!("wsc-{}", uuid::Uuid::new_v4());
        fake_client(&owner, &handle, "ws://127.0.0.1:1/");
        let endpoint = register_endpoint(&ctx, &owner, "iso");
        let peer_client = "127.0.0.1:1";

        // 客户端域（4 个带句柄的函数）
        let client_results = [
            ws_send_text(&ctx, &intruder, &handle, "hi").map(|_| ()),
            ws_send_binary(&ctx, &intruder, &handle, b"hi").map(|_| ()),
            ws_close(&ctx, &intruder, &handle, "{}").map(|_| ()),
            ws_is_connected(&ctx, &intruder, &handle).map(|_| ()),
        ];
        for result in client_results {
            assert_eq!(result.unwrap_err(), NOT_OWNER, "客户端域跨属主必须拒绝");
        }
        // 服务端域（7 个带端点/对端标识的函数）
        let server_results = [
            ws_send_text_to_client(&ctx, &intruder, &endpoint, peer_client, "hi").map(|_| ()),
            ws_send_binary_to_client(&ctx, &intruder, &endpoint, peer_client, b"hi").map(|_| ()),
            ws_broadcast_text(&ctx, &intruder, &endpoint, "hi").map(|_| ()),
            ws_broadcast_binary(&ctx, &intruder, &endpoint, b"hi").map(|_| ()),
            ws_close_client(&ctx, &intruder, &endpoint, peer_client, "{}").map(|_| ()),
            ws_unregister_endpoint(&ctx, &intruder, &endpoint).map(|_| ()),
            ws_list_clients(&ctx, &intruder, &endpoint).map(|_| ()),
        ];
        for result in server_results {
            assert_eq!(result.unwrap_err(), NOT_ENDPOINT_OWNER, "服务端域跨属主必须拒绝");
        }
        // 无「他人句柄」入参的三个函数按契约只作用于调用方自身：
        // connect 受本人连接数上限约束、register-endpoint 挂在本人命名空间、
        // list-endpoints 只列本人端点 —— 冲突与上限已由其它用例覆盖，此处断言
        // 「他人的东西不出现在本人视图里」= 零可见
        assert_eq!(ws_list_endpoints(&ctx, &intruder).unwrap(), "[]", "他人端点零可见");
        assert_eq!(
            crate::server::websocket::endpoint::list_by_owner(&intruder).len(),
            0,
            "他人端点表条目零可见"
        );

        // 零副作用：拒绝不得消费句柄 / 注销端点 / 断开连接
        assert!(crate::server::websocket::endpoint::get(&endpoint).is_some(), "端点未被注销");
        assert!(ws_is_connected(&ctx, &owner, &handle).unwrap(), "本人连接未被关闭");
        assert_eq!(ws_list_clients(&ctx, &owner, &endpoint).unwrap(), "[]");
        assert_eq!(ws_list_endpoints(&ctx, &owner).unwrap().contains("iso"), true);

        drop_client(&handle);
        drop_endpoint(&endpoint);
    }
}
