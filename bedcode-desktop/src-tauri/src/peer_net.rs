//! 对等网络接入（宿主侧引擎接入中枢）：节点身份初始化 + 节点/发现守护装配，
//! 以及 host-peer 原语（ADR 0022 v2）在宿主侧的引擎转发唇口。
//!
//! **定位（票 06 后）**：产品业务（传输任务 / 策略 / 历史 / 共享根 / 远端浏览编排、
//! 设置与加密）真源全部在 file-transfer 插件；本模块只保留「离宿主无法实现、且
//! 无业务语义」的引擎原语转发与节点生命周期管理，以及三条引擎连接态事件
//! （`peer-connected` / `peer-disconnected` / `peer-consent-requested`）的前端桥。
//!
//! - **节点身份**：NodeIdentity 与设备身份（auth 域 DeviceIdentity）刻意分离——
//!   首启纯随机生成、重装即新身份，不做设备标识派生（决策 D2）；由 crate 持久
//!   化，宿主只注入数据目录（决策 D3），目录与 DB 同源解析自 app_data_dir。
//! - **生命周期**：节点 + mDNS 发现守护随 file-transfer 插件启用状态运行（幂等
//!   装配）；命令面仅剩五条 Tauri 命令——`start_peer_node` / `stop_peer_node` /
//!   `respond_peer_consent` / `list_trusted_peers` / `revoke_trusted_peer`。
//! - **引擎接入**：`peer_engine_transfer` / `peer_engine_receive` /
//!   `peer_engine_remote` 是对 host-peer 原语（dial / send / cancel / pause /
//!   resume / respond / set-policy / set-download-dir / set-shared-roots / browse /
//!   pull）的引擎适配模块；插件经 host-peer `*_plugin` 转发入口逐原语调用，
//!   传输任务快照经 `publish_bus_only` 只推 `peer:transfer` / `peer:receive`
//!   总线 topic，插件按 batchId 归并持久化业务视图。

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncReadExt;

use bedcode_peer_net::{
    Connection, ConnectionHandler, DiscoveredPeerRecord, DiscoveryCache, DiscoveryConfig, DiscoveryDaemon,
    HandlerFuture, NodeId, NodeIdentity, PeerNetError, PeerNetNode, PeerNetNodeConfig, RunningNode, SharedDirEntry,
    SharedDirHandler, SharedDirRoot, SharedDirStore, StaticPeerRecord, TransferConfig, TransferEvent, TrustEvent,
    TrustStore, TrustedPeerEntry, CAP_FILE_TRANSFER,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

// ==================== 身份初始化（issue 01）====================

/// 加载或创建节点身份，并把节点 ID（全长 + 短指纹）写入运行日志
///
/// 错误快速上抛走既有启动错误路径：静默换身份会让对端可信列表里记录的本机
/// ID 全部失效（决策 D3），禁止吞错重试。
pub fn init_node_identity(data_dir: &Path) -> bedcode_peer_net::Result<NodeIdentity> {
    let identity = NodeIdentity::load_or_create(data_dir)?;
    tracing::info!(
        "peer-net node identity ready: node_id={} (short fingerprint {})",
        identity.node_id(),
        identity.node_id().short_fingerprint()
    );
    Ok(identity)
}

// ==================== 运行时装配（ticket 03）====================

/// 对等网络默认监听端口
///
/// 固定值便于防火墙放行与排查；被占则回退 `:0` 由系统分配（mDNS 广播实际端口，
/// 不依赖此常量的确定性）。两端宿主须保持一致。
const DEFAULT_PEER_PORT: u16 = 47613;

/// 发现缓存变更推送周期：远小于 TTL，保证上下线在 1-2 个周期内可见；
/// 仅快照比对无变化时不发事件，LAN 规模下成本可忽略

/// 无变化强制重发周期（tick 数）：首帧推送可能早于插件订阅完成而丢失
/// （delivered=0 静默丢弃），指纹锁定后若记录稳定则永不再发——插件前端
/// 只能靠 query-peer 兑底。周期性全量重发保证订阅晚到也能最终收到

/// 运行中节点的完整状态（命令面操作对象；`None` = 未启动）
struct PeerNetRuntime {
    /// 节点句柄（`dial_peer_endpoint` 拨号入口；Clone 廉价——内部全是 Arc 共享）
    node: PeerNetNode,
    /// TCP 监听运行句柄（优雅关停入口）
    running: RunningNode,
    /// mDNS 发现守护句柄（优雅关停入口；广播+浏览同守护，见 start_locked）
    daemon: DiscoveryDaemon,
    /// 宿主身份广播登记句柄（MdnsService ADVERTISERS owner=host；停机时注销）
    host_adv: String,
    /// 在线缓存句柄（list 命令读取；闸门桥接解析对端设备名共用同一实例）
    cache: Arc<DiscoveryCache>,
    /// 本节点 ID（状态摘要展示用）
    node_id: String,
}

/// 首连确认回执登记项：等待前端应答的弹窗请求
struct PendingConsent {
    node_id: NodeId,
    reply: tokio::sync::oneshot::Sender<bool>,
}

/// Tauri 托管的节点状态容器
///
/// - `runtime`：tokio Mutex 串行化 start/stop 并发调用，装配含多步 IO 与 spawn，
///   必须互斥防止「双启动」竞态产生两个广播实例；
/// - `trust`：可信列表句柄独立于运行时存活（节点停止后设置面仍可查看/撤销），
///   数据与磁盘文件同源，重复加载无一致性风险；
/// - `shared`：共享目录注册表句柄，与 trust 同款「独立于运行时存活」语义
///   （issue 07）；
/// - `consents`：std Mutex 即可——临界区只有 map 增删，无跨 await 持有；
/// - `connections`：本机主动拨号建立的存活连接（issue 08），句柄存活即连接
///   存活、drop 即断开；std Mutex 与 consents 同款瞬时临界区。键为节点 ID hex。
pub struct PeerNetState {
    runtime: tokio::sync::Mutex<Option<PeerNetRuntime>>,
    trust: tokio::sync::Mutex<Option<Arc<TrustStore>>>,
    shared: tokio::sync::Mutex<Option<Arc<SharedDirStore>>>,
    consents: std::sync::Mutex<HashMap<String, PendingConsent>>,
    /// 出站会话表：连接由活性泵自持，宿主只持关闭信号（见 OutboundSession）
    connections: std::sync::Mutex<HashMap<String, OutboundSession>>,
    /// 入站（被连侧）存活连接计数（node_id → 连接数）：入站句柄由 crate 的
    /// SharedDirHandler 自持，宿主仅记账供连接态重发还原首屏；
    /// 维护在 [`InboundConnectionBridge`]。按连接计数避免会话连接与数据面
    /// 短连接并存时，短连接结束过早摘除连接态。
    inbound_peers: std::sync::Mutex<std::collections::HashMap<String, usize>>,
    /// 入站连接关停句柄（node_id → conn id → 内层 handler AbortHandle）：
    /// disconnect_peer / stop_locked 据此中止 handler，连接随任务 drop 关闭
    inbound_conns: std::sync::Mutex<HashMap<String, HashMap<u64, tokio::task::AbortHandle>>>,
    /// 生命周期闸门（start/stop 装配串行化）：activate 外壳与 boot 对账可能
    /// 并发触发 start/stop，先前「幂等检查在锁内、装配在锁外」构成 TOCTOU——
    /// 双装配时次者 bind 回退 :0 随机端口、首个 runtime 泄漏。不持 runtime
    /// 锁跨 await（与全仓短持锁风格一致），闸门只串行化装配本身
    lifecycle_gate: tokio::sync::Semaphore,
    /// 节点属主（审计票 12）：起节点的插件 id，停即清空。
    /// 见 [`node_owner`] / [`start_node_owned`] / [`release_node_for`]
    node_owner: std::sync::Mutex<Option<String>>,
}

impl Default for PeerNetState {
    fn default() -> Self {
        Self {
            runtime: tokio::sync::Mutex::new(None),
            trust: tokio::sync::Mutex::new(None),
            shared: tokio::sync::Mutex::new(None),
            consents: std::sync::Mutex::new(HashMap::new()),
            connections: std::sync::Mutex::new(HashMap::new()),
            inbound_peers: std::sync::Mutex::new(HashMap::new()),
            inbound_conns: std::sync::Mutex::new(HashMap::new()),
            lifecycle_gate: tokio::sync::Semaphore::new(1),
            node_owner: std::sync::Mutex::new(None),
        }
    }
}

/// 出站会话句柄：连接由活性泵任务（session_watch）自持，宿主只持关闭信号。
/// id 供泵清算防误删——同节点重复拨号会替换表项，旧泵不得动新表项
struct OutboundSession {
    close: tokio::sync::watch::Sender<bool>,
    id: u64,
}

static NEXT_SESSION_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static NEXT_INBOUND_CONN_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// start/stop 命令与自动启动共用的状态摘要
#[derive(Debug, Serialize)]
pub struct PeerNodeStatus {
    pub started: bool,
    pub node_id: String,
    pub listen_addr: String,
}

/// 可信对端条目（设置面管理列表 DTO）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPeerDto {
    /// 完整节点 ID（64 位小写 hex）
    pub node_id: String,
    /// 展示名：持久化名优先，其次在线缓存广播名；均缺为 null（前端以短指纹兜底）
    pub display_name: Option<String>,
    /// 短指纹（前 8 位）
    pub fingerprint_short: String,
    /// 加入可信列表时刻（RFC3339）
    pub added_at: String,
}

