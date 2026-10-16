//! 对等网络接入（宿主适配层门面）：装配共享 peer 能力域 crate，宿主只做端口 / 平台缝注入。
//!
//! ## 口径（2026-10-10 能力域集中化；2026-10-14 收拢进 `src/peer/` 模块）
//!
//! 引擎控制面（节点装配 / 生命周期 / 连接 / 首连确认 / 可信表 / 共享目录 /
//! 发送-接收-远端控制面 / 引擎事件翻译）真源在仓库根
//! `packages/bedcode-server-peer-net`（纯引擎形态 `default = []`，双端同引，
//! spec `.scratch/2026-10-10-peer-capability-consolidation/`）；`src/peer/` 模块
//! 只保留宿主侧三件事：**(a) 端口实现**（`peer::ports`：`PeerEnginePorts` 五端口
//! bus / event_sink / paths / system_info / mdns，trait 词汇复用
//! `bedcode-server-base::ports` + 平台资源预热）**与平台缝**（`peer::seam`：
//! `PeerPlatformSeam` — SAF 共享访问 / 落点提升 / 内置共享条目）；**(b) 装配**
//! （本文件 `peer_ctx`：托管四状态 + 端口 + 平台缝）；**(c) 产品语义薄壳**
//! （插件激活外壳 `FILE_TRANSFER_PLUGIN_ID` / 节点生命周期与引擎入口委托）。
//!
//! mDNS 复用：`MdnsPort`（`peer::ports`）直接接 `bedcode-discovery-engine` 的
//! 共享守护与类型中枢出口（`shared_daemon` / `register_host_service` /
//! `stop_host_service` + `subscribe_type` / `request_type_requery`）——宿主零第二份
//! mDNS 代码；浏览面经中枢（ADR 0054）：全进程同服务类型只允许一条 `daemon.browse`，
//! peer-net 与插件 browse 订阅同一 fan-out。Android 多播锁随共享守护常驻（setup 的
//! init hook，见 `lib.rs`）。
//!
//! 发现刷新订阅（`peer:discovery-refresh`）由能力域自持（
//! `request_discovery_refresh`）；就绪判据经 `BusPort::is_ready` 下移到
//! `peer::ports`（本端 = 插件管理器就绪），能力域不再假设 `ServerPorts`
//! 全局注册表存在。

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use bedcode_peer_net::SharedDirEntry;
use bedcode_server_peer_net::engine_ports::PeerEnginePorts;

use super::ports::{
    prewarm_platform, MobileBusPort, MobileEventSink, MobileMdnsPort, MobilePathsPort, MobileSystemInfoPort,
};
use super::seam::MobilePlatformSeam;

// ==================== 能力域再导出（宿主其余模块只认本模块路径） ====================

pub use bedcode_server_peer_net::init_node_identity;
pub use bedcode_server_peer_net::peer_engine_receive::PeerReceiveState;
pub use bedcode_server_peer_net::peer_engine_remote::PeerRemoteState;
pub use bedcode_server_peer_net::peer_engine_transfer::PeerTransferState;
pub use bedcode_server_peer_net::{
    DialEndpoint, DialPeerResultDto, PeerCtx, PeerNetState, PeerNodeStatus, TrustedPeerDto,
};

// ==================== 常量 ====================

/// 文件传输插件 id：peer-net 节点的唯一消费方（全仓唯一声明 `peer` 权限的插件），
/// 节点生命周期随其启用状态（产品语义留宿主，能力域只按 caller 记账）
pub const FILE_TRANSFER_PLUGIN_ID: &str = "com.bedcode.file-transfer";

// ==================== 装配（PeerCtx） ====================

/// 从 `AppHandle` 装配对等网络引擎上下文（端口 + 平台缝 + 四个托管状态句柄）。
///
/// 四状态由 setup 以 `Arc::new(...)` 托管（与桌面同形：能力域持 `Arc` 句柄，
/// 进程内单实例）。
pub(crate) fn peer_ctx(app: &AppHandle) -> Arc<PeerCtx> {
    let state = app.state::<Arc<PeerNetState>>().inner().clone();
    let transfer = app.state::<Arc<PeerTransferState>>().inner().clone();
    let receive = app.state::<Arc<PeerReceiveState>>().inner().clone();
    let remote = app.state::<Arc<PeerRemoteState>>().inner().clone();

    let ports = Arc::new(PeerEnginePorts {
        bus: Arc::new(MobileBusPort),
        event_sink: Arc::new(MobileEventSink { app: app.clone() }),
        paths: Arc::new(MobilePathsPort { app: app.clone() }),
        system_info: Arc::new(MobileSystemInfoPort),
        mdns: Arc::new(MobileMdnsPort),
        platform: Some(Arc::new(MobilePlatformSeam)),
    });

    Arc::new(PeerCtx {
        ports,
        state,
        transfer,
        receive,
        remote,
    })
}

// ==================== 生命周期（宿主薄壳：插件启停驱动 + 平台预热） ====================

