//! 对等网络接入（宿主适配层）：装配共享 peer 能力域 crate，宿主只做端口 / 平台缝注入。
//!
//! ## 口径（2026-10-10 能力域集中化）
//!
//! 引擎控制面（节点装配 / 生命周期 / 连接 / 首连确认 / 可信表 / 共享目录 /
//! 发送-接收-远端控制面 / 引擎事件翻译）真源在仓库根
//! `packages/bedcode-server-peer-net`（纯引擎形态 `default = []`，双端同引，
//! spec `.scratch/2026-10-10-peer-capability-consolidation/`）；本模块只保留
//! 宿主侧三件事：
//!
//! ① **端口实现**：`PeerEnginePorts` 五端口（bus / event_sink / paths /
//!    system_info / mdns，trait 词汇复用 `bedcode-server-base::ports`）
//!    + `PeerPlatformSeam` 平台缝（SAF 共享访问 / 落点提升 / 内置共享条目）；
//! ② **装配**：`peer_ctx`（托管四状态 + 端口 + 平台缝）；
//! ③ **产品语义薄壳**：插件激活外壳（`FILE_TRANSFER_PLUGIN_ID`）/ 设备名与
//!    下载目录预热 / app 数据目录。
//!
//! mDNS 复用：`MdnsPort` 直接接 `bedcode-discovery-engine::engine` 共享守护三函数
//! （`shared_daemon` / `register_host_service` / `stop_host_service`）——宿主零
//! 第二份 mDNS 代码；Android 多播锁随共享守护常驻（setup 的 init hook，见
//! `lib.rs`）。
//!
//! 发现刷新订阅（`peer:discovery-refresh`）由能力域自持（
//! `request_discovery_refresh`）；就绪判据经 `BusPort::is_ready` 下移到本适配层
//! （本端 = 插件管理器就绪），能力域不再假设 `ServerPorts` 全局注册表存在。

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use tauri::{AppHandle, Emitter, Manager};

use async_trait::async_trait;
use bedcode_peer_net::SharedDirEntry;
use bedcode_server_base::ports::{BusMessageHandler, BusPort, EventSink, MdnsPort, PathsPort, SystemInfoPort};
use bedcode_server_peer_net::engine_ports::{PeerEnginePorts, PeerPlatformSeam};

// ==================== 能力域再导出（宿主其余模块路径零改动） ====================

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

/// SystemInfo 等待上限：setup 后台任务采集设备名（Android API 调用）可能晚于
/// 节点启动就绪；15s 后放弃等待以兜底名启动守护，避免无限期阻塞发现功能
const SYSTEM_INFO_WAIT_ATTEMPTS: u32 = 30;

// ==================== 平台资源预热（同步端口所需的异步解析结果） ====================

/// app 私有下载目录（`resolve_app_downloads_dir` 为 async，端口方法必须同步——
/// 装配前预热一次，进程内缓存；未就绪时引擎回退 app 数据目录内 `downloads/`，
/// 与集中化前「解析失败回退」语义一致）
static DOWNLOADS_DIR: OnceLock<Option<String>> = OnceLock::new();

fn downloads_dir_cache() -> Option<String> {
    DOWNLOADS_DIR.get().cloned().flatten()
}

/// 预热平台资源：设备名就绪等待 + 下载目录解析（幂等；节点启动与平台缝装配共用）
pub(crate) async fn prewarm_platform(app: &AppHandle) {
    prewarm_device_name().await;
    if DOWNLOADS_DIR.get().is_none() {
        let resolved = crate::plugin::android_plugins::resolve_app_downloads_dir(app).await;
        // 竞态下重复 set 被忽略（首次解析胜出）
        let _ = DOWNLOADS_DIR.set(resolved);
    }
}

