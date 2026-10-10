//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 组件（Component Model）加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//!
//! 宿主能力实现位于 `crate::host_api`（权限校验 + 宿主服务调用），
//! 组件绑定与实例化位于 [`component`] 子模块，本模块只负责运行时生命周期
//! 管理与宿主上下文定义

mod component;

// 夹具共享 target 目录（测试期编译产物落点，cfg(test) 门控：生产构建不编译本模块）
// pub(crate)：host/tests/* 下的夹具构建器与 runtime/component.rs 也要用同一落点
#[cfg(test)]
pub(crate) mod fixture_target;
// 夹具构建器：runtime::tests / runtime::component::tests / host::tests::* 三处共用
// 同一实现（`runtime::tests` 自身是私有模块，兄弟模块寻址不到它）
#[cfg(test)]
pub(crate) mod fixture_build;

/// 声明展开（不过滤授权，preauthorize 收集弹窗候选用，见 host.rs `preauthorize_plugin`）
pub(crate) use component::expand_preopen_declarations;
/// WASI 预打开目录解析（激活时重建实例判定用，见 host.rs `rebuild_wasm_instance`）
pub(crate) use component::resolve_preopen_dirs;
pub use component::LoadedWasmPlugin;
// 票 02 批次 02：生效能力模块白名单（内建在册 ∪ 宿主自报）——宿主侧集成锁
// （`src-tauri/tests/pty_wiring.rs` 的双向比对）必须用同一份合并结果，不得各拼一份。
pub use component::host_module_whitelist;
// 票 06：实例元数据与可选导出句柄快照（调用模型两侧共用的只读投影）
pub(crate) use component::{InstanceMeta, OptionalExports};

use crate::storage::PluginStorage;
// 组件引擎机制（编译 + AOT 缓存 / Store 装配 / 燃料 / 实例生死观测）双端唯一一份，
// 本模块只做形态接线（Engine 构建、WASI 预打开、宿主上下文与限额仲裁）
use crate::manager::runtime::engine;
// 缓存键判据的生产实现已迁引擎模块；测试仍按名消费同一份（防测试与生产两套口径）
#[cfg(test)]
use engine::aot_cache_key;
// Engine 构建面（build_engine_config 等纯函数）已迁引擎模块——桌面为真源、
// 移动侧消费同一份。`common/tests/engine_config.rs` 的判据（配置→wasmtime 的
// 逐项映射、逃生舱锁定项重申）按名消费同一批函数，不因实现位置变化而改写。
#[cfg(test)]
use engine::{
    build_engine_config, reassert_locked_knobs, wasmtime_backtrace_details, wasmtime_opt_level,
    wasmtime_profiling,
};
// 异步桥（`block_on_async`）已中立化到 core-runtime-util：本模块与它的消费者
// （host_api / security / task 等）同为其使用方，不再由本模块定义（票 01）；
// 本模块仅测试代码使用它，故 cfg(test) 门控（非 test 构建零 unused import）
#[cfg(test)]
use crate::runtime_util::block_on_async;
use crate::security::fs_auth::FsAuthChecker;
#[cfg(test)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
#[cfg(test)]
use tokio::sync::{Mutex, RwLock};
use wasmtime::Engine;

// ==================== 宿主上下文（票 04 迁移 host_api::context） ====================
// 定义已迁 `crate::host_api::context`（host_api 消费方家园）；以下 re-export
// 保持历史路径 `manager::runtime::WasmHostContext` 等编译绿——manager→host_api 合法
// 方向，后续迭代按需清理（见 .scratch/2026-09-24-wasm-core-decouple/issues/04）
pub use crate::host_api::context::{CapabilityProvider, PluginServices, ProcessRegistry, WasmHostContext};
// ==================== Resource Limits & Interruption ====================

// 运行参数单一事实源在配置模块（core-config，见 `crate::config`）：
// Engine 构建参数与 Store 资源上限的默认值即历史生产常量（`config::defaults`），
// 支持配置文件加载与运行时覆盖；燃料看门狗的语义说明随默认值一并迁入。
pub(crate) use crate::config::plugin_debug_mode;
use crate::config::CoreConfig;
use crate::monitor::{LifecycleEvent, MetricsRegistry};
use bedcode_plugin_api::{ResourceOverrides, WasiPreopenDir};

/// 从 catch_unwind 的 panic 载荷提取人类可读消息（统一 panic 诊断文案）
pub fn panic_payload_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// 票 03：桌面插件与测试 fixture 统一 wasm32-wasip3 构建参数
///
/// 单一事实来源 = `scripts/wasip3-toolchain.sh`（WASIP3_NIGHTLY）；stable 1.99
/// （预计 2026-10 中）发布后随工具链迁移移除 nightly pin，见
/// `docs/knowledge/wasip3-toolchain.md`。所有宿主内联 fixture 构建（plas组件测试/
/// SDK 测试/ws 测试/系统组件测试）与本常量保持同步。
// non-test 构建（cargo check --lib）不编译测试使用方 → 允许 dead_code
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const WASIP3_NIGHTLY: &str = "nightly-2026-09-16";