/// 拨号结果（issue 08）：denied/unreachable 属正常业务终态而非错误——
/// 前端按状态渲染文案，避免把对端拒绝误报成异常
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialPeerResultDto {
    /// 连接结果：`connected` | `denied` | `unreachable`
    pub status: String,
    /// 对端设备名（缓存解析；离线缺失为 null）
    pub device_name: Option<String>,
}

/// 启动对等网络节点（幂等：已启动直接返回现状）
#[tauri::command]
pub async fn start_peer_node(app: AppHandle) -> crate::Result<PeerNodeStatus> {
    let data_dir = app_data_dir(&app)?;
    let device_name = resolve_device_name(&app);
    let state = app.state::<PeerNetState>();
    start_locked(&state, &data_dir, device_name, &app).await
}

/// 优雅关停对等网络节点（幂等：未启动直接成功）
#[tauri::command]
pub async fn stop_peer_node(app: AppHandle) -> crate::Result<()> {
    let state = app.state::<PeerNetState>();
    stop_locked(&state, &app).await
}

// ==================== endpoint 拨号（ADR 0022 v2）====================

/// endpoint 拨号入参：插件从自身设备缓存（mdns:found 事件派生）解析后显式传入
///
/// 宿主不再内藏 node-id → 地址解析表（ADR 0022 v2 裁决）：寻址来源由调用方持有，
/// 握手期「证书指纹 ↔ nodeId 绑定 + 信任检查」语义与旧发现缓存路径 dial_peer 完全一致。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialEndpoint {
    /// 对端节点 ID（64 位小写 hex 公钥指纹）
    pub node_id: String,
    /// 对端监听地址（IP 或主机名，不带端口）
    pub addr: String,
    /// 对端监听端口
    pub port: u16,
}

/// 按 endpoint 拨号连接（host-peer `dial-peer-endpoint` 原语的引擎入口）
///
/// 与已退役的 dial_peer 发现缓存路径唯一差异是寻址来源：不再要求目标在发现缓存中。缓存命中时
/// 复用其展示名；未命中则回退短指纹占位，并观察一条回退记录进缓存——过渡期
/// 桥接：数据面函数（send/browse/pull）内部仍按 node-id 寻址且依赖缓存解析
/// 元数据，Phase 4 数据面全面句柄化后此观察分支随旧路径一并退役。
pub async fn dial_peer_endpoint(app: AppHandle, endpoint: DialEndpoint) -> crate::Result<DialPeerResultDto> {
    let parsed = parse_node_id(&endpoint.node_id)?;
    let addr: SocketAddr = format!("{}:{}", endpoint.addr.trim(), endpoint.port)
        .parse()
        .map_err(|_| {
            crate::AppError::InvalidInput(format!(
                "peer-net dial failed: invalid endpoint addr '{}:{}'",
                endpoint.addr, endpoint.port
            ))
        })?;
    tracing::info!(
        node_id = %parsed,
        short = %parsed.short_fingerprint(),
        "peer dial requested via endpoint"
    );
    let state = app.state::<PeerNetState>();

    let (node, cached_record) = {
        let guard = state.runtime.lock().await;
        let runtime = guard
            .as_ref()
            .ok_or_else(|| crate::AppError::Internal("peer-net dial failed: node not started".to_string()))?;
        (runtime.node.clone(), runtime.cache.get(&parsed))
    };
    let cache_miss = cached_record.is_none();

    let device_name = cached_record
        .map(|r| r.device_name)
        .unwrap_or_else(|| format!("node-{}", parsed.short_fingerprint()));
    let static_record = StaticPeerRecord {
        node_id: parsed.clone(),
        addr,
    };
    match node.dial(&static_record).await {
        Ok(connection) => {
            // 会话连接移交常驻活性泵（同 dial_peer：对端断开泵感知，本机断开
            // 经泵信号落地；同节点重复拨号以最新会话为准）
            let (session_close, session_close_rx) = tokio::sync::watch::channel(false);
            let session_id = NEXT_SESSION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(old) = state
                .connections
                .lock()
                .expect("connections table lock poisoned")
                .insert(
                    parsed.to_string(),
                    OutboundSession {
                        close: session_close,
                        id: session_id,
                    },
                )
            {
                let _ = old.close.send(true);
            }
            spawn_session_watch(
                app.clone(),
                parsed.to_string(),
                session_id,
                session_close_rx,
                connection,
            );
            // 过渡期桥接：缓存未命中时补一条回退记录，让按 node-id 寻址的
            // 数据面函数可用（见函数文档，Phase 4 退役）
            if cache_miss {
                if let Some((_, cache)) = runtime_snapshot(&app).await {
                    cache.observe(DiscoveredPeerRecord {
                        node_id: parsed.clone(),
                        addr,
                        device_name: device_name.clone(),
                        protocol_version: 0,
                        capabilities: 0,
                        last_seen: Instant::now(),
                    });
                }
            }
            tracing::info!(
                node_id = %parsed,
                short = %parsed.short_fingerprint(),
                "peer dialed via endpoint and connected"
            );
            emit_json(
                &app,
                "peer-connected",
                dial_connected_payload(parsed.as_str(), &device_name),
            );
            Ok(DialPeerResultDto {
                status: "connected".to_string(),
                device_name: Some(device_name),
            })
        }
        Err(PeerNetError::DialDeniedByPeer { .. }) => {
            tracing::info!(node_id = %parsed, "peer dial denied by remote");
            Ok(DialPeerResultDto {
                status: "denied".to_string(),
                device_name: Some(device_name),
            })
        }
        Err(e) => {
            tracing::warn!(node_id = %parsed, "peer dial unreachable: {e}");
            Ok(DialPeerResultDto {
                status: "unreachable".to_string(),
                device_name: Some(device_name),
            })
        }
    }
}

/// 断开对等连接：出站会话通知活性泵关闭（drop 连接 → 对端经 EOF 感知）、
/// 入站连接中止内层 handler（连接随任务 drop 关闭，对端拨号侧泵感知）。
/// 返回是否存在出站会话。本机确有断开发 `peer-disconnected` 供前端摘除已连接徽标。
pub async fn disconnect_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    let parsed = parse_node_id(&node_id)?;
    let state = app.state::<PeerNetState>();
    let removed = {
        let mut sessions = state.connections.lock().expect("connections table lock poisoned");
        sessions.remove(&parsed.to_string())
    };
    if let Some(session) = &removed {
        let _ = session.close.send(true);
    }
    // 被连侧此前无法响应断开（只有拨号侧持句柄），对端会一直显示已连接——
    // 入站关停句柄让两端断开语义对称
    let inbound_closed = {
        let mut conns = state.inbound_conns.lock().expect("inbound conns lock poisoned");
        conns.remove(&parsed.to_string())
    };
    if let Some(handles) = &inbound_closed {
        tracing::info!(count = handles.len(), "aborting inbound connections on user disconnect");
        for (_, handle) in handles {
            handle.abort();
        }
    }
    if removed.is_some() || inbound_closed.is_some() {
        tracing::info!(node_id = %parsed, "peer connection dropped by user");
        emit_json(
            &app,
            "peer-disconnected",
            serde_json::json!({ "nodeId": parsed.as_str(), "connected": false }),
        );
    }
    Ok(removed.is_some())
}

