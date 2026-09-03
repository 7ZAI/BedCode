//! 对等网络专用 mDNS 节点发现（ticket 03）：
//! 独立服务类型广播 + 浏览并存、TXT 携带设备名/协议版本/能力位图、带 TTL 的在线缓存。
//!
//! ## 与终端链路旧 mDNS 的关系
//!
//! 服务类型 `_bedcode-peer._tcp.local.` 与旧 `_bedcode._tcp.local.` 刻意错开：
//! 两套发现互不感知、可同进程共存（mdns-sd 多 ServiceDaemon 实例合法）。
//! 本模块完全不触碰旧链路与 file_service announce（冻结区）。
//!
//! ## 「同一通路」注入缝（issue 03 核心验收点）
//!
//! mDNS 守护回调与单测喂入统一收敛到 [`DiscoveryCache::observe`]：守护任务在
//! `ServiceResolved` 时构造 [`DiscoveredPeerRecord`] 后调用 `observe()`，单测以
//! 完全相同的调用形态注入手工记录——不存在守护专属的第二条插入路径。
//! 产物经 [`DiscoveredPeerRecord::to_static_peer_record`] 即可直接拨号。
//!
//! ## 「在线」语义与 TTL 三层机制
//!
//! 「在线」= 缓存中存在记录，与是否维持连接无关（ADR 0002：未信任节点也可见）。
//! ① 每次 `ServiceResolved` 盖章续期；② `ServiceRemoved`（goodbye）立即移除；
//! ③ 守护任务每 [`SWEEP_INTERVAL`] 清扫一次超时项——崩溃进程没有 goodbye，
//! 只能靠 TTL 兜底（[`PEER_DISCOVERY_TTL`]）。过期判定抽成接受显式 `now` 的
//! 纯逻辑入口，inline 单测直接构造过去时间戳，全程不用 sleep（CI 无 flake）。
//!
//! ## 平台边界（决策 D2）
//!
//! crate 层保持平台无关：MulticastLock 是 Android 宿主职责，由移动端在启动
//! 守护前经 Kotlin 插件 acquire；本模块不做平台开关。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo, ScopedIp};
use serde::Serialize;
use tokio::sync::watch;

use crate::error::{PeerNetError, Result};
use crate::identity::NodeId;
use crate::node::{PeerNetNode, StaticPeerRecord};
use crate::transport::RunningNode;

// ==================== 常量 ====================

/// 对等网络专用服务类型：与终端链路 `_bedcode._tcp.local.` 错开（AC#4）
pub const SERVICE_TYPE: &str = "_bedcode-peer._tcp.local.";

/// 发现通告协议版本
///
/// 刻意独立于 `frame.rs` 的 [`crate::frame::PROTOCOL_VERSION`]：帧格式是 TLS
/// 层契约、发现广播是通告契约，二者演进节奏不同，版本号不得互相牵制。
pub const DISCOVERY_PROTOCOL_VERSION: u32 = 1;

/// 能力位图 bit0：文件传输（首个能力位，后续能力按位追加）
pub const CAP_FILE_TRANSFER: u64 = 1 << 0;

/// 在线记录 TTL：超过该时长未续期即视为离线
///
/// 大于 [`SWEEP_INTERVAL`] 数倍，保证崩溃节点的残留记录在一个清扫周期粒度内
/// 被移除的同时，不会因单次丢包误删活节点（解析事件本身会持续盖章续期）。
/// 120s：mDNS 周期广播间隔随网络抖动可达 30-60s+（多网卡环境更甚），30s TTL
/// 会形成「过期清扫→再发现」循环——发现推送把空/非空列表交替推给前端，
/// 设备面板表现为反复闪空（2026-08-26 双端互不可见排查实证）；TTL 取广播
/// 间隔的 2 倍以上余量，异常离线由 goodbye 包即时移除兜底，不依赖超时。
pub const PEER_DISCOVERY_TTL: Duration = Duration::from_secs(120);

/// 守护任务的过期清扫周期
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// 设备名截断上限（字节）
///
/// mdns-sd 要求单条 TXT 键值 ≤255 字节，200 留足余量；按 UTF-8 字符边界截断。
pub(crate) const DEVICE_NAME_MAX_BYTES: usize = 200;

/// 实例名前缀：实例名绑定短指纹而非设备名——设备改名不会引发 remove/found
/// 抖动（D5）；人类可读名走 TXT `name`
pub(crate) const INSTANCE_PREFIX: &str = "bedcode-peer-";

// ---- TXT keys（全小写：mdns-sd 按 RFC 6763 规范化键名）----

/// TXT key：完整节点 ID（64 位小写 hex）
///
/// 必要补充（D4 未列出）：实例名只含 8 位短指纹，而拨号侧 TLS 身份钉扎需要
/// 完整 NodeId 比对证书指纹——不带全量 ID 的记录根本无法用于建立可信连接。
pub(crate) const TXT_KEY_ID: &str = "id";
/// TXT key：设备名（UTF-8，截断至 [`DEVICE_NAME_MAX_BYTES`]）
pub(crate) const TXT_KEY_NAME: &str = "name";
/// TXT key：协议版本（十进制 u32 字符串）
pub(crate) const TXT_KEY_VER: &str = "ver";
/// TXT key：能力位图（小写 hex u64；与 NodeId 小写风格一致且 >32 bit 时紧凑稳定）
pub(crate) const TXT_KEY_CAP: &str = "cap";

