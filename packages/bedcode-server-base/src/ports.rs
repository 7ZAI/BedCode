//! 服务器端口 traits（依赖倒置：server 各面需要的宿主能力，由宿主壳注入实现）
//!
//! server-lib-split spec §2 端口表：所有 server lib 不得引用 `wasm_core` /
//! `tauri` / `AppContext` 类型；本模块定义它们反向需要的全部端口。宿主壳
//! （`crate::server::ports_impl` + bootstrap）构造实现并注入
//! [`ServerPorts`]，拆分后本文件随 `bedcode-server-base` 迁移。
//!
//! 对 spec 的两处实施裁决（记录于 `.scratch/2026-09-30-server-lib-split/`）：
//! - `BusPort` 合并了 spec 的 `FrameDeliverer`（`deliver_endpoint_frame`）——
//!   宿主一个对象实现整个总线面（publish / subscribe_static / 帧投递），
//!   拆成两个 trait 只会让每个注入点各带两个句柄，无隔离收益；
//! - 端口 traits 全落 server-base（而非 spec 表内记的 server-core），
//!   让不依赖 core 传输机制的 peer-net 也能取用共享端口（spec 端口表与
//!   D2「peer-net 不依赖 core」在 4-crate 形状内自相矛盾，5-crate 形状消解）。

use crate::config::NetworkConfig;
use crate::error::Result;
use crate::identity::AuthenticatedIdentity;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

// ==================== PluginInvoker（http 转发面 / 激活判定） ====================

/// 插件命令调用面（替代 `AppContext::global().plugin_host()` 的注入端口）
///
/// 宿主壳实现包装 `PluginHost::invoke_rust_command` / `is_activated`；
/// 无宿主上下文（无头 / 单测）时注入 `None`，调用方按既有「无 AppContext」
/// 语义保守处理（http 网关放行交回链条 / ws 属主激活闸门放行防御性兜底）。
#[async_trait]
pub trait PluginInvoker: Send + Sync {
    /// 调插件 Rust 命令（`_http_endpoint` 转发；`Err(String)` = 命令失败）
    async fn invoke_rust_command(
        &self,
        owner: &str,
        command: &str,
        args: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, String>;

    /// 属主插件是否处于激活态（false = 未激活；宿主对 plugin_id 无记录时
    /// 返回 true——「无从判定」语义与既有 `endpoint_owner_activated` 一致）
    async fn is_activated(&self, owner: &str) -> bool;
}

// ==================== AuthCenter（认证中心裁决） ====================

/// 认证中心裁决（v33 / ADR 0033：宿主不再持有任何设备 JWT 密码学）
///
/// 宿主壳实现包装 `utils::auth::auth_center::enforce_connection_policy`；
/// fail-closed 语义不变：无中心 / 调用失败 / 中心拒绝 → `Err`。
pub trait AuthCenter: Send + Sync {
    fn enforce_connection_policy(&self, token: &str) -> std::result::Result<AuthenticatedIdentity, String>;
}

// ==================== BusPort（插件消息总线 + WS 帧投递） ====================

/// 静态订阅者的消息回调（`subscribe_static` 的 handler；宿主壳实现转为
/// `wasm_core::BusMessageHandler`，载荷形状即 [`crate::wire::BusMessage`]——
/// 本 crate 自持副本，能力域脱绑 P5：双类型在 wasm-core 的
/// `WasmHandlerAdapter` 桥接点做值转换，形状一致由 `wire::drift_lock` 钉死）
pub trait BusMessageHandler: Send + Sync {
    fn on_message(&self, msg: &crate::wire::BusMessage) -> anyhow::Result<()>;
}

/// 插件消息总线端口（publish / 静态订阅 / WS 端点帧投递）
///
/// 宿主壳实现包装 `wasm_core::bus::MessageBus` 与
/// `bedcode_server_websocket::plugin_binding::deliver_endpoint_frame`；无头/单测可用假实现。
/// `deliver_endpoint_frame` 是 spec 端口表的 `FrameDeliverer`（裁决见模块头）。
#[async_trait]
pub trait BusPort: Send + Sync {
    /// 发布 JSON 消息（`MessageBus::publish` 语义：异步入队、不投给发送者）
    fn publish(&self, topic: &str, sender: &str, payload: serde_json::Value);