///
/// Engine 和 Linker 是线程安全的可复用结构：
/// - Engine: WASM 编译器，全局单例
/// - Linker: Host function 注册表，所有插件实例共享
pub struct WasmRuntime {
    engine: Engine,
    /// Component 形态插件的 linker（阶段 C 后唯一形态）
    ///
    /// 已接线的 import 接口见 [`component::add_to_linker`]，实例化导入
    /// 未接线接口的组件会报 unknown import 错误
    linker: wasmtime::component::Linker<WasmPluginState>,
    /// 文件系统访问校验器
    fs_auth: Arc<FsAuthChecker>,
    /// AOT 编译产物（`.cwasm`）缓存目录（宿主 cache 目录，非插件目录）
    ///
    /// 插件目录可被安装方/插件自身写入，若把反序列化产物放回插件目录，
    /// 能写插件目录的攻击者可投放伪造产物触发宿主进程 UB
    /// （`Component::deserialize` 是 unsafe，假定数据可信）。
    /// 无 app_handle 时（无头/测试）为 None，禁用文件级 AOT 缓存。
    /// pub(crate)：`crate::test_support` 的无头装配（票 05b 上提）注入临时目录；
    /// 生产面经 `with_config` 构造时设置，无其它写者。
    pub(crate) aot_cache_dir: Option<PathBuf>,
    /// 内核配置（core-config）：Engine 参数构建期固化，Store 上限每次实例化读快照
    /// （运行时覆盖只影响新建立的 Store）
    config: Arc<std::sync::RwLock<CoreConfig>>,
    /// 指标注册表（core-monitor）：内核运行时埋点数据中心
    monitor: Arc<MetricsRegistry>,
}

// ==================== 迁往机制内核的类型（转发） ====================
//
// `WasmPluginState` / `StoreSpec` 连同 `WasiView` 与 `ResourceLimiter` 实现整体搬入
// `bedcode-host-kit`（wasm-core-lib-split 票 03）：它们是**插件实例状态的机制面**，
// 且必须与「能力 crate 能命名状态类型」这一硬约束同 crate（`add_to_linker::<S,D>`
// 单态 ⇒ 住在宿主 bin crate 内会与能力 crate 构成 Cargo 环）。
//
// 可见性以 `pub use` 原样转发，既有 `runtime::WasmPluginState` 等引用路径逐字不变。
// 唯一实质变化：状态里的宿主引用字段 `host_ctx: Arc<WasmHostContext>` 改为
// `host: Arc<dyn HostPorts>`（见 `host_api::context::host_ctx` 向下转型辅助）。
pub use bedcode_host_kit::state::{StoreSpec, WasmPluginState};

/// 已加载的 WASM 插件（迁移阶段 C：组件形态唯一）
///
/// 类型别名 `pub use component::LoadedWasmPlugin`（见文件头）保留历史名称：
/// 宿主各模块（host.rs 等）以 `LoadedWasmPlugin` 引用插件实例，
/// 方法接口与迁移前枚举完全一致。

// ==================== 引擎定制面（A 面配置 + B 面逃生舱） ====================

/// 引擎定制钩子：宿主在**内核自身设置之后**、`Engine::new` **之前**拿到裸
/// [`wasmtime::Config`]，可覆盖任意**非锁定**项——配置面（[`crate::config::EngineTuning`]）枚举不到的
/// 原语都走这里（`target` / `allocation_strategy(pooling)` / 自定义内存分配器…）。
///
/// 返回 `Err` → Engine **构建失败**（错误带操作上下文上抛，不静默降级为一个
/// 「看起来能用但参数不对」的 Engine）。
///
/// **锁定项**：资源看门狗 4 项（`consume_fuel` / `max_wasm_stack` /
/// `memory_reservation` / `memory_may_move`）在钩子执行后被
/// [`reassert_locked_knobs`] 重申，钩子改不动；要改走 [`CoreConfig`]（有跨字段校验）。
///
/// 钩子类型需命名 `&mut wasmtime::Config`——写钩子的宿主**不必自己加 wasmtime 依赖**，
/// 经 `bedcode_wasm_core::wasmtime` 再导出即可（ADR 0019 双端锁版，宿主侧不重复锁版本）。
pub type EngineCustomizer = Arc<dyn Fn(&mut wasmtime::Config) -> crate::Result<()> + Send + Sync>;

/// 引擎装配输入（[`WasmRuntime::with_setup`] 的配置包）
///
/// 为什么成结构体而不是继续加位置参数：钩子 + 内核配置 + 免弹窗清单三件同属
/// 「宿主装配期的引擎输入」，位置参数超过三个后调用点不可读（`with_config` 的四个
/// 位置参数已经足够难核对）。
#[derive(Default)]
pub struct EngineSetup {
    /// 内核配置（Engine 参数在构建期固化；`store` 部分仍可运行时覆盖）
    pub config: CoreConfig,
    /// 第一方免弹窗归属清单（票 08/P0-2；空表 = 无免弹窗项）
    pub first_party_dirs: Vec<(&'static str, Vec<crate::security::fs_auth::TrustedDir>)>,
    /// 引擎定制钩子（可选，见 [`EngineCustomizer`]）
    pub customizer: Option<EngineCustomizer>,
}

impl EngineSetup {
    /// 以指定内核配置装配（免弹窗清单为空、无钩子）
    pub fn new(config: CoreConfig) -> Self {
        Self {
            config,
            first_party_dirs: Vec::new(),
            customizer: None,
        }
    }