// ==================== 发现记录 ====================

/// 一条通过 mDNS 发现的对端记录：拨号所需最小信息 + 展示/门控元数据
///
/// `last_seen` 由 [`DiscoveryCache::observe`] 盖章维护，是缓存簿记元数据而非
/// 载荷数据，序列化时跳过（`Instant` 无 serde 表示且跨进程无意义，前端列表
/// 只需可见性与名称/能力）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveredPeerRecord {
    /// 对端节点 ID（公钥指纹，来自 TXT `id`；序列化为 64 位小写 hex 字符串）
    #[serde(serialize_with = "serialize_node_id_as_str")]
    pub node_id: NodeId,
    /// 对端监听地址（mDNS 解析的 IP:port，优先 IPv4）
    pub addr: SocketAddr,
    /// 设备名（来自 TXT `name`，缺失时回退实例名）
    pub device_name: String,
    /// 对端通告的协议版本（来自 TXT `ver`，缺失/损坏记 0；仅供未来门控参考）
    pub protocol_version: u32,
    /// 能力位图（来自 TXT `cap` 小写 hex，损坏记 0）
    pub capabilities: u64,
    /// 最近一次被发现/续期时刻（见结构体文档的序列化说明）
    #[serde(skip)]
    pub last_seen: Instant,
}

impl DiscoveredPeerRecord {
    /// 转为拨号入口所需的静态对端记录（发现元数据不污染拨号通路，D1）
    pub fn to_static_peer_record(&self) -> StaticPeerRecord {
        StaticPeerRecord {
            node_id: self.node_id.clone(),
            addr: self.addr,
        }
    }
}

/// `NodeId` 的序列化辅助：输出 64 位小写 hex 字符串
///
/// 不给 [`NodeId`] 派生 serde（identity.rs 明确禁止任何绕过 parse 校验的
/// 反序列化构造路径）；序列化只读不构造，安全输出字符串形态。
fn serialize_node_id_as_str<S: serde::Serializer>(
    node_id: &NodeId,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_str(node_id.as_str())
}

// ==================== TXT 编解码（纯函数） ====================

/// 编码广播用 TXT 载荷（编码/解码为纯函数，D4）
pub(crate) fn encode_txt_properties(
    node_id: &NodeId,
    device_name: &str,
    protocol_version: u32,
    capabilities: u64,
) -> HashMap<String, String> {
    HashMap::from([
        (
            TXT_KEY_ID.to_string(),
            node_id.as_str().to_string(),
        ),
        (
            TXT_KEY_NAME.to_string(),
            truncate_utf8(device_name, DEVICE_NAME_MAX_BYTES),
        ),
        (TXT_KEY_VER.to_string(), protocol_version.to_string()),
        // 小写 hex：与 NodeId 小写风格一致，且能力位超过 32 bit 时比二进制紧凑稳定
        (TXT_KEY_CAP.to_string(), format!("{capabilities:x}")),
    ])
}

/// 从 TXT 载荷解码发现记录（容忍降级，D4）
///
/// - `id` 缺失或非法 → 返回 `None`（整条记录跳过）：无法钉扎身份的记录不可拨号；
/// - `name` 缺失 → 回退实例名；`ver` 缺失/坏 → 0；`cap` 坏 → 0；
/// - 广播载荷解析失败不应产生错误传播链，调用方 warn 后跳过即可。
///
/// 返回值中的 `last_seen` 仅为占位（调用方 [`DiscoveryCache::observe`] 统一盖章）。
pub(crate) fn decode_txt_properties(
    instance_name: &str,
    addr: SocketAddr,
    properties: &HashMap<String, String>,
) -> Option<DiscoveredPeerRecord> {
    let node_id = NodeId::parse(properties.get(TXT_KEY_ID)?).ok()?;
    let device_name = properties
        .get(TXT_KEY_NAME)
        .cloned()
        .unwrap_or_else(|| instance_name.to_string());
    let protocol_version = properties
        .get(TXT_KEY_VER)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let capabilities = properties
        .get(TXT_KEY_CAP)
        .and_then(|v| u64::from_str_radix(v, 16).ok())
        .unwrap_or(0);
    Some(DiscoveredPeerRecord {
        node_id,
        addr,
        device_name,
        protocol_version,
        capabilities,
        last_seen: Instant::now(),
    })
}

/// 按字符边界安全截断 UTF-8 字符串（防 TXT 超 255 上限被 mdns-sd 拒注）
fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// 从服务全名提取实例名段：`bedcode-peer-<指纹>.<SERVICE_TYPE>` → `bedcode-peer-<指纹>`
fn instance_from_fullname(fullname: &str) -> Option<&str> {
    let instance = fullname.strip_suffix(&format!(".{SERVICE_TYPE}"))?;
    instance.starts_with(INSTANCE_PREFIX).then_some(instance)
}