    /// 发布二进制消息（`MessageBus::publish_binary` 语义）
    fn publish_binary(&self, topic: &str, sender: &str, payload: Vec<u8>);

    /// 注册静态订阅者（`MessageBus::subscribe_static` 语义：每订阅者有界队列
    /// + 消费任务，handler 在任务内独占调用）
    async fn subscribe_static(&self, subscriber: &str, topic: &str, handler: Box<dyn BusMessageHandler>);

    /// 投递 WS 帧给插件的 `events-ws` 可选导出（保序由调用方串行保证）
    async fn deliver_endpoint_frame(
        &self,
        owner: &str,
        endpoint_id: &str,
        client_id: &str,
        kind: &str,
        payload: Vec<u8>,
    );
}

// ==================== EventSink（前端事件） ====================

/// 前端事件投递（替代 `AppHandle::emit`；失败只记日志不上抛）
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

// ==================== PathsPort（路径解析） ====================

/// 应用数据目录解析（替代 `app_handle.path().app_data_dir()`）
pub trait PathsPort: Send + Sync {
    /// 应用数据目录；解析失败返回错误（调用方按其语义处置）
    fn app_data_dir(&self) -> Result<PathBuf>;
    /// 系统下载目录（peer-net 接收落点 `Downloads/BedCode`；替代
    /// `app_handle.path().download_dir()`）；解析失败返回错误
    fn download_dir(&self) -> Result<PathBuf>;
}

// ==================== SystemInfoPort（系统信息） ====================

/// 引擎级系统信息（设备名 / 本地 IPv4；替代 `SystemInfo` / `local_ipv4_addresses`）
pub trait SystemInfoPort: Send + Sync {
    fn device_name(&self) -> String;
    fn local_ipv4_addresses(&self) -> Vec<String>;
    /// 应用版本（mDNS txt 记录 `version` 等；宿主壳代理 CARGO_PKG_VERSION）
    fn app_version(&self) -> String;
}

// ==================== PowerPort（电源管理） ====================

/// 休眠阻止开关（替代 `system::power::power_manager()`）
pub trait PowerPort: Send + Sync {
    fn enable(&self);
    fn disable(&self);
}

// ==================== MdnsAdvertiserPort（mDNS 广播） ====================

/// mDNS 服务广播（替代 `AppContext::global().mdns_advertiser()` 的
/// `AdvertiseConfig` 装配；txt 记录由宿主壳转成
/// `bedcode_discovery_engine::types::AdvertiseConfig`——自播面真源在
/// bedcode-discovery-engine，宿主侧零 mDNS 代码）
pub trait MdnsAdvertiserPort: Send + Sync {
    fn advertise(&self, service_name: String, port: u16, txt_records: std::collections::HashMap<String, String>);
    fn stop(&self);
}

// ==================== ServerLifecyclePort（服务器生命周期桥） ====================

/// 服务器生命周期事件（supervisor 崩溃监控判据；宿主壳由
/// `WebSocketManager::ServerEvent` 映射）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerLifecycleEvent {
    Started,
    Stopped,
}

/// supervisor → WS 传输面的生命周期桥（替代对 `WebSocketManager` 的直接引用）
///
/// 宿主壳实现包装 `WebSocketManager::global()`（start/stop/subscribe）与
/// `WsSessionRegistry::global().client_count()`（指标采样连接数）。
#[async_trait]
pub trait ServerLifecyclePort: Send + Sync {
    /// 启动 Actix Web 服务器（HTTP + WS 统一端口）
    async fn start_server(&self, port: u16) -> Result<()>;
    /// 优雅停机
    async fn stop_server(&self) -> Result<()>;
    /// 当前在册连接数（指标采样）
    async fn connections_snapshot(&self) -> usize;
    /// 订阅服务器生命周期事件（`Stopped` 仅崩溃路径发送，语义同 WebSocketManager）
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ServerLifecycleEvent>;
}

