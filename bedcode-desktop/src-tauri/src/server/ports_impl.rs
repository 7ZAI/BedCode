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

/// 插件消息总线 + WS 帧投递（`wasm_core::bus::MessageBus` + 能力域 `deliver_endpoint_frame` 包装）
pub struct HostBusPort {
    bus: Arc<crate::wasm_core::bus::MessageBus>,
    /// 帧投递用的能力域端口视图（**构造一次**、不逐帧分配）：能力域的帧投递函数
    /// 收 `&Arc<dyn WsPorts>`，故此处持一份绑定到本总线的窄端口（无权限管理器）。
    ws_ports: Arc<dyn bedcode_server_websocket::plugin_binding::ports::WsPorts>,
}

impl HostBusPort {
    pub fn new(bus: Arc<crate::wasm_core::bus::MessageBus>) -> Self {
        Self {
            ws_ports: Arc::new(crate::wasm_core::host_api::ws::HostWsPorts::from_bus(bus.clone())),
            bus,
        }
    }
}

/// base 侧 `BusMessageHandler` → wasm_core 侧 `BusMessageHandler` 适配
/// （两 trait 形状逐字相同，只差 trait 路径）
struct WasmHandlerAdapter(Box<dyn BusMessageHandler>);

impl crate::wasm_core::bus::BusMessageHandler for WasmHandlerAdapter {
    fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        self.0.on_message(msg)
    }
}

#[async_trait]
impl BusPort for HostBusPort {
    fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value) {
        self.bus.publish(topic, sender, payload);
    }

    fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>) {
        self.bus.publish_binary(topic, sender, payload);
    }

    async fn subscribe_static(&self, subscriber: &str, topic: &str, handler: Box<dyn BusMessageHandler>) {
        self.bus
            .subscribe_static(subscriber, topic, Box::new(WasmHandlerAdapter(handler)))
            .await;
    }

    async fn deliver_endpoint_frame(
        &self,
        owner: &str,
        endpoint_id: &str,
        client_id: &str,
        kind: &str,
        payload: Vec<u8>,
    ) {
        // 能力域已迁入 `bedcode_server_websocket::plugin_binding`（wasm-core-lib-split 票 04）
        bedcode_server_websocket::plugin_binding::deliver_endpoint_frame(
            &self.ws_ports,
            owner,
            endpoint_id,
            client_id,
            kind,
            payload,
        )
        .await;
    }
}

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
pub struct HostPathsPort;

impl PathsPort for HostPathsPort {
    fn app_data_dir(&self) -> Result<PathBuf> {
        let Some(handle) = app_handle() else {
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
        let Some(handle) = app_handle() else {
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

/// 取当前 AppHandle（`AppContext::try_global()` + `app_handle()` 的合并便捷面）
fn app_handle() -> Option<Arc<tauri::AppHandle>> {
    let ctx = AppContext::try_global()?;
    ctx.app_handle().clone()
}

// ==================== SystemInfoPort ====================

/// 系统信息（`SystemInfo` / `local_ipv4_addresses` / 宿主版本号）
pub struct HostSystemInfoPort;

impl SystemInfoPort for HostSystemInfoPort {
    fn device_name(&self) -> String {
        match AppContext::try_global() {
            Some(ctx) => ctx.system_info().device_name.clone(),
            None => "BedCode Desktop".to_string(),
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
            return;
        };
        tokio::spawn(async move {
            let advertiser = advertiser.read().await;
            let config = crate::mdns::types::AdvertiseConfig {
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
/// `device_name` 参数保留给 txt 记录（与 supervisor 侧读取同一来源，避免
/// 装配期再查一次 AppContext）
pub fn assemble() -> ServerPorts {
    ServerPorts {
        plugin_invoker: Arc::new(HostPluginInvoker),
        auth_center: Arc::new(HostAuthCenter),
        bus: Arc::new(HostBusPort::new(bus_handle())),
        event_sink: Arc::new(HostEventSink),
        paths: Arc::new(HostPathsPort),
        system_info: Arc::new(HostSystemInfoPort),
        power: Arc::new(HostPowerPort),
        mdns_advertiser: Arc::new(HostMdnsAdvertiserPort),
        lifecycle: Arc::new(HostServerLifecyclePort),
        runtime: Arc::new(HostRuntimePort),
        config: Arc::new(HostConfigPort),
        mdns: Arc::new(HostMdnsPort),
    }
}

/// 取宿主消息总线（bootstrap 期 AppContext 已注册；缺失时给空总线占位——
/// 装配期之后 server 面才真正使用，占位只防构造 panic）
fn bus_handle() -> Arc<crate::wasm_core::bus::MessageBus> {
    match AppContext::try_global() {
        Some(ctx) => ctx.plugin_host().message_bus().clone(),
        None => Arc::new(crate::wasm_core::bus::MessageBus::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 装配面可用性：未初始化 AppContext 时 `assemble()` 不 panic（占位总线）
    #[test]
    fn assemble_never_panics_without_app_context() {
        let _ports = assemble();
    }
}