/// 从实例名提取短指纹段：`bedcode-peer-<指纹>` → `<指纹>`
fn fingerprint_of_instance(instance: &str) -> Option<&str> {
    instance.strip_prefix(INSTANCE_PREFIX).filter(|s| !s.is_empty())
}

// ==================== 在线缓存 ====================

/// 发现结果的在线缓存：「在线」的唯一判定来源（存在记录即可见，与连接无关）
///
/// 同步 `std::sync::RwLock` + `Arc` 共享（禁 unsafe impl Send/Sync 的替代）：
/// 所有方法瞬时完成、无跨 await 持锁，同步锁足够且免去异步锁的生命周期负担。
#[derive(Debug, Default)]
pub struct DiscoveryCache {
    peers: RwLock<HashMap<NodeId, DiscoveredPeerRecord>>,
}

impl DiscoveryCache {
    /// 空缓存
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一条发现结果并盖章当前时刻（mDNS 守护回调与单测喂入的同一入口）
    ///
    /// 同一 node_id 重复发现即覆盖更新（携带最新地址/元数据），天然完成续期。
    pub fn observe(&self, mut record: DiscoveredPeerRecord) {
        record.last_seen = Instant::now();
        self.peers
            .write()
            .expect("discovery cache lock poisoned")
            .insert(record.node_id.clone(), record);
    }

    /// 立即移除指定节点（`ServiceRemoved`/goodbye 语义的直接形态）
    pub fn remove(&self, node_id: &NodeId) -> Option<DiscoveredPeerRecord> {
        self.peers
            .write()
            .expect("discovery cache lock poisoned")
            .remove(node_id)
    }

    /// 按短指纹移除并返回被移除记录
    ///
    /// goodbye 事件（`ServiceRemoved`）只携带实例名，而实例名内嵌短指纹（D5），
    /// 据此反查缓存条目。LAN 规模下线性扫描成本可忽略。
    pub(crate) fn remove_by_fingerprint(&self, short: &str) -> Option<DiscoveredPeerRecord> {
        let mut peers = self.peers.write().expect("discovery cache lock poisoned");
        let victim = peers
            .iter()
            .find(|(_, record)| record.node_id.short_fingerprint() == short)
            .map(|(id, _)| id.clone())?;
        peers.remove(&victim)
    }

    /// 移除所有超过 TTL 未续期的记录，返回移除数量（显式时间的纯逻辑核心，D3）
    pub fn sweep_expired_at(&self, now: Instant) -> usize {
        let mut peers = self.peers.write().expect("discovery cache lock poisoned");
        let before = peers.len();
        peers.retain(|_, record| !is_expired(record.last_seen, now));
        before - peers.len()
    }

    /// 以真实时钟执行过期清扫（守护任务的便捷包装）
    pub fn sweep_expired(&self) -> usize {
        self.sweep_expired_at(Instant::now())
    }

    /// 读取单条记录（快照克隆，不暴露内部锁）
    pub fn get(&self, node_id: &NodeId) -> Option<DiscoveredPeerRecord> {
        self.peers
            .read()
            .expect("discovery cache lock poisoned")
            .get(node_id)
            .cloned()
    }

    /// 当前全部在线记录（按 node_id 稳定排序，供命令面/UI 复用）
    pub fn list(&self) -> Vec<DiscoveredPeerRecord> {
        let mut records: Vec<_> = self
            .peers
            .read()
            .expect("discovery cache lock poisoned")
            .values()
            .cloned()
            .collect();
        records.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        records
    }

    /// 以指定时间戳直接插入（绕过盖章）
    ///
    /// 仅 inline 测试使用：构造「过去发现、现已过期」的时间线而无需 sleep。
    #[cfg(test)]
    pub(crate) fn insert_with_last_seen(&mut self, record: DiscoveredPeerRecord, last_seen: Instant) {
        let mut stamped = record;
        stamped.last_seen = last_seen;
        self.peers
            .write()
            .expect("discovery cache lock poisoned")
            .insert(stamped.node_id.clone(), stamped);
    }
}

/// TTL 过期判定（唯一判定函数，`sweep_expired_at` 与单测共用）
///
/// 时钟倒退（last_seen 晚于 now）时 `duration_since` 饱和为零，判为未过期：
/// 宁可晚删也不误删，符合「在线」语义的保守取向。
fn is_expired(last_seen: Instant, now: Instant) -> bool {
    now.duration_since(last_seen) > PEER_DISCOVERY_TTL
}

// ==================== 守护任务 ====================

/// 发现守护配置：广播自身时携带的本机元数据
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// 本机设备名（写入 TXT `name`，供对端展示）
    pub device_name: String,
    /// 本机能力位图（写入 TXT `cap`）
    pub capabilities: u64,
}