    /// 追加第一方免弹窗归属清单（构建器方法）
    pub fn with_first_party_dirs(
        mut self,
        dirs: Vec<(&'static str, Vec<crate::security::fs_auth::TrustedDir>)>,
    ) -> Self {
        self.first_party_dirs = dirs;
        self
    }

    /// 追加引擎定制钩子（构建器方法；重复调用以最后一个为准）
    pub fn with_customizer(
        mut self,
        customize: impl Fn(&mut wasmtime::Config) -> crate::Result<()> + Send + Sync + 'static,
    ) -> Self {
        self.customizer = Some(Arc::new(customize));
        self
    }
}

impl WasmRuntime {
    /// 创建 WASM 运行时
    ///
    /// 初始化 Engine、Linker，注册所有 Host Functions。
    /// 宿主能力（db / session / permission 等）不在本结构持有，
    /// 而是通过 [`WasmHostContext`] 注入到每个插件实例的 Store state 中。
    /// `app_handle` 为 None 时（无头/测试上下文）依赖前端事件的宿主能力降级。
    ///
    /// `first_party_dirs`：第一方免弹窗归属清单（票 08/P0-2，宿主装配时注入；空表
    /// = 无免弹窗项）——经本构造函数直通 `FsAuthChecker`。
    pub fn new(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        first_party_dirs: Vec<(&'static str, Vec<crate::security::fs_auth::TrustedDir>)>,
    ) -> crate::Result<Self> {
        // 内核配置：编译期默认 < 配置文件 < 运行时覆盖（set_config）。
        // 配置文件缺失/非法不阻断启动——记录 warn 并回落编译期默认
        let core_config = app_handle
            .as_ref()
            .and_then(|h| h.path().app_config_dir().ok())
            .map(|d| d.join(CoreConfig::FILE_NAME))
            .map(|path| match CoreConfig::load_from(&path) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "内核配置加载失败，回落编译期默认");
                    CoreConfig::default()
                }
            })
            .unwrap_or_default();
        Self::with_setup(
            storage,
            app_handle,
            EngineSetup::new(core_config).with_first_party_dirs(first_party_dirs),
        )
    }

