//! BedCode 组播发现能力域（`host-mdns`，5 条原语）
//!
//! 一个「可独立组合的宿主能力」的完整样板：mDNS 引擎、宿主绑定层、以及经
//! [`bedcode_host_kit`] 自动注册表的装配入口，全部住在这一个 crate 里，
//! `wasm_core` 侧只剩一个开机期的端口装配调用。
//!
//! ## 分层
//!
//! ```text
//!   engine.rs      机制：共享守护 + 浏览/广播双句柄表（零宿主依赖）
//!   advertiser.rs  机制：宿主自播面（`MdnsAdvertiser`；登记/停播走 engine 共享守护，
//!                   owner=host，与插件 advertise / peer-net 身份同守护）
//!   types.rs       契约：`SERVICE_TYPE` 常量 + `AdvertiseConfig` + `AdvertiserError`（自持错误，不反向依赖宿主）
//!   wire.rs        wire 契约词汇自持副本（发现事件名 / topic 命名规则；桌面 SDK 漂移锁）
//!   ports.rs       边界：宿主能力端口（权限门 / 本机节点 ID / 事件投递 / 后台任务）
//!   lib.rs         接线：`bindgen!` 生成的 Host trait 实现 + `HostModule` 自报
//! ```
//!
//! **脱绑（能力域脱绑 P2）**：WIT 绑定层（`bindgen!` / `HostModule` / `inventory::submit!` /
//! `impl Host`）以 `#[cfg(feature = "desktop-host")]` 门控——桌面宿主开 feature 后行为
//! 逐字不变（ABI / WIT / 权限位 / 事件形状 / inventory 注册零变化），无 feature 的宿主
//! （移动端 / 无头）拿纯引擎：机制（engine/advertiser/types）、端口（ports）、装配
//! （`install` / `install_ports`）默认全部可用。
//!
//! 判定该域「非 POSIX 原生」的依据：WASI 预览 3 只提供
//! `cli / clocks / filesystem / random / sockets`，组播 DNS 是应用层协议
//! （spec §3.1 判据表）。
//!
//! ## 红线（AGENTS §5.1）
//!
//! 本 crate **不含任何产品概念**：无「设备」「会话」「配对」「传输」等名词。
//! 浏览/广播是**纯引擎参数透传**（serviceType / port / TXT 原样携带），业务含义
//! 由插件自己解释——这是 §5.1.3「引擎实现」允许进宿主的形态。属主隔离、事件定向
//! 投递、停用回收三项语义在 [`engine`] 中逐字保留。

use std::sync::Arc;

pub mod engine;
pub mod ports;
/// wire 契约词汇自持副本（能力域脱绑 P2：发现事件名 / topic 命名规则，
/// 与桌面 SDK 原版逐字一致由 `wire::drift_lock` 钉死）
pub mod wire;

/// 自播面：宿主把本机 `_bedcode._tcp.local.` 服务广播到局域网供移动端发现
/// （自 `src-tauri/src/mdns/` 迁入；配置校验与错误文案逐字保留，守护模型随后并入
/// engine 共享守护——见 `advertiser` 模块头的「共享守护模型」段）
pub mod advertiser;
/// 自播面契约：服务类型常量 / 广播配置 / 自持错误
pub mod types;

pub use advertiser::MdnsAdvertiser;
pub use ports::{install_ports, DiscoveryPorts, DiscoveryTask};

// ==================== 能力模块自报（WIT 绑定层，`desktop-host` feature） ====================

// WIT 绑定层依赖（host-kit / wasmtime）只随 `desktop-host` feature 编译：
// 能力域默认形态 = 纯引擎机制（零 WIT 依赖），任何宿主可直接引用。
#[cfg(feature = "desktop-host")]
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
#[cfg(feature = "desktop-host")]
use wasmtime::component::{bindgen, Linker};