/// mDNS 发现守护句柄：持有优雅关闭的全部凭据
///
/// 由 [`spawn_peer_mdns_daemon`] 产出；drop 句柄**不会**停止守护（任务已移交
/// 运行时），显式关停必须调用 [`DiscoveryDaemon::stop`]（Graceful Shutdown
/// 架构决策：退订浏览 → 注销广播 → 关停守护线程 → join 事件循环）。
pub struct DiscoveryDaemon {
    /// 底层 mdns-sd 守护句柄；stop 中 take 掉，drop 最终触发守护线程退出
    daemon: Option<ServiceDaemon>,
    /// 自身服务的完整实例名（注销必需：unregister 按全名寻址）
    fullname: String,
    /// 事件循环停机标志（stop_browse 失败时的兜底退出通道）
    shutdown_tx: watch::Sender<bool>,
    /// 事件循环任务句柄（join 用）
    event_task: Option<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for DiscoveryDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryDaemon")
            .field("fullname", &self.fullname)
            .finish_non_exhaustive()
    }
}

impl DiscoveryDaemon {
    /// 优雅关停：退订浏览 → 注销广播（让对端即时移除本机）→ 关停守护线程 → join
    ///
    /// 清理中途遇到的首个错误会延迟到最后返回：半关闭状态下泄漏一个 mDNS
    /// 守护线程比返回部分错误更难排查，剩余清理步骤必须继续执行完。
    pub async fn stop(mut self) -> Result<()> {
        // ① 请求事件循环退出：正常路径由 ② stop_browse 断开 channel 触发；
        //    此标志保证 stop_browse 失败时循环仍能在至多一个 SWEEP_INTERVAL 内收尾
        if self.shutdown_tx.send(true).is_err() {
            tracing::debug!("peer discovery event loop already exited before stop");
        }

        let mut first_err: Option<PeerNetError> = None;
        if let Some(daemon) = self.daemon.take() {
            // ② 退订浏览：browse channel 发送端随订阅销毁，事件循环随即看到断连
            if let Err(source) = daemon.stop_browse(SERVICE_TYPE) {
                first_err.get_or_insert(PeerNetError::MdnsStopBrowse { source });
            }
            // ③ 注销自身广播：对端收到 goodbye 即时移除本机（不等 TTL 过期）；
            //    NotFound 属幂等重入，视为成功
            match daemon.unregister(&self.fullname) {
                Ok(status) => match status.recv_async().await {
                    Ok(status) => {
                        tracing::debug!(status = ?status, "peer mDNS service unregistered")
                    }
                    Err(e) => tracing::warn!(
                        "unregister status channel closed before reply: {e}"
                    ),
                },
                Err(source) => {
                    first_err.get_or_insert(PeerNetError::MdnsUnregister { source });
                }
            }
            // ④ 关停守护线程并等待退出确认
            match daemon.shutdown() {
                Ok(status) => match status.recv_async().await {
                    Ok(status) => {
                        tracing::debug!(status = ?status, "mDNS daemon shutdown acknowledged")
                    }
                    Err(e) => tracing::warn!("shutdown status channel closed before reply: {e}"),
                },
                Err(source) => {
                    first_err.get_or_insert(PeerNetError::MdnsShutdown { source });
                }
            }
        }
        // ⑤ join 事件循环：JoinError 如实分级记录，panic 不得静默
        if let Some(task) = self.event_task.take() {
            match task.await {
                Ok(()) => {}
                Err(e) if e.is_cancelled() => {
                    tracing::debug!("peer discovery event loop cancelled")
                }
                Err(e) => tracing::error!("peer discovery event loop panicked: {e}"),
            }
        }
        tracing::info!(service = %self.fullname, "peer mDNS discovery daemon stopped");
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// 启动对等网络 mDNS 发现守护：注册自身广播 + 浏览他人 + 事件循环入缓存
///
/// 必须在 tokio 运行时上下文中调用（内部 spawn 依赖 Handle::current）：生产侧
/// 为宿主异步环境，测试侧为 `#[tokio::test]`。节点必须已通过
/// [`PeerNetNode::with_discovery`] 挂载缓存——守护回调与外部喂入共用该实例。
///
/// 广播端口取宿主移交 listener 的实际端口（`running`），广播地址走 addr_auto
/// 由 mdns-sd 枚举本机全部接口（多网卡/IP 变化免手工维护）。
/// 常见虚拟/回环网卡名片段（不区分大小写子串匹配）
///
/// mDNS 多接口监听会在这些网卡上空等解析响应：Windows VMware/Hyper-V/WSL
/// 虚拟交换机不转发组播，对端 resolve 可延迟分钟级（2026-08-26 排查实证：
/// 桌面四接口监听下首次 resolve 移动端耗时 12 分钟）。启动时统一禁用；
/// 移动端接口名（wlan0 等）不会命中这些模式，行为不变。
const VIRTUAL_IFACE_PATTERNS: &[&str] = &[
    "vmware", "virtualbox", "vbox", "vethernet", "hyper-v", "wsl", "docker",
    "loopback", "loopback pseudo", "virbr", "libvirt",
];

/// 禁用已知虚拟/回环网卡的 mDNS 收发（枚举失败则保持默认全接口，降级不阻断）
///
/// pub：宿主侧独立浏览订阅（host-mdns）同样受多网卡解析延迟困扰，复用本逻辑。
pub fn disable_virtual_interfaces(daemon: &ServiceDaemon) {
    let Ok(ifaces) = local_ip_address::list_afinet_netifas() else {
        tracing::debug!("peer mDNS interface enumeration unavailable, keep all interfaces");
        return;
    };
    let mut disabled: Vec<String> = Vec::new();
    for (name, _) in &ifaces {
        let lower = name.to_lowercase();
        if VIRTUAL_IFACE_PATTERNS.iter().any(|p| lower.contains(p))
            && !disabled.iter().any(|d| d == name)
        {
            match daemon.disable_interface(name.as_str()) {
                Ok(()) => disabled.push(name.clone()),
                Err(e) => tracing::debug!(iface = %name, error = %e, "disable interface failed"),
            }
        }
    }
    if !disabled.is_empty() {
        tracing::info!(ifaces = ?disabled, "peer mDNS virtual interfaces disabled");
    }
}

/// mDNS 广播守护句柄（advertise-only）：仅持有注销/关停凭据
///
/// Phase 4（issue 13）：插件侧经 `host-mdns` 按需自建浏览，引擎侧不再常开
/// 浏览——本句柄只负责「本机可被发现」的注册/注销（与 TLS listener 同生命周期）。
pub struct DiscoveryAdvertiser {
    daemon: Option<ServiceDaemon>,
    fullname: String,
}

impl std::fmt::Debug for DiscoveryAdvertiser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryAdvertiser")
            .field("fullname", &self.fullname)
            .finish_non_exhaustive()
    }
}

