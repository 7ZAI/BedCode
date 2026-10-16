//! core 域 WIT 实现（移动宿主 adapter；票 05 绑定归位收口 → 2026-10-13 单形态收口）
//!
//! ## 单形态（2026-10-13）后本模块只留**移动独有 / 端差异实现**
//!
//! `core.wit` 现有 12 个 interface（8 import + 4 export，ADR 0052 D1）：其中**双端全等
//! 语义的 8 域**（host-log / host-storage / host-plugin-database / host-events /
//! host-fs / host-config / host-bus / host-platform）由内核
//! `manager::runtime::core_binding` 单份实现（统一 Linker 装配；权限门收敛统一
//! `PermissionManager`，私有库布局统一内核 `plugins/<plugin_id>/plugin.db`）——
//! 移动宿主**不再自报**这 8 域（同一 interface 两份 `add_to_linker` 注册 ⇒ 装配期
//! `defined twice`）。
//!
//! 本模块保留**实现按端不同**的 4 个能力域（host-http / host-peer / host-mdns /
//! host-websocket）：契约真源已随 ADR 0052 D2 迁各能力域 crate 的 `wit/<domain>.wit`，
//! 由移动 compose caps 直接消费；端差异经移动宿主自报（`HostModule` +
//! `inventory`）；移动独有 5 域（`plugin/{notify,fs_mobile,terminal_stream,connection,auth}.rs`）
//! 各自自报。
//!
//! ## 结构
//!
//! - 每域一个文件：域逻辑（共享核 adapter）+ 该 interface 的 `Host` impl 同处；
//! - 域逻辑的机制语义在共享 crate（`bedcode-server-http` / `bedcode-server-peer-net`
//!   / `bedcode-discovery-engine` / `bedcode-ws-client-engine`），本层只做
//!   「`WasmPluginState` → 端口」接线；
//! - 装配经 [`MobileHostCoreModule`] 自报：4 个 interface 一次注册，
//!   `expect_host_module!` 与自报同处（漏一处即装载期 `missing` / `unlisted` 点名）。

pub(crate) mod http;
// mdns 对外可见（其余三域 pub(crate)）：宿主开机期要装一次**进程级**发现端口
// （`mdns::register_process_ports`，类型中枢的泵任务经它派生），无头探针
// （`tests/shared_artifact_probe`）也要走同一条生产装配入口
pub mod mdns;
pub(crate) mod peer;
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

/// 本模块提供的 WIT 接口（移动独有 / 端差异能力域；契约真源在各能力域 crate 的 wit 分片，
/// 接口名与拼装 package 内定义逐字一致）
pub const MODULE_INTERFACES: &[&str] = &[
    "bedcode:plugin@0.2.0/host-http",
    "bedcode:plugin@0.2.0/host-peer",
    "bedcode:plugin@0.2.0/host-mdns",
    "bedcode:plugin@0.2.0/host-websocket",
];

/// 本模块读写的权限位（各域权限门的并集；装载期一致性核对用）
pub const MODULE_PERMISSIONS: &[&str] = &[
    "network:http",
    "peer",
    "mdns",
    "ws:client",
];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；禁带产品名词）
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    // 移动 ABI 20（core 域的当前契约面；接口自 v15 起未破坏性变更）
    abi_min: 20,
};

/// 移动独有 / 端差异能力域能力模块（4 个；双端全等 8 域在内核 core_binding）
pub struct MobileHostCoreModule;

impl HostModule for MobileHostCoreModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    /// 端差异域实现：跨端**同进程**装配（cross-end rig：桌面 lib 与移动 lib 同时链入）
    /// 时让位给已安装的另一端实现（接口同名，重复注册即 `defined twice`）；
    /// 单端装配（生产移动宿主）无竞争，照常安装。
    fn fallback(&self) -> bool {
        true
    }

    fn register(
        &self,
        linker: &mut wasmtime::component::Linker<WasmPluginState>,
    ) -> wasmtime::Result<()> {
        type D = wasmtime::component::HasSelf<WasmPluginState>;
        for iface in [
            bedcode::plugin::host_http::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_peer::add_to_linker::<WasmPluginState, D>,
            bedcode::plugin::host_mdns::add_to_linker::<WasmPluginState, D>,
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