/// 设备名就绪等待：SystemInfo 由 setup 后台任务异步采集，此处轮询等待其就绪；
/// 超时兜底由 `SystemInfoPort::device_name_fallback`（"BedCode Mobile"）承担
async fn prewarm_device_name() {
    for attempt in 0..SYSTEM_INFO_WAIT_ATTEMPTS {
        if let Some(info) = crate::state::try_get_system_info() {
            let name = info.device_name.clone();
            if !name.is_empty() {
                if attempt > 0 {
                    tracing::debug!("device name resolved after {attempt} wait cycles");
                }
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    tracing::warn!(
        "SystemInfo not ready after {}ms, falling back to the port-provided device name",
        SYSTEM_INFO_WAIT_ATTEMPTS * 500
    );
}

// ==================== 端口实现（宿主侧） ====================

/// 插件消息总线端口：`publish` 经插件管理器；`is_ready` = 管理器就绪
/// （发现刷新订阅的轮询判据，见能力域 `spawn_discovery_refresh_subscriber`）
struct MobileBusPort;

#[async_trait]
impl BusPort for MobileBusPort {
    fn is_ready(&self) -> bool {
        crate::state::try_get_plugin_manager().is_some()
    }

    fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        match crate::state::try_get_plugin_manager() {
            Some(pm) => pm.message_bus().publish(topic, sender, payload),
            // 管理器未就绪（启动窗口）：事件丢弃与集中化前 emit_json 静默跳过同口径
            None => tracing::debug!(topic, "peer event dropped: plugin manager not ready"),
        }
    }

    fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        if let Some(pm) = crate::state::try_get_plugin_manager() {
            pm.message_bus().publish_binary(topic, sender, payload);
        }
    }

    async fn subscribe_static(&self, subscriber: &str, topic: &str, handler: Box<dyn BusMessageHandler>) {
        let pm = match crate::state::try_get_plugin_manager() {
            Some(pm) => pm,
            None => {
                tracing::warn!(
                    subscriber,
                    topic,
                    "peer static subscribe skipped: plugin manager not ready"
                );
                return;
            }
        };
        pm.message_bus()
            .subscribe_static(subscriber, topic, Box::new(BaseBusHandlerAdapter(handler)))
            .await;
    }

    async fn deliver_endpoint_frame(
        &self,
        _owner: &str,
        _endpoint_id: &str,
        _client_id: &str,
        _kind: &str,
        _payload: Vec<u8>,
    ) {
        // peer 引擎不消费该面（它是 WS 端点帧投递，属 host-websocket 客户端域）；
        // no-op 而非 panic：订阅面可能被通用转发路径触达
    }
}

/// base 侧 `BusMessageHandler` → 移动插件机制侧 handler 适配（值转换，形状逐字一致）
struct BaseBusHandlerAdapter(Box<dyn BusMessageHandler>);

impl crate::plugin::message_bus::BusMessageHandler for BaseBusHandlerAdapter {
    fn on_message(&self, msg: &bedcode_server_base::wire::BusMessage) -> anyhow::Result<()> {
        // 签名与内层端口 trait 同型（票 02 M3 单形态收敛后双 trait 共用同一
        // `bedcode_server_base::wire::BusMessage`），不再需要逐字段值转换
        self.0.on_message(msg)
    }
}

/// 前端事件出口（连接态 / 首连确认弹窗请求；失败只记日志不上抛）
struct MobileEventSink {
    app: AppHandle,
}

impl EventSink for MobileEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.app.emit(event, payload) {
            tracing::error!(event, "emit {event} failed: {e}");
        }
    }
}

/// 路径端口：数据目录直取句柄；落点 = 各端自定根目录（移动 = app 私有下载目录，
/// 未预热时回退 app 数据目录内 `downloads/`，与集中化前回退语义一致）
struct MobilePathsPort {
    app: AppHandle,
}

impl PathsPort for MobilePathsPort {
    fn app_data_dir(&self) -> bedcode_server_base::error::Result<PathBuf> {
        app_data_dir(&self.app)
    }

    fn download_dir(&self) -> bedcode_server_base::error::Result<PathBuf> {
        match downloads_dir_cache() {
            Some(dir) => Ok(PathBuf::from(dir)),
            None => Ok(app_data_dir(&self.app)?.join("downloads")),
        }
    }
}

/// 系统信息端口：节点 mDNS 广播设备名（SystemInfo 就绪前由预热等待兜底；
/// 空值兜底名归本端——"BedCode Mobile"，能力域不硬编码平台名）
struct MobileSystemInfoPort;

impl SystemInfoPort for MobileSystemInfoPort {
    fn device_name(&self) -> String {
        crate::state::try_get_system_info()
            .map(|info| info.device_name.clone())
            .unwrap_or_default()
    }

