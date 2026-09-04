//! 对等网络接入（宿主侧薄封装）：节点身份初始化 + 节点/发现守护装配与命令面。
//!
//! 节点身份与设备身份（auth 域 DeviceIdentity / device_identity.json）刻意分离
//! ——NodeIdentity 首启纯随机生成、重装即新身份，不做 ANDROID_ID 等任何设备标识
//! 派生（决策 D2）；持久化由 crate 全权负责，宿主只注入数据目录（决策 D3），
//! 目录复用 `init_identity` 同源的 app_data_dir 解析点，两份身份文件并列存放。
//!
//! ticket 03 扩展（决策 D7）：
//! - setup 阶段自动启动节点 + mDNS 发现守护——自动启动是真机冒烟（AC#2/#3）
//!   的零操作路径；
//! - **Android 专属**：启动守护前经 Kotlin MulticastLockPlugin 申请多播锁，
//!   关停时释放（D6）——不持锁时 mDNS 响应收不到，表现为单侧可见。
//!
//! ticket 04 扩展（首连确认 UI + 可信对端管理）：
//! - 闸门事件桥接到前端：`ConfirmRequested` → 解析发现缓存设备名 → 发
//!   `peer-consent-requested` 事件，前端弹应用内确认框（迁移规则匹配在
//!   前端层完成——终端配对名单只有前端持有，见 usePeerConsent）；
//! - `respond_peer_consent` / `list_trusted_peers` / `revoke_trusted_peer`
//!   三个命令供确认应答与设置面管理；可信列表句柄独立于节点运行时存活。
//!
//! issue 07 扩展（共享目录暴露 + 远程浏览/拉取）：
//! - 受信连接 handler 从占位换成 crate 的 [`SharedDirHandler`]：push 接收
//!   管线 + 浏览/拉取服务共用一条信任放行连接（首帧分流在 crate 内完成）；
//! - SAF 根经 [`SafSharedAccess`] 适配既有 `SafIo`（list_tree/open_stream，
//!   持久化授权 URI 重启有效）；app 私有下载目录作为免授权内置条目恒可用；
//! - 拉取落点：私有下载目录落盘后经 `MediaLanding` 尽力提升进
//!   MediaStore.Downloads——失败保留私有副本即天然回退（AC#5 单测覆盖）。
//!
//! issue 08 扩展（设备列表 + 发起连接，与桌面端同构）：
//! - 发现缓存变更推送：后台任务周期比对缓存快照指纹，变化时发全量列表事件
//!   `peer-devices-changed`——前端免轮询 IPC 即可随节点上下线自动刷新；
//! - `dial_peer` 命令：从发现缓存取记录 → mTLS 拨号 → 对端确认后进入已连接
//!   态。连接句柄存于状态容器保持会话存活（drop 即断开），重复拨号以最新为
//!   准替换旧句柄；
//! - `disconnect_peer` 命令与 `peer-connected` / `peer-disconnected` 事件维护
//!   前端已连接徽标。入站方向不登记——被连侧会话由 SharedDirHandler 自持，
//!   真实传输语义属 issue 09/10。
//!
//! issue 09 扩展（发送侧命令面支撑，与桌面端同构）：
//! - `runtime_snapshot` / `parse_node_id` / `app_data_dir` / `emit_json` 以
//!   pub(crate) 开放给 [`crate::peer_transfer`]：发送编排需要节点句柄（拨号）、
//!   发现缓存（对端名解析）、数据目录（历史落盘）与事件发射，均复用本模块
//!   既有装配，不复制状态。

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bedcode_peer_net::{
    CAP_FILE_TRANSFER, Connection, DiscoveryCache, DiscoveryConfig,
    DiscoveryAdvertiser, DiscoveredPeerRecord, NodeId, NodeIdentity, PeerNetError,
    PeerNetNode, PeerNetNodeConfig, RunningNode, SharedDirEntry, SharedDirHandler,
    SharedDirRoot, SharedSafAccess, SharedDirStore, StaticPeerRecord, TrustEvent, TrustStore,
    TransferConfig, TransferEvent,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::plugin::saf_io::SafIo;

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
/// 固定值便于排查；被占则回退 `:0` 由系统分配（mDNS 广播实际端口，不依赖此
/// 常量的确定性）。两端宿主须保持一致。
const DEFAULT_PEER_PORT: u16 = 47613;

/// SystemInfo 等待上限：setup 后台任务采集设备名（Android API 调用）可能晚于
/// 自动启动就绪；15s 后放弃等待以兜底名启动守护，避免无限期阻塞发现功能
const SYSTEM_INFO_WAIT_ATTEMPTS: u32 = 30;

/// 运行中节点的完整状态（命令面操作对象；`None` = 未启动）
struct PeerNetRuntime {
    /// 节点句柄（`dial_peer` 拨号入口；Clone 廉价——内部全是 Arc 共享）
    node: PeerNetNode,
    /// TCP 监听运行句柄（优雅关停入口）
    running: RunningNode,
    /// mDNS 发现守护句柄（优雅关停入口）
    daemon: DiscoveryAdvertiser,
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
#[derive(Default)]
pub struct PeerNetState {
    runtime: tokio::sync::Mutex<Option<PeerNetRuntime>>,
    trust: tokio::sync::Mutex<Option<Arc<TrustStore>>>,
    shared: tokio::sync::Mutex<Option<Arc<SharedDirStore>>>,
    consents: std::sync::Mutex<HashMap<String, PendingConsent>>,
    connections: std::sync::Mutex<HashMap<String, Connection>>,
}

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

/// 发现的对端条目（设备列表 DTO，issue 08；与桌面端同形；缓存内即在线态）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeerDto {
    /// 完整节点 ID（64 位小写 hex）
    pub node_id: String,
    /// 设备名（mDNS TXT 广播名）
    pub device_name: String,
    /// 对端监听地址
    pub addr: String,
    /// 通告协议版本
    pub protocol_version: u32,
    /// 原始能力位图（供未来能力位扩展展示）
    pub capabilities: u64,
    /// 是否具备文件传输能力（bit0；无此能力的节点可见但不可发起连接传输）
    pub file_transfer: bool,
}

impl From<&DiscoveredPeerRecord> for DiscoveredPeerDto {
    fn from(record: &DiscoveredPeerRecord) -> Self {
        Self {
            node_id: record.node_id.to_string(),
            device_name: record.device_name.clone(),
            addr: record.addr.to_string(),
            protocol_version: record.protocol_version,
            capabilities: record.capabilities,
            file_transfer: record.capabilities & CAP_FILE_TRANSFER != 0,
        }
    }
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
    let device_name = resolve_device_name().await;
    let state = app.state::<PeerNetState>();
    start_locked(&state, &data_dir, device_name, &app).await
}

/// 优雅关停对等网络节点并释放多播锁（幂等：未启动直接成功）
#[tauri::command]
pub async fn stop_peer_node(app: AppHandle) -> crate::Result<()> {
    let state = app.state::<PeerNetState>();
    stop_locked(&state, &app).await
}

/// 当前发现的对端列表（未启动返回空表）
#[tauri::command]
pub async fn list_discovered_peers(app: AppHandle) -> crate::Result<Vec<DiscoveredPeerDto>> {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.lock().await;
    let peers: Vec<DiscoveredPeerDto> = guard
        .as_ref()
        .map(|runtime| runtime.cache.list().iter().map(DiscoveredPeerDto::from).collect())
        .unwrap_or_default();
    // 诊断插桩：query-peer 是否被调用、缓存当时有几条（排查设备列表空可见性盲区）
    tracing::info!(count = peers.len(), started = guard.is_some(), "list_discovered_peers queried");
    Ok(peers)
}

/// 发起对等连接（issue 08）：从发现缓存取记录 → mTLS 拨号 → 对端确认后进入
/// 已连接态（与桌面端同构）
///
/// 确认发生在对端（首连弹 `peer-consent-requested` 弹窗），本机无需确认；
/// 成功后连接句柄登记进状态容器保持会话存活，并发 `peer-connected` 事件。
/// 重复拨号同一节点以新连接替换旧句柄（旧连接随即关闭）。
#[tauri::command]
pub async fn dial_peer(app: AppHandle, node_id: String) -> crate::Result<DialPeerResultDto> {
    let parsed = parse_node_id(&node_id)?;
    let state = app.state::<PeerNetState>();

    // 锁内只取快照与句柄：mTLS 握手可达秒级，不得跨 await 持锁阻塞 start/stop
    let (record, node) = {
        let guard = state.runtime.lock().await;
        let runtime = guard.as_ref().ok_or_else(|| {
            crate::AppError::Internal("peer-net dial failed: node not started".to_string())
        })?;
        match runtime.cache.get(&parsed) {
            Some(record) => (record, runtime.node.clone()),
            None => {
                return Err(crate::AppError::Internal(format!(
                    "peer-net dial failed: peer {node_id} not in discovery cache (offline or unknown)"
                )));
            }
        }
    };

    let device_name = record.device_name.clone();
    // 诊断插桩：拨号入口（出口三态已有日志，此处补发起时刻与目标可见性）
    tracing::info!(
        node_id = %parsed,
        short = %parsed.short_fingerprint(),
        peer = %record.addr,
        "peer dial requested"
    );
    match node.dial(&record.to_static_peer_record()).await {
        Ok(connection) => {
            // 替换语义：同节点重复拨号以最新句柄为准，旧连接 drop 即关闭
            let replaced = state
                .connections
                .lock()
                .expect("connections table lock poisoned")
                .insert(parsed.to_string(), connection);
            drop(replaced);
            tracing::info!(
                node_id = %parsed,
                short = %parsed.short_fingerprint(),
                "peer dialed and connected"
            );
            let result = DialPeerResultDto {
                status: "connected".to_string(),
                device_name: Some(device_name.clone()),
            };
            emit_json(
                &app,
                "peer-connected",
                serde_json::json!({ "nodeId": parsed.as_str(), "deviceName": device_name }),
            );
            Ok(result)
        }
        Err(PeerNetError::DialDeniedByPeer { .. }) => {
            tracing::info!(node_id = %parsed, "peer dial denied by remote");
            Ok(DialPeerResultDto { status: "denied".to_string(), device_name: Some(device_name) })
        }
        Err(e) => {
            tracing::warn!(node_id = %parsed, "peer dial unreachable: {e}");
            Ok(DialPeerResultDto { status: "unreachable".to_string(), device_name: Some(device_name) })
        }
    }
}

// ==================== endpoint 拨号（ADR 0022 v2）====================

/// endpoint 拨号入参：插件从自身设备缓存（mdns:found 事件派生）解析后显式传入
///
/// 宿主不再内藏 node-id → 地址解析表（ADR 0022 v2 裁决）：寻址来源由调用方持有，
/// 握手期「证书指纹 ↔ nodeId 绑定 + 信任检查」语义与 [`dial_peer`] 完全一致。
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
/// 与 [`dial_peer`] 唯一差异是寻址来源：不再要求目标在发现缓存中。缓存命中时
/// 复用其展示名；未命中则回退短指纹占位，并观察一条回退记录进缓存——过渡期
/// 桥接（数据面函数内部仍按 node-id 寻址且依赖缓存解析元数据，Phase 4 退役）。
pub async fn dial_peer_endpoint(
    app: AppHandle,
    endpoint: DialEndpoint,
) -> crate::Result<DialPeerResultDto> {
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
        peer = %addr,
        "peer dial requested via endpoint"
    );
    let state = app.state::<PeerNetState>();

    let (node, cached_record) = {
        let guard = state.runtime.lock().await;
        let runtime = guard.as_ref().ok_or_else(|| {
            crate::AppError::Internal("peer-net dial failed: node not started".to_string())
        })?;
        (runtime.node.clone(), runtime.cache.get(&parsed))
    };
    let cache_miss = cached_record.is_none();

    let device_name = cached_record
        .map(|r| r.device_name)
        .unwrap_or_else(|| format!("node-{}", parsed.short_fingerprint()));
    let static_record = StaticPeerRecord { node_id: parsed.clone(), addr };
    match node.dial(&static_record).await {
        Ok(connection) => {
            let replaced = state
                .connections
                .lock()
                .expect("connections table lock poisoned")
                .insert(parsed.to_string(), connection);
            drop(replaced);
            // 过渡期桥接：缓存未命中时补一条回退记录，让按 node-id 寻址的
            // 数据面函数可用（见函数文档，Phase 4 退役）
            if cache_miss {
                if let Some(runtime) = state.runtime.lock().await.as_ref() {
                    runtime.cache.observe(DiscoveredPeerRecord {
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
                serde_json::json!({ "nodeId": parsed.as_str(), "deviceName": device_name }),
            );
            Ok(DialPeerResultDto {
                status: "connected".to_string(),
                device_name: Some(device_name),
            })
        }
        Err(PeerNetError::DialDeniedByPeer { .. }) => {
            tracing::info!(node_id = %parsed, "peer dial denied by remote");
            Ok(DialPeerResultDto { status: "denied".to_string(), device_name: Some(device_name) })
        }
        Err(e) => {
            tracing::warn!(node_id = %parsed, "peer dial unreachable: {e}");
            Ok(DialPeerResultDto { status: "unreachable".to_string(), device_name: Some(device_name) })
        }
    }
}

/// 断开对等连接：丢弃持有的连接句柄（drop 即 TCP 关闭），返回是否存在。
/// 成功断开发 `peer-disconnected` 事件供前端摘除已连接徽标。
#[tauri::command]
pub async fn disconnect_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    let parsed = parse_node_id(&node_id)?;
    let state = app.state::<PeerNetState>();
    let removed = state
        .connections
        .lock()
        .expect("connections table lock poisoned")
        .remove(&parsed.to_string())
        .is_some();
    if removed {
        tracing::info!(node_id = %parsed, "peer connection dropped by user");
        emit_json(&app, "peer-disconnected", serde_json::json!({ "nodeId": parsed.as_str() }));
    }
    Ok(removed)
}

/// 应答首连确认弹窗（issue 04 命令面）
///
/// 接受路径先带展示名落库再回执：transport 随后的 `add` 变 no-op，保证连接
/// 建立时可信条目已带元数据（设置面立即可见名称）。返回是否成功送达回执——
/// false 表示请求已超时/已应答/ID 未知（前端应关闭对应弹窗）。
#[tauri::command]
pub async fn respond_peer_consent(
    app: AppHandle,
    request_id: String,
    accepted: bool,
) -> crate::Result<bool> {
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
        .map(|entry| TrustedPeerDto {
            display_name: entry
                .display_name
                .or_else(|| online_names.get(&entry.node_id).cloned()),
            node_id: entry.node_id.to_string(),
            fingerprint_short: entry.node_id.short_fingerprint().to_string(),
            added_at: entry.added_at.to_rfc3339(),
        })
        .collect())
}

/// 撤销可信对端（返回该 ID 原本是否存在；撤销后对端重连重新走首连确认）
#[tauri::command]
pub async fn revoke_trusted_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    let parsed = NodeId::parse(&node_id)
        .map_err(|e| crate::AppError::Internal(format!("invalid peer node id: {e}")))?;
    let trust = trust_handle(&app).await?;
    trust.remove(&parsed).map_err(map_peer_net_error)
}