/// 出站会话活性泵挂载（spawn_with_error_boundary 包装，宿主后台任务规范）
fn spawn_session_watch(
    app: AppHandle,
    node_id: String,
    session_id: u64,
    close_rx: tokio::sync::watch::Receiver<bool>,
    conn: Connection,
) {
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_session_watch",
        session_watch(app, node_id, session_id, close_rx, conn),
    );
}

/// 出站会话活性泵：持有会话连接直到对端断开或本机关闭
///
/// 会话连接不承载业务数据（数据面操作各自新拨），读到数据一律丢弃。
/// 对端断开（EOF/错误）→ 清算连接表（仍属本会话才动）并发 `peer-disconnected`，
/// 前端即时摘除「已连接」徽标；本机关闭 → 静默退出（disconnect_peer 已发事件）。
/// TCP keepalive（crate 拨号/接听时设置）保证对端静默死亡（WiFi 骤断等无 FIN
/// 场景）也能在探测窗口内以错误浮现。
async fn session_watch(
    app: AppHandle,
    node_id: String,
    session_id: u64,
    mut close: tokio::sync::watch::Receiver<bool>,
    mut conn: Connection,
) {
    let mut buf = [0u8; 256];
    let remote_closed = loop {
        tokio::select! {
            biased;
            _ = close.changed() => break false,
            n = conn.read(&mut buf) => match n {
                Ok(0) => break true,
                Ok(_) => {}
                Err(_) => break true,
            },
        }
    };
    drop(conn);
    let state = app.state::<PeerNetState>();
    let owned = {
        let mut sessions = state.connections.lock().expect("connections table lock poisoned");
        let owned = matches!(sessions.get(&node_id), Some(s) if s.id == session_id);
        if owned {
            sessions.remove(&node_id);
        }
        owned
    };
    // 仅当无入站连接时才发断开：同节点可能同时存在出站会话 + 入站数据面
    // 短连接，出站会话结束不代表节点整体断开（对称于 InboundConnectionBridge）
    if remote_closed && owned {
        let has_inbound = state
            .inbound_peers
            .lock()
            .expect("inbound peers lock poisoned")
            .contains_key(&node_id);
        if !has_inbound {
            emit_json(
                &app,
                "peer-disconnected",
                serde_json::json!({ "nodeId": node_id, "connected": false }),
            );
        }
    }
}

/// 本机节点 ID（host-mdns 自播回显过滤用；节点未启动/运行时锁忙返回 None）
///
/// 独立浏览订阅（host-mdns）会收到本机自己的广播，需按 TXT `id` 与本机
/// NodeId 比对剔除（引擎 handle_browse_event 已有同口径过滤）。try_lock 非阻塞：
/// 浏览循环跑在独立线程，禁止等待运行时锁。
pub(crate) fn current_node_id(app: &AppHandle) -> Option<String> {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.try_lock().ok()?;
    guard.as_ref().map(|r| r.node_id.clone())
}

/// 应答首连确认弹窗（issue 04 命令面）
///
/// 接受路径先带展示名落库再回执：transport 随后的 `add` 变 no-op，保证连接
/// 建立时可信条目已带元数据（设置面立即可见名称）。返回是否成功送达回执——
/// false 表示请求已超时/已应答/ID 未知（前端应关闭对应弹窗）。
#[tauri::command]
pub async fn respond_peer_consent(app: AppHandle, request_id: String, accepted: bool) -> crate::Result<bool> {
    let state = app.state::<PeerNetState>();
    let pending = state
        .consents
        .lock()
        .expect("consent table lock poisoned")
        .remove(&request_id);
    let Some(PendingConsent { node_id, reply }) = pending else {
        tracing::warn!(request_id = %request_id, "consent respond for unknown/expired request");
        return Ok(false);
    };

    if accepted {
        let display_name = cached_device_name(&app, &node_id).await;
        if let Some(trust) = state.trust.lock().await.as_ref() {
            // 落库失败不阻断放行：远端已确认、连接本身有效，仅影响重启后免确认
            if let Err(e) = trust.add_with_metadata(&node_id, display_name.as_deref()) {
                tracing::error!(node_id = %node_id, "persist trusted peer before accept reply failed: {e}");
            }
        }
    }
    // 请求方已超时/关停时发送失败属预期：无后续动作可做
    let delivered = reply.send(accepted).is_ok();
    tracing::info!(
        node_id = %node_id,
        short = %node_id.short_fingerprint(),
        accepted,
        delivered,
        "peer consent answered"
    );
    Ok(delivered)
}

/// 可信对端列表（设置面管理用；节点未启动仍可读——句柄独立于运行时存活）
#[tauri::command]
/// 可信条目 → DTO（纯函数，供测试）：展示名持久化名优先、在线缓存名兑底，
/// 均缺为 None（前端以短指纹兑底）；短指纹取前 8 位；加入时刻转 RFC3339
pub(crate) fn trust_entry_to_dto(entry: &TrustedPeerEntry, online_names: &HashMap<NodeId, String>) -> TrustedPeerDto {
    TrustedPeerDto {
        display_name: entry
            .display_name
            .clone()
            .or_else(|| online_names.get(&entry.node_id).cloned()),
        node_id: entry.node_id.to_string(),
        fingerprint_short: entry.node_id.short_fingerprint().to_string(),
        added_at: entry.added_at.to_rfc3339(),
    }
}