/// 当前节点属主（None = 未跑）
pub fn node_owner(app: &AppHandle) -> Option<String> {
    bedcode_server_peer_net::node_owner(&peer_ctx(app))
}

/// 插件按需启动本机节点（host-peer `start-node` 引擎入口；幂等）；先预热平台资源
pub async fn start_node_owned(app: &AppHandle, caller: &str) -> crate::Result<bool> {
    prewarm_platform(app).await;
    bedcode_server_peer_net::start_node_owned(peer_ctx(app), caller).await
}

/// 属主插件让节点下线（host-peer `stop-node` 引擎入口；非属主拒绝）
pub async fn stop_node_owned(app: &AppHandle, caller: &str) -> crate::Result<bool> {
    bedcode_server_peer_net::stop_node_owned(peer_ctx(app), caller).await
}

/// 内核侧按属主清理节点（插件停用/激活失败时调用；不匹配 no-op）
pub async fn release_node_for(app: &AppHandle, former_owner: &str) -> crate::Result<bool> {
    bedcode_server_peer_net::release_node_for(peer_ctx(app), former_owner).await
}

/// 文件传输插件激活 → 确保节点运行（幂等；boot 对账与运行时启用共用漏斗）
pub async fn ensure_node_started(app: &AppHandle) -> crate::Result<()> {
    start_node_owned(app, FILE_TRANSFER_PLUGIN_ID).await.map(|_| ())
}

/// 文件传输插件停用 → 服务下线（幂等：节点未跑 / 非本插件所起为 no-op）
pub async fn stop_node_for_plugin(app: &AppHandle) -> crate::Result<()> {
    if node_owner(app).as_deref() != Some(FILE_TRANSFER_PLUGIN_ID) {
        return Ok(());
    }
    stop_node_owned(app, FILE_TRANSFER_PLUGIN_ID).await.map(|_| ())
}

/// 文件传输插件当前是否已激活（插件管理器未就绪视为未激活）
async fn plugin_transfer_activated() -> bool {
    match crate::state::try_get_plugin_manager() {
        Some(pm) => pm.is_activated(FILE_TRANSFER_PLUGIN_ID).await,
        None => false,
    }
}

/// peer-net 节点与文件传输插件运行状态对齐（状态驱动对账，幂等）：
/// 插件运行 mDNS 就运行，插件不运行 mDNS 也不运行
pub async fn sync_node_with_plugin_state(app: &AppHandle) -> crate::Result<bool> {
    let activated = plugin_transfer_activated().await;
    if activated {
        ensure_node_started(app).await?;
    } else {
        stop_node_for_plugin(app).await?;
    }
    Ok(activated)
}

// ==================== 引擎入口委托 ====================

/// 本机节点 ID（host-mdns 自播回显过滤用；节点未启动/运行时锁忙返回 None）
pub(crate) fn current_node_id(app: &AppHandle) -> Option<String> {
    bedcode_server_peer_net::current_node_id(&peer_ctx(app))
}

/// 按 endpoint 拨号连接（host-peer `dial-peer` 原语的引擎入口）
pub async fn dial_peer_endpoint(app: AppHandle, endpoint: DialEndpoint) -> crate::Result<DialPeerResultDto> {
    bedcode_server_peer_net::dial_peer_endpoint(peer_ctx(&app), endpoint).await
}

/// 断开对等连接（host-peer `close` 引擎入口；返回是否存在出站会话）
pub async fn disconnect_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::disconnect_peer(peer_ctx(&app), node_id).await
}

/// 应答首连确认弹窗（host-peer `respond-consent` 引擎入口）
pub async fn respond_peer_consent(app: AppHandle, request_id: String, accepted: bool) -> crate::Result<bool> {
    bedcode_server_peer_net::respond_peer_consent(peer_ctx(&app), request_id, accepted).await
}

/// 可信对端列表（host-peer `list-trusted` 引擎入口；节点未启动仍可读）
pub async fn list_trusted_peers(app: AppHandle) -> crate::Result<Vec<TrustedPeerDto>> {
    bedcode_server_peer_net::list_trusted_peers(peer_ctx(&app)).await
}

/// 撤销可信对端（host-peer `revoke-trusted` 引擎入口）
pub async fn revoke_trusted_peer(app: AppHandle, node_id: String) -> crate::Result<bool> {
    bedcode_server_peer_net::revoke_trusted_peer(peer_ctx(&app), node_id).await
}

/// 全量幂等替换引擎广播源（host-peer `set-shared-roots` 引擎入口）
pub async fn set_shared_roots(app: AppHandle, entries: Vec<SharedDirEntry>) -> crate::Result<()> {
    bedcode_server_peer_net::set_shared_roots(peer_ctx(&app), entries).await
}

// ==================== 路径（宿主薄壳） ====================

/// 解析 app 数据目录（身份/可信列表所在，与 auth 身份文件同源）
pub(crate) fn app_data_dir(app: &AppHandle) -> crate::Result<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| crate::AppError::Internal(format!("resolve app data dir failed: {e}")))
}