/// 能力模块描述符（只描述机制，禁带产品名词——AGENTS §5.1 B1/B5）
#[cfg(feature = "desktop-host")]
const DESC: HostModuleDesc = HostModuleDesc {
    name: "discovery",
    interfaces: &["bedcode:plugin/host-mdns"],
    permissions: &["network:mdns"],
    abi_min: 1,
};

/// mDNS 能力域模块
#[cfg(feature = "desktop-host")]
pub struct DiscoveryModule;

#[cfg(feature = "desktop-host")]
impl HostModule for DiscoveryModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_mdns::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与宿主既有接线同款）
#[cfg(feature = "desktop-host")]
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `submit_module!` 取址）
#[cfg(feature = "desktop-host")]
static MODULE: DiscoveryModule = DiscoveryModule;

// 能力模块自报（linker-section 静态）
//
// **依赖前提**：宿主必须有一行 `use bedcode_discovery_engine as _;` 强制引用
// （见 `wasm_core::manager::runtime::component`），否则本 rlib 不进最终二进制、
// 静态不执行 ⇒ 注册丢失，且 guest 会在实例化期报「无该 import」。
// 无 `desktop-host` feature 的宿主（移动端 / 无头）**不应**注册——它没有插件宿主
// 机制，桌面侧强制引用行同步 `#[cfg(feature = "desktop-host")]`。
#[cfg(feature = "desktop-host")]
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

#[cfg(feature = "desktop-host")]
bindgen!({
    // provider 侧绑定：spec D8「能力 crate 不自带 generate!」的前提在本 crate
    // 不成立——宿主自己的 `bedcode` 模块是 **guest 视角**（import 是调用函数，
    // 不是 `Host` trait + `add_to_linker`）。能力 crate 要自己装配就必须生成
    // provider 侧。
    //
    // ⚠️ 由此产生的**硬约束**：本 crate 与宿主各自生成的
    // `bedcode::plugin::host_mdns::Host` 是**同名但不同类型**的 trait。宿主必须
    // 同时删掉自己的 mdns `Host` impl 与 `add_to_linker` 行，否则同一个
    // interface 被注册两次 → 装配期 `defined twice`。
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
    // 与宿主同款：全部导出绑定生成 async 变体（wasmtime async store 要求）
    exports: { default: async },
});

// ==================== 宿主绑定层（Host trait 实现，`desktop-host` feature） ====================
//
// 每个接口方法 = 一条 WIT 原语。权限门在 [`engine`] 内的域函数里（随实现
// 同迁），此层只做「拿端口 → 转调 → 按 WIT `result` 形状返回」。

#[cfg(feature = "desktop-host")]
impl bedcode::plugin::host_mdns::Host for WasmPluginState {
    fn browse(&mut self, service_type: String) -> Result<String, String> {
        engine::browse(&ports::ports(), &self.plugin_id, &service_type)
    }

    fn stop_browse(&mut self, browser_id: String) -> Result<bool, String> {
        engine::stop_browse(&ports::ports(), &self.plugin_id, &browser_id)
    }

    fn advertise(&mut self, config_json: String) -> Result<String, String> {
        engine::advertise(&ports::ports(), &self.plugin_id, &config_json)
    }

    fn stop_advertise(&mut self, advertise_id: String) -> Result<bool, String> {
        engine::stop_advertise(&ports::ports(), &self.plugin_id, &advertise_id)
    }

    fn is_advertising(&mut self, advertise_id: String) -> Result<bool, String> {
        engine::is_advertising(&ports::ports(), &self.plugin_id, &advertise_id)
    }
}

/// 装配端口的便捷入口（宿主开机期调用）
///
/// 传实现即可；实现方应当是零大小类型（[`ports::install_ports`] 会把它放进
/// `Arc`，浏览事件循环要长期持有）。
pub fn install<P: DiscoveryPorts + 'static>(ports: P) {
    install_ports(Arc::new(ports));
}
