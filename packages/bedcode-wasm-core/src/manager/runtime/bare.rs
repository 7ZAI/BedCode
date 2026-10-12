//! WASM 插件运行时（形态薄壳，票 05 起 · ADR 0049）
//!
//! 基于 wasmtime 组件模型（Component Model）的 WASM 插件加载、实例化、调用：
//! - 契约绑定：内核**只持 core 交集**（[`super::core_binding`]，绑
//!   `crate::wit/core.wit` 的 `world core`——双端全等的 17 个 interface）；
//!   移动端差异由移动宿主 `cap-mobile` 分片 + 宿主自报能力模块承载，**内核不
//!   直接绑定移动 world**（票 05 绑定归位，见 `component.rs` 模块头）
//! - 加载/实例化/调用实现在子模块 component（`bare/component.rs`）：
//!   linker 装配（宿主自报模块）+ `LoadedComponentPlugin` 导出调用面
//! - 本文件管理 Engine/Linker/Store 生命周期、资源限制与 AOT 缓存
//!
//! 安全机制（09 清理后原样保留）：
//! - 燃料看门狗：单次导出调用预算，宿主调用阻塞不消耗燃料
//! - ResourceLimiter：单插件线性内存 256MB / 表 1M 条，
//!   实例/内存/表数量封顶（防多内存声明放大 VA 占用）
//! - 线性内存预留即硬顶：`memory_reservation` 按估算的最大线性内存
//!   （= limiter 上限）预留 VA + `memory_may_move(false)` 杜绝搬移；
//!   栈深 512KiB 上限防递归打穿真实线程栈
//! - granted_permissions 校验（manifest.permissions，host 调用前检查）
//! - `abi.version()` 协商：插件版本高于宿主支持版本拒绝加载（fail-closed）
//! - AOT `.cwasm` 缓存：宿主 cache 目录（非插件目录，防产物投毒），
//!   组件产物统一 `c` 前缀命名（与桌面端一致）
//!
//! 与桌面端差异（移动契约独立演进，ADR 0018）：
//! - 移动独有域（host-notify / host-fs-mobile / host-terminal-stream /
//!   host-connection-target / host-auth-client / 移动 events）连同 12 个 core 域
//!   的域逻辑与 `Host` impl 一并住宿主
//!   `bedcode-mobile/src-tauri/src/plugin/host_domains/`，内核只留
//!   core 交集绑定（`core_binding`）与本壳
//! - 无 session / plugin-database / api-call / timer / process / app 等桌面域
//!   （`host-database` 主库面已随双端退役）
//! - 宿主上下文统一为 `crate::host_api::context::WasmHostContext`（票 02 B5
//!   上下文合一），无旧形态的 session_manager 字段；host-task/host-terminal-
//!   stream 等视形态归位

use std::path::PathBuf;
use std::sync::Arc;
use wasmtime::component::Linker;
use wasmtime::Engine;

// ==================== Component Model 路径 ====================
//
// 组件形态加载/校验/调用全部在子模块 component（bare/component.rs）：
// - linker 装配（宿主自报能力模块经 `ModuleRegistry` 收集 → 白名单双向校验
//   → `install_all`）+ WASI（[`super::wasi`]）
// - 导出调用面：core 交集导出走 `core_binding` + 原生探测 + 宿主注入的
//   `GuestEventDispatcher`（移动 events 导出带 `result`，无法原生探测）
// - **无 `bindgen!`、无 `impl … Host`**：票 05 已把移动端那份绑定与 12 域实现
//   全部迁到移动宿主（`src-tauri/src/plugin/bindings.rs` + `host_domains/`）
// - 自研 ABI core 路径（`__bedcode_*` 导出、(ptr,len) 内存搬运、签名表校验）
//   已在 ticket 09 删除，本文件为组件单路径
pub(crate) mod component;

pub use component::LoadedComponentPlugin;
/// 宿主垫片面：`crate::plugin::wasm_runtime::WasmHostContext` 路径经此转发。
/// 真源 = `host_api::context`（票 02 B5 上下文合一：统一类型，原
/// `bare_context` 已删除）
pub use crate::host_api::context::WasmHostContext;