/// 可信对端列表（设置面管理用；节点未启动仍可读——句柄独立于运行时存活）
#[tauri::command]
pub async fn list_trusted_peers(app: AppHandle) -> crate::Result<Vec<TrustedPeerDto>> {
    let trust = trust_handle(&app).await?;
    let state = app.state::<PeerNetState>();
    let online_names: HashMap<NodeId, String> = {
        let guard = state.runtime.lock().await;
        guard
            .as_ref()
            .map(|runtime| {
                runtime
                    .cache
                    .list()
                    .into_iter()
                    .map(|record| (record.node_id, record.device_name))
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(trust
        .list_entries()
        .into_iter()
        .map(|entry| trust_entry_to_dto(&entry, &online_names))
        .collect())
}

/// 撤销可信对端（返回该 ID 原本是否存在；撤销后对端重连重新走首连确认）
#[tauri::command]
pub async fn revoke_trusted_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    let parsed =
        NodeId::parse(&node_id).map_err(|e| crate::AppError::Internal(format!("invalid peer node id: {e}")))?;
    let trust = trust_handle(&app).await?;
    let removed = trust.remove(&parsed).map_err(map_peer_net_error)?;
    // 撤销信任的同时断开与该节点的活跃连接（出站/入站均关停）：信任撤销只影响
    // 下次首连确认，对已建立的连接无作用，不主动断开会让对端仍显示已连接
    // （2026-09-07 实机反馈）。断开失败仅记日志不阻断撤销结果——信任已移除。
    if removed {
        if let Err(e) = disconnect_peer(app.clone(), node_id).await {
            tracing::warn!(node_id = %parsed, "disconnect after revoke failed: {e}");
        }
    }
    Ok(removed)
}

// ==================== 共享目录（issue 07）====================

/// 全量幂等替换引擎广播源（host-peer `set-shared-roots` 原语的引擎入口，
/// ADR 0022 v2）：注册表 CRUD 真源已移插件侧，本函数只同步暴露面镜像；
/// 条目 id/name/root 由调用方构造（桌面 Fs / 移动 Saf 根形态均支持）。
pub async fn set_shared_roots(app: AppHandle, entries: Vec<SharedDirEntry>) -> crate::Result<()> {
    let store = shared_handle(&app).await?;
    store.replace_all(&entries).map_err(map_peer_net_error)
}

// ==================== File-transfer engine bridge ====================
//
// These adapters keep the host-peer boundary in the peer-net module. Product
// state remains owned by the file-transfer plugin; the engine adapters only
// expose the existing peer-net session controls and wire-compatible DTOs.
pub(crate) use crate::peer_engine_remote::{PeerSharedRootDto, RemoteBrowseDto, RemotePullFileDto};
pub(crate) use crate::peer_engine_transfer::PeerTransferDto;

pub(crate) async fn send_files_for_plugin(
    app: AppHandle,
    node_id: String,
    paths: Vec<String>,
    encrypt: Option<bool>,
) -> crate::Result<PeerTransferDto> {
    crate::peer_engine_transfer::send_files_to_peer_with_policy(app, node_id, paths, encrypt).await
}

pub(crate) async fn cancel_transfer_for_plugin(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    crate::peer_engine_transfer::cancel_peer_transfer(app, batch_id).await
}

/// 接收侧取消（host-peer `close` 第 ③ 分支）：与发送侧取消分开，pending 询问
/// 视同拒绝、在途接收/拉取按会话令牌中止
pub(crate) async fn cancel_receiving_for_plugin(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    crate::peer_engine_receive::cancel_peer_receiving(app, batch_id).await
}

pub(crate) async fn respond_transfer_for_plugin(
    app: AppHandle,
    batch_id: String,
    accepted: bool,
) -> crate::Result<bool> {
    crate::peer_engine_receive::respond_peer_transfer(app, batch_id, accepted).await
}

pub(crate) async fn set_receive_policy_for_plugin(
    app: AppHandle,
    mode: String,
    timeout_secs: u64,
) -> crate::Result<()> {
    crate::peer_engine_receive::set_peer_receive_policy(app, mode, timeout_secs).await
}

pub(crate) async fn set_transfer_concurrency_for_plugin(app: AppHandle, concurrency: u8) -> crate::Result<()> {
    crate::peer_engine_receive::set_peer_transfer_concurrency(app, concurrency).await
}

pub(crate) async fn pause_transfer_for_plugin(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    crate::peer_engine_transfer::pause_peer_transfer(app, batch_id).await
}

pub(crate) async fn resume_transfer_for_plugin(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    crate::peer_engine_transfer::resume_peer_transfer(app, batch_id).await
}

pub(crate) async fn resume_all_transfers_for_plugin(app: AppHandle) -> crate::Result<u32> {
    Ok(crate::peer_engine_transfer::resume_all_peer_transfers(app).await? as u32)
}

pub(crate) async fn list_remote_roots_for_plugin(
    app: AppHandle,
    node_id: String,
) -> crate::Result<Vec<PeerSharedRootDto>> {
    crate::peer_engine_remote::list_peer_shared_roots(app, node_id).await
}

pub(crate) async fn browse_remote_for_plugin(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    rel_path: String,
) -> crate::Result<RemoteBrowseDto> {
    crate::peer_engine_remote::browse_peer_directory(app, node_id, dir_id, rel_path).await
}

pub(crate) async fn pull_files_for_plugin(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    files: Vec<RemotePullFileDto>,
) -> crate::Result<u32> {
    Ok(crate::peer_engine_remote::pull_peer_files(app, node_id, dir_id, files).await? as u32)
}

pub(crate) async fn set_download_dir_for_plugin(app: AppHandle, path: Option<String>) -> crate::Result<()> {
    crate::peer_engine_receive::set_peer_download_dir(app, path).await
}

/// 节点属主记账（审计票 12）：把节点从「未跑」带到「跑」的那个**调用方插件 id**。
///
/// 取代旧的两处按硬编码产品 id 分支（`activation.rs` 的激活/停用外壳）与 boot 末尾
/// 按 id 对账的 `sync_node_with_plugin_state`：内核不再认得任何产品，只按
/// 「谁起谁停」这条与产品无关的规则记账。`None` = 节点未跑，或跑着但无插件属主
/// （只可能来自宿主命令面 `start_peer_node`——它不是插件，不认领属主）。
pub fn node_owner(app: &AppHandle) -> Option<String> {
    let state = app.state::<PeerNetState>();
    let owner = state.node_owner.lock().expect("node owner lock poisoned").clone();
    owner
}

/// 插件按需启动本机节点（引擎级生命周期原语，审计票 12；幂等）
///
/// 返回 `true` = 本次调用把节点从「未跑」带到「跑」。属主规则：
/// 无主时可被认领（含宿主命令面先起的情况）；同主重复调用幂等；
/// **他主拒绝接管**，错误文案不回带对方 id（与 `host-peer` 句柄属主门同口径，票 05）。
pub async fn start_node_owned(app: &AppHandle, caller: &str) -> crate::Result<bool> {
    let state = app.state::<PeerNetState>();
    {
        let mut guard = state.node_owner.lock().expect("node owner lock poisoned");
        if let Some(owner) = guard.as_ref() {
            if owner != caller {
                return Err(crate::AppError::Plugin(
                    "peer node is already owned by another plugin".to_string(),
                ));
            }
        }
    }
    let was_running = { state.runtime.lock().await.is_some() };
    let data_dir = app_data_dir(app)?;
    let device_name = resolve_device_name(app);
    start_locked(&state, &data_dir, device_name, app).await?;
    // 认领放在装配成功之后：起不来的插件不该把节点锁在自己名下
    {
        let mut guard = state.node_owner.lock().expect("node owner lock poisoned");
        if guard.is_none() {
            *guard = Some(caller.to_string());
        }
    }
    Ok(!was_running)
}

/// 属主插件让节点下线（引擎级原语，审计票 12；节点未跑为 no-op）
///
/// 非属主一律拒绝（文案不回带属主 id）。关停与清账同处——`stop_locked` 里清属主，
/// 所以本函数不需要在关停失败时补偿
pub async fn stop_node_owned(app: &AppHandle, caller: &str) -> crate::Result<bool> {
    let state = app.state::<PeerNetState>();
    let owned = state
        .node_owner
        .lock()
        .expect("node owner lock poisoned")
        .as_deref()
        == Some(caller);
    if !owned {
        return Err(crate::AppError::Plugin(
            "not owner of peer node".to_string(),
        ));
    }
    stop_locked(&state, app).await?;
    Ok(true)
}

/// 内核侧按属主清理节点（审计票 12）：插件停用或激活失败时调用
///
/// 属主匹配才关停；不匹配（含无主）一律 no-op 且**不报错**——这条是任意插件的
/// 生命周期都能安全挂上的通用钩子，替代旧的两个 `plugin_id == FILE_TRANSFER_PLUGIN_ID`
/// 分支。返回是否真的关停了节点
pub async fn release_node_for(app: &AppHandle, former_owner: &str) -> crate::Result<bool> {
    let state = app.state::<PeerNetState>();
    let owned = state
        .node_owner
        .lock()
        .expect("node owner lock poisoned")
        .as_deref()
        == Some(former_owner);
    if !owned {
        return Ok(false);
    }
    stop_locked(&state, app).await?;
    Ok(true)
}

// ==================== 内部装配 ====================

/// 入站连接生命周期桥：进入 handler 前发 `peer-connected`、结束后发
/// `peer-disconnected`
///
/// 既有设计只在本机主动拨号路径发连接事件（dial_peer / endpoint 拨号），
/// 被连侧零事件——对端拨入本机时本机 UI 连接态永不点亮、设备行「连接」
/// 按钮不变（2026-09-06 实机实证）。包装 crate handler 桥接两端事件，并
/// 记账 [`PeerNetState::inbound_peers`] 供刷新重发还原首屏连接态。
struct InboundConnectionBridge {
    inner: Arc<SharedDirHandler>,
    app: AppHandle,
    cache: Arc<DiscoveryCache>,
}

impl ConnectionHandler for InboundConnectionBridge {
    fn handle(&self, conn: Connection) -> HandlerFuture {
        let node_id = conn.peer_node_id().clone();
        let device_name = self.cache.get(&node_id).map(|r| r.device_name);
        // 按连接计数记账 + 仅首个连接发 peer-connected：同节点可能同时有会话
        // 连接与数据面短连接（浏览/拉取各自新拨），短连接不得重复点亮连接态
        //（2026-09-07 实机实证：数据面短连接 churn 造成移动端仍显示未连接）
        let is_first_connection = {
            let state = self.app.state::<PeerNetState>();
            let mut peers = state.inbound_peers.lock().expect("inbound peers lock poisoned");
            let count = peers.entry(node_id.as_str().to_string()).or_default();
            *count += 1;
            let has_outbound = state
                .connections
                .lock()
                .expect("connections table lock poisoned")
                .contains_key(node_id.as_str());
            *count == 1 && !has_outbound
        };
        if is_first_connection {
            tracing::info!(
                node_id = %node_id,
                short = %node_id.short_fingerprint(),
                "inbound connection established, peer-connected emitted"
            );
            emit_json(
                &self.app,
                "peer-connected",
                serde_json::json!({
                    "nodeId": node_id.as_str(),
                    "deviceName": device_name,
                    "direction": "inbound",
                    "connected": true
                }),
            );
        }
        let inner = Arc::clone(&self.inner);
        let app = self.app.clone();
        let conn_id = NEXT_INBOUND_CONN_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Box::pin(async move {
            // 内层 handler 放入独立任务并登记 JoinHandle：disconnect_peer /
            // stop_locked 据此按节点中止（连接随任务 drop 关闭，对端活性泵经
            // EOF 感知）。handler 以 EOF/错误结束时本清算路径发 peer-disconnected
            let join =
                crate::system::error_boundary::spawn_with_error_boundary("peer_inbound_handler", inner.handle(conn));
            {
                let state = app.state::<PeerNetState>();
                state
                    .inbound_conns
                    .lock()
                    .expect("inbound conns lock poisoned")
                    .entry(node_id.as_str().to_string())
                    .or_default()
                    .insert(conn_id, join.abort_handle());
            }
            let _ = join.await;
            // 摘除记账（按连接计数递减）：与 stop_locked 的排水路径互斥去重。
            // 仅当本节点最后一个入站连接结束且无出站会话时才发 peer-disconnected
            //（会话连接与数据面短连接并存时，短连接结束不得摘除连接态）
            let was_last_inbound = {
                let state = app.state::<PeerNetState>();
                let mut peers = state.inbound_peers.lock().expect("inbound peers lock poisoned");
                match peers.entry(node_id.as_str().to_string()) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        let count = e.get_mut();
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            e.remove();
                            true
                        } else {
                            false
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(_) => false,
                }
            };
            if let Some(conns) = app
                .state::<PeerNetState>()
                .inbound_conns
                .lock()
                .expect("inbound conns lock poisoned")
                .get_mut(node_id.as_str())
            {
                conns.remove(&conn_id);
            }
            if was_last_inbound {
                let has_outbound = app
                    .state::<PeerNetState>()
                    .connections
                    .lock()
                    .expect("connections table lock poisoned")
                    .contains_key(node_id.as_str());
                if !has_outbound {
                    emit_json(
                        &app,
                        "peer-disconnected",
                        serde_json::json!({ "nodeId": node_id.as_str(), "connected": false }),
                    );
                }
            }
        })
    }
}

/// 持锁装配路径：start 命令与自动启动共用（调用方已持有状态互斥锁）
async fn start_locked(
    state: &tauri::State<'_, PeerNetState>,
    data_dir: &Path,
    device_name: String,
    app: &AppHandle,
) -> crate::Result<PeerNodeStatus> {
    // 生命周期闸门：全程持 permit 串行化并发装配/关停（activate 外壳
    // ensure_node_started 与 boot 对账 sync_node_with_plugin_state 可能并发
    // 触发）。修复 TOCTOU：先前幂等检查在锁内、装配在锁外，双装配时次者
    // bind 回退 :0 随机端口、首个 runtime 泄漏
    let _gate = state
        .lifecycle_gate
        .acquire()
        .await
        .expect("lifecycle gate never closed");
    // 幂等：重复 start 返回现状而非报错（命令面与自动启动可能竞争触发）
    {
        let guard = state.runtime.lock().await;
        if let Some(runtime) = guard.as_ref() {
            return Ok(PeerNodeStatus {
                started: true,
                node_id: runtime.node_id.clone(),
                listen_addr: runtime.running.local_addr().to_string(),
            });
        }
    }

    // 身份与可信表同目录（与 DB 并列）；init_node_identity 的日志在首启后可见
    let identity = init_node_identity(data_dir)
        .map_err(|e| crate::AppError::Internal(format!("peer-net identity load failed: {e}")))?;
    let trust = Arc::new(TrustStore::load_or_create(data_dir).map_err(map_peer_net_error)?);
    *state.trust.lock().await = Some(Arc::clone(&trust));

    // 占位 bind 固定默认端口（防火墙规则友好），被占回退 :0；占位句柄移交节点
    // 消除端口竞态（Decision 4）
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, DEFAULT_PEER_PORT)))
        .or_else(|_| TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))))
        .map_err(|e| crate::AppError::Internal(format!("bind peer-net listener failed: {e}")))?;
    let bind_addr = listener
        .local_addr()
        .map_err(|e| crate::AppError::Internal(format!("read peer-net listener addr failed: {e}")))?;

    let node = PeerNetNode::new(PeerNetNodeConfig {
        bind_addr,
        identity,
        static_peers: Vec::new(),
    })
    .map_err(map_peer_net_error)?
    .with_trust_store(Arc::clone(&trust))
    .with_discovery(Arc::new(DiscoveryCache::new()));
    let cache = node.discovery().expect("discovery cache just attached");

    // 首连确认桥接：闸门事件 → 前端应用内确认弹窗（issue 04）
    let (events_tx, events_rx) = tokio::sync::mpsc::channel::<TrustEvent>(16);
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_net_gate",
        drive_gate(events_rx, Arc::clone(&cache), app.clone()),
    );

    // 共享目录注册表 + 接收落点（issue 07）：桌面端缺省 Downloads\BedCode\
    let shared = shared_handle_at(&data_dir).await?;
    *state.shared.lock().await = Some(Arc::clone(&shared));
    let download_dir = resolve_download_dir(app)?;
    if let Err(e) = tokio::fs::create_dir_all(&download_dir).await {
        tracing::error!(dir = %download_dir.display(), "create peer download dir failed: {e}");
    }

    // 受信连接统一走共享目录复合处理器：push 接收管线 + 浏览/拉取服务
    // （首帧分流在 crate 内完成）；引擎事件由接收侧模块消费（issue 10：
    // 询问弹窗/任务登记/进度推送）。服务侧拉取会话走独立通道（双端记账：
    // 供流方要在自己的传输列表展示 send 任务，与 push 接收通道分流）
    let (transfer_tx, transfer_rx) = tokio::sync::mpsc::channel::<TransferEvent>(256);
    let (serve_tx, serve_rx) = tokio::sync::mpsc::channel::<TransferEvent>(64);
    let config = TransferConfig {
        policy: bedcode_peer_net::ReceivePolicy::Ask {
            timeout: std::time::Duration::from_secs(60),
        },
        download_dir,
        chunk_size: 64 * 1024,
        landing: None,
    };
    let handler = Arc::new(SharedDirHandler::new(
        shared,
        None,
        config.clone(),
        transfer_tx,
        serve_tx,
    ));
    // 接收侧登记句柄与配置快照（设置热更新/按批取消入口），并按持久化
    // 设置纠正首份策略与落点；事件消费任务随后启动
    super::peer_engine_receive::register_handler(app, Arc::clone(&handler), config).await;
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_net_transfer_events",
        super::peer_engine_receive::drive_receive_events(app.clone(), transfer_rx),
    );
    // 服务侧供流记账（双端记账）：PullServed/Progress/Terminal → 发送侧任务
    // （发送会话事件适配器注册/推进/结算 direction=send 任务，
    // 对端拉取发起方另有自己的 receive 任务，两端各自展示同一次传输）
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_net_serve_events",
        super::peer_engine_transfer::drive_serve_events(app.clone(), serve_rx),
    );
    // 远端浏览/拉取会话上下文（issue 11）：事件通道发送端快照供拉取入账任务表
    super::peer_engine_remote::register_session(app, handler.event_sender()).await;

    let running = node
        .start_with_listener(
            listener,
            events_tx,
            Arc::new(InboundConnectionBridge {
                inner: handler,
                app: app.clone(),
                cache: Arc::clone(&cache),
            }),
        )
        .map_err(map_peer_net_error)?;
    let listen_addr = running.local_addr();

    // ===== mDNS 基础能力服务收敛（spec v2 / ticket 04）=====
    // 节点发现接入 MdnsService 全局共享守护：不再自建 daemon（消灭双 daemon
    // 同绑 5353 互抢多播包的历史病灶）。节点身份广播（owner=host）在基础服务
    // 登记——TXT/ServiceInfo 仍由引擎构造（D3），本处只做句柄登记；全局
    // `mdns:found` / `mdns:lost` 桥接与缓存重发通道已退役（D1）——插件发现
    // 改经 host-mdns 自建 browse 收定向事件（file-transfer 一期同迁，D2）
    let mdns_daemon = crate::wasm_core::host_api::mdns::shared_daemon();
    let daemon = bedcode_peer_net::spawn_peer_mdns_daemon(
        &node,
        &running,
        DiscoveryConfig {
            device_name,
            capabilities: CAP_FILE_TRANSFER,
        },
        mdns_daemon,
    )
    .map_err(map_peer_net_error)?;
    // 宿主身份广播登记（owner=host）：节点停机时随 runtime 注销（stop_host_service）
    let host_adv = crate::wasm_core::host_api::mdns::register_host_service(
        bedcode_peer_net::SERVICE_TYPE,
        daemon.service_fullname(),
    )
    .map_err(|e| crate::AppError::Plugin(format!("mdns host service registration failed: {e}")))?;

    let node_id = node.node_id().to_string();
    *state.runtime.lock().await = Some(PeerNetRuntime {
        node,
        running,
        daemon,
        host_adv,
        cache,
        node_id,
    });
    tracing::info!(
        "peer-net node started: addr={listen_addr}, discovery service={}",
        bedcode_peer_net::SERVICE_TYPE
    );
    spawn_discovery_refresh_subscriber(app.clone());
    Ok(PeerNodeStatus {
        started: true,
        node_id: state
            .runtime
            .lock()
            .await
            .as_ref()
            .expect("just stored")
            .node_id
            .clone(),
        listen_addr: listen_addr.to_string(),
    })
}

