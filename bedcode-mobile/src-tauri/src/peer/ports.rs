//! 宿主端口实现：`PeerEnginePorts` 五端口（bus / event_sink / paths / system_info /
//! mdns）+ 平台资源预热。
//!
//! 端口 trait 词汇复用 `bedcode-server-base::ports`（双端共享）；本文件是移动侧
//! 实现——插件总线经插件管理器、前端事件经 Tauri emit、路径取 app 数据 / 下载
//! 目录、mDNS 直连 `bedcode-discovery-engine` 共享守护（零第二份 mDNS 代码）。
//!
//! 平台资源预热：`resolve_app_downloads_dir` 与 SystemInfo 设备名都是异步或延迟
//! 就绪的，而端口方法必须同步——装配前经 [`prewarm_platform`] 预热一次并进程内
//! 缓存（见 `DOWNLOADS_DIR`）。

use std::path::PathBuf;
use std::sync::OnceLock;

use async_trait::async_trait;
use tauri::{AppHandle, Emitter};

use bedcode_server_base::ports::{
    BusMessageHandler, BusPort, EventSink, MdnsPort, MdnsTypeSubscription, PathsPort, SystemInfoPort,
};

use super::net::app_data_dir;

/// SystemInfo 等待上限：setup 后台任务采集设备名（Android API 调用）可能晚于
/// 节点启动就绪；15s 后放弃等待以兜底名启动守护，避免无限期阻塞发现功能
const SYSTEM_INFO_WAIT_ATTEMPTS: u32 = 30;

// ==================== 平台资源预热（同步端口所需的异步解析结果） ====================

/// app 私有下载目录（`resolve_app_downloads_dir` 为 async，端口方法必须同步——
/// 装配前预热一次，进程内缓存；未就绪时引擎回退 app 数据目录内 `downloads/`，
/// 与集中化前「解析失败回退」语义一致）
static DOWNLOADS_DIR: OnceLock<Option<String>> = OnceLock::new();

/// 单测覆写槽：优先进程内预热缓存，使同步端口投影能在无 `AppHandle` 的单测里
/// 注入确定值（`#[cfg(test)]` 专用，生产不编译）
#[cfg(test)]
static TEST_DOWNLOADS_DIR: OnceLock<Option<String>> = OnceLock::new();

/// 已预热的 app 私有下载目录（未预热 / 解析失败返回 None，调用方自行回退）；
/// 平台缝（`peer::seam`）、路径端口与 `host-config.app.downloads_dir` 端口
/// （`plugin::host_ports::HostPorts::app_downloads_dir`）共用同一份缓存
pub(crate) fn downloads_dir_cache() -> Option<String> {
    #[cfg(test)]
    if let Some(v) = TEST_DOWNLOADS_DIR.get() {
        return v.clone();
    }
    DOWNLOADS_DIR.get().cloned().flatten()
}

/// 单测注入下载目录（`#[cfg(test)]` 专用；见 [`TEST_DOWNLOADS_DIR`]）
#[cfg(test)]
pub(crate) fn set_downloads_dir_for_test(dir: Option<String>) {
    let _ = TEST_DOWNLOADS_DIR.set(dir);
}

/// 只预热 app 下载目录（不等设备名）
///
/// 除 peer 平台缝外，`host-config.app.downloads_dir` 的同步端口也读这份缓存，
/// 而插件 `activate()` 期就可能读该键——节点启动预热（[`prewarm_platform`]，由
/// 文件传输插件驱动）晚于插件激活，故宿主 WASM 运行时装配时必须单独预热一次，
/// 否则在 activate 读目录的插件（ai-chatbox）会拿到「host injected no downloads
/// dir」而激活失败。
pub(crate) async fn prewarm_downloads_dir(app: &AppHandle) {
    if DOWNLOADS_DIR.get().is_none() {
        let resolved = crate::plugin::android_plugins::resolve_app_downloads_dir(app).await;
        // 竞态下重复 set 被忽略（首次解析胜出）
        let _ = DOWNLOADS_DIR.set(resolved);
    }
}

/// 预热平台资源：设备名就绪等待 + 下载目录解析（幂等；节点启动与平台缝装配共用）
pub(crate) async fn prewarm_platform(app: &AppHandle) {
    prewarm_device_name().await;
    prewarm_downloads_dir(app).await;
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
pub(crate) struct MobileBusPort;

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
pub(crate) struct MobileEventSink {
    pub(crate) app: AppHandle,
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
pub(crate) struct MobilePathsPort {
    pub(crate) app: AppHandle,
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
pub(crate) struct MobileSystemInfoPort;

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
/// 引擎函数（与桌面 `HostMdnsPort` 同源实现，零第二份 mDNS 代码）
///
/// **浏览面归类型中枢**（ADR 0054 / 票 04）：`subscribe_type` /
/// `request_type_requery` 直转引擎的类型中枢出口——全进程对该服务类型的
/// `daemon.browse` 只由中枢发起（peer-net 的重查不再覆盖插件 browse 的 listener）。
/// 两个值形态（引擎 `MdnsBrowseSubscription` / 端口 `MdnsTypeSubscription`）只在
/// 这里转换，receiver 与退订守卫**整份移交**（`into_parts`），不得分家。
pub(crate) struct MobileMdnsPort;

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

    fn subscribe_type(&self, service_type: &str) -> std::result::Result<MdnsTypeSubscription, String> {
        let subscription = bedcode_discovery_engine::subscribe_type(service_type)?;
        let (receiver, guard) = subscription.into_parts();
        Ok(MdnsTypeSubscription::new(receiver, guard))
    }

    fn request_type_requery(&self, service_type: &str) {
        bedcode_discovery_engine::request_type_requery(service_type);
    }
}