    fn device_name_fallback(&self) -> String {
        "BedCode Mobile".to_string()
    }

    fn local_ipv4_addresses(&self) -> Vec<String> {
        // peer 引擎不消费该面（mDNS 地址由共享守护 `enable_addr_auto` 自动装配）
        Vec::new()
    }

    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

/// mDNS 端口：共享守护 + 宿主身份广播登记——**直接复用** `bedcode-discovery-engine`
/// 引擎三函数（与桌面 `HostMdnsPort` 同源实现，零第二份 mDNS 代码）
struct MobileMdnsPort;

impl MdnsPort for MobileMdnsPort {
    fn shared_daemon(&self) -> mdns_sd::ServiceDaemon {
        bedcode_discovery_engine::engine::shared_daemon()
    }

    fn register_host_service(&self, service_type: &str, fullname: &str) -> std::result::Result<String, String> {
        bedcode_discovery_engine::engine::register_host_service(service_type, fullname)
    }

    fn stop_host_service(&self, advertise_id: &str) -> std::result::Result<bool, String> {
        bedcode_discovery_engine::engine::stop_host_service(advertise_id)
    }
}

// ==================== 平台缝（SAF / 落点提升 / 内置共享条目） ====================

/// 移动平台缝：三处差异面都只是参数注入（peer-net 引擎已预留接缝），
/// 能力域自身零平台代码
struct MobilePlatformSeam;

impl PeerPlatformSeam for MobilePlatformSeam {
    /// Android SAF 共享目录访问（无 SAF 平台返回 None——dev 窗口无此形态，
    /// 注册表若存在 saf 条目会按 read-failed 拒绝，防御性兜底）
    fn shared_saf_access(&self) -> Option<Arc<dyn bedcode_peer_net::SharedSafAccess>> {
        #[cfg(target_os = "android")]
        {
            Some(Arc::new(android_saf_bridge::SafSharedAccess::new(
                crate::plugin::saf_io::default_saf_io(),
            )))
        }
        #[cfg(not(target_os = "android"))]
        {
            None
        }
    }

    /// 落点提升钩子：私有下载目录中的完整文件尽力写入 MediaStore.Downloads
    /// （下载目录不可用时与集中化前一致：不提升）
    fn file_landing(&self) -> Option<Arc<dyn bedcode_peer_net::FileLanding>> {
        downloads_dir_cache().map(|_| {
            let io = crate::plugin::saf_io::default_saf_io();
            Arc::new(MediaLanding { saf: io }) as Arc<dyn bedcode_peer_net::FileLanding>
        })
    }

    /// 内置免授权共享条目：app 私有下载目录（解析失败仅跳过条目，不阻断启动）
    fn builtin_shared_dirs(&self) -> Vec<(String, PathBuf)> {
        downloads_dir_cache()
            .map(|dir| vec![("下载".to_string(), PathBuf::from(dir))])
            .unwrap_or_default()
    }
}

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

// ==================== 平台缝：SAF 桥（Android） ====================
#[cfg(target_os = "android")]
mod android_saf_bridge {
    use super::*;
    use crate::file_service::saf_tree;
    use crate::plugin::saf_io::SafEntry;
    use bedcode_peer_net::SeqReader;

    /// SAF 共享目录访问适配器：把既有 SafIo（Kotlin 桥）映射到 crate 的
    /// `SharedSafAccess` 缝。相对路径解析用 list_tree 逐层下降（与
    /// file_service/saf_tree 的 walk 同构，此处同步化以匹配 trait 签名）。
    pub(crate) struct SafSharedAccess {
        io: Arc<dyn crate::plugin::saf_io::SafIo>,
    }

    impl SafSharedAccess {
        pub(crate) fn new(io: Arc<dyn crate::plugin::saf_io::SafIo>) -> Self {
            Self { io }
        }