/// 持锁关停路径：先停发现守护（注销广播让对端即时移除本机），再关监听
///
/// 可信列表句柄刻意保留（`trust` 槽位不清空）：节点停止后设置面仍可查看/撤销。
async fn stop_locked(state: &tauri::State<'_, PeerNetState>, app: &AppHandle) -> crate::Result<()> {
    // 与 start_locked 串行：装配/关停互斥，避免与并发装配交错（闸门见
    // PeerNetState::lifecycle_gate）
    let _gate = state
        .lifecycle_gate
        .acquire()
        .await
        .expect("lifecycle gate never closed");
    // 关停与清账同处（审计票 12）：不管走哪条路径停下来（属主原语 / 内核属主清理 /
    // 宿主命令面人工停 / 退出排水），节点一旦下线就不再属于任何插件
    *state.node_owner.lock().expect("node owner lock poisoned") = None;
    // 主动拨号的存活连接随关停一并终结：通知活性泵关闭（drop 连接 → 对端经
    // EOF 感知本机下线），并逐个通知前端摘除已连接徽标——连接表随节点生命
    // 周期走，重启后从空表开始
    let drained: Vec<(String, OutboundSession)> = state
        .connections
        .lock()
        .expect("connections table lock poisoned")
        .drain()
        .collect();
    // 已发断开事件的节点集合：入站排水与之去重，同节点出站+入站并存时
    // 只发一次（前端幂等但冗余事件会刷新两遍 UI）
    let mut emitted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (node_id, session) in drained {
        let _ = session.close.send(true);
        emit_json(
            app,
            "peer-disconnected",
            serde_json::json!({ "nodeId": node_id, "connected": false }),
        );
        emitted.insert(node_id);
    }
    // 入站连接随节点关停一并终结：中止内层 handler（连接随任务 drop 关闭，
    // 对端活性泵感知）+ 排水记账并通知前端（桥的完成路径因记账已摘除而不重复发）
    let inbound_conns: Vec<HashMap<u64, tokio::task::AbortHandle>> = state
        .inbound_conns
        .lock()
        .expect("inbound conns lock poisoned")
        .drain()
        .map(|(_, handles)| handles)
        .collect();
    for handles in inbound_conns {
        for (_, handle) in handles {
            handle.abort();
        }
    }
    let inbound_drained: Vec<String> = state
        .inbound_peers
        .lock()
        .expect("inbound peers lock poisoned")
        .drain()
        .map(|(node_id, _)| node_id)
        .collect();
    for node_id in inbound_drained {
        // 与出站排水去重：同节点出站+入站并存时，断开事件已在出站循环发出
        if emitted.contains(&node_id) {
            continue;
        }
        emit_json(
            app,
            "peer-disconnected",
            serde_json::json!({ "nodeId": node_id, "connected": false }),
        );
    }
    let runtime = state.runtime.lock().await.take();
    match runtime {
        Some(runtime) => {
            // 接收侧句柄随节点下线摘除（设置命令此后仅改持久化，下次启动生效）；
            // 远端拉取队列同步中止（issue 11）
            super::peer_engine_receive::clear_handler(app).await;
            super::peer_engine_remote::clear_state(app).await;
            // 引擎 daemon 停：退订浏览 + 注销自身广播（共享守护不 shutdown）
            runtime.daemon.stop().await.map_err(map_peer_net_error)?;
            // 注销宿主身份广播登记（owner=host，MdnsService ADVERTISERS）
            let _ = crate::wasm_core::host_api::mdns::stop_host_service(&runtime.host_adv);
            runtime.running.shutdown().await;
            tracing::info!("peer-net node stopped");
            Ok(())
        }
        None => Ok(()),
    }
}