    /// 以指定内核配置构建（测试与运行时覆盖路径；配置须先通过 [`CoreConfig::validate`]）
    pub fn with_config(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        core_config: CoreConfig,
        first_party_dirs: Vec<(&'static str, Vec<crate::security::fs_auth::TrustedDir>)>,
    ) -> crate::Result<Self> {
        Self::with_setup(
            storage,
            app_handle,
            EngineSetup::new(core_config).with_first_party_dirs(first_party_dirs),
        )
    }

    /// 以完整引擎装配输入构建（A+B 面新入口；`new` / `with_config` 均经此委托）
    ///
    /// 引擎参数优先级链（高者胜，但锁定项除外）：
    /// `编译期默认 < wasm-core.json（Engine 参数构建期固化，重启生效） < 本次传入的
    /// CoreConfig < EngineCustomizer（仅构建期，最后一道）`
    pub fn with_setup(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        setup: EngineSetup,
    ) -> crate::Result<Self> {
        let EngineSetup {
            config: core_config,
            first_party_dirs,
            customizer,
        } = setup;
        if let Err(reason) = core_config.validate() {
            return Err(crate::AppError::Config(format!("内核配置非法: {}", reason)));
        }
        let mut config = engine::build_engine_config(&core_config, plugin_debug_mode());

        // 逃生舱（B 面）：宿主在内核自身设置之后拿到裸 Config，覆盖配置面枚举不到的
        // 原语（`target` / pooling / 自定义分配器…）。返回 Err 直接让构建失败——
        // 定制失败绝不静默降级为一个「看起来能用但参数不对」的 Engine。
        if let Some(customize) = &customizer {
            customize(&mut config).map_err(|e| {
                crate::AppError::Config(format!("引擎定制钩子失败，Engine 未构建: {}", e))
            })?;
            // 锁定项重申：钩子能改 Config 但不可回读校验，而看门狗 4 项在配置面
            // 有跨字段约束（见 reassert_locked_knobs）——不锁则「配置已校验」的
            // 承诺在最后一步静默失效
            engine::reassert_locked_knobs(&mut config, &core_config);
            tracing::info!("WASM engine customizer applied (locked knobs re-asserted)");
        }
        let engine = Engine::new(&config)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e)))?;
        let mut linker = wasmtime::component::Linker::new(&engine);

        // 注册已接线的 Component import 接口（实现见 host_impl + component）
        component::add_to_linker(&mut linker)?;

        // AOT 缓存目录：宿主 cache 目录（非插件目录，见结构体字段注释）。
        // 须在 app_handle move 进 FsAuthChecker 之前取出
        let aot_cache_dir = app_handle
            .as_ref()
            .and_then(|h| h.path().app_cache_dir().ok())
            .map(|d| d.join("wasm-aot"));
        if let Some(dir) = &aot_cache_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!(
                    path = %dir.display(),
                    error = %e,
                    "Failed to create AOT cache dir, AOT cache disabled"
                );
            }
        }

        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), app_handle, first_party_dirs));

        // task 段快照源注册（spec 票 02 去环）：monitor 经注册回调取 host-task
        // 指标，monitor.rs 不再内联引用 manager::task。注册幂等；monitor 仅经本
        // runtime 暴露，快照必在注册之后——生产路径无「未注册段降级」窗口
        // （降级仅裸 registry 单测可观测，见 monitor.rs tests）
        let monitor = Arc::new(MetricsRegistry::new());
        crate::manager::task::register_task_metrics_source(&monitor);

        Ok(Self {
            engine,
            linker,
            fs_auth,
            aot_cache_dir,
            config: Arc::new(std::sync::RwLock::new(core_config)),
            monitor,
        })
    }

    /// 当前内核配置快照
    pub fn config(&self) -> CoreConfig {
        self.config.read().expect("core config lock poisoned").clone()
    }

    /// 运行时覆盖内核配置：只影响覆盖后新建立的 Store（Engine 参数构建期已固化）
    ///
    /// **Engine 段改动会 warn 不静默**：Engine 在构建期固化，此后 `set_config` 里的
    /// `engine` 段（含 [`crate::config::EngineTuning`] 的 17 个字段）对已建 Engine
    /// 完全无效——不检测的话，宿主「配了没生效」会成为最难查的一类问题
    /// （配置读回来是对的，行为却没变）。要改 Engine 参数只能重新构建
    /// （[`Self::with_setup`] / 重启）。
    pub fn set_config(&self, cfg: CoreConfig) -> crate::Result<()> {
        if let Err(reason) = cfg.validate() {
            return Err(crate::AppError::Config(format!("内核配置非法: {}", reason)));
        }
        {
            let current = self.config.read().expect("core config lock poisoned");
            if current.engine != cfg.engine {
                tracing::warn!(
                    "set_config 携带了 Engine 参数变更，但 Engine 构建期已固化，本次覆盖不会生效（需重建运行时）"
                );
            }
        }
        *self.config.write().expect("core config lock poisoned") = cfg;
        Ok(())
    }

    /// 指标注册表句柄（core-monitor）：生命周期事件记账与快照导出入口
    pub fn monitor(&self) -> Arc<MetricsRegistry> {
        self.monitor.clone()
    }

    /// 从字节流编译 WASM 组件（Component Model，迁移阶段 A）
    pub fn compile_component(&self, bytes: &[u8]) -> crate::Result<wasmtime::component::Component> {
        wasmtime::component::Component::from_binary(&self.engine, bytes)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to compile WASM component: {}", e)))
    }

    /// 从文件编译 WASM 组件（带 AOT 缓存，与 core 路径同构）
    ///
    /// 机制本体在 [`engine`]（双端唯一一份）；本方法只把形态面（Engine 句柄 +
    /// 宿主 cache 目录）传进去。
    pub fn compile_component_from_file(&self, path: &Path) -> crate::Result<wasmtime::component::Component> {
        engine::compile_component_from_file(&self.engine, self.aot_cache_dir.as_deref(), path)
    }

    /// 从文件加载 WASM 插件（阶段 C 起仅组件形态）
    ///
    /// `declared_preopen_dirs`：manifest 声明的 WASI 预打开目录（原始值，
    /// 支持 ${home} 与只读档；实例化时经展开+授权过滤并按档位挂载，
    /// 见 component::build_wasi_ctx）
    pub fn load_plugin_from_file(
        &self,
        path: &Path,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        declared_preopen_dirs: &[WasiPreopenDir],
        resource_overrides: Option<&ResourceOverrides>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let bytes = std::fs::read(path).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to read WASM artifact '{}': {}", path.display(), e))
        })?;
        let component = self.compile_component(&bytes)?;
        self.instantiate_component(
            &component,
            plugin_id,
            host_ctx,
            declared_preopen_dirs,
            resource_overrides,
        )
    }

    /// 实例化 WASM 组件
    ///
    /// 创建 Store + WasmPluginState，通过 linker 实例化，
    /// 校验 ABI 版本与形态字段（见 [`component::LoadedWasmPlugin::new`]）
    ///
    /// `resource_overrides` 为插件 manifest 的资源覆盖请求，经安全模块仲裁后
    /// 作为本 Store 的限额（见 [`crate::security::SecurityFramework::resolve_store_limits`]）。
    pub fn instantiate_component(
        &self,
        component: &wasmtime::component::Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        declared_preopen_dirs: &[WasiPreopenDir],
        resource_overrides: Option<&ResourceOverrides>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let (limits, fuel_enabled) = {
            let cfg = self.config.read().expect("core config lock poisoned");
            (
                host_ctx
                    .security()
                    .resolve_store_limits(plugin_id, &cfg.store, resource_overrides),
                cfg.engine.consume_fuel,
            )
        };
        let metrics = self.monitor.plugin(plugin_id);
        let plugin = component::LoadedWasmPlugin::new(
            &self.engine,
            &self.linker,
            component,
            plugin_id,
            host_ctx,
            declared_preopen_dirs,
            StoreSpec {
                limits,
                fuel_enabled,
                metrics: metrics.clone(),
            },
        )?;
        metrics.record_lifecycle(LifecycleEvent::Instantiate);
        // 实例创建日志：启动加载与热重载均经此路径，与 LoadedWasmPlugin::drop 的
        // 死亡日志成对，构成实例生命周期观测（plugin_id 键控，实现见 engine）
        engine::log_instance_created(plugin_id);
        Ok(plugin)
    }

    /// 获取文件系统访问校验器引用
    pub fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }

    /// Engine 句柄（**仅测试可见**：wasmtime `Config` 构建后不可回读，锁定项 /
    /// 逃生舱的行为契约只能经构建出来的 Engine 观测；生产不开放该面，
    /// 避免宿主绕过实例化路径直接用 Engine 编译产物）
    #[cfg(test)]
    pub(crate) fn engine(&self) -> &Engine {
        &self.engine
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::fixture_build::build_sdk_fixture;
    // fixture_keeper 用 fixture 构建器 + wasip3 工具链探针（consumer 在宿主 e2e，见该文件头注）
    use super::*;
    // 测试基建上提（wasm-core 纯净性收口票 05b）：无头运行时装配 / 插件私有库根 /
    // 会话中心串行锁 / 认证中心注册表闸门迁至 `crate::test_support`（常编译 pub，
    // lib 集成测试同源消费）。本 mod 及各域测试文件（`use super::*` 继承）经此 glob
    // 继续可见同名符号。
    use crate::test_support::*;

    // A0-3 前置探针（P1/P2/P5）：async store 兼容性 + 资源限制 async 语义 + 性能基线。
    // 文档：.scratch/2026-09-21-a0-3-host-async/spec.md + report.md（只读探针，不碰生产路径）
    mod a03_probe;
    // 终端输出消费插件化性能前置验证（terminal_output_perf，P1-P3）随票 02 批次 02
    // 迁宿主 `src-tauri/tests/terminal_output_perf.rs`（跨 crate 集成测试一律住宿主；
    // 本 crate 已不依赖 bedcode-pty-engine）。
    // WS 终端输出路径吞吐探针（票 09 性能门禁 §9.3：插件 ring-fetch + WIT binary + WS send）
    // 认证中心热路径性能探针（ADR 0031 v32 fail-closed 裁决的每请求成本：
    // 宿主原生验签 vs 认证中心往返），只读探针
    // 域拆分（P0）：测试函数自本文件拆至 wasm_runtime/tests/，共享脚手架留在下方；
    // 各域文件 `use super::*` 复用，fixture 互斥与产物构建语义不变
    mod component_e2e;
    mod engine_config;
    mod engine_limits;
    // crypto 夹具产物 keeper（票 02 批次 04）：夹具消费者 = 宿主 e2e，内核只负责构建
    mod fixture_keeper;
    mod http_e2e;
    // 票 06 P1：属主任务（event-loop 调用模型）直接驱动测试
    mod owner_e2e;
    mod p3_async_host_import;
    // host-pty 端到端矩阵（pty_e2e）随票 02 批次 02 迁宿主
    // `src-tauri/tests/pty_e2e.rs`（本 crate 已不依赖 bedcode-pty-engine）。
    mod sdk_e2e;
    // host-task 闭环（task_e2e）随票 02 批次 05 迁宿主 `src-tauri/tests/task_e2e.rs`
    // ——WIT impl 迁出后内核测试二进制不再注册 host-task import，task 夹具无法
    // 在内核实例化（产物由本 crate fixture_keeper 构建供给宿主，见该文件）。
    mod preopen_e2e;

    /// 测试用插件 ID
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";

    /// 校验 RFC3339 UTC 字符串格式（trust addedAt 断言用；与插件
    /// pairing::code 的 format_rfc3339_utc 输出格式一致）
    fn parse_rfc3339_for_test(s: &str) -> bool {
        use chrono::{DateTime, Utc};
        DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)).is_ok()
    }

    // ==================== Component Model 测试 ====================

    /// 构建测试用组件插件（合集 `packages/plugin-sdk-fixtures` `feature = "sdk"`）
    ///
    /// 原先指向 `packages/plugin-component-test`（手写 wit-bindgen 绑定的独立夹具），
    /// 已被 30+ 处通用引擎测试当「随便一个可加载组件」使用。夹具删除后改用 SDK 夹具：
    /// 宿主加载组件的代码路径与客体绑定方式无关（宿主无 `ComponentEncoder` 路径，
    /// 两种写法都直出组件），实际依赖的只有 `test.echo` / `test.panic` /
    /// `test.storage-get` / 启动失败注入四个可观测点，已并入 SDK 夹具。
    ///
    /// `BEDCODE_PLUGIN_DEBUG=1`（dev 构建下）时以 debug profile 构建（保留
    /// DWARF 行号，供行号冒烟测试断言 trap 错误串含 file:line）——由
    /// `fixture_build::build_sdk_fixture` 内部按 profile 分档处理。
    fn build_test_component() -> Vec<u8> {
        build_sdk_fixture("sdk")
    }

    // ==================== host-websocket 客户端域端到端（ABI v14） ====================

    /// host-websocket fixture e2e 串行锁
    ///
    /// 三个用例共用 fixture 常量属主 id（`com.bedcode.ws-test`，宿主侧连接表 / 端点表 /
    /// 事件 topic 均按属主**进程级全局**登记），彼此 `purge_for_plugin` 会清掉对方的
    /// 连接与端点（并行时现象：握手 404、连接被回收）。libtest 并行执行下必须串行。
    static WS_FIXTURE_E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 取得 fixture e2e 串行锁（跨用例共享全局表；中毒后取回内部值继续）
    fn lock_ws_fixture_e2e() -> std::sync::MutexGuard<'static, ()> {
        WS_FIXTURE_E2E_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// fixture e2e 兜底超时：把「挂起（疑似死锁）」变成明确失败
    ///
    /// 历史踩点：插件投递任务占住 actix arbiter → 用例无限挂起（无超时则整轮
    /// `cargo test` 永不返回）。上限取 60s（正常用例秒级完成）。
    const WS_E2E_TIMEOUT_SECS: u64 = 60;

    /// 以兜底超时驱动 e2e 主体
    async fn ws_e2e_guard<F>(label: &str, fut: F) -> F::Output
    where
        F: std::future::Future,
    {
        match tokio::time::timeout(std::time::Duration::from_secs(WS_E2E_TIMEOUT_SECS), fut).await {
            Ok(out) => out,
            Err(_) => {
                panic!("{label}: 超过 {WS_E2E_TIMEOUT_SECS}s 未完成（疑似死锁；检查投递任务是否占住 actix arbiter）")
            }
        }
    }

    /// 读 fixture 的 `ws-state` 快照（锁在返回前释放，避免阻塞帧投递）
    async fn ws_fixture_state(plugin: &Arc<Mutex<LoadedWasmPlugin>>) -> serde_json::Value {
        let raw = {
            let mut guard = plugin.lock().await;
            guard.invoke_command("ws-state", "{}").expect("ws-state")
        };
        serde_json::from_str(&raw).expect("ws-state json")
    }

    /// 轮询快照直到谓词命中或超时（帧与事件均为异步投递，不能单次读取断言）
    async fn ws_poll_state(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        pred: impl Fn(&serde_json::Value) -> bool,
        timeout: std::time::Duration,
    ) -> serde_json::Value {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let state = ws_fixture_state(plugin).await;
            if pred(&state) || std::time::Instant::now() >= deadline {
                return state;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// 快照中是否含指定 kind 的帧（`text` 为 Some 时要求文本一致）
    fn ws_has_frame(state: &serde_json::Value, kind: &str, text: Option<&str>) -> bool {
        state["frames"]
            .as_array()
            .map(|frames| {
                frames
                    .iter()
                    .any(|f| f["kind"] == kind && text.map(|t| f["text"] == t).unwrap_or(true))
            })
            .unwrap_or(false)
    }

    /// 快照中首个指定 kind 帧的载荷长度
    fn ws_frame_len(state: &serde_json::Value, kind: &str) -> Option<u64> {
        state["frames"]
            .as_array()?
            .iter()
            .find(|f| f["kind"] == kind)?
            .get("len")?
            .as_u64()
    }

    /// 快照中指定 topic 的事件 payload
    fn ws_event_payload(state: &serde_json::Value, topic: &str) -> Option<serde_json::Value> {
        state["events"]
            .as_array()?
            .iter()
            .find(|e| e["topic"] == topic)
            .map(|e| e["payload"].clone())
    }

    // ==================== host-websocket 服务端域端到端（ABI v14，票 05） ====================

    /// WS 客户端类型（tokio-tungstenite 直连 ws://，与集成测试同构）
    type WsTestClient = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    /// 探测空闲端口（OS 分配后立即释放，交给宿主服务器绑定）
    fn ws_pick_free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("probe free port");
        listener.local_addr().expect("probed port").port()
    }

    /// 读客户端下一条业务帧（跳过心跳帧），超时返回 `None`
    async fn ws_client_recv(
        client: &mut WsTestClient,
        timeout: std::time::Duration,
    ) -> Option<tokio_tungstenite::tungstenite::Message> {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match tokio::time::timeout(remaining, client.next()).await {
                Ok(Some(Ok(msg))) => match msg {
                    Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
                    other => return Some(other),
                },
                _ => return None,
            }
        }
    }

    /// 轮询 `ws-list-clients` 直到在线客户端数达到期望（注册表登记为异步）
    async fn ws_wait_clients(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        endpoint_id: &str,
        expected: usize,
    ) -> Vec<serde_json::Value> {
        let args = serde_json::json!({ "endpointId": endpoint_id }).to_string();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let raw = {
                let mut guard = plugin.lock().await;
                guard.invoke_command("ws-list-clients", &args).expect("ws-list-clients")
            };
            let clients: Vec<serde_json::Value> = serde_json::from_str::<serde_json::Value>(&raw)
                .expect("ws-list-clients json")["clients"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if clients.len() == expected || std::time::Instant::now() >= deadline {
                return clients;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// 构建 host-websocket fixture 插件（合集 `packages/plugin-sdk-fixtures`
    /// `feature = "ws"`）并编码为组件
    ///
    /// 原先的新鲜度检查额外盯 SDK 的 `wasm_ws.rs` / `host/ws.rs` 等链路文件；合集
    /// crate 后不再单独盯——SDK 任一文件变动会经 cargo 依赖指纹触发重建。
    fn build_ws_test_component() -> Vec<u8> {
        build_sdk_fixture("ws")
    }

    // ==================== host-http 服务端域端到端（ABI v29，路由注册下沉） ====================

    /// host-http 服务端域 fixture（合集 `packages/plugin-sdk-fixtures` `feature = "http"`）
    fn build_http_test_component() -> Vec<u8> {
        build_sdk_fixture("http")
    }

    // host-pty 一族测试助手（串行锁 `lock_pty_fixture_e2e` / 构建器
    // `build_pty_test_component` / 断言助手 `pty_command_error` 等）随 wasm-core
    // 纯净性收口票 02 批次 02 迁宿主 `src-tauri/tests/{pty_e2e,terminal_output_perf}.rs`
    // ——本 crate 已不依赖 `bedcode-pty-engine`，也不链接 pty fixture。

    // host-task fixture 闭环（task_e2e，6 用例）随票 02 批次 05 迁宿主
    // `src-tauri/tests/task_e2e.rs`：host-task 的 WIT impl 迁宿主后内核测试二进制
    // 不再注册该 interface，携带其 import 的 task 夹具无法在内核实例化
    // （与批次 04 crypto 探针同判据）。产物仍由本 crate 的 keeper 用例构建
    // （`fixture_keeper::task_fixture_artifact_is_built_for_host_e2e`，宿主只读）。
    // host-task 消费派发测试替身（MockTaskServices）与 `task_fixture_plugin_id`
    // 同批随用例迁宿主。

    /// 构建 SDK 组件形态测试插件（合集 `packages/plugin-sdk-fixtures`
    /// `feature = "sdk"`）并编码为组件
    ///
    /// 与 `build_test_component` 的区别：插件经真实 SDK（wasm_entry! 宏 + WasmHost）
    /// 构建，验证 SDK 组件产物链路。
    ///
    /// 注：原先本 builder 的新鲜度检查额外盯 `plugin-sdk-desktop/rust/src/{wasm,
    /// wasm_host,api_call}.rs` 与 `rust-macros/src/lib.rs`（`#[plugin_api]` 宏）。
    /// 合集 crate 后不再单独盯——合集产物在 `plugin-sdk-desktop` 任一文件变动时
    /// 经 cargo 自身的依赖指纹重建（feature 变更会触发），此处只需盯夹具自身源文件。
    fn build_sdk_test_component() -> Vec<u8> {
        build_sdk_fixture("sdk")
    }

    /// 构建 wasm32-wasip2 测试组件（WASI preopen E2E 用）
    ///
    /// 与其它测试组件不同：目标为 WASI preview2（std::fs 直连预打开目录），
    /// 依赖宿主当前机器已安装 wasm32-wasip2 target（rustup target add）。
    /// 预装的其它测试组件不依赖该 target，互不干扰。
    fn build_wasi_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // 2026-10-08 迁根：本 crate 在根 `packages/`，plugin-* 夹具与 SDK 契约留在
        // `bedcode-desktop/packages/`（契约与夹具 crate），须回两级再进桌面 packages
        let packages_dir = manifest_dir.join("../../bedcode-desktop/packages");
        let plugin_dir = packages_dir.join("plugin-wasi-test");

        let module_path = fixture_target::artifact("wasm32-wasip2", "release", "bedcode_plugin_wasi_test");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/wit/bedcode.wit"),
            ];
            let module_modified = std::fs::metadata(&module_path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            let needs_rebuild = src_files.iter().any(|f| {
                std::fs::metadata(f)
                    .and_then(|m| m.modified())
                    .map(|t| t > module_modified)
                    .unwrap_or(true)
            });

            if !needs_rebuild {
                return std::fs::read(&module_path).expect("Failed to read WASI test component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("CARGO_TARGET_DIR", fixture_target::dir())
            .args([
                "build",
                "--target",
                "wasm32-wasip2",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for WASI test component");
        assert!(status.success(), "WASI test component WASM build failed");

        // wasm32-wasip2 目标（Rust 1.85+）已内嵌 wasm-component-ld：产物直接是
        // 组件（magic \0asm 0d），无需再经 encode_component 编码
        //
        // 注：preopen 目前只在 wasip2 上真正可用。曾尝试迁到 wasip3（统一夹具
        // target 着想），但实测两个 preopen E2E 均 trap 于
        // `filesystem_method_descriptor_open_at`——p3 linker 虽接上，预打开目录
        // 的能力没建到 p3 filesystem 接口上。故本夹具固定 wasip2，不参与夹具
        // 合并（合并不要求统一 target，见合并方案）。
        // **preopen 仅 worker 类别可用**（ADR 0034）：本夹具是 worker 预留能力的
        // 机制守门测试（在策略闸门之下，不走 manifest 校验路径）；worker 启用
        // 专项需一并解决 wasip3 的 preopen 装配问题（见 ADR 0034 §worker 启用时需补）。
        std::fs::read(&module_path).expect("Failed to read WASI test component after build")
    }

    /// wasip3 固定 nightly（与 scripts/wasip3-toolchain.sh WASIP3_NIGHTLY 单一
    /// 事实来源）；stable 1.99 发布后随工具链切换，见 docs/knowledge/wasip3-toolchain.md
    const WASIP3_NIGHTLY: &str = "nightly-2026-09-16";

    /// wasip3 工具链（pinned nightly + wasm32-wasip3 target）是否可用
    ///
    /// CI（dtolnay stable）无该 target → 返回 false，调用侧测试值跳过；
    /// 本地经 scripts/wasip3-toolchain.sh install 后为 true。结果惰性缓存。
    fn wasip3_toolchain_ready() -> bool {
        use std::sync::OnceLock;
        static READY: OnceLock<Option<bool>> = OnceLock::new();
        match READY.get_or_init(|| {
            let out = std::process::Command::new("rustup")
                .args(["run", WASIP3_NIGHTLY, "rustup", "target", "list", "--installed"])
                .output()
                .ok()?;
            Some(
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .any(|l| l == "wasm32-wasip3"),
            )
        }) {
            Some(ready) => *ready,
            None => false,
        }
    }

    /// 构建 wasm32-wasip3 测试组件（票 02 A1 async 闭环用）
    ///
    /// target 产物 cdylib 直接是 Component（magic \0asm 0d，免 componentize）；
    /// pinned nightly 经 RUSTUP_TOOLCHAIN 注入（与 scripts/wasip3-toolchain.sh
    /// health 子命令同构）。产物缺失（本地未装工具链）时返回 None，测试跳过。
    fn build_wasip3_test_component() -> Option<Vec<u8>> {
        if !wasip3_toolchain_ready() {
            return None;
        }
        Some(build_sdk_fixture("wasip3"))
    }

    // ==================== WASI preopen E2E ====================

    // ==================== host-task 闭环（ABI v20，spec `.scratch/2026-09-21-host-task-concurrency/`） ====================
    //
    // task_e2e（含 MockTaskServices / task_fixture_plugin_id 脚手架）已随票 02 批次 05
    // 迁宿主 `src-tauri/tests/task_e2e.rs`（本文件 task 段的指路注释）。

    // ==================== ADR 0033：认证面在无头上下文里的边界 ====================
    //
    // **为什么本 harness 里没有「正向认证」用例**（一次踩坑的记录，避免后人重走）：
    //
    // v33 之后，WS / HTTP 认证的**唯一**判定点是认证中心插件，而宿主侧两处调用
    // （`authenticate_endpoint_connection` / `auth_gateway`）都经 `AppContext::try_global()` 取
    // `PluginHost`——**无 AppContext ⇒ 无中心 ⇒ fail-closed 全拒**（这本身是对的：
    // 宿主已经没有本地验签面了，不存在「本地验签通过就放行」的旁路）。
    //
    // 本 harness 曾试过装一个进程级 `AppContext`（`OnceCell` + 真实中心产物）来
    // 造正向认证，三条实测阻塞把这条路堵死了：
    // 1. **端点表是进程级全局**：那个中心一旦 `activate` 就登记了
    //    `session-control` / `terminal` 端点，本文件里各自装载实例的 WS 用例再登记
    //    同路径 → `path already registered` 直接 panic；
    // 2. **端点归属与实例绑定**：端点由某个实例登记、帧投递给那个实例，于是「用
    //    全局中心签的 token」+「用例自己的实例收帧」必然错配（认证身份与状态快照
    //    分属两个实例）；
    // 3. **AppContext 是 `OnceLock`**：一个用例装上后全 harness 共享，隔离性消失，
    //    且 `PluginHost::new` 的重量级初始化被所有用例连带。
    //
    // **正向认证的覆盖因此落在自带真实 AppContext 的独立测试二进制**：
    // `tests/ws_auth_rules.rs`（WS 首消息认证四规则 + 有效凭证后业务帧可达）与
    // `tests/http_auth_biometric.rs`（中心签发 → 验签闭环）。宿主 harness 这边只锁
    // **无中心即全拒**这条边界——它恰好是 v33 之后最该被钉住的新性质。

    /// 需要**正向认证**（中心签发 → 首消息认证 → 业务帧往返）的用例在无头 harness
    /// 里**不可达**，此处显式退出并说明去哪儿跑
    ///
    /// v33 之后宿主无签发面也无验签面，认证判定全在认证中心插件里，而宿主侧两处
    /// 调用都经 `AppContext::try_global()`——本 harness 装不出隔离的 AppContext
    /// （三条阻塞见上方「ADR 0033：认证面在无头上下文里的边界」）。故这些用例
    /// **显式跳过并打印去向**（不是静默 skip：静默会把「未验证」伪装成「通过」）。
    /// 正向认证的覆盖在 `tests/ws_auth_rules.rs`（WS 首消息认证四规则 + 有效
    /// 凭证后业务帧可达）与 `tests/http_auth_biometric.rs`（中心签发闭环）。
    fn positive_auth_needs_dedicated_binary(test: &str) -> bool {
        eprintln!(
            "[skip] {test}: 需要正向认证（中心签发 → 首消息认证 → 业务帧往返），\
             但 v33 之后认证判定在认证中心插件里，宿主侧调用需要 AppContext——\
             无头 harness 装不出隔离的 AppContext。正向认证覆盖见 \
             tests/ws_auth_rules.rs 与 tests/http_auth_biometric.rs。"
        );
        true
    }

    // ==================== 会话中心互调 api 清单（已上提 test_support） ====================
}

