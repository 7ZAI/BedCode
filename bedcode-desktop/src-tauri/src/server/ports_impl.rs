//! 宿主壳端口实现（server-lib-split spec §2 端口表「注入实现（宿主壳）」列）
//!
//! 本模块**留在宿主内**（不随 lib 走）：server 各 lib 反向需要的宿主能力
//! （AppContext / AppHandle / MessageBus / power / mdns / auth_center / config）
//! 全部在这里包装成 [`bedcode_server_base::ports`] 的 trait 实现；bootstrap
//! （`lib.rs` setup）构造 [`ServerPorts`] 并 [`init`]。
//!
//! 语义约定：所有「无头 / 单测上下文」分支与既有 `AppContext::try_global()` /
//! `endpoint_owner_activated` 的保守行为逐字对齐（fail-closed 优先，绝不把
//! 请求交给不存在的插件 / 中心）。

use crate::system::app_context::AppContext;
use crate::system::constants::TXT_KEY_DEVICE_NAME;
use crate::AppError;
use crate::Result;
use async_trait::async_trait;
use bedcode_server_base::config::NetworkConfig;
use bedcode_server_base::identity::AuthenticatedIdentity;
use bedcode_server_base::ports::{
    AuthCenter, BusMessageHandler, BusPort, ConfigPort, EventSink, MdnsAdvertiserPort, MdnsPort, PathsPort,
    PluginInvoker, PowerPort, RuntimePort, ServerLifecycleEvent, ServerLifecyclePort, ServerPorts, SystemInfoPort,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};

// ==================== PluginInvoker ====================

/// 插件命令调用面（`AppContext::global().plugin_host()` 包装）
pub struct HostPluginInvoker;

#[async_trait]
impl PluginInvoker for HostPluginInvoker {
    async fn invoke_rust_command(
        &self,
        owner: &str,
        command: &str,
        args: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String> {
        let Some(ctx) = AppContext::try_global() else {
            return Err("plugin invoke unavailable: no runtime context".to_string());
        };
        ctx.plugin_host()
            .invoke_rust_command(owner, command, args)
            .await
            .map_err(|e| e.to_string())
    }

    async fn is_activated(&self, owner: &str) -> bool {
        // 与 `endpoint_owner_activated` 语义逐字对齐：无 AppContext / 宿主对
        // 该 plugin_id 无记录 → 无从判定 → true（防御性兜底，端点本身只可能
        // 由运行中的插件注册）；仅当有记录且非激活时否决
        let Some(ctx) = AppContext::try_global() else {
            return true;
        };
        let host = ctx.plugin_host();
        if host.get_plugin(owner).await.is_none() {
            return true;
        }
        host.is_activated(owner).await
    }
}

// ==================== AuthCenter ====================

/// 认证中心裁决（`utils::auth::auth_center::enforce_connection_policy` 包装）
pub struct HostAuthCenter;

impl AuthCenter for HostAuthCenter {
    fn enforce_connection_policy(&self, token: &str) -> std::result::Result<AuthenticatedIdentity, String> {
        let Some(ctx) = AppContext::try_global() else {
            return Err("auth center unavailable: no runtime context".to_string());
        };
        crate::utils::auth::auth_center::enforce_connection_policy(ctx.plugin_host(), token)
    }
}

// ==================== BusPort ====================

/// 插件消息总线 + WS 帧投递（整核抽出：`HostBusPort` 已迁入
/// `bedcode_wasm_core::bus` —— 它包 `MessageBus`（crate 属物）。本文件保留路径，
/// lib 其余代码经 `crate::server::ports_impl::HostBusPort` 引用不受影响；
/// `assemble()` 本体留 lib（组合根唯一性）。
///
/// 装配走 **late-bound** 形态而非钉死某条总线：真实总线在 `PluginHost` 构造时创建，
/// 而端口可能在它之前就被装配（见 [`HostPathsPort`] 的窗口说明）。钉死会让提前装配
/// 的端口永远指向占位总线——插件订阅永收不到消息。
pub use bedcode_wasm_core::bus::HostBusPort;

// ==================== EventSink ====================

/// 前端事件（`AppHandle::emit` 包装；无 AppContext / AppHandle 静默跳过）
pub struct HostEventSink;

impl EventSink for HostEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let Some(ctx) = AppContext::try_global() else {
            return;
        };
        let Some(handle) = ctx.app_handle().as_ref() else {
            return;
        };
        if let Err(e) = handle.emit(event, payload) {
            tracing::error!(event = %event, error = %e, "emit failed");
        }
    }
}