// ==================== Resource Limits & Interruption ====================
//
// 资源上限/燃料常量与 `ResourceLimiter` 实现统一在 host-kit（`state.rs` 的
// `impl ResourceLimiter for WasmPluginState` / `limits.rs` 的常量）——本节
// 不再自持副本（wasm-core-mobile-purity 票 04：单一 state 形态）。燃料预算的
// 取用点（Store 装配注入 / ABI 协商注入 / 每次导出调用续费）与 **Engine 构建
// 参数**均已收敛到 `runtime::engine`，与桌面形态同源（含 debug 构建的放大倍率
// 与 backtrace 帧数）。

use crate::config::plugin_debug_mode;
use crate::manager::runtime::engine;

/// WASM 插件运行时（全局共享）
///
/// Engine 和 Linker 是线程安全的可复用结构：
/// - Engine: WASM 编译器，全局单例
/// - Linker: 组件 import 接口注册表，所有插件实例共享（见 component 子模块）
///
/// 宿主 `PluginManager` 持有并消费（crate 单独编译时字段读取面在宿主侧）
#[allow(dead_code)] // 宿主消费面：crate 单独编译无内部读取方
pub struct WasmRuntime {
    engine: Engine,
    /// 组件模型 Linker（组件 import 接口注册表，见 component 子模块）
    linker: Linker<WasmPluginState>,
    /// AOT 编译产物（`.cwasm`）缓存目录（宿主 cache 目录，非插件目录）
    ///
    /// 插件目录可被安装方/插件自身写入，若把反序列化产物放回插件目录，
    /// 能写插件目录的攻击者可投放伪造产物触发宿主进程 UB
    /// （`Component::deserialize_file` 是 unsafe，假定数据可信）。
    aot_cache_dir: Option<PathBuf>,
    /// 内核配置快照（Engine 参数在构建期固化、WASI 注册面同；预打开装配每次实例化读）
    ///
    /// 移动形态此前只用 `CoreConfig::default()`（无外部配置面）；用户裁决
    /// 2026-10-12 后 WASI 面也由配置定（默认全开），故壳持有配置本身——
    /// 宿主经 [`WasmRuntime::with_config`] 注入（`new` 即默认配置，行为不变）。
    config: crate::config::CoreConfig,
    /// 移动 guest 事件派发器（票 05 / ADR 0022）
    ///
    /// 缺省 `UnwiredGuestEvents`（派发即 fail-visible）；生产由
    /// `PluginManager::init_wasm_runtime_with` 注入移动实现
    /// （`plugin/guest_events.rs`，与 message_bus dispatcher 同批装配）。
    /// 已加载实例持有创建时的快照（见 `component::instantiate_component`）。
    guest_events: std::sync::Arc<
        std::sync::RwLock<Arc<dyn crate::manager::runtime::guest_events::GuestEventDispatcher>>,
    >,
}

/// 单个 WASM 插件实例的状态（真源 = host-kit，双形态单一 state）
///
/// 每插件实例化时创建独立 `Store<WasmPluginState>`；宿主上下文经
/// `Arc<dyn HostPorts>`（向下转型还原移动宿主上下文，见 `host_ctx` 访问器）+
/// 可选导出探测句柄，字段面与桌面形态逐字一致（wasm-core-mobile-purity 票 04）。
pub use bedcode_host_kit::state::{StoreSpec, WasmPluginState};

// 宿主上下文取用（旧 `host_ctx`）与运行时句柄（旧 `current_handle`）随域实现
// 迁移动宿主（票 05 绑定归位）：真源 = `bedcode-mobile/src-tauri/src/plugin/
// host_domains/mod.rs`（同一向下转型 + 同一 ambient 兜底语义）。

// WasmHostContext 真源 = `crate::host_api::context`（批次 2 迁入后单一定义；
// 本文件经 use 引用，见文件头 import）。

/// AOT 缓存键与组件加载（编译 + 缓存写回 / 命中）已迁`crate::manager::runtime::engine`
/// （双端唯一一份）；本壳只保留形态接线（Engine 构建、linker 装配、宿主上下文接线）。