        /// 同步逐层下降到 rel 指向的条目（parts 已过 crate 路径清洗）
        fn walk(&self, tree_uri: &str, root_doc_id: &str, parts: &[String]) -> bedcode_peer_net::Result<SafEntry> {
            use bedcode_peer_net::{saf_not_found, PeerNetError};

            if parts.is_empty() {
                return Ok(self.root_entry(tree_uri, root_doc_id));
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

    impl bedcode_peer_net::SharedSafAccess for SafSharedAccess {
        fn list_dir(&self, tree_uri: &str, rel: &str) -> bedcode_peer_net::Result<Vec<bedcode_peer_net::DirEntry>> {
            use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

            let root_doc_id = saf_tree::tree_document_id(tree_uri).ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
            let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
            let target = self.walk(tree_uri, &root_doc_id, &parts)?;
            if !target.is_dir {
                return Err(saf_not_found(format!("'{rel}' is not a directory")));
            }
            let children =
                self.io
                    .list_tree(tree_uri, &target.document_id)
                    .map_err(|e| PeerNetError::TransferSession {
                        role: "saf",
                        detail: format!("list_tree failed (permission may be revoked): {e}"),
                    })?;
            Ok(children
                .into_iter()
                .map(|c| bedcode_peer_net::DirEntry::new(c.name, c.is_dir, c.size.max(0) as u64))
                .collect())
        }

        fn open_read(&self, tree_uri: &str, rel: &str) -> bedcode_peer_net::Result<Arc<dyn SeqReader>> {
            use bedcode_peer_net::{resolve_rel_path, saf_not_found, PeerNetError};

            let root_doc_id = saf_tree::tree_document_id(tree_uri).ok_or_else(|| PeerNetError::TransferSession {
                role: "saf",
                detail: format!("invalid saf tree uri: {tree_uri}"),
            })?;
            let parts = resolve_rel_path(rel).ok_or_else(|| saf_not_found(rel))?;
            let entry = self.walk(tree_uri, &root_doc_id, &parts)?;
            if entry.is_dir {
                return Err(saf_not_found(format!("'{rel}' is a directory")));
            }
            let handle = self
                .io
                .open_stream(&entry.uri, 0)
                .map_err(|e| PeerNetError::TransferSession {
                    role: "saf",
                    detail: format!("open stream for '{rel}' failed: {e}"),
                })?;
            // 大小真源：条目元数据优先（OpenableColumns.SIZE），缺失回退流探测值
            let size = if entry.size > 0 { entry.size as u64 } else { handle.size };
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
        io: Arc<dyn crate::plugin::saf_io::SafIo>,
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

// ==================== 平台缝：落点提升（MediaStore 回退语义） ====================

/// 落地钩子：私有下载目录中的完整文件尽力写入 MediaStore.Downloads 公共下载
///
/// 成功 → 删除私有副本（公共目录为唯一副本，对齐 legacy upload 语义）；
/// 失败 → 保留私有副本（即「回退私有目录」）。阻塞桥接由 KotlinSafIo 内部承担
/// （scoped-thread 惯例）。
struct MediaLanding {
    saf: Arc<dyn crate::plugin::saf_io::SafIo>,
}

impl bedcode_peer_net::FileLanding for MediaLanding {
    fn landed(&self, final_path: &Path, display_name: &str) {
        let src = final_path.to_string_lossy().into_owned();
        match self
            .saf
            .write_media_downloads(&src, display_name, mime_type_for(display_name))
        {
            Ok(()) => match std::fs::remove_file(final_path) {
                Ok(()) => tracing::info!(
                    display_name = %display_name,
                    "pulled file promoted to MediaStore.Downloads (private copy removed)"
                ),
                Err(e) => tracing::warn!(
                    path = %final_path.display(),
                    error = %e,
                    "media promote ok but private copy cleanup failed"
                ),
            },
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

// ==================== 测试（MediaStore 回退语义） ====================

#[cfg(test)]
mod share_landing_tests {
    use super::*;
    use crate::plugin::saf_io::{SafCopyHandle, SafCopyStatus, SafEntry, SafStreamHandle};
    // trait 方法调用需要 trait 在作用域内
    use bedcode_peer_net::FileLanding as _;

    /// fake SafIo：write_media_downloads 结果可编程（None = 成功），其余方法不可达
    struct FakeSaf {
        fail_with: Option<String>,
    }

    impl crate::plugin::saf_io::SafIo for FakeSaf {
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
        fn write_media_downloads(&self, _src: &str, _name: &str, _mime: &str) -> crate::Result<()> {
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