// ==================== PathsPort ====================

/// 路径解析（`app_handle.path()` 包装；无 AppContext / AppHandle → 错误）
///
/// **句柄随装配传入而不是逐次读全局**：端口可能在 `AppContext` 注册**之前**被装配
/// ——宿主组合根的顺序是「建 `PluginHost`（内部即激活插件，激活期 guest 会立刻调
/// `host-peer.start-node` 等原语）→ 注册 AppContext → 装端口」，而 AppHandle 在
/// `PluginHost::new` 之前就已就位。逐次读全局会让提前装配的端口在窗口内恒定解析
/// 失败（2026-10-07 实机：file-transfer 起 peer 节点报 `resolve app data dir
/// failed: no runtime context`，节点起不来 → 桌面端不广播 → 移动端发现不到）。
pub struct HostPathsPort {
    /// 装配期捕获的句柄；`None` = 无头 / 单测装配面，回退 `AppContext` 取用
    handle: Option<Arc<tauri::AppHandle>>,
}

impl HostPathsPort {
    pub fn new(handle: Option<Arc<tauri::AppHandle>>) -> Self {
        Self { handle }
    }

    /// 当前可用句柄：装配期捕获优先，缺失时回退全局（`AppContext` 注册之后的装配面）
    fn handle(&self) -> Option<Arc<tauri::AppHandle>> {
        match &self.handle {
            Some(handle) => Some(handle.clone()),
            None => AppContext::try_global()?.app_handle().clone(),
        }
    }
}

impl PathsPort for HostPathsPort {
    fn app_data_dir(&self) -> Result<PathBuf> {
        let Some(handle) = self.handle() else {
            return Err(AppError::Internal(
                "resolve app data dir failed: no runtime context".to_string(),
            ));
        };
        handle
            .path()
            .app_data_dir()
            .map_err(|e| AppError::Internal(format!("resolve app data dir failed: {e}")))
    }

    fn download_dir(&self) -> Result<PathBuf> {
        let Some(handle) = self.handle() else {
            return Err(AppError::Internal(
                "resolve downloads dir failed: no runtime context".to_string(),
            ));
        };
        handle
            .path()
            .download_dir()
            .map_err(|e| AppError::Internal(format!("resolve downloads dir failed: {e}")))
    }
}

// ==================== SystemInfoPort ====================

/// 系统信息（`SystemInfo` / `local_ipv4_addresses` / 宿主版本号）
pub struct HostSystemInfoPort;

impl SystemInfoPort for HostSystemInfoPort {
    fn device_name(&self) -> String {
        match AppContext::try_global() {
            Some(ctx) => ctx.system_info().device_name.clone(),
            // 注册前窗口：现采一次真实设备名（与 bootstrap 的 `SystemInfo::collect()`
            // 同源），**不回退硬编码名**——peer 节点把它写进 mDNS TXT，落占位名会让
            // 对端显示成「BedCode Desktop」
            None => crate::system::info::SystemInfo::collect().device_name,
        }
    }

    fn local_ipv4_addresses(&self) -> Vec<String> {
        crate::system::info::local_ipv4_addresses()
    }

    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

// ==================== PowerPort ====================

/// 休眠阻止（`system::power::power_manager()` 包装）
pub struct HostPowerPort;

impl PowerPort for HostPowerPort {
    fn enable(&self) {
        crate::system::power::power_manager().enable();
    }
    fn disable(&self) {
        crate::system::power::power_manager().disable();
    }
}

// ==================== MdnsAdvertiserPort ====================

/// mDNS 服务广播（`AppContext::mdns_advertiser()` 包装，装配 `AdvertiseConfig`）
pub struct HostMdnsAdvertiserPort;

impl MdnsAdvertiserPort for HostMdnsAdvertiserPort {
    fn advertise(&self, service_name: String, port: u16, txt_records: std::collections::HashMap<String, String>) {
        let Some(advertiser) = AppContext::try_global().map(|ctx| ctx.mdns_advertiser().clone()) else {
            // 静默返回会让「广播没起来」无处可查（对端只看到设备凭空消失）：
            // 显性 warn，调用方为 server 启停（非热路径，不会成日志风暴）
            tracing::warn!(
                service_name = %service_name,
                port = port,
                "[ServerSupervisor] mDNS advertisement skipped: app context not initialized"
            );
            return;
        };
        tokio::spawn(async move {
            let advertiser = advertiser.read().await;
            let config = bedcode_discovery_engine::types::AdvertiseConfig {
                service_name,
                port,
                txt_records,
            };
            if let Err(e) = advertiser.start(config).await {
                tracing::error!("[ServerSupervisor] Failed to start mDNS advertisement: {}", e);
            }
        });
    }