/// 首连确认事件消费：桥接到前端应用内确认弹窗（issue 04）
///
/// 收到 `ConfirmRequested` 后：解析发现缓存中的设备名 → 登记回执通道 → 发
/// `peer-consent-requested` 事件。应答经 `respond_peer_consent` 命令回流；
/// 30s 内无应答由 crate 闸门按拒绝结算（`CONFIRM_TIMEOUT`），弹窗超时语义
/// 与之天然对齐（前端按同值倒计时收起）。
async fn drive_gate(mut events: tokio::sync::mpsc::Receiver<TrustEvent>, cache: Arc<DiscoveryCache>, app: AppHandle) {
    while let Some(event) = events.recv().await {
        match event {
            TrustEvent::ConfirmRequested { node_id, reply } => {
                // 设备名取自发现缓存：拨入方与本机同网互见，正常必有记录；
                // 缺失时前端以短指纹兜底展示
                let device_name = cache.get(&node_id).map(|record| record.device_name);
                let request_id = uuid::Uuid::new_v4().to_string();
                {
                    let state = app.state::<PeerNetState>();
                    let mut pending = state.consents.lock().expect("consent table lock poisoned");
                    pending.insert(
                        request_id.clone(),
                        PendingConsent {
                            node_id: node_id.clone(),
                            reply,
                        },
                    );
                }
                tracing::info!(
                    node_id = %node_id,
                    short = %node_id.short_fingerprint(),
                    name = ?device_name,
                    "first-connect confirmation requested, consent dialog emitted"
                );
                // issue 12 迁移后插件前端是确认弹窗唯一消费者：必须经 emit_json
                // 同步桥接总线 peer:consent（裸 app.emit 只达主前端，插件永远
                // 收不到→弹窗不出现→30s 超时自动拒，2026-08-26 双端实测实证）
                emit_json(
                    &app,
                    "peer-consent-requested",
                    serde_json::json!({
                        "requestId": request_id,
                        "nodeId": node_id.as_str(),
                        "fingerprintShort": node_id.short_fingerprint(),
                        "deviceName": device_name,
                    }),
                );
            }
        }
    }
}

/// 共享目录注册表句柄：未加载时惰性从磁盘加载（管理面在节点从未启动时也可用）
async fn shared_handle(app: &AppHandle) -> crate::Result<Arc<SharedDirStore>> {
    let state = app.state::<PeerNetState>();
    let mut guard = state.shared.lock().await;
    if let Some(store) = guard.as_ref() {
        return Ok(Arc::clone(store));
    }
    let data_dir = app_data_dir(app)?;
    let store = shared_handle_at(&data_dir).await?;
    *guard = Some(Arc::clone(&store));
    Ok(store)
}

/// 按数据目录加载共享目录注册表（start 路径与惰性路径共用）
async fn shared_handle_at(data_dir: &Path) -> crate::Result<Arc<SharedDirStore>> {
    let dir = data_dir.to_path_buf();
    // crate 的 load_or_create 为同步阻塞 IO：移出异步上下文
    let store = tauri::async_runtime::spawn_blocking(move || SharedDirStore::load_or_create(&dir))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join shared dirs load failed: {e}")))?
        .map_err(map_peer_net_error)?;
    Ok(Arc::new(store))
}

/// 接收落点解析：系统 Downloads\BedCode\（解析失败回退应用数据目录内 downloads/）
pub(crate) fn resolve_download_dir(app: &AppHandle) -> crate::Result<PathBuf> {
    let base = app
        .path()
        .download_dir()
        .map_err(|e| crate::AppError::Internal(format!("resolve downloads dir failed: {e}")))?;
    Ok(base.join("BedCode"))
}