/// setup 阶段自动启动入口（决策 D7）
///
/// 失败只记 error 日志不阻断应用其余功能：发现属增强能力，不应拖垮终端主链路；
/// 但日志必须醒目便于真机排查。内部经 spawn_with_error_boundary 包装防 panic
/// 静默（AGENTS.md 宿主后台任务规范）。
pub fn spawn_autostart(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        crate::system::error_boundary::spawn_with_error_boundary("peer_net_autostart", async move {
            let data_dir = match app_data_dir(&app) {
                Ok(dir) => dir,
                Err(e) => {
                    tracing::error!("peer-net autostart aborted: {e}");
                    return;
                }
            };
            let device_name = resolve_device_name().await;
            let state = app.state::<PeerNetState>();
            match start_locked(&state, &data_dir, device_name, &app).await {
                Ok(status) => tracing::info!(
                    "peer-net autostart ok: node={} addr={}",
                    status.node_id,
                    status.listen_addr
                ),
                Err(e) => tracing::error!("peer-net autostart failed: {e}"),
            }
        });
    });
}

// ==================== 内部装配 ====================

/// 持锁装配路径：start 命令与自动启动共用（调用方已持有状态互斥锁）
async fn start_locked(
    state: &tauri::State<'_, PeerNetState>,
    data_dir: &Path,
    device_name: String,
    app: &AppHandle,
) -> crate::Result<PeerNodeStatus> {
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

    // 身份与可信表同目录（与 auth 域身份文件并列）
    let identity = init_node_identity(data_dir)
        .map_err(|e| crate::AppError::Internal(format!("peer-net identity load failed: {e}")))?;
    let trust = Arc::new(TrustStore::load_or_create(data_dir).map_err(map_peer_net_error)?);
    *state.trust.lock().await = Some(Arc::clone(&trust));

    // 占位 bind 固定默认端口，被占回退 :0；占位句柄移交节点消除端口竞态
    // （Decision 4）。移动端 WiFi 接口地址由 DHCP 分配，绑 UNSPECIFIED 即可覆盖
    let listener =
        TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, DEFAULT_PEER_PORT)))
            .or_else(|_| TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))))
            .map_err(|e| {
                crate::AppError::Internal(format!("bind peer-net listener failed: {e}"))
            })?;
    let bind_addr = listener.local_addr().map_err(|e| {
        crate::AppError::Internal(format!("read peer-net listener addr failed: {e}"))
    })?;

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

    // 共享目录注册表 + SAF 缝 + 接收落点（issue 07）
    // 免授权内置条目：app 私有下载目录（解析失败仅跳过内置条目，不阻断启动）
    let downloads_dir =
        crate::plugin::android_plugins::resolve_app_downloads_dir(app).await.map(PathBuf::from);
    if let Some(dir) = &downloads_dir {
        if let Err(e) = tokio::fs::create_dir_all(dir).await {
            tracing::error!(dir = %dir.display(), "create peer download dir failed: {e}");
        }
    }
    let shared_store = shared_handle_at(data_dir).await?;
    let shared_store = match &downloads_dir {
        Some(dir) => shared_store.with_builtin_download_dir("下载", dir.clone()),
        None => {
            tracing::warn!("app downloads dir unavailable, builtin shared entry omitted");
            shared_store
        }
    };
    let shared = Arc::new(shared_store);
    *state.shared.lock().await = Some(Arc::clone(&shared));

    // SAF 缝：Android 用 KotlinSafIo 适配；其余平台无 SAF 根可服务（注册表
    // 若存在 saf 条目会按 read-failed 拒绝——dev 窗口无此形态，防御性兜底）
    #[cfg(target_os = "android")]
    let saf_access: Option<Arc<dyn SharedSafAccess>> =
        Some(Arc::new(SafSharedAccess::new(crate::plugin::saf_io::default_saf_io())));
    #[cfg(not(target_os = "android"))]
    let saf_access: Option<Arc<dyn SharedSafAccess>> = None;

    // 受信连接统一走共享目录复合处理器；引擎事件由接收侧模块消费（issue 10：
    // 询问弹窗/任务登记/进度推送）。拉取/接收落位后经 MediaLanding 尽力提升
    // 进公共下载（失败保留私有副本）
    let landing: Option<Arc<dyn bedcode_peer_net::FileLanding>> = downloads_dir
        .as_ref()
        .map(|_| {
            let io = crate::plugin::saf_io::default_saf_io();
            Arc::new(MediaLanding { saf: io }) as Arc<dyn bedcode_peer_net::FileLanding>
        });
    let (transfer_tx, transfer_rx) = tokio::sync::mpsc::channel::<TransferEvent>(256);
    let config = TransferConfig {
        policy: bedcode_peer_net::ReceivePolicy::Ask {
            timeout: Duration::from_secs(60),
        },
        download_dir: downloads_dir
            .clone()
            .unwrap_or_else(|| data_dir.join("downloads")),
        chunk_size: 64 * 1024,
        landing,
    };
    let handler = Arc::new(SharedDirHandler::new(shared, saf_access, config.clone(), transfer_tx));
    // 接收侧登记句柄与配置快照（策略热更新/按批取消入口），并按持久化
    // 设置纠正首份策略
    super::peer_receive::register_handler(app, Arc::clone(&handler), config).await;
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_net_transfer_events",
        super::peer_receive::drive_receive_events(app.clone(), transfer_rx),
    );
    // 远端浏览/拉取会话上下文（issue 11）：事件通道发送端快照供拉取入账任务表
    super::peer_remote::register_session(app, handler.event_sender()).await;

    let running = node
        .start_with_listener(listener, events_tx, handler)
        .map_err(map_peer_net_error)?;
    let listen_addr = running.local_addr();

    // Android：申请多播锁后再启动发现守护（D6）。失败只 warn 不阻断——广播不受
    // 锁影响，缺锁仅退化收包（单侧可见），真机冒烟时按此线索排查
    #[cfg(target_os = "android")]
    match crate::plugin::android_plugins::multicast_lock_acquire().await {
        Ok(held) => tracing::info!("multicast lock held={held} before discovery daemon"),
        Err(e) => tracing::warn!(
            "multicast lock acquire failed ({e}); mDNS receive may be degraded (one-way visibility)"
        ),
    }

    let daemon = bedcode_peer_net::spawn_peer_mdns_advertiser(
        &node,
        &running,
        DiscoveryConfig {
            device_name,
            capabilities: CAP_FILE_TRANSFER,
        },
    )
    .map_err(map_peer_net_error)?;

    let node_id = node.node_id().to_string();
    *state.runtime.lock().await = Some(PeerNetRuntime {
        node,
        running,
        daemon,
        cache,
        node_id,
    });

    tracing::info!(
        "peer-net node started: addr={listen_addr}, discovery service={}",
        bedcode_peer_net::SERVICE_TYPE
    );
    Ok(PeerNodeStatus {
        started: true,
        node_id: state.runtime.lock().await.as_ref().expect("just stored").node_id.clone(),
        listen_addr: listen_addr.to_string(),
    })
}

