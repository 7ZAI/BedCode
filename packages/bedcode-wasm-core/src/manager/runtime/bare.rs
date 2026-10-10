//! WASM 插件运行时（移动端）
//!
//! 基于 wasmtime 组件模型（Component Model）的 WASM 插件加载、实例化、调用：
//! - 契约单一事实来源：SDK `packages/plugin-sdk-mobile/rust/wit/bedcode.wit`
//!   （移动端独立 WIT，11 import / 8 export，wasmtime 48 自带 bindgen! 宏）
//! - 加载/实例化/调用实现在子模块 component（wasm_runtime/component.rs）：
//!   Host trait 接线（11 组）、LoadedComponentPlugin 业务方法
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
//! 与桌面端差异：
//! - WasmHostContext 无 session_manager 和 permission
//! - host-notify 域（移动特有，ABI v18：通知/震动/声音——收编原 host-events.notify）
//! - host-terminal（send）已随票 15 阶段 B 退役（ABI v17，终端 UI 域迁插件）
//! - 无 session/plugin-database/params/api-call/timer/process 接口
//! - 新增 host_mark_plugin_error（插件生命周期失败上报，置 Error + 持久化未启用）

use std::path::PathBuf;
use wasmtime::component::Linker;
use wasmtime::Engine;

// ==================== Component Model 路径 ====================
//
// 组件形态加载/校验/调用全部在子模块 component（wasm_runtime/component.rs）：
// - bindgen! 绑定（wasmtime 48 自带宏）、17 组 Host trait 接线（v18）
// - LoadedComponentPlugin 业务方法（ticket 03）
// - 自研 ABI core 路径（`__bedcode_*` 导出、(ptr,len) 内存搬运、签名表校验）
//   已在 ticket 09 删除，本文件为组件单路径
pub(crate) mod component;

pub use component::LoadedComponentPlugin;
/// 宿主垫片面：`crate::plugin::wasm_runtime::WasmHostContext` 路径经此转发。
/// 真源 = `host_api::bare_context`（fork host_api/context.rs 迁入改名——与桌面
/// host_api/context.rs 同名不同物，票 06 批次 03 消歧）
pub use crate::host_api::bare_context::WasmHostContext;

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
}

/// 单个 WASM 插件实例的状态（真源 = host-kit，双形态单一 state）
///
/// 每插件实例化时创建独立 `Store<WasmPluginState>`；宿主上下文经
/// `Arc<dyn HostPorts>`（向下转型还原移动宿主上下文，见 `host_ctx` 访问器）+
/// 可选导出探测句柄，字段面与桌面形态逐字一致（wasm-core-mobile-purity 票 04）。
pub use bedcode_host_kit::state::{StoreSpec, WasmPluginState};

/// 从统一 state 取移动宿主上下文（类型不符即 panic——装配期编程错误，fail-visible）
///
/// 取代旧自持 state 的 `state.host_ctx` 字段访问：state 的 `host` 槽是
/// `Arc<dyn HostPorts>`（host-kit 只能命名 trait 对象），本访问器做一次向下转型。
/// 域函数（`host_impl/*`）一律经此取 `WasmHostContext`（含 `granted_permissions`
/// 权限面——由宿主在实例化时按插件注入，见 `bare_context::WasmHostContext::for_plugin`）。
pub(crate) fn host_ctx(state: &WasmPluginState) -> &WasmHostContext {
    bedcode_host_kit::ports::downcast_host::<WasmHostContext>(state.host.as_ref())
}

/// 取当前运行时句柄（无上下文线程回落 ambient）——旧 `state.runtime_handle` 的替代
///
/// 统一 state（host-kit）不再携带运行时句柄：host fn 通常运行在 tokio 上下文内
/// （`Handle::try_current` 命中）；spawn_blocking / 纯 std 线程上回落
/// `runtime_util::ambient_handle()`（multi_thread + enable_all，任务派生照常可用）。
pub(crate) fn current_handle() -> tokio::runtime::Handle {
    tokio::runtime::Handle::try_current()
        .unwrap_or_else(|_| crate::runtime_util::ambient_handle())
}

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
        // Engine 参数：桌面形态的 `CoreConfig` 是唯一真源（ADR 0045 D5——能力域 /
        // 形态壳不得自带配置面），移动形态消费**同一份** `build_engine_config`，
        // 取编译期默认值。移动插件同为 wasmtime 组件模型消费者，燃料看门狗 /
        // 内存预留 / 栈深度 / 编译缓存 / backtrace 帧数与桌面逐值一致，不再各钉一份。
        let config = engine::build_engine_config(&crate::config::CoreConfig::default(), plugin_debug_mode());
        let engine = Engine::new(&config)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e)))?;

        let linker = component::build_component_linker(&engine)?;

        Ok(Self {
            engine,
            linker,
            aot_cache_dir,
        })
    }

    /// 测试访问器：Engine 引用（组件编译与实例化必须同一 Engine 实例，
    /// 跨 Engine 实例化 wasmtime 直接拒绝）
    #[cfg(any(test, feature = "test-support"))]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

// ==================== Host 能力实现层 ====================
//
// 业务逻辑（值传递、权限校验、block_on 执行）在 host_impl/，
// 组件路径的 Host trait impl（runtime/component.rs）直接调用。
// core 形态的 func_wrap 胶水（Caller + (ptr,len) 内存搬运）已在 09 清理。
// pub（批次 2）：宿主 manager 经垫片 `crate::plugin::wasm_runtime::host_impl`
// 调用停用回收聚合入口。
pub mod host_impl;

// ==================== Async Blocking Helper ====================

// 同步↔异步桥统一走 `crate::runtime_util::block_on_async`（双端唯一一份：
// block_in_place 让出 / 重入检测 / current_thread 与无上下文线程的 ambient
// 兜底；本文件旧的自持副本与 3 个同源测试随 wasm-core-mobile-purity 票 04
// state 换型删除——旧副本要求外传 runtime_handle，而统一 state 不再携带它）。
// 原移动副本与 runtime_util 版的语义差（无上下文线程用应用 handle 阻塞 vs
// ambient runtime 阻塞）在 multi_thread/current_thread 两个主路径上等价。