/// PeerNetError → AppError 映射：crate 错误链保留完整文案（本票宿主侧只打日志不细分）
pub(crate) fn map_peer_net_error(e: PeerNetError) -> crate::AppError {
    crate::AppError::Internal(format!("peer-net operation failed: {e}"))
}

/// 节点 ID hex 字符串解析（issue 08 命令面共用）：统一错误上下文
pub(crate) fn parse_node_id(node_id: &str) -> crate::Result<NodeId> {
    NodeId::parse(node_id).map_err(|e| crate::AppError::Internal(format!("peer-net invalid node id '{node_id}': {e}")))
}

/// 向前端发 JSON 载荷事件（失败只记日志不上抛：窗口缺失/前端未就绪属预期场景）。
/// 同步桥接到插件消息总线 `peer:*` topic（issue 12 切换后 file-transfer 插件
/// 经 host-peer/总线感知对等状态），无头上下文静默跳过。
/// 仅连接生命周期与首连确认事件走此双路面（桌面全局通知消费 Tauri 侧）；
/// 业务快照（传输任务/接收列表）只走 [`publish_bus_only`]，票 06。
pub(crate) fn emit_json(app: &AppHandle, event: &str, payload: serde_json::Value) {
    if let Err(e) = app.emit(event, payload.clone()) {
        tracing::error!("emit {event} failed: {e}");
    }
    publish_bus_only(event, payload);
}

/// 业务快照仅进插件总线（票 06 事件桥收敛）：宿主 Tauri 前端不再消费
/// 传输/接收列表事件，产品状态由 file-transfer 插件经 `peer:*` 快照驱动。
/// `event` 为宿主事件名，经 [`bus_topic_for`] 映射总线 topic；无映射则丢弃。
pub(crate) fn publish_bus_only(event: &str, payload: serde_json::Value) {
    let Some(topic) = bus_topic_for(event) else {
        return;
    };
    // 诊断插桩：peer 事件推送可见性（对齐移动端 INFO-only 日志口径）
    tracing::info!(event, topic, "peer event pushed to plugin bus");
    if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
        ctx.plugin_host().message_bus().publish(topic, "host", payload);
    }
}

/// 引擎发现事件直推插件总线（`mdns:found` / `mdns:lost`）——已退役（spec v2
/// D1：全局发现桥接整体退役，插件改经 host-mdns 自建 browse 收定向事件）。
/// 本函数仅保留 `peer:*` topic 的直推（拨号/传输状态，与 mDNS 无关）
pub(crate) fn publish_mdns_bus(topic: &str, payload: serde_json::Value) {
    // 诊断插桩：与 peer 事件同口径 INFO-only
    tracing::info!(topic, "peer event pushed to plugin bus");
    if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
        ctx.plugin_host().message_bus().publish(topic, "host", payload);
    }
}

// ==================== 插件「探索发现」直达路径 ====================

/// 插件刷新请求 topic：插件 rust 经 `bus_publish` 发布，宿主静态订阅消费
const DISCOVERY_REFRESH_TOPIC: &str = "peer:discovery-refresh";
/// 宿主静态订阅者注册名（≠ 插件 id，避免 publish 的 sender 过滤误伤）
const DISCOVERY_REFRESH_SUBSCRIBER: &str = "host-peer-discovery";

/// 刷新请求处理器：同步回调内不 await，重活移交 tauri 异步运行时
struct DiscoveryRefreshHandler {
    app: AppHandle,
}

impl crate::wasm_core::BusMessageHandler for DiscoveryRefreshHandler {
    fn on_message(&self, _msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        let app = self.app.clone();
        crate::system::error_boundary::spawn_with_error_boundary("peer_net_discovery_refresh_handler", async move {
            handle_discovery_refresh(app).await;
        });
        Ok(())
    }
}

/// 处理刷新请求：① 触发即时重查（查询→应答是本环境唯一可靠发现路径）；
/// ② 当前发现缓存逐条以 `mdns:found` 重发——插件前端激活晚于发现事件时
/// （引擎启动即发现对端，插件订阅在秒级之后）按钮/首屏仍有完整设备列表
async fn handle_discovery_refresh(app: AppHandle) {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.lock().await;
    let Some(runtime) = guard.as_ref() else {
        tracing::debug!("peer discovery refresh: node not running");
        return;
    };
    runtime.daemon.request_requery();
    let snapshot = runtime.cache.list();
    let mut connected: Vec<String> = state
        .connections
        .lock()
        .expect("connections table lock poisoned")
        .keys()
        .cloned()
        .collect();
    // 入站（被连侧）连接同样纳入重发：插件前端挂载晚于入站连接建立时首屏即真
    connected.extend(
        state
            .inbound_peers
            .lock()
            .expect("inbound peers lock poisoned")
            .keys()
            .cloned(),
    );
    drop(guard);
    // 设备名映射（连接态重发携带；记录已随 requery 事件流刷入插件定向 browse）
    // 注意：不再逐条重发 mdns:found（spec v2 D1 缓存重发通道退役）——requery
    // 触发引擎重新 browse，插件自建 browse 的 receiver 会收到 mdns-sd 缓存
    // 重放（含新发现），设备列表各端自行收敛
    let device_names: std::collections::HashMap<String, String> = snapshot
        .iter()
        .map(|record| (record.node_id.as_str().to_string(), record.device_name.clone()))
        .collect();
    // 连接态重发：本机主动拨号的存活连接逐个以 peer:connection 重推
    // （入站连接的连接态由 accept 时刻的实时事件维护）。插件前端挂载晚于
    // 连接建立时，连接徽标/状态胶囊首屏即真，不再误用宿主主连接状态。
    // deviceName 与正常路径（dial_connected_payload / 入站桥）一致携带：
    // 重发缺 deviceName 时前端首屏只能用指纹兜底展示
    for node_id in connected {
        publish_mdns_bus(
            "peer:connection",
            serde_json::json!({
                "nodeId": node_id,
                "deviceName": device_names.get(&node_id).map(String::as_str).unwrap_or(""),
                "connected": true,
            }),
        );
    }
    tracing::info!("peer discovery requery + connection state republished after refresh request");
}

/// 挂载刷新请求静态订阅：节点由插件激活驱动启动，注册时插件管理器可能
/// 尚未进全局态，轮询等就绪后再注册（进程内仅注册一次）
/// 注册护栏：peer 节点重启会重入 start_locked，静态订阅只挂一次
static DISCOVERY_REFRESH_SUBSCRIBED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn spawn_discovery_refresh_subscriber(app: AppHandle) {
    use std::sync::atomic::Ordering;
    if DISCOVERY_REFRESH_SUBSCRIBED.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::system::error_boundary::spawn_with_error_boundary("peer_net_discovery_refresh_subscriber", async move {
        loop {
            if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
                ctx.plugin_host()
                    .message_bus()
                    .subscribe_static(
                        DISCOVERY_REFRESH_SUBSCRIBER,
                        DISCOVERY_REFRESH_TOPIC,
                        Box::new(DiscoveryRefreshHandler { app }),
                    )
                    .await;
                tracing::info!("peer discovery refresh subscriber registered");
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    });
}

/// 前端事件名 → 插件总线 topic 映射（非对等事件返回 None 不桥接）
fn bus_topic_for(event: &str) -> Option<&'static str> {
    match event {
        // "peer-devices-changed" → peer:devices 已随 DiscoveryCache 守护退役
        // （issue 13 Phase 4：设备列表由插件经 host-mdns 自建）
        "peer-connected" | "peer-disconnected" => Some("peer:connection"),
        "peer-consent-requested" => Some("peer:consent"),
        "peer-transfer-changed" => Some("peer:transfer"),
        "peer-receive-changed" => Some("peer:receive"),
        _ => None,
    }
}

/// 拨号成功 `peer-connected` 载荷（dial_peer / dial_peer_endpoint 共用）。
///
/// `connected` 是插件前端 handleConnectionChanged 的连接判据：缺失即按断开
/// 处理，会把 dial-peer 命令成功后前端的 markConnected 回滚成「未连接」。
/// 出站拨号路径曾漏发该字段（入站桥/刷新重发/断开事件均带），集中构造防再漏
fn dial_connected_payload(node_id: &str, device_name: &str) -> serde_json::Value {
    serde_json::json!({ "nodeId": node_id, "deviceName": device_name, "connected": true })
}