/// 持锁关停路径：先停发现守护（注销广播让对端即时移除本机），再关监听、放锁
///
/// 可信列表句柄刻意保留（`trust` 槽位不清空）：节点停止后设置面仍可查看/撤销。
async fn stop_locked(state: &tauri::State<'_, PeerNetState>, app: &AppHandle) -> crate::Result<()> {
    // 主动拨号的存活连接随关停一并丢弃（drop 即 TCP 关闭），并逐个通知前端
    // 摘除已连接徽标——连接表随节点生命周期走，重启后从空表开始
    let drained: Vec<String> = state
        .connections
        .lock()
        .expect("connections table lock poisoned")
        .drain()
        .map(|(node_id, _)| node_id)
        .collect();
    for node_id in drained {
        emit_json(app, "peer-disconnected", serde_json::json!({ "nodeId": node_id }));
    }
    let runtime = state.runtime.lock().await.take();
    match runtime {
        Some(runtime) => {
            // 接收侧句柄随节点下线摘除（设置命令此后仅改持久化，下次启动生效）；
            // 远端拉取队列同步中止（issue 11）
            super::peer_receive::clear_handler(app).await;
            super::peer_remote::clear_state(app).await;
            runtime.daemon.stop().await.map_err(map_peer_net_error)?;
            runtime.running.shutdown().await;
            // 锁在守护完全停止后释放：保证停止过程中仍可收对端的 goodbye 确认
            #[cfg(target_os = "android")]
            match crate::plugin::android_plugins::multicast_lock_release().await {
                Ok(_) => tracing::info!("multicast lock released after discovery stopped"),
                Err(e) => tracing::warn!("multicast lock release failed: {e}"),
            }
            tracing::info!("peer-net node stopped");
            Ok(())
        }
        None => Ok(()),
    }
}