impl DiscoveryAdvertiser {
    /// 优雅关停：注销广播（对端即时移除本机）→ 关停守护线程
    pub async fn stop(mut self) -> Result<()> {
        let mut first_err: Option<PeerNetError> = None;
        if let Some(daemon) = self.daemon.take() {
            match daemon.unregister(&self.fullname) {
                Ok(status) => match status.recv_async().await {
                    Ok(status) => {
                        tracing::debug!(status = ?status, "peer mDNS service unregistered")
                    }
                    Err(e) => tracing::warn!(
                        "unregister status channel closed before reply: {e}"
                    ),
                },
                Err(source) => {
                    first_err.get_or_insert(PeerNetError::MdnsUnregister { source });
                }
            }
            match daemon.shutdown() {
                Ok(status) => match status.recv_async().await {
                    Ok(_) => tracing::debug!("peer mDNS daemon shut down"),
                    Err(e) => tracing::warn!("daemon shutdown channel closed before reply: {e}"),
                },
                Err(source) => {
                    first_err.get_or_insert(PeerNetError::MdnsShutdown { source });
                }
            }
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

/// 启动 advertise-only mDNS 守护：注册自身服务供对端发现，不做任何浏览
/// （发现事件由消费插件经 `host-mdns` browse 自行订阅）
pub fn spawn_peer_mdns_advertiser(
    node: &PeerNetNode,
    running: &RunningNode,
    config: DiscoveryConfig,
) -> Result<DiscoveryAdvertiser> {
    let own_node_id = node.node_id().clone();
    let short = own_node_id.short_fingerprint();
    let instance_name = format!("{INSTANCE_PREFIX}{short}");
    // 全名格式由 mdns-sd 固定为 "{escaped_instance}.{service_type}"（见 ServiceInfo::new）
    let fullname = format!("{instance_name}.{SERVICE_TYPE}");
    let listen_port = running.local_addr().port();

    let daemon =
        ServiceDaemon::new().map_err(|source| PeerNetError::MdnsDaemon { source })?;
    disable_virtual_interfaces(&daemon);

    let properties = encode_txt_properties(
        &own_node_id,
        &config.device_name,
        DISCOVERY_PROTOCOL_VERSION,
        config.capabilities,
    );
    // ip 参数传空串 = 启用 addr_auto 的库约定（AsIpAddrs 对空串返回空集合）
    let service_info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &format!("{instance_name}.local."),
        "",
        listen_port,
        properties,
    )
    .map_err(|source| PeerNetError::MdnsServiceInfo { source })?
    .enable_addr_auto();

    daemon
        .register(service_info)
        .map_err(|source| PeerNetError::MdnsRegister { source })?;
    tracing::info!(
        service = %fullname,
        port = listen_port,
        device = %config.device_name,
        "peer mDNS advertiser started (browse retired to host-mdns)"
    );
    Ok(DiscoveryAdvertiser { daemon: Some(daemon), fullname })
}

pub fn spawn_peer_mdns_daemon(
    node: &PeerNetNode,
    running: &RunningNode,
    config: DiscoveryConfig,
) -> Result<DiscoveryDaemon> {
    let own_node_id = node.node_id().clone();
    let short = own_node_id.short_fingerprint();
    let instance_name = format!("{INSTANCE_PREFIX}{short}");
    // 全名格式由 mdns-sd 固定为 "{escaped_instance}.{service_type}"（见 ServiceInfo::new）
    let fullname = format!("{instance_name}.{SERVICE_TYPE}");
    let listen_port = running.local_addr().port();

    let cache = node.discovery().ok_or(PeerNetError::DiscoveryNotAttached)?;

    let daemon =
        ServiceDaemon::new().map_err(|source| PeerNetError::MdnsDaemon { source })?;
    disable_virtual_interfaces(&daemon);

    let properties = encode_txt_properties(
        &own_node_id,
        &config.device_name,
        DISCOVERY_PROTOCOL_VERSION,
        config.capabilities,
    );
    // ip 参数传空串 = 启用 addr_auto 的库约定（AsIpAddrs 对空串返回空集合）
    let service_info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance_name,
        &format!("{instance_name}.local."),
        "",
        listen_port,
        properties,
    )
    .map_err(|source| PeerNetError::MdnsServiceInfo { source })?
    .enable_addr_auto();

    daemon
        .register(service_info)
        .map_err(|source| PeerNetError::MdnsRegister { source })?;
    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|source| PeerNetError::MdnsBrowse { source })?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let event_task = tokio::spawn(run_discovery_loop(
        receiver,
        cache,
        own_node_id,
        shutdown_rx,
    ));
    tracing::info!(
        service = %fullname,
        port = listen_port,
        device = %config.device_name,
        "peer mDNS discovery daemon started"
    );
    Ok(DiscoveryDaemon {
        daemon: Some(daemon),
        fullname,
        shutdown_tx,
        event_task: Some(event_task),
    })
}