    fn stop(&self) {
        let Some(advertiser) = AppContext::try_global().map(|ctx| ctx.mdns_advertiser().clone()) else {
            tracing::warn!("[ServerSupervisor] mDNS advertisement stop skipped: app context not initialized");
            return;
        };
        tokio::spawn(async move {
            let advertiser = advertiser.read().await;
            if let Err(e) = advertiser.stop().await {
                tracing::error!("[ServerSupervisor] Failed to stop mDNS advertisement: {}", e);
            }
        });
    }
}

// ==================== ServerLifecyclePort ====================

/// 服务器生命周期桥（`WebSocketManager::global()` + `WsSessionRegistry::global()` 包装）
pub struct HostServerLifecyclePort;

#[async_trait]
impl ServerLifecyclePort for HostServerLifecyclePort {
    async fn start_server(&self, port: u16) -> Result<()> {
        let manager = bedcode_server_websocket::WebSocketManager::global();
        // faces 由组合根装配（ws 面不认识 http 面，I1 靠注入而非引用）
        manager
            .start(port, crate::server::composition::transport_faces())
            .await
            .map(|_handle| ())
    }

    async fn stop_server(&self) -> Result<()> {
        let manager = bedcode_server_websocket::WebSocketManager::global();
        manager.stop().await.map(|_| ())
    }

    async fn connections_snapshot(&self) -> usize {
        bedcode_server_websocket::registry::WsSessionRegistry::global()
            .client_count()
            .await
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ServerLifecycleEvent> {
        bedcode_server_websocket::WebSocketManager::global().subscribe()
    }
}

// ==================== RuntimePort ====================

/// ambient runtime 句柄（`wasm_core::runtime_util::ambient_handle()` 包装）
pub struct HostRuntimePort;

impl RuntimePort for HostRuntimePort {
    fn ambient_handle(&self) -> tokio::runtime::Handle {
        crate::wasm_core::runtime_util::ambient_handle()
    }
}

// ==================== ConfigPort ====================

/// 网络配置（`AppConfig::global().network` 快照）
pub struct HostConfigPort;

impl ConfigPort for HostConfigPort {
    fn network(&self) -> NetworkConfig {
        crate::system::config::AppConfig::global().network.clone()
    }
}

// ==================== MdnsPort ====================

/// peer-net 发现守护接入（`bedcode-discovery-engine` 三函数包装）
///
/// **方向倒置已终结**（wasm-core-lib-split 票 03）：改造前三函数取自
/// `wasm_core::host_api::mdns`——即宿主 server 的端口层依赖 wasm_core 的**插件
/// 绑定模块**（ADR 0022 裁剪线要消除的方向）。现在直接依赖平台无关引擎 crate。
pub struct HostMdnsPort;

impl MdnsPort for HostMdnsPort {
    fn shared_daemon(&self) -> mdns_sd::ServiceDaemon {
        bedcode_discovery_engine::engine::shared_daemon()
    }

    fn register_host_service(&self, service_type: &str, fullname: &str) -> std::result::Result<String, String> {
        bedcode_discovery_engine::engine::register_host_service(
            &bedcode_discovery_engine::ports::ports(),
            service_type,
            fullname,
        )
    }