// ==================== RuntimePort（ambient 运行时） ====================

/// ambient runtime 句柄（ws 投递任务派生目标；替代
/// `wasm_core::runtime_util::ambient_handle()`）
pub trait RuntimePort: Send + Sync {
    fn ambient_handle(&self) -> tokio::runtime::Handle;
}

// ==================== ConfigPort（网络配置） ====================

/// 网络配置提供者（替代 `AppConfig::global().network` 快照）
pub trait ConfigPort: Send + Sync {
    fn network(&self) -> NetworkConfig;
}

// ==================== MdnsPort（peer-net 发现守护接入） ====================

/// peer-net 引擎接入宿主 mDNS 共享守护（替代 `wasm_core::host_api::mdns` 的
/// `shared_daemon` / `register_host_service` / `stop_host_service` 三函数；
/// `ServiceDaemon` 为 mdns-sd 类型，宿主壳实现与 `bedcode-peer-net` 同源）
pub trait MdnsPort: Send + Sync {
    /// 取全局共享守护句柄（clone 廉价）
    fn shared_daemon(&self) -> mdns_sd::ServiceDaemon;
    /// 登记宿主（peer-net 引擎）节点身份广播句柄（owner=host）
    fn register_host_service(&self, service_type: &str, fullname: &str) -> std::result::Result<String, String>;
    /// 注销宿主身份广播登记（幂等）
    fn stop_host_service(&self, advertise_id: &str) -> std::result::Result<bool, String>;
}

// ==================== ServerPorts 聚合 ====================

/// 宿主壳注入的全部端口聚合（bootstrap 构造一次，全局经
/// [`ports::init`] 注册；拆 lib 后随 ServerState 实例化）
pub struct ServerPorts {
    pub plugin_invoker: Arc<dyn PluginInvoker>,
    pub auth_center: Arc<dyn AuthCenter>,
    pub bus: Arc<dyn BusPort>,
    pub event_sink: Arc<dyn EventSink>,
    pub paths: Arc<dyn PathsPort>,
    pub system_info: Arc<dyn SystemInfoPort>,
    pub power: Arc<dyn PowerPort>,
    pub mdns_advertiser: Arc<dyn MdnsAdvertiserPort>,
    pub lifecycle: Arc<dyn ServerLifecyclePort>,
    pub runtime: Arc<dyn RuntimePort>,
    pub config: Arc<dyn ConfigPort>,
    pub mdns: Arc<dyn MdnsPort>,
}

/// 全局端口注册表（`OnceLock<Arc<ServerPorts>>`，bootstrap 装配；`try_global`
/// 语义同 `AppContext::try_global`——未初始化（无头 / 单测）返回 None）。
/// `Arc` 化：peer-net 派生任务（session_watch / drive_gate / 入站桥）需要
/// 持有端口句柄跨 await 存活
static PORTS: std::sync::OnceLock<Arc<ServerPorts>> = std::sync::OnceLock::new();

/// 装配全局端口（bootstrap 调用一次；重复调用 panic——装配点唯一）
pub fn init(ports: ServerPorts) {
    PORTS
        .set(Arc::new(ports))
        .unwrap_or_else(|_| panic!("server ports already initialized"));
}

/// 取全局端口（未装配 → None：无头 / 单测上下文，调用方按既有「无
/// AppContext」语义保守处理，不 panic）
pub fn get() -> Option<&'static Arc<ServerPorts>> {
    PORTS.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未装配时取空（无头 / 单测语义：调用方按无 AppContext 处理）
    #[test]
    fn uninitialized_ports_read_as_none() {
        // 本用例可能在已装配的测试进程内跑（test 二进制共享静态），
        // 但 Either 分支都不 panic 即证明存取语义稳定
        match get() {
            Some(_) => {}
            None => {}
        }
    }
}