impl WasmRuntime {
    /// 创建 WASM 运行时
    ///
    /// 初始化 Engine、组件 Linker（注册共有机制域 + 宿主自报域）。
    /// Host Function 的同步↔异步桥统走 `crate::runtime_util::block_on_async`
    /// （自取 runtime 上下文 / ambient 兜底）——本结构不再持运行时句柄。
    ///
    /// `aot_cache_dir`：AOT 编译产物缓存目录（宿主 cache 目录），
    /// None 时禁用文件级 AOT 缓存（退化为纯编译）
    pub fn new(aot_cache_dir: Option<PathBuf>) -> crate::Result<Self> {
        // 编译期默认配置：与历史行为逐值一致（WASI 面默认全开）
        Self::with_config(aot_cache_dir, crate::config::CoreConfig::default())
    }

    /// 以指定内核配置创建运行时（外部配置面，用户裁决 2026-10-12）
    ///
    /// WASI 面（[`crate::config::WasiConfig`]）与 Engine 参数一样是**构建期固化**：
    /// p2/p3 在 linker 一次注册，运行中改配置不生效（改需重建运行时）；
    /// 「预打开目录装配」则是每次实例化读配置（见 `wasi::build_ctx`）。
    ///
    /// Engine 参数：桌面形态的 `CoreConfig` 是唯一真源（ADR 0045 D5——能力域 /
    /// 形态壳不得自带配置面），移动形态消费**同一份** `build_engine_config`。
    /// 移动插件同为 wasmtime 组件模型消费者，燃料看门狗 / 内存预留 / 栈深度 /
    /// 编译缓存 / backtrace 帧数与桌面逐值一致，不再各钉一份。
    pub fn with_config(
        aot_cache_dir: Option<PathBuf>,
        core_config: crate::config::CoreConfig,
    ) -> crate::Result<Self> {
        if let Err(reason) = core_config.validate() {
            return Err(crate::AppError::Config(format!("内核配置非法: {}", reason)));
        }
        let engine_config = engine::build_engine_config(&core_config, plugin_debug_mode());
        let engine = Engine::new(&engine_config)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e)))?;

        let linker = component::build_component_linker(&engine, &core_config.wasi)?;

        Ok(Self {
            engine,
            linker,
            aot_cache_dir,
            config: core_config,
            guest_events: std::sync::Arc::new(std::sync::RwLock::new(Arc::new(
                crate::manager::runtime::guest_events::UnwiredGuestEvents,
            )
                as Arc<dyn crate::manager::runtime::guest_events::GuestEventDispatcher>)),
        })
    }

    /// 注入移动 guest 事件派发器（生产在 `PluginManager::init_wasm_runtime_with`）
    ///
    /// **必须在首次 `instantiate_component` 之前调用**：已加载实例持有创建时的
    /// 派发器快照。漏注入的后果是事件派发 fail-visible 报错（不是静默丢消息）。
    pub fn set_guest_events(
        &self,
        dispatcher: Arc<dyn crate::manager::runtime::guest_events::GuestEventDispatcher>,
    ) {
        *self.guest_events.write().unwrap_or_else(|e| e.into_inner()) = dispatcher;
    }

    /// 测试访问器：Engine 引用（组件编译与实例化必须同一 Engine 实例，
    /// 跨 Engine 实例化 wasmtime 直接拒绝）
    #[cfg(any(test, feature = "test-support"))]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

// ==================== Host 能力实现层（已迁移动宿主） ====================
//
// 旧的 `bare/host_impl/`（12 个 core 域 adapter + 停用回收聚合）随票 05 绑定归位
// 整体迁出：真源 = `bedcode-mobile/src-tauri/src/plugin/host_domains/`
// （各域逻辑 + 对应 interface 的 `Host` impl 同处，经 `mobile-host-core` 模块自报）。
// 内核侧只留本壳（Engine/Linker/Store 生命周期 + 导出调用骨架）。

// ==================== Async Blocking Helper ====================

// 同步↔异步桥统一走 `crate::runtime_util::block_on_async`（双端唯一一份：
// block_in_place 让出 / 重入检测 / current_thread 与无上下文线程的 ambient
// 兜底；本文件旧的自持副本与 3 个同源测试随 wasm-core-mobile-purity 票 04
// state 换型删除——旧副本要求外传 runtime_handle，而统一 state 不再携带它）。
// 原移动副本与 runtime_util 版的语义差（无上下文线程用应用 handle 阻塞 vs
// ambient runtime 阻塞）在 multi_thread/current_thread 两个主路径上等价。