    fn stop_host_service(&self, advertise_id: &str) -> std::result::Result<bool, String> {
        bedcode_discovery_engine::engine::stop_host_service(advertise_id)
    }
}

// ==================== 装配便捷面 ====================

/// 宿主壳标准装配（bootstrap 调用；全部实现均为上述 Host* 单态包装）
///
/// `app_handle` 随装配传入供 [`HostPathsPort`] 直取（注册前的窗口也能解析路径）；
/// `None` = 无头 / 单测装配面（无句柄，路径面回退全局）。
pub fn assemble(app_handle: Option<Arc<tauri::AppHandle>>) -> ServerPorts {
    ServerPorts {
        plugin_invoker: Arc::new(HostPluginInvoker),
        auth_center: Arc::new(HostAuthCenter),
        bus: Arc::new(HostBusPort::late_bound(current_bus)),
        event_sink: Arc::new(HostEventSink),
        paths: Arc::new(HostPathsPort::new(app_handle)),
        system_info: Arc::new(HostSystemInfoPort),
        power: Arc::new(HostPowerPort),
        mdns_advertiser: Arc::new(HostMdnsAdvertiserPort),
        lifecycle: Arc::new(HostServerLifecyclePort),
        runtime: Arc::new(HostRuntimePort),
        config: Arc::new(HostConfigPort),
        mdns: Arc::new(HostMdnsPort),
    }
}

/// 当前插件消息总线（late-bound 解析面：端口可能早于 `AppContext` 注册被装配）
fn current_bus() -> Arc<crate::wasm_core::bus::MessageBus> {
    match AppContext::try_global() {
        Some(ctx) => ctx.plugin_host().message_bus().clone(),
        // 注册前窗口：占位总线（进程内同一实例，注册后就位即被真实总线取代）
        None => standby_bus(),
    }
}

/// 占位总线（`AppContext` 注册前的那一小段窗口用；注册后不再被取到）
fn standby_bus() -> Arc<crate::wasm_core::bus::MessageBus> {
    static STANDBY: std::sync::OnceLock<Arc<crate::wasm_core::bus::MessageBus>> = std::sync::OnceLock::new();
    STANDBY.get_or_init(|| Arc::new(crate::wasm_core::bus::MessageBus::new())).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 装配面可用性：未初始化 AppContext 时 `assemble()` 不 panic（占位总线）
    #[test]
    fn assemble_never_panics_without_app_context() {
        let _ports = assemble(None);
    }

    /// 占位总线在同一进程内是**同一实例**（late-bound 解析面在注册前反复取到它，
    /// 不能每次新建——否则注册前注册的静态订阅会散到不同总线上去）
    #[test]
    fn standby_bus_is_a_single_instance_per_process() {
        let first = standby_bus();
        let second = standby_bus();
        assert!(Arc::ptr_eq(&first, &second), "占位总线必须是进程级单例");
        assert!(Arc::ptr_eq(&current_bus(), &first), "无 AppContext 时 late-bound 解析面应返回占位总线");
    }

    /// 无 AppContext 时设备名取**真实**采集值，不得回退到硬编码占位名
    ///
    /// 契约来源：peer 节点把它写进 mDNS TXT（对端展示名）。回退占位名会让移动端
    /// 把桌面显示成「BedCode Desktop」，且没有一行日志能解释。
    #[test]
    fn device_name_without_app_context_is_real_collected_name() {
        let collected = crate::system::info::SystemInfo::collect().device_name;
        let actual = HostSystemInfoPort.device_name();
        assert!(!actual.is_empty(), "设备名不得为空（空串会让 TXT 记录失去意义）");
        assert_eq!(actual, collected, "无 AppContext 时设备名应与 bootstrap 同源现采，不得落占位名");
    }

    /// 无头装配面（无句柄、无 AppContext）：路径面显性报错而非 panic
    ///
    /// 生产窗口内的失败文案就是这个（修复前 `host-peer.start-node` 正是撞上它而
    /// 起不来节点）；这里钉住「报错文案不变、无头面仍可用」。
    #[test]
    fn headless_paths_port_reports_missing_runtime_context() {
        let port = HostPathsPort::new(None);
        assert!(port.handle().is_none(), "无头装配面不得凭空造出句柄");
        let app_data_err = port.app_data_dir().expect_err("无头面必须报错而不是 panic");
        assert!(
            app_data_err.to_string().contains("no runtime context"),
            "错误文案必须指向缺失的运行期上下文，实际：{app_data_err}"
        );
        let download_err = port.download_dir().expect_err("无头面必须报错而不是 panic");
        assert!(
            download_err.to_string().contains("no runtime context"),
            "错误文案必须指向缺失的运行期上下文，实际：{download_err}"
        );
    }
}