/// 守护事件循环：三相事件处理 + 周期性过期清扫
///
/// 退出条件（任一）：① `stop_browse` 断开 channel；② 停机标志置位
/// （stop_browse 失败时的兜底，最迟一个 SWEEP_INTERVAL 后生效）。
async fn run_discovery_loop(
    receiver: mdns_sd::Receiver<ServiceEvent>,
    cache: Arc<DiscoveryCache>,
    own_node_id: NodeId,
    shutdown_rx: watch::Receiver<bool>,
) {
    let mut last_sweep = Instant::now();
    loop {
        if *shutdown_rx.borrow() {
            break;
        }
        match receiver.recv_timeout(SWEEP_INTERVAL) {
            Ok(event) => {
                handle_browse_event(&cache, &own_node_id, event);
                // 高频事件流下 Timeout 分支可能长期不触发，清扫按墙钟到期兜底
                let now = Instant::now();
                if now.duration_since(last_sweep) >= SWEEP_INTERVAL {
                    sweep_and_log(&cache);
                    last_sweep = now;
                }
            }
            Err(recv_err) => match recv_err {
                flume::RecvTimeoutError::Disconnected => break,
                // Timeout = 本周期无事件，正好执行周期性过期清扫
                flume::RecvTimeoutError::Timeout => {
                    sweep_and_log(&cache);
                    last_sweep = Instant::now();
                }
            },
        }
    }
    tracing::debug!("peer mDNS discovery event loop exited");
}

/// 单次清扫并按移除数量分级记录
fn sweep_and_log(cache: &DiscoveryCache) {
    let removed = cache.sweep_expired();
    if removed > 0 {
        tracing::debug!(count = removed, "peer discovery cache swept expired records");
    }
}

/// 处理单个浏览事件：found/resolved 进缓存（含自播过滤）、removed 即时移除
fn handle_browse_event(cache: &DiscoveryCache, own_node_id: &NodeId, event: ServiceEvent) {
    match event {
        ServiceEvent::SearchStarted(ty) => {
            tracing::debug!(service_type = %ty, "peer mDNS search started")
        }
        ServiceEvent::SearchStopped(ty) => {
            tracing::debug!(service_type = %ty, "peer mDNS search stopped")
        }
        ServiceEvent::ServiceFound(_, fullname) => {
            tracing::debug!(instance = %fullname, "peer mDNS instance found, waiting for resolve")
        }
        ServiceEvent::ServiceResolved(info) => {
            let Some(ip) = info
                .get_addresses()
                .iter()
                .find(|a| a.is_ipv4())
                .or_else(|| info.get_addresses().iter().next())
                .map(ScopedIp::to_ip_addr)
            else {
                tracing::warn!(
                    instance = %info.get_fullname(),
                    "resolved peer record carries no address, skipped"
                );
                return;
            };
            let addr = SocketAddr::new(ip, info.get_port());
            let Some(instance) = instance_from_fullname(info.get_fullname()) else {
                tracing::warn!(fullname = %info.get_fullname(), "unexpected peer mDNS fullname shape, skipped");
                return;
            };
            let properties: HashMap<String, String> = info
                .get_properties()
                .iter()
                .map(|p| (p.key().to_string(), p.val_str().to_string()))
                .collect();
            match decode_txt_properties(instance, addr, &properties) {
                Some(record) => {
                    // 自播回显过滤：本机的广播也会被自己的浏览收到
                    if record.node_id == *own_node_id {
                        return;
                    }
                    tracing::info!(
                        name = %record.device_name,
                        cap = format!("{:#x}", record.capabilities),
                        ver = record.protocol_version,
                        peer = %record.addr,
                        short = %record.node_id.short_fingerprint(),
                        "peer discovered"
                    );
                    cache.observe(record);
                }
                None => tracing::warn!(
                    instance = %info.get_fullname(),
                    "peer TXT payload lacks valid node id, record skipped"
                ),
            }
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            let removed = instance_from_fullname(&fullname)
                .and_then(fingerprint_of_instance)
                .and_then(|fp| cache.remove_by_fingerprint(fp));
            match removed {
                Some(record) => tracing::info!(
                    name = %record.device_name,
                    short = %record.node_id.short_fingerprint(),
                    "peer removed (mDNS goodbye)"
                ),
                None => tracing::debug!(
                    instance = %fullname,
                    "goodbye for unknown or already swept instance"
                ),
            }
        }
        // ServiceEvent 标记 #[non_exhaustive]：上游新增变体时先忽略并留痕，
        // 避免未来版本升级在事件热路径上 panic
        other => {
            tracing::debug!(event = ?other, "unhandled peer mDNS event variant");
        }
    }
}