/// 运行时句柄快照（issue 09 发送编排用）：节点句柄 + 发现缓存
///
/// 锁内仅 Clone 廉价句柄，不跨 await 持锁；节点未启动返回 None。
pub(crate) async fn runtime_snapshot(app: &AppHandle) -> Option<(PeerNetNode, Arc<DiscoveryCache>)> {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.lock().await;
    guard.as_ref().map(|r| (r.node.clone(), r.cache.clone()))
}

/// 可信列表句柄：未加载时惰性从磁盘加载（设置面在节点从未启动时也可用）
async fn trust_handle(app: &AppHandle) -> crate::Result<Arc<TrustStore>> {
    let state = app.state::<PeerNetState>();
    let mut guard = state.trust.lock().await;
    if guard.is_none() {
        let data_dir = app_data_dir(app)?;
        let store = Arc::new(TrustStore::load_or_create(&data_dir).map_err(map_peer_net_error)?);
        *guard = Some(Arc::clone(&store));
    }
    Ok(Arc::clone(guard.as_ref().expect("just filled")))
}

/// 对端设备名的在线缓存解析（确认应答落库时补展示元数据用；离线时为 None）
async fn cached_device_name(app: &AppHandle, node_id: &NodeId) -> Option<String> {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.lock().await;
    guard
        .as_ref()
        .and_then(|runtime| runtime.cache.get(node_id))
        .map(|record| record.device_name)
}

/// 解析 app 数据目录（身份/可信列表所在，与 DB 同源）
pub(crate) fn app_data_dir(app: &AppHandle) -> crate::Result<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Internal(format!("resolve app data dir failed: {e}")))
}

/// 设备名解析：SystemInfo 已在 setup 阶段 manage；缺省名仅兜底异常初始化顺序
fn resolve_device_name(app: &AppHandle) -> String {
    app.try_state::<Arc<crate::system::info::SystemInfo>>()
        .map(|info| info.device_name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "BedCode Desktop".to_string())
}

/// 拨号连接事件载荷契约：`connected` 字段必须存在（插件前端缺失即按断开处理）
#[cfg(test)]
mod dial_payload_tests {
    use super::*;

    #[test]
    fn dial_connected_payload_carries_connected_true() {
        let payload = dial_connected_payload("aa", "Pixel 9");
        assert_eq!(payload["nodeId"], "aa");
        assert_eq!(payload["deviceName"], "Pixel 9");
        assert_eq!(payload["connected"], true);
    }
}

#[cfg(test)]
mod peer_net_tests {
    use super::*;
    use bedcode_peer_net::TrustedPeerEntry;
    use chrono::Utc;

    /// 64 位小写 hex 节点 ID（格式校验通过的最小形态）
    fn node_id(suffix: u8) -> NodeId {
        NodeId::parse(&format!("{:02x}{}", suffix, "ab".repeat(31))).unwrap()
    }

    // ==================== 可信条目 → DTO 转换（list_trusted_peers） ====================

    #[test]
    fn trust_entry_to_dto_prefers_persisted_name_over_online_name() {
        let entry = TrustedPeerEntry {
            node_id: node_id(1),
            display_name: Some("persisted-name".to_string()),
            added_at: Utc::now(),
        };
        let mut online_names = HashMap::new();
        online_names.insert(entry.node_id.clone(), "online-name".to_string());

        let dto = trust_entry_to_dto(&entry, &online_names);
        // 持久化名优先，在线缓存名不得覆盖
        assert_eq!(dto.display_name.as_deref(), Some("persisted-name"));
        assert_eq!(dto.node_id, entry.node_id.as_str());
        assert_eq!(dto.fingerprint_short.len(), 8, "短指纹固定 8 字符");
        assert_eq!(dto.added_at, entry.added_at.to_rfc3339());
    }

    #[test]
    fn trust_entry_to_dto_falls_back_to_online_name_when_persisted_missing() {
        let entry = TrustedPeerEntry {
            node_id: node_id(2),
            display_name: None,
            added_at: Utc::now(),
        };
        let mut online_names = HashMap::new();
        online_names.insert(entry.node_id.clone(), "online-only".to_string());

        let dto = trust_entry_to_dto(&entry, &online_names);
        assert_eq!(dto.display_name.as_deref(), Some("online-only"));
    }

    #[test]
    fn trust_entry_to_dto_returns_none_when_both_names_missing() {
        let entry = TrustedPeerEntry {
            node_id: node_id(3),
            display_name: None,
            added_at: Utc::now(),
        };
        let dto = trust_entry_to_dto(&entry, &HashMap::new());
        assert_eq!(dto.display_name, None, "两处均缺 → None（前端以短指纹兜底）");
        assert_eq!(dto.fingerprint_short, &entry.node_id.as_str()[..8]);
    }

    // ==================== 发现事件 → 总线 topic（bus_topic_for） ====================

    #[test]
    fn bus_topic_for_maps_known_events() {
        assert_eq!(bus_topic_for("peer-connected"), Some("peer:connection"));
        assert_eq!(bus_topic_for("peer-disconnected"), Some("peer:connection"));
        assert_eq!(bus_topic_for("peer-consent-requested"), Some("peer:consent"));
        assert_eq!(bus_topic_for("peer-transfer-changed"), Some("peer:transfer"));
        assert_eq!(bus_topic_for("peer-receive-changed"), Some("peer:receive"));
    }

    #[test]
    fn bus_topic_for_unknown_event_is_none() {
        assert_eq!(bus_topic_for("peer-devices-changed"), None, "设备列表已随缓存守护退役");
        assert_eq!(bus_topic_for(""), None);
    }

    // ==================== 可信列表 CRUD（TrustStore，tempdir 隔离） ====================

    #[test]
    fn trust_store_crud_roundtrip_with_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = TrustStore::load_or_create(dir.path()).expect("load_or_create");
        let id = node_id(5);

        // 空列表起步
        assert!(!store.list_entries().iter().any(|e| e.node_id == id));
        // 带名新增 → true；重复新增 → false（已存在）
        assert!(store.add_with_metadata(&id, Some("Pixel 9")).expect("add"));
        assert!(!store.add_with_metadata(&id, Some("Pixel 9")).expect("dup add"));
        // 列表可查，元数据落库
        let entry = store
            .list_entries()
            .into_iter()
            .find(|e| e.node_id == id)
            .expect("entry persisted");
        assert_eq!(entry.display_name.as_deref(), Some("Pixel 9"));
        // 移除 → true；再移除 → false
        assert!(store.remove(&id).expect("remove"));
        assert!(!store.remove(&id).expect("remove again"));
        assert!(!store.list_entries().iter().any(|e| e.node_id == id));
    }

    // ==================== 共享目录注册表 CRUD（SharedDirStore，tempdir 隔离） ====================

    #[test]
    fn shared_dir_store_crud_and_replace_all() {
        let dir = tempfile::tempdir().expect("tempdir");
        // 注册表要求 root 真实存在（add/replace_all 均校验），先建真实子目录
        let real_root = dir.path().join("Projects");
        std::fs::create_dir_all(&real_root).expect("mkdir");
        let real_root_b = dir.path().join("B");
        std::fs::create_dir_all(&real_root_b).expect("mkdir");
        let store = SharedDirStore::load_or_create(dir.path()).expect("load_or_create");
        assert!(store.list().is_empty());

        let entry = store
            .add(
                "Projects",
                SharedDirRoot::Fs {
                    path: real_root.clone(),
                },
            )
            .expect("add");
        assert!(!entry.id.is_empty());
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "Projects");

        // 移除 → true；不存在 → false
        assert!(store.remove(&entry.id).expect("remove"));
        assert!(!store.remove(&entry.id).expect("remove again"));
        assert!(store.list().is_empty());

        // replace_all 幂等批量替换
        let entries = vec![
            SharedDirEntry {
                id: "a".to_string(),
                name: "A".to_string(),
                root: SharedDirRoot::Fs {
                    path: real_root.clone(),
                },
            },
            SharedDirEntry {
                id: "b".to_string(),
                name: "B".to_string(),
                root: SharedDirRoot::Fs {
                    path: real_root_b.clone(),
                },
            },
        ];
        store.replace_all(&entries).expect("replace_all");
        assert_eq!(store.list().len(), 2);
        assert_eq!(store.list()[0].id, "a");
        assert_eq!(store.list()[1].id, "b");
    }
}