/// 设备名解析：SystemInfo 由 setup 后台任务异步采集，此处轮询等待其就绪
///
/// 用 tokio sleep 而非阻塞 sleep：自动启动任务运行在共享执行器上，阻塞会拖累
/// 其他任务。超时兜底固定名保证守护总能启动（广播名仅影响对端展示）。
async fn resolve_device_name() -> String {
    for attempt in 0..SYSTEM_INFO_WAIT_ATTEMPTS {
        if let Some(info) = crate::state::try_get_system_info() {
            let name = info.device_name.clone();
            if !name.is_empty() {
                if attempt > 0 {
                    tracing::debug!("device name resolved after {attempt} wait cycles");
                }
                return name;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tracing::warn!(
        "SystemInfo not ready after {}ms, using fallback device name for peer discovery",
        SYSTEM_INFO_WAIT_ATTEMPTS * 500
    );
    "BedCode Mobile".to_string()
}

/// 首连确认事件消费：桥接到前端应用内确认弹窗（issue 04）
///
/// 收到 `ConfirmRequested` 后：解析发现缓存中的设备名 → 登记回执通道 → 发
/// `peer-consent-requested` 事件。应答经 `respond_peer_consent` 命令回流；
/// 30s 内无应答由 crate 闸门按拒绝结算（`CONFIRM_TIMEOUT`），弹窗超时语义
/// 与之天然对齐（前端按同值倒计时收起）。
async fn drive_gate(
    mut events: tokio::sync::mpsc::Receiver<TrustEvent>,
    cache: Arc<DiscoveryCache>,
    app: AppHandle,
) {
    while let Some(event) = events.recv().await {
        match event {
            TrustEvent::ConfirmRequested { node_id, reply } => {
                // 设备名取自发现缓存：拨入方与本机同网互见，正常必有记录；
                // 缺失时前端以短指纹兜底展示
                let device_name = cache.get(&node_id).map(|record| record.device_name);
                let request_id = uuid::Uuid::new_v4().to_string();
                {
                    let state = app.state::<PeerNetState>();
                    let mut pending = state
                        .consents
                        .lock()
                        .expect("consent table lock poisoned");
                    pending.insert(
                        request_id.clone(),
                        PendingConsent { node_id: node_id.clone(), reply },
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

// ==================== 共享目录（issue 07）====================

/// 共享目录条目（设置面管理列表 DTO；与桌面端同形）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedDirDto {
    /// 条目 ID（浏览/拉取请求按此寻址）
    pub id: String,
    /// 展示名
    pub name: String,
    /// 根形态：`fs` | `saf`
    pub kind: String,
    /// Fs 根绝对路径（kind=fs 时存在）
    pub path: Option<String>,
    /// SAF 树 URI（kind=saf 时存在）
    pub tree_uri: Option<String>,
    /// 是否免授权内置条目（私有下载目录；不可移除）
    pub builtin: bool,
}

impl From<&SharedDirEntry> for SharedDirDto {
    fn from(entry: &SharedDirEntry) -> Self {
        let (kind, path, tree_uri) = match &entry.root {
            SharedDirRoot::Fs { path } => (
                "fs",
                Some(path.to_string_lossy().into_owned()),
                None,
            ),
            SharedDirRoot::Saf { tree_uri } => ("saf", None, Some(tree_uri.clone())),
        };
        Self {
            builtin: entry.id == bedcode_peer_net::BUILTIN_DOWNLOADS_ID,
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: kind.to_string(),
            path,
            tree_uri,
        }
    }
}

/// 共享目录列表（节点未启动仍可读——句柄独立于运行时存活）
#[tauri::command]
pub async fn list_shared_directories(app: AppHandle) -> crate::Result<Vec<SharedDirDto>> {
    let store = shared_handle(&app).await?;
    Ok(store.list().iter().map(SharedDirDto::from).collect())
}

/// 新增共享目录——SAF 形态：弹出系统目录选择器，取回的树 URI 经
/// takePersistableUriPermission 持久授权（Kotlin SafPickerPlugin 承载），
/// 重启后仍有效。用户取消返回 `Ok(None)`。
#[tauri::command]
pub async fn add_shared_directory_saf(app: AppHandle) -> crate::Result<Option<SharedDirDto>> {
    let Some((tree_uri, display_name, _doc_id)) =
        crate::plugin::android_plugins::pick_shared_directory_android().await?
    else {
        return Ok(None);
    };
    let store = shared_handle(&app).await?;
    let entry = store
        .add(display_name, SharedDirRoot::Saf { tree_uri })
        .map_err(map_peer_net_error)?;
    tracing::info!(dir_id = %entry.id, tree_uri = ?entry.root, "shared saf directory added");
    Ok(Some(SharedDirDto::from(&entry)))
}

/// 移除共享目录（返回该 ID 原本是否存在；内置条目不可移除恒 false）
///
/// 注：SAF 条目移除仅摘除暴露面，系统侧持久授权的主动回收属后续票。
#[tauri::command]
pub async fn remove_shared_directory(app: AppHandle, id: String) -> crate::Result<bool> {
    let store = shared_handle(&app).await?;
    store.remove(&id).map_err(map_peer_net_error)
}

/// 全量幂等替换引擎广播源（host-peer `set-shared-roots` 原语的引擎入口，
/// ADR 0022 v2）：注册表 CRUD 真源已移插件侧，本函数只同步暴露面镜像；
/// 条目 id/name/root 由调用方构造（移动 SAF 树以 SharedDirRoot::Saf 承载）。
pub async fn set_shared_roots(app: AppHandle, entries: Vec<SharedDirEntry>) -> crate::Result<()> {
    let store = shared_handle(&app).await?;
    store.replace_all(&entries).map_err(map_peer_net_error)
}

/// 共享目录注册表句柄：未加载时惰性从磁盘加载（管理面在节点从未启动时也可用）
async fn shared_handle(app: &AppHandle) -> crate::Result<Arc<SharedDirStore>> {
    let state = app.state::<PeerNetState>();
    let mut guard = state.shared.lock().await;
    if let Some(store) = guard.as_ref() {
        return Ok(Arc::clone(store));
    }
    let data_dir = app_data_dir(app)?;
    let store = Arc::new(shared_handle_at(&data_dir).await?);
    *guard = Some(Arc::clone(&store));
    Ok(store)
}

/// 按数据目录加载共享目录注册表（start 路径与惰性路径共用；返回值供调用方
/// 注入内置条目后再包 `Arc`）
async fn shared_handle_at(data_dir: &Path) -> crate::Result<SharedDirStore> {
    // crate 的 load_or_create 为同步阻塞 IO：移出异步上下文
    let dir = data_dir.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || SharedDirStore::load_or_create(&dir))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join shared dirs load failed: {e}")))?
        .map_err(map_peer_net_error)
}

// ==================== SAF 缝适配（SafIo → crate SharedSafAccess）====================
#[cfg(target_os = "android")]
mod android_saf_bridge {
use super::*;
use crate::file_service::saf_tree;
use crate::plugin::saf_io::SafEntry;
use bedcode_peer_net::SeqReader;

/// SAF 共享目录访问适配器：把既有 [`SafIo`]（Kotlin 桥）映射到 crate 的
/// [`SharedSafAccess`] 缝。相对路径解析用 list_tree 逐层下降（与
/// file_service/saf_tree 的 walk 同构，此处同步化以匹配 trait 签名）。
struct SafSharedAccess {
    io: Arc<dyn SafIo>,
}

impl SafSharedAccess {
    fn new(io: Arc<dyn SafIo>) -> Self {
        Self { io }
    }

    /// 同步逐层下降到 rel 指向的条目（parts 已过 crate 路径清洗）
    fn walk(
        &self,
        tree_uri: &str,
        root_doc_id: &str,
        parts: &[String],
    ) -> bedcode_peer_net::Result<SafEntry> {
        use bedcode_peer_net::{saf_not_found, PeerNetError};

        if parts.is_empty() {
            return Ok(SafEntry {
                name: saf_tree::tree_alias(tree_uri).unwrap_or_else(|| tree_uri.to_string()),
                is_dir: true,
                size: 0,
                mime: String::new(),
                uri: tree_uri.to_string(),
                document_id: root_doc_id.to_string(),
            });
        }
        let mut doc_id = root_doc_id.to_string();
        let mut current = self.root_entry(tree_uri, root_doc_id);
        for part in parts {
            let children = self.io.list_tree(tree_uri, &doc_id).map_err(|e| {
                tracing::warn!(tree_uri, doc_id = %doc_id, error = %e, "saf walk list_tree failed");
                PeerNetError::TransferSession {
                    role: "saf",
                    detail: format!("list_tree failed (permission may be revoked): {e}"),
                }
            })?;
            let child = children
                .into_iter()
                .find(|c| &c.name == part)
                .ok_or_else(|| saf_not_found(format!("'{part}' in tree {tree_uri}")))?;
            doc_id = child.document_id.clone();
            current = child;
        }
        Ok(current)
    }

    fn root_entry(&self, tree_uri: &str, root_doc_id: &str) -> SafEntry {
        SafEntry {
            name: saf_tree::tree_alias(tree_uri).unwrap_or_else(|| tree_uri.to_string()),
            is_dir: true,
            size: 0,
            mime: String::new(),
            uri: tree_uri.to_string(),
            document_id: root_doc_id.to_string(),
        }
    }
}

impl SharedSafAccess for SafSharedAccess {
    fn list_dir(
        &self,
        tree_uri: &str,
        rel: &str,
    ) -> bedcode_peer_net::Result<Vec<bedcode_peer_net::DirEntry>> {
        use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

        let root_doc_id = saf_tree::tree_document_id(tree_uri)
            .ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
        let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
        let target = self.walk(tree_uri, &root_doc_id, &parts)?;
        if !target.is_dir {
            return Err(saf_not_found(format!("'{rel}' is not a directory")));
        }
        let children = self.io.list_tree(tree_uri, &target.document_id).map_err(|e| {
            PeerNetError::TransferSession {
                role: "saf",
                detail: format!("list_tree failed (permission may be revoked): {e}"),
            }
        })?;
        Ok(children
            .into_iter()
            .map(|c| bedcode_peer_net::DirEntry::new(c.name, c.is_dir, c.size.max(0) as u64))
            .collect())
    }

    fn open_read(&self, tree_uri: &str, rel: &str) -> bedcode_peer_net::Result<Arc<dyn SeqReader>> {
        use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

        let root_doc_id = saf_tree::tree_document_id(tree_uri)
            .ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
        let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
        let entry = self.walk(tree_uri, &root_doc_id, &parts)?;
        if entry.is_dir {
            return Err(saf_not_found(format!("'{rel}' is a directory")));
        }
        let handle = self.io.open_stream(&entry.uri, 0).map_err(|e| {
            PeerNetError::TransferSession {
                role: "saf",
                detail: format!("open stream for '{rel}' failed: {e}"),
            }
        })?;
        // 大小真源：条目元数据优先（OpenableColumns.SIZE），缺失回退流探测值
        let size = if entry.size > 0 {
            entry.size as u64
        } else {
            handle.size
        };
        Ok(Arc::new(SafSeqReader {
            io: Arc::clone(&self.io),
            uri: entry.uri,
            handle_id: handle.handle_id,
            size,
        }))
    }
}

/// SAF 流顺序读取器：定位读 = seek_stream + read_stream（Kotlin 句柄复用），
/// Drop 时尽力关闭底层 fd
struct SafSeqReader {
    io: Arc<dyn SafIo>,
    uri: String,
    handle_id: String,
    size: u64,
}

impl SeqReader for SafSeqReader {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, offset: u64, cap: usize) -> std::io::Result<Vec<u8>> {
        self.io.seek_stream(&self.handle_id, offset).map_err(io_err)?;
        self.io.read_stream(&self.handle_id, cap).map_err(io_err)
    }
}

impl Drop for SafSeqReader {
    fn drop(&mut self) {
        if let Err(e) = self.io.close_stream(&self.handle_id) {
            tracing::debug!(handle_id = %self.handle_id, uri = %self.uri, "close saf stream failed: {e}");
        }
    }
}

fn io_err(e: crate::AppError) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
} // mod android_saf_bridge

// ==================== 接收落点提升（MediaStore 回退语义）====================

/// 落地钩子：私有下载目录中的完整文件尽力写入 MediaStore.Downloads 公共下载
///
/// 成功 → 删除私有副本（公共目录为唯一副本，对齐 legacy upload 语义）；
/// 失败 → 保留私有副本（即「回退私有目录」，AC#5 的单测对象）。
/// 阻塞桥接由 KotlinSafIo 内部承担（scoped-thread 惯例）。
struct MediaLanding {
    saf: Arc<dyn SafIo>,
}

impl bedcode_peer_net::FileLanding for MediaLanding {
    fn landed(&self, final_path: &Path, display_name: &str) {
        let src = final_path.to_string_lossy().into_owned();
        match self
            .saf
            .write_media_downloads(&src, display_name, mime_type_for(display_name))
        {
            Ok(()) => {
                match std::fs::remove_file(final_path) {
                    Ok(()) => tracing::info!(
                        display_name = %display_name,
                        "pulled file promoted to MediaStore.Downloads (private copy removed)"
                    ),
                    Err(e) => tracing::warn!(
                        path = %final_path.display(),
                        error = %e,
                        "media promote ok but private copy cleanup failed"
                    ),
                }
            }
            Err(e) => {
                tracing::info!(
                    path = %final_path.display(),
                    error = %e,
                    "MediaStore write failed, keeping private copy (fallback)"
                );
            }
        }
    }
}

/// 展示名 → MIME（轻量扩展名映射；未知走 octet-stream）
fn mime_type_for(name: &str) -> &'static str {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "txt" | "md" => "text/plain",
        "json" => "application/json",
        "apk" => "application/vnd.android.package-archive",
        _ => "application/octet-stream",
    }
}

/// PeerNetError → AppError 映射：crate 错误链保留完整文案（本票宿主侧只打日志不细分）
pub(crate) fn map_peer_net_error(e: PeerNetError) -> crate::AppError {
    crate::AppError::Internal(format!("peer-net operation failed: {e}"))
}

/// 节点 ID hex 字符串解析（issue 08 命令面共用）：统一错误上下文
pub(crate) fn parse_node_id(node_id: &str) -> crate::Result<NodeId> {
    NodeId::parse(node_id).map_err(|e| {
        crate::AppError::Internal(format!("peer-net invalid node id '{node_id}': {e}"))
    })
}

/// 向前端发 JSON 载荷事件（失败只记日志不上抛：窗口缺失/前端未就绪属预期场景）。
/// 同步桥接到插件消息总线 `peer:*` topic（issue 12 切换后 file-transfer 插件
/// 经 host-peer/总线感知对等状态），插件管理器未就绪时静默跳过
pub(crate) fn emit_json(app: &AppHandle, event: &str, payload: serde_json::Value) {
    if let Err(e) = app.emit(event, payload.clone()) {
        tracing::error!("emit {event} failed: {e}");
    }
    let Some(topic) = bus_topic_for(event) else {
        return;
    };
    // 诊断插桩：peer 事件推送可见性（移动端 logcat 只落 INFO+，publish 的 DEBUG 日志不可见）
    tracing::info!(event, topic, "peer event pushed to plugin bus");
    if let Some(pm) = crate::state::try_get_plugin_manager() {
        pm.message_bus().publish(topic, "host", payload);
    }
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

/// 运行时句柄快照（issue 09 发送编排用）：节点句柄 + 发现缓存
///
/// 锁内仅 Clone 廉价句柄，不跨 await 持锁；节点未启动返回 None。
pub(crate) async fn runtime_snapshot(
    app: &AppHandle,
) -> Option<(PeerNetNode, Arc<DiscoveryCache>)> {
    let state = app.state::<PeerNetState>();
    let guard = state.runtime.lock().await;
    guard.as_ref().map(|r| (r.node.clone(), r.cache.clone()))
}

/// 发现缓存变更推送（issue 08）：周期比对缓存快照指纹，变化才发全量列表事件
///
/// 宿主侧比对而非给 crate 的 DiscoveryCache 加回调：保持共享 crate 接口最小，
/// LAN 规模下 list + 序列化为微秒级、轮询成本趋零。节点停止后补发一次空列表

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

/// 解析 app 数据目录（身份/可信列表所在，与 auth 身份文件同源）
pub(crate) fn app_data_dir(app: &AppHandle) -> crate::Result<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Internal(format!("resolve app data dir failed: {e}")))
}

// ==================== Tests（issue 07：MediaStore 回退语义 AC#5）====================

#[cfg(test)]
mod share_landing_tests {
    use super::*;
    use crate::plugin::saf_io::{SafCopyHandle, SafCopyStatus, SafEntry, SafStreamHandle};
    // trait 方法调用需要 trait 在作用域内
    use bedcode_peer_net::FileLanding as _;

    /// fake SafIo：write_media_downloads 结果可编程（None = 成功），其余方法
    /// 不可达（headless 惯例，镜像 plugin/commands.rs 的 fake 注入模式）
    struct FakeSaf {
        fail_with: Option<String>,
    }

    impl SafIo for FakeSaf {
        fn list_tree(&self, _t: &str, _d: &str) -> crate::Result<Vec<SafEntry>> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn read_to_cache(&self, _u: &str, _n: &str) -> crate::Result<SafCopyHandle> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn copy_status(&self, _c: &str) -> crate::Result<SafCopyStatus> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn cancel_copy(&self, _c: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn cleanup_stale_copies(&self) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn check_authorized(&self, _t: &str) -> crate::Result<bool> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn write_media_downloads(
            &self,
            _src: &str,
            _name: &str,
            _mime: &str,
        ) -> crate::Result<()> {
            match &self.fail_with {
                Some(reason) => Err(crate::AppError::Plugin(reason.clone())),
                None => Ok(()),
            }
        }
        fn open_stream(&self, _u: &str, _o: u64) -> crate::Result<SafStreamHandle> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn read_stream(&self, _h: &str, _l: usize) -> crate::Result<Vec<u8>> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn seek_stream(&self, _h: &str, _o: u64) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn close_stream(&self, _h: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn stream_seekable(&self, _u: &str) -> crate::Result<bool> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
        fn save_to_document(&self, _s: &str, _n: &str, _m: &str) -> crate::Result<()> {
            Err(crate::AppError::Plugin("unreachable".into()))
        }
    }

    fn make_private_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"pulled-bytes").expect("write private file");
        path
    }

    /// MediaStore 写入成功 → 私有副本删除（公共目录为唯一副本）
    #[test]
    fn media_promote_success_removes_private_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = make_private_file(dir.path(), "report.pdf");

        let landing = MediaLanding {
            saf: Arc::new(FakeSaf { fail_with: None }),
        };
        landing.landed(&file, "report.pdf");

        assert!(
            !file.exists(),
            "private copy must be removed after successful MediaStore promotion"
        );
    }

    /// MediaStore 写入失败（如 API<29 / 存储不可用）→ 私有副本保留 = 回退语义
    #[test]
    fn media_promote_failure_keeps_private_copy_fallback() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = make_private_file(dir.path(), "clip.mp4");

        let landing = MediaLanding {
            saf: Arc::new(FakeSaf {
                fail_with: Some("media_store unavailable (api < 29)".to_string()),
            }),
        };
        landing.landed(&file, "clip.mp4");

        assert!(
            file.exists(),
            "fallback semantics: private copy must survive failed promotion"
        );
        assert_eq!(std::fs::read(&file).expect("intact"), b"pulled-bytes");
    }

    #[test]
    fn mime_mapping_covers_common_types_with_octet_stream_fallback() {
        assert_eq!(mime_type_for("photo.JPG"), "image/jpeg");
        assert_eq!(mime_type_for("video.mp4"), "video/mp4");
        assert_eq!(mime_type_for("notes.txt"), "text/plain");
        assert_eq!(mime_type_for("app.apk"), "application/vnd.android.package-archive");
        assert_eq!(mime_type_for("noext"), "application/octet-stream");
    }
}