// ==================== inline 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 合法 64 位 hex NodeId 构造助手
    fn test_node_id(seed: u8) -> NodeId {
        NodeId::parse(&hex::encode([seed; 32])).expect("valid node id")
    }

    // ---- TXT codec ----

    #[test]
    fn txt_roundtrip_preserves_all_fields() {
        let node_id = test_node_id(0x2a);
        let properties = encode_txt_properties(&node_id, "开发机 Dev", 1, CAP_FILE_TRANSFER | (1 << 3));
        let addr: SocketAddr = "192.168.1.10:47613".parse().expect("valid addr");

        let decoded =
            decode_txt_properties("bedcode-peer-deadbeef", addr, &properties)
                .expect("round-trip must decode");

        assert_eq!(decoded.node_id, node_id);
        assert_eq!(decoded.addr, addr);
        assert_eq!(decoded.device_name, "开发机 Dev");
        assert_eq!(decoded.protocol_version, DISCOVERY_PROTOCOL_VERSION);
        assert_eq!(decoded.capabilities, CAP_FILE_TRANSFER | (1 << 3));
    }

    #[test]
    fn txt_decode_tolerates_missing_and_broken_fields() {
        let addr: SocketAddr = "192.168.1.20:1".parse().expect("valid addr");
        let node_id = test_node_id(0x11);

        // 缺 name → 回退实例名；缺 ver/cap → 0
        let minimal = HashMap::from([(TXT_KEY_ID.to_string(), node_id.as_str().to_string())]);
        let decoded = decode_txt_properties("bedcode-peer-feedface", addr, &minimal)
            .expect("minimal payload still decodes");
        assert_eq!(decoded.device_name, "bedcode-peer-feedface");
        assert_eq!(decoded.protocol_version, 0);
        assert_eq!(decoded.capabilities, 0);

        // 坏 cap / 坏 ver → 记 0，不产生错误
        let broken = HashMap::from([
            (TXT_KEY_ID.to_string(), node_id.as_str().to_string()),
            (TXT_KEY_VER.to_string(), "not-a-number".to_string()),
            (TXT_KEY_CAP.to_string(), "zzz".to_string()),
        ]);
        let decoded = decode_txt_properties("bedcode-peer-feedface", addr, &broken)
            .expect("broken ver/cap degrade to zero");
        assert_eq!(decoded.protocol_version, 0);
        assert_eq!(decoded.capabilities, 0);

        // 缺 id / 坏 id → 整条跳过（None）：无法钉扎身份的记录不可拨号
        assert!(decode_txt_properties("x", addr, &HashMap::new()).is_none());
        let bad_id = HashMap::from([(TXT_KEY_ID.to_string(), "nothex".to_string())]);
        assert!(decode_txt_properties("x", addr, &bad_id).is_none());
    }

    #[test]
    fn txt_encode_truncates_long_device_name_on_char_boundary() {
        let node_id = test_node_id(0x33);
        // 100 个三字节汉字 = 300 字节 > 200 上限
        let long_name = "测".repeat(100);
        let properties = encode_txt_properties(&node_id, &long_name, 1, 0);
        let encoded_name = &properties[TXT_KEY_NAME];
        assert!(encoded_name.len() <= DEVICE_NAME_MAX_BYTES);
        // 截断必须落在字符边界上（String 切片本身保证 UTF-8 合法性，此处断言未丢整字）
        assert_eq!(encoded_name.chars().count(), DEVICE_NAME_MAX_BYTES / 3);

        // mdns-sd 的硬约束：键 + 值总长 ≤255
        for (key, value) in &properties {
            assert!(
                key.len() + value.len() <= u8::MAX as usize,
                "TXT property {key} exceeds 255-byte limit"
            );
        }
    }

    #[test]
    fn fullname_helpers_extract_instance_and_fingerprint() {
        let fullname = format!("{INSTANCE_PREFIX}deadbeef.{SERVICE_TYPE}");
        let instance = instance_from_fullname(&fullname).expect("well-shaped fullname");
        assert_eq!(instance, "bedcode-peer-deadbeef");
        assert_eq!(fingerprint_of_instance(instance), Some("deadbeef"));

        // 外域服务 / 非本应用实例 → None
        assert!(instance_from_fullname("other._bedcode._tcp.local.").is_none());
        assert!(fingerprint_of_instance("not-our-prefix").is_none());
    }

    // ---- 在线缓存（显式时间戳，无 sleep）----

    fn sample_record(seed: u8, last_octet: u8) -> DiscoveredPeerRecord {
        DiscoveredPeerRecord {
            node_id: test_node_id(seed),
            addr: format!("192.168.1.{last_octet}:476{last_octet:02}")
                .parse()
                .expect("valid addr"),
            device_name: format!("device-{seed}"),
            protocol_version: DISCOVERY_PROTOCOL_VERSION,
            capabilities: CAP_FILE_TRANSFER,
            last_seen: Instant::now(),
        }
    }

    #[test]
    fn observe_enters_cache_with_full_metadata() {
        let cache = DiscoveryCache::new();
        let record = sample_record(1, 11);

        cache.observe(record.clone());

        let cached = cache.get(&record.node_id).expect("observed record visible");
        // 字段完整性：AC#1 断言点（名称/版本/能力位随记录进入缓存）
        assert_eq!(cached.device_name, "device-1");
        assert_eq!(cached.protocol_version, DISCOVERY_PROTOCOL_VERSION);
        assert_eq!(cached.capabilities, CAP_FILE_TRANSFER);
        assert_eq!(cached.addr, record.addr);
        // observe 盖章真实时刻：last_seen 不早于 observe 调用点
        assert!(cached.last_seen >= record.last_seen);
        assert_eq!(cache.list().len(), 1);
    }

    #[test]
    fn sweep_expired_at_removes_stale_records_only() {
        let mut cache = DiscoveryCache::new();
        let fresh = sample_record(2, 22);
        let stale = sample_record(3, 33);

        // fresh 走公开 observe（盖章 now）；stale 回填「TTL+1s 前」的过去时间戳
        let now = Instant::now();
        cache.observe(fresh.clone());
        let past = now.checked_sub(PEER_DISCOVERY_TTL + Duration::from_secs(1)).expect("backwards instant");
        cache.insert_with_last_seen(stale.clone(), past);

        let removed = cache.sweep_expired_at(now);
        assert_eq!(removed, 1, "only the stale record expires");
        assert!(cache.get(&fresh.node_id).is_some(), "fresh record survives");
        assert!(cache.get(&stale.node_id).is_none());

        // 边界：恰好等于 TTL 不算过期（严格大于才移除）
        let boundary = sample_record(4, 44);
        let at_ttl = now.checked_sub(PEER_DISCOVERY_TTL).expect("backwards instant");
        cache.insert_with_last_seen(boundary.clone(), at_ttl);
        assert_eq!(cache.sweep_expired_at(now), 0, "age == TTL stays online");
    }

    #[test]
    fn remove_is_immediate_goodbye_semantics() {
        let cache = DiscoveryCache::new();
        let record = sample_record(5, 55);
        cache.observe(record.clone());

        // 移除返回的是缓存中的盖章副本：last_seen 已被 observe 续期到不早于原值
        let removed = cache.remove(&record.node_id).expect("record removed");
        assert_eq!(removed.node_id, record.node_id);
        assert_eq!(removed.addr, record.addr);
        assert!(removed.last_seen >= record.last_seen, "observe must have stamped");
        assert!(cache.get(&record.node_id).is_none(), "gone immediately");
        assert!(cache.list().is_empty());

        // remove_by_fingerprint：goodbye 只带实例名时的反查路径
        cache.observe(record.clone());
        let short = record.node_id.short_fingerprint();
        assert!(cache.remove_by_fingerprint(short).is_some());
        assert!(cache.remove_by_fingerprint(short).is_none(), "second goodbye idempotent");
    }

    #[test]
    fn discovered_record_converts_to_static_peer_record() {
        let record = sample_record(6, 66);
        let converted = record.to_static_peer_record();
        // 拨号通路只需要 node_id + addr：发现元数据不随之流动（D1）
        assert_eq!(converted.node_id, record.node_id);
        assert_eq!(converted.addr, record.addr);
    }

    /// 序列化形状守卫：`last_seen` 跳过、其余字段齐全（list_discovered_peers 响应契约）
    #[test]
    fn serialization_skips_runtime_bookkeeping_field() {
        let record = sample_record(7, 77);
        let json = serde_json::to_value(&record).expect("serializable");
        let obj = json.as_object().expect("object shape");
        assert!(!obj.contains_key("last_seen"), "bookkeeping field must be skipped");
        for key in ["node_id", "addr", "device_name", "protocol_version", "capabilities"] {
            assert!(obj.contains_key(key), "payload field {key} must serialize");
        }
    }
}
