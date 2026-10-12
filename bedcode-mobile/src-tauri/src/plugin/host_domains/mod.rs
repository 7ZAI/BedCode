//! core 域 WIT 实现（移动宿主 adapter；票 05 绑定归位收口）
//!
//! ## 为什么这 12 个接口的实现住移动宿主
//!
//! `core.wit` 的 17 个双端全等 interface 里，**实现按端不同**的有 12 个（本模块
//! 覆盖）+ 5 个移动独有域（`plugin/{notify,fs_mobile,terminal_stream,connection,auth}.rs`）。
//! 桌面这 12 个里属「双端全等语义」的 8 域由内核 `manager::runtime::core_binding`
//! 单份实现；移动不能复用那份，因为**权限门不同源**（移动 = 实例级
//! `granted_permissions` 集，桌面 = `PermissionManager` 桌面词表）且**插件私有库
//! 布局不同**（移动 `plugins/<id 去点划>.db`，内核 `plugins/<plugin_id>/plugin.db`）
//! ——直接切会让 storage / fs / db 门恒拒、既有插件私有库失联。
//! ⇒ 按「绑定层归位」原则（T-05），这 12 域的实现随宿主自报（`HostModule` +
//! `inventory`），行为与保留在 wasm-core `bare/host_impl` 时逐字一致。
//!
//! ## 结构
//!
//! - 每域一个文件：域逻辑（共享核 adapter）+ 该 interface 的 `Host` impl 同处；
//! - 域逻辑的机制语义在共享 crate（`bedcode-host-api-core` / `bedcode-server-http`
//!   / `bedcode-server-peer-net` / `bedcode-discovery-engine` / `bedcode-ws-client-engine`），
//!   本层只做「`WasmPluginState` → 端口」接线；
//! - 装配经 [`MobileHostCoreModule`] 自报：12 个 interface 一次注册，
//!   `expect_host_module!` 与自报同处（漏一处即装载期 `missing` / `unlisted` 点名）。

pub(crate) mod bus;
pub(crate) mod config;
pub(crate) mod db;
pub(crate) mod event;
pub(crate) mod fs;
pub(crate) mod http;
pub(crate) mod log;
pub(crate) mod mdns;
pub(crate) mod peer;
pub(crate) mod platform;
pub(crate) mod storage;
pub(crate) mod support;
pub(crate) mod ws;

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use crate::plugin::bindings::bedcode;

// ==================== 宿主上下文 / 运行时句柄 ====================

/// 从统一 state 取移动宿主上下文（类型不符即 panic——装配期编程错误，fail-visible）
///
/// 与 [`crate::plugin::bindings::ctx_of`] 同一实现：域逻辑与域 adapter 都按
/// `host_ctx(state)` 命名消费（wasm-core `bare` 形态的历史调用面逐字保留）。
pub(crate) use crate::plugin::bindings::ctx_of as host_ctx;

/// 取当前运行时句柄（无上下文线程回落 ambient）——旧 `state.runtime_handle` 的替代
///
/// host fn 通常运行在 tokio 上下文内（`Handle::try_current` 命中）；spawn_blocking /
/// 纯 std 线程上回落 `runtime_util::ambient_handle()`（multi_thread + enable_all，
/// 任务派生照常可用）。语义与 wasm-core `bare.rs` 的原实现逐字一致。
pub(crate) fn current_handle() -> tokio::runtime::Handle {
    tokio::runtime::Handle::try_current()
        .unwrap_or_else(|_| bedcode_wasm_core::runtime_util::ambient_handle())
}

// ==================== 停用回收聚合入口 ====================

/// 停用回收聚合入口（插件管理器 `deactivate_inner` 唯一调用点）
///
/// 逐域回收：mDNS 浏览/广播双表 + WS 出站连接表（各域只碰本人句柄，宿主与其它
/// 插件登记不受影响）+ 终端输出流窄转发表清空（输出流唯一消费者是终端插件，
/// 停用后无转发目标，残留表位只会让后续转发打到已失效的页面通道）。
/// 新域接入回收必须在此登记——漏登记 = 停用后连接泄漏（ADR 0022 §5.1.3
/// 「停用可回收」硬要求）。
pub fn purge_for_plugin(
    host_ctx: &bedcode_wasm_core::host_api::WasmHostContext,
    plugin_id: &str,
) -> usize {
    let mut purged = mdns::purge_for_plugin(&host_ctx.ports, plugin_id);
    purged += ws::purge_for_plugin(plugin_id);
    crate::plugin::terminal_stream::terminal_stream_gateway().clear_all();
    purged
}

// ==================== 能力模块自报 ====================

/// 能力模块名（白名单键 = 装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "mobile-host-core";

/// 本模块提供的 WIT 接口（必须与 `core.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &[
    "bedcode:plugin/host-log",
    "bedcode:plugin/host-storage",
    "bedcode:plugin/host-plugin-database",
    "bedcode:plugin/host-events",
    "bedcode:plugin/host-http",
    "bedcode:plugin/host-fs",
    "bedcode:plugin/host-config",
    "bedcode:plugin/host-bus",
    "bedcode:plugin/host-peer",
    "bedcode:plugin/host-mdns",
    "bedcode:plugin/host-platform",
    "bedcode:plugin/host-websocket",
];

/// 本模块读写的权限位（各域权限门的并集；装载期一致性核对用）
pub const MODULE_PERMISSIONS: &[&str] = &[
    "storage",
    "network:http",
    "fs:read",
    "fs:write",
    "bus",
    "peer",
    "mdns",
    "ws:client",
];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；禁带产品名词）
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    // 移动 ABI 20（core 12 域的当前契约面；接口自 v15 起未破坏性变更）
    abi_min: 20,
};

/// core 12 域能力模块
pub struct MobileHostCoreModule;

impl HostModule for MobileHostCoreModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(
        &self,
        linker: &mut wasmtime::component::Linker<WasmPluginState>,
    ) -> wasmtime::Result<()> {
        type D = wasmtime::component::HasSelf<WasmPluginState>;
        for iface in [
            bedcode::plugin::host_log::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_plugin_database::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_events::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_http::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_fs::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_config::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_bus::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_peer::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_mdns::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_platform::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_websocket::add_to_linker::<WasmPluginState, D>,
        ] {
            iface(linker, |s| s)?;
        }
        Ok(())
    }
}

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: MobileHostCoreModule = MobileHostCoreModule;

bedcode_host_kit::expect_host_module!(MODULE_NAME);

// 能力模块自报（自报静态住在宿主 lib = 最终二进制 ⇒ 无需强制引用行）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}
