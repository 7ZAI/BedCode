//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 组件（Component Model）加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//!
//! 宿主能力实现位于 `crate::wasm_core::host_api`（权限校验 + 宿主服务调用），
//! 组件绑定与实例化位于 [`component`] 子模块，本模块只负责运行时生命周期
//! 管理与宿主上下文定义

mod component;

/// 声明展开（不过滤授权，preauthorize 收集弹窗候选用，见 host.rs `preauthorize_plugin`）
pub(crate) use component::expand_preopen_declarations;
/// WASI 预打开目录解析（激活时重建实例判定用，见 host.rs `rebuild_wasm_instance`）
pub(crate) use component::resolve_preopen_dirs;
pub use component::LoadedWasmPlugin;

use crate::wasm_core::storage::PluginStorage;
// 异步桥（`block_on_async`）已中立化到 core-runtime-util：本模块与它的消费者
// （host_api / security / task 等）同为其使用方，不再由本模块定义（票 01）；
// 本模块仅测试代码使用它，故 cfg(test) 门控（非 test 构建零 unused import）
#[cfg(test)]
use crate::wasm_core::runtime_util::block_on_async;
// 票 04 迁走 WasmHostContext 定义后下述类型从生产面消失，但 runtime 测试子模块
// （mod tests 展开）经 `use super::*` 继承父作用域名字仍消费它们——cfg(test)
// 门控恢复（生产构建不引，零 unused import 警告）
#[cfg(test)]
use crate::db::Database;
#[cfg(test)]
use crate::wasm_core::permission::PermissionManager;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::pin::Pin;
#[cfg(test)]
use tokio::sync::{Mutex, RwLock};
use crate::wasm_core::security::fs_auth::FsAuthChecker;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
use wasmtime::{Cache, CacheConfig, Config, Engine, ResourceLimiter, WasmBacktraceDetails};


// ==================== 宿主上下文（票 04 迁移 host_api::context） ====================
// 定义已迁 `crate::wasm_core::host_api::context`（host_api 消费方家园）；以下 re-export
// 保持历史路径 `manager::runtime::WasmHostContext` 等编译绿——manager→host_api 合法
// 方向，后续迭代按需清理（见 .scratch/2026-09-24-wasm-core-decouple/issues/04）
pub use crate::wasm_core::host_api::context::{
    CapabilityProvider,
    PluginServices,
    ProcessRegistry,
    WasmHostContext,
};
// ==================== Resource Limits & Interruption ====================

// 运行参数单一事实源在配置模块（core-config，见 `crate::wasm_core::config`）：
// Engine 构建参数与 Store 资源上限的默认值即历史生产常量（`config::defaults`），
// 支持配置文件加载与运行时覆盖；燃料看门狗的语义说明随默认值一并迁入。
pub(crate) use crate::wasm_core::config::plugin_debug_mode;
use crate::wasm_core::config::{CoreConfig, StoreLimits};
use crate::wasm_core::monitor::{LifecycleEvent, MetricsRegistry, PluginMetrics};
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
    aot_cache_dir: Option<PathBuf>,
    /// 内核配置（core-config）：Engine 参数构建期固化，Store 上限每次实例化读快照
    /// （运行时覆盖只影响新建立的 Store）
    config: Arc<std::sync::RwLock<CoreConfig>>,
    /// 指标注册表（core-monitor）：内核运行时埋点数据中心
    monitor: Arc<MetricsRegistry>,
}

/// 实例化一个插件 Store 所需的全部配置与埋点句柄（core-config × core-monitor 交汇）
#[derive(Clone)]
pub(crate) struct StoreSpec {
    /// Store 资源上限快照
    pub(crate) limits: StoreLimits,
    /// 燃料看门狗是否开启（Engine 级 consume_fuel 的投影；关闭时 set_fuel/get_fuel 不可用）
    pub(crate) fuel_enabled: bool,
    /// 插件指标句柄（core-monitor 埋点入口）
    pub(crate) metrics: Arc<PluginMetrics>,
}

/// 单个 WASM 插件实例的状态
///
/// 每个插件实例化时创建独立的 Store<WasmPluginState>，
/// state 中包含插件 ID 和宿主上下文引用
pub struct WasmPluginState {
    /// 插件 ID（用于权限校验和数据隔离）
    plugin_id: String,
    /// 宿主上下文（注入宿主能力）
    host_ctx: Arc<WasmHostContext>,
    /// WASI preview2 上下文（预打开目录见 component.rs `resolve_preopen_dir`；
    /// 未开启自身文件访问的插件为空上下文，不干扰现有 host_fs 路径）
    wasi_ctx: wasmtime_wasi::WasiCtx,
    /// WASI 资源表（文件句柄 / 流等，随每个插件实例独立生命周期）
    wasi_table: wasmtime::component::ResourceTable,
    /// Store 资源上限快照（实例化时自内核配置读取，见 core-config）
    limits: StoreLimits,
    /// 燃料看门狗是否开启（见 [`StoreSpec::fuel_enabled`]）
    fuel_enabled: bool,
    /// 插件指标句柄（core-monitor 埋点入口）
    metrics: Arc<PluginMetrics>,
    /// v11：可选导出 `events-binary#on-message-binary` 的动态探测句柄。
    /// 旧插件（v10 及更早）不导出该函数 → None，二进制消息对其按
    /// 「格式不匹配」拒绝（总线侧过滤，不会到达本字段为 None 的实例）
    on_message_binary: Option<wasmtime::component::TypedFunc<(String, String, Vec<u8>), ()>>,
    /// v14：可选导出 `events-ws#on-message`（客户端域帧回调）的探测句柄。
    /// 未导出 → None：宿主按 spec §2.2 降级（消息帧丢弃 + 首次 warn + 计数，
    /// 不缓存），状态事件仍经消息总线照常投递
    on_ws_message: Option<wasmtime::component::TypedFunc<(String, String, Vec<u8>), ()>>,
    /// v14：可选导出 `events-ws#on-client-message`（服务端域帧回调）的探测句柄
    on_ws_client_message: Option<wasmtime::component::TypedFunc<(String, String, String, Vec<u8>), ()>>,
    /// v20：可选导出 `events-task#on-task-event`（宿主并发任务进度/终态回调）的
    /// 探测句柄。未导出 → None：宿主按 spec §5.3 降级（事件丢弃 + 首次 warn +
    /// 计数，不缓存），离线查询原语 `status`/`list-jobs` 自愈
    on_task_event: Option<wasmtime::component::TypedFunc<(String,), ()>>,
}

impl WasmPluginState {
    /// 构建插件状态（wasi_ctx 由调用方按插件配置构建，见 component.rs）
    pub(crate) fn new(
        plugin_id: String,
        host_ctx: Arc<WasmHostContext>,
        wasi_ctx: wasmtime_wasi::WasiCtx,
        spec: StoreSpec,
    ) -> Self {
        Self {
            plugin_id,
            host_ctx,
            wasi_ctx,
            wasi_table: wasmtime::component::ResourceTable::new(),
            limits: spec.limits,
            fuel_enabled: spec.fuel_enabled,
            metrics: spec.metrics,
            // v11 / v14 / v20：可选导出在实例化后动态探测（verify_abi 内写入，见 component.rs）
            on_message_binary: None,
            on_ws_message: None,
            on_ws_client_message: None,
            on_task_event: None,
        }
    }
}

/// WASI preview2 视图：`p2::add_to_linker_sync` 通过此 trait 访问每个
/// 插件实例的 WasiCtx + ResourceTable（linker 共享、ctx 每实例）
impl wasmtime_wasi::WasiView for WasmPluginState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.wasi_table,
        }
    }
}

/// 插件实例资源限制器
///
/// 直接借用 Store 状态（`Store::limiter` 的闭包返回本状态的可变引用），
/// 限制单插件线性内存与表大小，防止失控/恶意插件耗尽宿主内存。
impl ResourceLimiter for WasmPluginState {
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        if desired > self.limits.max_memory_bytes {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_bytes = desired,
                max_bytes = self.limits.max_memory_bytes,
                "WASM memory growth denied by resource limiter"
            );
            Ok(false)
        } else {
            // core-monitor 记账：当前值/峰值（纯原子操作，不进日志）
            self.metrics.record_memory_growth(desired);
            Ok(true)
        }
    }

    fn table_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        if desired > self.limits.max_table_entries {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_entries = desired,
                max_entries = self.limits.max_table_entries,
                "WASM table growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn instances(&self) -> usize {
        self.limits.max_instances
    }

    fn memories(&self) -> usize {
        self.limits.max_memories
    }

    fn tables(&self) -> usize {
        self.limits.max_tables
    }
}


/// 已加载的 WASM 插件（迁移阶段 C：组件形态唯一）
///
/// 类型别名 `pub use component::LoadedWasmPlugin`（见文件头）保留历史名称：
/// 宿主各模块（host.rs 等）以 `LoadedWasmPlugin` 引用插件实例，
/// 方法接口与迁移前枚举完全一致。

/// 根据 wasm 路径生成 AOT 缓存文件名（稳定 hash，避免路径字符/长度问题）
/// 产物 key：路径 + 源码大小双因子哈希
///
/// 源码大小进入 key：解压器保留旧 mtime 时，仅 mtime 比较发现不了内容
/// 变更；大小变化必然换 key → 缓存 miss → 重新编译。产物自身长度与源码
/// 长度无固定关系，不能作为新鲜度因子（比较会恒不等、永久禁用缓存）
fn aot_cache_key(path: &Path, source_len: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    source_len.hash(&mut hasher);
    hasher.finish()
}

impl WasmRuntime {
    /// 创建 WASM 运行时
    ///
    /// 初始化 Engine、Linker，注册所有 Host Functions。
    /// 宿主能力（db / session / permission 等）不在本结构持有，
    /// 而是通过 [`WasmHostContext`] 注入到每个插件实例的 Store state 中。
    /// `app_handle` 为 None 时（无头/测试上下文）依赖前端事件的宿主能力降级
    pub fn new(storage: Arc<PluginStorage>, app_handle: Option<Arc<tauri::AppHandle>>) -> crate::Result<Self> {
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
        Self::with_config(storage, app_handle, core_config)
    }

    /// 以指定内核配置构建（测试与运行时覆盖路径；配置须先通过 [`CoreConfig::validate`]）
    pub fn with_config(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        core_config: CoreConfig,
    ) -> crate::Result<Self> {
        if let Err(reason) = core_config.validate() {
            return Err(crate::AppError::Config(format!("内核配置非法: {}", reason)));
        }
        let mut config = Config::new();
        // 票 02 宿主 async 化门禁：CM_ASYNC 引擎级异步支持（wasmtime 46+ 默认
        // 编译进 runtime，此处开启 wasm 特性）。wasip3 组件导入 async wasi 0.3
        // 函数，实例化后 Store 为 async-required；既有同步插件不受影响
        // （/tmp/wasip3-probe 场景 4 实证：同步组件 sync/async 双路径均可用）。
        // wasmtime 48 中 async_support() 配置已废弃为 no-op（异步为引擎级）。
        config.wasm_component_model_async(true);
        // 燃料看门狗：guest 指令计数耗尽即 trap（宿主调用阻塞不消耗，见 config FUEL_PER_CALL）
        config.consume_fuel(core_config.engine.consume_fuel);
        // WASM 内部调用栈：trap（panic/栈溢出/燃料耗尽/内存越界）错误串携带
        // 插件内部函数调用链（names section 函数名，release 构建即有），随
        // AppError::Plugin 进 error.log 与插件 Degraded 状态，AI agent 无需重跑
        // 即可定位插件内部故障点。wasmtime 47 的 backtrace 在 default features
        // 内（零编译成本），此处显式钉死 32 帧防止上游默认（20 帧）漂移
        config.wasm_backtrace_max_frames(Some(
            std::num::NonZeroUsize::new(core_config.engine.wasm_backtrace_max_frames as usize)
                .expect("backtrace frames > 0（配置校验保证）"),
        ));
        // 行号解析：Environment 模式读 WASMTIME_BACKTRACE_DETAILS——无 DWARF 时
        // 零开销回退到函数名栈（release 插件无调试信息，不硬编码强制解析）；
        // 插件调试模式（BEDCODE_PLUGIN_DEBUG=1）下宿主先置该环境变量再构建
        // Engine，调试产物（debug profile 保留 DWARF）即可拿到 file:line 行号
        if plugin_debug_mode() {
            // edition 2021 下 set_var 非 unsafe；此处单线程启动早期调用，
            // 无并发读写风险。Environment 模式在 wasm_backtrace_details 调用
            // 时读取该变量，必须先设置再配置
            std::env::set_var("WASMTIME_BACKTRACE_DETAILS", "1");
        }
        config.wasm_backtrace_details(WasmBacktraceDetails::Environment);
        // 线性内存预留 = 估算的最大线性内存（与 limiter 上限严格一致，见
        // MAX_PLUGIN_MEMORY_BYTES）：实例化时一次性预留 256MiB 虚拟地址空间，
        // 增长零系统调用、基址恒定；相比 64-bit 默认（4GiB 预留 + 32MiB guard/
        // 内存）大幅降低 VA 占用。GC 堆未显式配置时沿用同值（wasmtime 语义：
        // gc_heap_* 缺省继承 memory_* 配置）
        config.memory_reservation(core_config.engine.memory_reservation_bytes);
        // 预留即硬顶：初始分配与增长超出预留前均被 limiter 拒绝（memory_growing
        // 在物理分配前调用），内存永不搬移；编译器可静态假设基址不变做优化，
        // 同时杜绝任何路径触发重定位
        config.memory_may_move(false);
        // Wasm 执行栈深度上限：深度递归在 wasm 侧确定性栈溢出 trap，
        // 而非打穿真实线程栈导致进程 abort（见 MAX_WASM_STACK_BYTES）
        config.max_wasm_stack(core_config.store.max_wasm_stack_bytes);
        // 编译缓存：跨进程复用已编译产物（初始化失败降级为不缓存，不阻断运行时）
        if core_config.engine.compile_cache {
            match Cache::new(CacheConfig::new()) {
                Ok(cache) => {
                    config.cache(Some(cache));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "WASM compile cache disabled");
                }
            }
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

        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), app_handle));

        // task 段快照源注册（spec 票 02 去环）：monitor 经注册回调取 host-task
        // 指标，monitor.rs 不再内联引用 manager::task。注册幂等；monitor 仅经本
        // runtime 暴露，快照必在注册之后——生产路径无「未注册段降级」窗口
        // （降级仅裸 registry 单测可观测，见 monitor.rs tests）
        let monitor = Arc::new(MetricsRegistry::new());
        crate::wasm_core::manager::task::register_task_metrics_source(&monitor);

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
    pub fn set_config(&self, cfg: CoreConfig) -> crate::Result<()> {
        if let Err(reason) = cfg.validate() {
            return Err(crate::AppError::Config(format!("内核配置非法: {}", reason)));
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
    /// 缓存文件名带 `c` 前缀区分 core module 产物（同一路径的插件切换形态
    /// 时不会误读对方产物）；`Component::serialize` 产物与 `Module::serialize`
    /// 不同，混用会反序列化失败。
    pub fn compile_component_from_file(&self, path: &Path) -> crate::Result<wasmtime::component::Component> {
        // 无 AOT 缓存目录（无头/测试上下文）时退化为纯编译
        let Some(cache_dir) = &self.aot_cache_dir else {
            return wasmtime::component::Component::from_file(&self.engine, path).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to compile WASM component from '{}': {}",
                    path.display(),
                    e
                ))
            });
        };

        // 源码大小计入缓存 key（内容变化但 mtime 未更新的场景：大小变化必然换 key）；
        // 新鲜度主判据为 mtime——同大小同 mtime 的编辑无法探测（无成本方案），
        // 但旧产物是合法编译代码不会崩溃，仅行为漂移，属可接受残留
        let wasm_md = std::fs::metadata(path).ok();
        let cache_path = cache_dir.join(format!(
            "c{:016x}.cwasm",
            aot_cache_key(path, wasm_md.as_ref().map(|md| md.len()).unwrap_or(0))
        ));

        let cache_fresh = wasm_md
            .and_then(|w| w.modified().ok())
            .zip(std::fs::metadata(&cache_path).ok().and_then(|c| c.modified().ok()))
            .map(|(wm, cm)| cm >= wm)
            .unwrap_or(false);

        if cache_fresh {
            // unsafe：产物为本机自写缓存；Engine 版本/特性不匹配时 deserialize 失败，
            // 回退到完整编译路径
            if let Ok(bytes) = std::fs::read(&cache_path) {
                if let Ok(component) = unsafe { wasmtime::component::Component::deserialize(&self.engine, &bytes) } {
                    tracing::debug!(
                        path = %cache_path.display(),
                        "Loaded WASM component from AOT cache"
                    );
                    return Ok(component);
                }
            }
        }

        let component = wasmtime::component::Component::from_file(&self.engine, path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to compile WASM component from '{}': {}",
                path.display(),
                e
            ))
        })?;

        // 写回 AOT 缓存：先写临时文件再 rename（原子替换，避免崩溃留半截产物）；
        // 失败不阻断加载（下次启动重新编译）
        match component.serialize() {
            Ok(bytes) => {
                if let Err(e) = std::fs::create_dir_all(cache_dir) {
                    tracing::warn!(
                        path = %cache_dir.display(),
                        error = %e,
                        "Failed to create AOT cache dir, will recompile next time"
                    );
                    return Ok(component);
                }
                let tmp_path = cache_path.with_extension("cwasm.tmp");
                let write_result =
                    std::fs::write(&tmp_path, &bytes).and_then(|_| std::fs::rename(&tmp_path, &cache_path));
                if let Err(e) = write_result {
                    tracing::warn!(
                        path = %cache_path.display(),
                        error = %e,
                        "Failed to write AOT cache, will recompile next time"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize component for AOT cache");
            }
        }

        Ok(component)
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
    /// 作为本 Store 的限额（见 [`crate::wasm_core::security::SecurityFramework::resolve_store_limits`]）。
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
        // 死亡日志成对，构成实例生命周期观测（plugin_id 键控）
        tracing::info!(
            plugin_id = %plugin_id,
            "WASM plugin instance created (component model)"
        );
        Ok(plugin)
    }

    /// 获取文件系统访问校验器引用
    pub fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }
}


// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // A0-3 前置探针（P1/P2/P5）：async store 兼容性 + 资源限制 async 语义 + 性能基线。
    // 文档：.scratch/2026-09-21-a0-3-host-async/spec.md + report.md（只读探针，不碰生产路径）
    mod a03_probe;
    // 终端输出消费插件化性能前置验证（P1-P3，只读探针；文档 .scratch/2026-09-21-terminal-output-consumer-perf/）
    mod terminal_output_perf;
    // WS 终端输出路径吞吐探针（票 09 性能门禁 §9.3：插件 ring-fetch + WIT binary + WS send）
    mod ws_output_perf;
    // 域拆分（P0）：测试函数自本文件拆至 wasm_runtime/tests/，共享脚手架留在下方；
    // 各域文件 `use super::*` 复用，fixture 互斥与产物构建语义不变
    mod component_e2e;
    mod engine_limits;
    mod http_e2e;
    mod p3_async_host_import;
    mod pty_e2e;
    mod sdk_e2e;
    mod session_e2e;
    mod task_e2e;
    mod wasi_e2e;
    mod ws_e2e;

    /// 测试用插件 ID
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";

    /// 校验 RFC3339 UTC 字符串格式（trust addedAt 断言用；与插件
    /// pairing::code 的 format_rfc3339_utc 输出格式一致）
    fn parse_rfc3339_for_test(s: &str) -> bool {
        use chrono::{DateTime, Utc};
        DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)).is_ok()
    }

    /// 创建 WasmRuntime + 宿主上下文（不实例化插件）
    ///
    /// 供需要独立编译/实例化组件的测试复用。
    /// 无头构建（app_handle = None）：tao 事件循环不允许在测试线程创建，
    /// emit/数据目录类能力在测试中不被调用路径覆盖；
    /// AOT 缓存目录注入到系统临时目录，保证 compile_component_from_file 走缓存路径。
    fn setup_wasm_runtime() -> (WasmRuntime, Arc<WasmHostContext>) {
        setup_wasm_runtime_with_config(CoreConfig::default())
    }

    /// 以指定内核配置构建无头运行时（测试专用；`a03_probe` 的燃料禁用/紧内存探针用）
    fn setup_wasm_runtime_with_config(core_config: CoreConfig) -> (WasmRuntime, Arc<WasmHostContext>) {
        use crate::db::Database;
        use crate::wasm_core::bus::MessageBus;
        use crate::wasm_core::storage::PluginStorage;
        use crate::system::config::AppConfig;

        // AppConfig 初始化
        static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
        CONFIG_INIT.call_once(|| {
            let mut config = AppConfig::default();
            config.network.port = 8765;
            AppConfig::init(config);
        });

        let all_permissions: &[&str] = &[
            "storage",
            "database:main",
            "broadcast",
            "terminal:input",
            "terminal:output",
            "session:read",
            "fs:read",
            "fs:write",
            "ui:sidebar",
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db = Database::new(&std::path::PathBuf::from(":memory:")).unwrap();
            db.init_schema().unwrap();
            let db = Arc::new(Mutex::new(db));

            let storage = Arc::new(PluginStorage::new(db.clone()));

            // 配置管理器持一个建好 schema 的内核库：票 02 的 legacy 迁移通道经
            // `host-session.config-*` 读 `session_configs`，表不存在即报
            // `no such table`（票 07 的旧教训）。会话管理器自 v21 起无库依赖
            // （内核不再读配置表），故此处不再共库。
            let kernel_db = Arc::new(Mutex::new({
                let db = Database::new(&std::path::PathBuf::from(":memory:")).unwrap();
                db.init_schema().unwrap();
                db
            }));
            let permission = Arc::new(PermissionManager::new());
            permission.grant_permissions(
                TEST_PLUGIN_ID,
                &all_permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );

            let message_bus = Arc::new(MessageBus::new());

            // 无头构建：不创建 AppHandle（tao 事件循环不允许在测试线程初始化）
            let mut wasm_runtime = WasmRuntime::with_config(storage.clone(), None, core_config).unwrap();
            // 注入 AOT 缓存目录（生产由 app_handle 派生，测试无头上下文手动注入）
            wasm_runtime.aot_cache_dir = Some(std::env::temp_dir().join(format!("bedcode_aot_{}", std::process::id())));

            let mut host_ctx = WasmHostContext::new(
                db,
                Arc::new(Mutex::new(std::collections::HashMap::new())),
                storage,
                None,
                permission,
                wasm_runtime.fs_auth().clone(),
                message_bus,
                Arc::new(crate::wasm_core::manager::capability::CapabilityRegistry::new()),
            );
            // 注入插件私有库根目录（无头上下文无 AppHandle，见字段文档）：
            // 票 08 的 S1 闭环需要真实私有库（host-plugin-database）
            host_ctx.set_plugin_db_root(Some(plugin_db_root()));
            let host_ctx = Arc::new(host_ctx);

            // 注入 host-task 执行引擎 + 单元执行器注册表（与 PluginHost 生产装配同构）：
            // host_api/task.rs 经 TaskEngine 接口调用 core-task；execute_unit 经
            // UnitExecutor 注册表分发域执行器（幂等去重，多测试共用进程级注册表）
            host_ctx
                .set_task_engine(Arc::new(crate::wasm_core::manager::task::CoreTaskEngine))
                .await;
            crate::wasm_core::manager::task::register_unit_executor(Arc::new(
                crate::wasm_core::host_api::fs::FsUnitExecutor,
            ));
            crate::wasm_core::manager::task::register_unit_executor(Arc::new(
                crate::wasm_core::host_api::process::ProcessUnitExecutor,
            ));
            crate::wasm_core::manager::task::register_unit_executor(Arc::new(
                crate::wasm_core::host_api::http::HttpUnitExecutor,
            ));

            (wasm_runtime, host_ctx)
        })
    }

    /// 无头测试的插件私有库根目录（`aot_cache_dir` 同模式：进程级固定路径，
    /// 供需要用真实私有库的用例定位/清理 `com.bedcode.terminal-session/plugin.db`）
    fn plugin_db_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bedcode_plugin_dbs_{}", std::process::id()))
    }

    // ==================== Component Model 测试 ====================

    /// 构建测试用组件插件并编码为组件
    ///
    /// 测试插件为独立 crate（packages/plugin-component-test），基于
    /// WIT 契约（packages/plugin-sdk-desktop/rust/wit）生成绑定；
    /// 源码变更检测与 build_test_wasm 同策略
    ///
    /// `BEDCODE_PLUGIN_DEBUG=1`（dev 构建下）时以 debug profile 构建（保留
    /// DWARF 行号，供行号冒烟测试断言 trap 错误串含 file:line）
    fn build_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-component-test");

        let profile = if plugin_debug_mode() { "debug" } else { "release" };
        let output_dir = plugin_dir.join(format!("target/wasm32-wasip3/{}", profile));
        let module_path = output_dir.join("bedcode_plugin_component_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read test component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let mut args = vec!["build", "--target", "wasm32-wasip3"];
        if profile == "release" {
            args.push("--release");
        }
        args.extend(["--manifest-path", manifest_path.to_str().unwrap()]);
        // 与 ws / pty / sdk / wasip3 fixture 同构：wasm32-wasip3 只存在于 pinned
        // nightly，ambient 工具链（stable）没有该 target → 必须显式注入
        // RUSTUP_TOOLCHAIN，否则 WIT/fixture 源一变更（mtime 触发重建）就报
        // 「Test component WASM build failed」，与代码无关地一次红几十项
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", WASIP3_NIGHTLY)
            .args(&args)
            .status()
            .expect("Failed to run cargo build for test component");
        assert!(status.success(), "Test component WASM build failed");

        std::fs::read(&module_path).expect("Failed to read test component after build")
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

    /// 构建 host-websocket fixture 插件（packages/plugin-ws-test）并编码为组件
    ///
    /// 源码变更检测覆盖 fixture 与 SDK 的 host-websocket 链路文件
    fn build_ws_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-ws-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_ws_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_ws.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/host/ws.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read ws fixture component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for ws fixture component");
        assert!(status.success(), "ws fixture component WASM build failed");

        // wasm32-wasip3（同 wasip2）已内嵌 wasm-component-ld：产物直接是组件，无需 encode
        std::fs::read(&module_path).expect("Failed to read ws fixture component after build")
    }

    // ==================== host-http 服务端域端到端（ABI v29，路由注册下沉） ====================

    /// 构建 host-http fixture 插件（packages/plugin-http-test）并编码为组件
    fn build_http_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-http-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_http_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/host/http.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read http fixture component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for http fixture component");
        assert!(status.success(), "http fixture component WASM build failed");

        // wasm32-wasip3（同 wasip2）已内嵌 wasm-component-ld：产物直接是组件，无需 encode
        std::fs::read(&module_path).expect("Failed to read http fixture component after build")
    }

    // ==================== host-pty 创建→拉取端到端（ABI v16，票 02） ====================

    /// host-pty fixture e2e 串行锁
    ///
    /// 插件 PTY 注册表按属主**进程级全局**登记（`host_impl/pty.rs` 的 `PTYS`），
    /// 用例间共用 fixture 常量属主 id；票 04 起停用回收会清对方句柄，故并行必须串行。
    static PTY_FIXTURE_E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 取得 host-pty fixture e2e 串行锁（中毒后取回内部值继续）
    fn lock_pty_fixture_e2e() -> std::sync::MutexGuard<'static, ()> {
        PTY_FIXTURE_E2E_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 构建 host-pty fixture 插件（packages/plugin-pty-test）
    fn build_pty_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-pty-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_pty_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/host/pty.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read pty fixture component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for pty fixture component");
        assert!(status.success(), "pty fixture component WASM build failed");

        std::fs::read(&module_path).expect("Failed to read pty fixture component after build")
    }

    /// 构建 host-task fixture 插件（packages/plugin-task-test，ABI v20）
    fn build_task_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-task-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_task_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_task.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/host/task.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read task fixture component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for task fixture component");
        assert!(status.success(), "task fixture component WASM build failed");

        // wasm32-wasip3（同 wasip2）已内嵌 wasm-component-ld：产物直接是组件，无需 encode
        std::fs::read(&module_path).expect("Failed to read task fixture component after build")
    }

    /// fixture 命令的 error 载荷（`wasm_entry!` 把 guest Err 编码为 `{"error": ...}`
    /// 字符串返回，不上抛为 trap）
    fn pty_command_error(raw: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(raw).expect("command json");
        value["error"]
            .as_str()
            .unwrap_or_else(|| panic!("期望 error 载荷，got: {raw}"))
            .to_string()
    }

    /// guest error 载荷 → `Some(错误文案)`；成功载荷 → `None`（矩阵分格自带命令名定位）
    fn pty_error_of(raw: &str) -> Option<String> {
        serde_json::from_str::<serde_json::Value>(raw)
            .expect("command json")
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// `data` 字段（list<u8>）→ 文本（PTY 输出含控制字符，按 lossy 处理）
    fn pty_fetched_text(value: &serde_json::Value) -> String {
        let bytes: Vec<u8> = value
            .get("data")
            .and_then(|d| d.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
            .unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// 经 fixture 的 `pty-ring-fetch` 命令按游标拉取一次
    async fn pty_fixture_fetch(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        pty_id: &str,
        from_offset: u64,
    ) -> serde_json::Value {
        let args = serde_json::json!({ "ptyId": pty_id, "fromOffset": from_offset, "maxBytes": 4096 }).to_string();
        let raw = {
            let mut guard = plugin.lock().await;
            guard.invoke_command("pty-ring-fetch", &args).expect("pty-ring-fetch")
        };
        serde_json::from_str(&raw).expect("pty-ring-fetch json")
    }

    /// 轮询拉取直到输出含 `want`（真 PTY 产出异步：断言内容，不断言时序）
    async fn pty_fetch_until(plugin: &Arc<Mutex<LoadedWasmPlugin>>, pty_id: &str, want: &str) -> serde_json::Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let fetched = pty_fixture_fetch(plugin, pty_id, 0).await;
            if pty_fetched_text(&fetched).contains(want) || std::time::Instant::now() >= deadline {
                return fetched;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// 加载并激活一个 host-pty fixture 实例（编译组件 + 接线 dispatcher + activate 订阅）
    async fn pty_activate_fixture(
        runtime: &WasmRuntime,
        ctx: &Arc<WasmHostContext>,
        plugin_id: &str,
    ) -> Arc<Mutex<LoadedWasmPlugin>> {
        let component = runtime
            .compile_component(&build_pty_test_component())
            .expect("compile pty fixture component");
        let plugin = Arc::new(Mutex::new(
            runtime
                .instantiate_component(&component, plugin_id, Arc::clone(ctx), &[], None)
                .expect("instantiate pty fixture"),
        ));
        ctx.message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(plugin_id.to_string(), plugin.clone())]))),
            }))
            .await;
        plugin.lock().await.activate().expect("activate pty fixture");
        plugin
    }

    /// fixture 命令调用（成功路径）：返回解析后的 JSON 载荷
    async fn pty_fixture_call(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        command: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let raw = {
            let mut guard = plugin.lock().await;
            guard
                .invoke_command(command, &args.to_string())
                .unwrap_or_else(|e| panic!("{command} 调用失败: {e}"))
        };
        let value: serde_json::Value = serde_json::from_str(&raw).expect("command json");
        assert!(value.get("error").is_none(), "{command} 期望成功载荷，got: {value}");
        value
    }

    /// 读 fixture 已收事件列表（`<owner>::pty:exit` 投递事实源）
    async fn pty_fixture_events(plugin: &Arc<Mutex<LoadedWasmPlugin>>) -> Vec<serde_json::Value> {
        let state = pty_fixture_call(plugin, "pty-state", serde_json::json!({})).await;
        state["events"].as_array().cloned().unwrap_or_default()
    }

    /// 轮询 fixture 事件直至出现指定 ptyId 的退出事件（宿主→guest 投递异步）
    async fn pty_wait_exit_event(plugin: &Arc<Mutex<LoadedWasmPlugin>>, pty_id: &str) -> serde_json::Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(event) = pty_fixture_events(plugin)
                .await
                .into_iter()
                .find(|e| e["payload"]["ptyId"] == pty_id)
            {
                return event;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "属主插件必须在超时前收到 {pty_id} 的 pty:exit 事件"
            );
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// 构建 SDK 组件形态测试插件（packages/plugin-sdk-test）并编码为组件
    ///
    /// 与 build_test_component 的区别：插件经真实 SDK（wasm_entry! 宏 + WasmHost）
    /// 构建，验证迁移阶段 B 的 SDK 组件产物链路；源码变更检测覆盖 SDK 关键文件
    fn build_sdk_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-sdk-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_sdk_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("plugin.json"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/api_call.rs"),
                packages_dir.join("plugin-sdk-desktop/rust-macros/src/lib.rs"),
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
                return std::fs::read(&module_path).expect("Failed to read SDK test component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for SDK test component");
        assert!(status.success(), "SDK test component WASM build failed");

        std::fs::read(&module_path).expect("Failed to read SDK test component after build")
    }

    /// 构建 wasm32-wasip2 测试组件（WASI preopen E2E 用）
    ///
    /// 与其它测试组件不同：目标为 WASI preview2（std::fs 直连预打开目录），
    /// 依赖宿主当前机器已安装 wasm32-wasip2 target（rustup target add）。
    /// 预装的其它测试组件不依赖该 target，互不干扰。
    fn build_wasi_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-wasi-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip2/release");
        let module_path = output_dir.join("bedcode_plugin_wasi_test.wasm");

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
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let plugin_dir = manifest_dir.join("../packages/plugin-wasip3-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_wasip3_test.wasm");

        // 复用策略同其它测试组件：产物存在且源码未更新则跳过构建（跑测试不重复编译）
        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                plugin_dir.join("Cargo.toml"),
                manifest_dir.join("../packages/plugin-sdk-desktop/rust/wit/bedcode.wit"),
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
                return Some(std::fs::read(&module_path).expect("read wasip3 test component"));
            }
        }

        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                plugin_dir.join("Cargo.toml").to_str().unwrap(),
            ])
            .status()
            .expect("run cargo build for wasip3 test component");
        assert!(status.success(), "wasip3 test component WASM build failed");

        Some(std::fs::read(&module_path).expect("Failed to read wasip3 test component after build"))
    }

    // ==================== WASI preopen E2E ====================

    // ==================== host-task 闭环（ABI v20，spec `.scratch/2026-09-21-host-task-concurrency/`） ====================

    /// host-task 消费派发测试替身：收集 dispatch_task_event 事件 + 真实投递到
    /// 注册实例（验证 SDK 回调链路：dispatch → LoadedWasmPlugin::on_task_event →
    /// WIT events-task 导出 → fixture on_task_event）
    struct MockTaskServices {
        events: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    }

    impl PluginServices for MockTaskServices {
        fn mark_plugin_error(&self, _plugin_id: String, _error: String) {}
        fn register_plugin_timer(&self, _plugin_id: String, _interval_secs: u64, _command: String) {}
        fn dispatch_process_done(&self, _plugin_id: String, _event: serde_json::Value) {}
        fn dispatch_task_event(&self, plugin_id: String, event: serde_json::Value) {
            // 收集事件序列（phase 顺序断言用）。真实投递（with_wasm_plugin_call →
            // LoadedWasmPlugin::on_task_event）由 PluginHost 的 dispatch 实现承担，
            // 与 dispatch_process_done 同构；SDK 回调链路在 submit 用例中手动验证
            let _ = plugin_id;
            self.events.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        }
        fn install_cli(
                        &self,
            _plugin_id: String,
            _file_name: String,
            _bin_dir: String,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>> {
            Box::pin(async { Err("mock: no cli".to_string()) })
        }
        fn uninstall_cli(
                        &self,
            _plugin_id: String,
            _file_name: String,
            _bin_dir: String,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }
        fn plugin_resource_dir(
                        &self,
            _plugin_id: String,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>> {
            Box::pin(async { Err("mock: no resource dir".to_string()) })
        }
    }

    impl MockTaskServices {
        fn new() -> Self {
            Self {
                events: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn phases(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .filter_map(|e| e.get("phase").and_then(|v| v.as_str()).map(str::to_string))
                .collect()
        }
    }

    /// 无头 setup 的 fs_auth（security 框架）默认放行：fixture 需真实 fs:read
    /// 权限走 permission 门 + fs_auth 三层（授权语义见 host_impl/fs.rs）
    fn task_fixture_plugin_id() -> String {
        "com.bedcode.task-test".to_string()
    }

    /// 会话中心「插件私有库」用例串行锁：`plugin_db_root()` 是**进程级**路径
    /// （`aot_cache_dir` 同模式），所有 `activate()` 会话中心的用例共用同一份
    /// `com.bedcode.terminal-session/plugin.db`，于是两类竞态都会把断言变成 flaky：
    /// - 配置面用例先 `remove_dir_all` 清库再断言「legacy 两条全部迁入」，而任何一次
    ///   并发 `activate()` 都会写入 `config.migrated_at` marker → 本方读到 0 行；
    /// - tick 按时间条件批量改行（超宽限的 pending → missed），并发用例注入的
    ///   `now_utc` 会提前推进另一方的定时任务。
    /// 持锁即把「同一份私有库」上的写入排成一条序列（用例内仍各自清库）。
    ///
    /// 覆盖范围是**全部**会话中心闭环用例（host-business-decarriage 收尾补全：此前
    /// create-with-spec / actions / annotate / config-api / trust 五个用例未持锁，
    /// 只要时序一变（如重启用例的完成信号从广播改为 Created 事件）就会让本用例
    /// 读到空列表而翻红——新会话闭环用例必须同样持锁）。
    static SESSION_PLUGIN_DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn session_plugin_db_guard() -> std::sync::MutexGuard<'static, ()> {
        SESSION_PLUGIN_DB_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 互调 wire 捕获器（票 10 闭环）：静态订阅认证中心的请求 topic，记录
    /// file-transfer → auth center 的 JSON-RPC 请求（含 params 原样）
    struct AuthCenterCaptureHandler {
        captures: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl crate::wasm_core::bus::BusMessageHandler for AuthCenterCaptureHandler {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            // std Mutex 短临界区（总线消费任务内，禁止阻塞锁/await）
            self.captures
                .lock()
                .expect("capture lock")
                .push((msg.topic.clone(), msg.payload.clone()));
            Ok(())
        }
    }

    /// 会话中心互调 api 清单：读插件工程 manifest（与 `#[plugin_api]` 编译期防漂移
    /// 比对同一真源）。宿主测试按它登记注册表——在测试里再抄一份 api 字符串就是
    /// 第二真源，桥接锚点漂移会退化成「本来就该被测出来的静默降级」。
    fn session_apis() -> Vec<String> {
        let manifest_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/terminal-session/plugin.json");
        let raw = std::fs::read_to_string(&manifest_path).expect("session plugin.json 可读");
        let manifest: serde_json::Value = serde_json::from_str(&raw).expect("session manifest JSON");
        manifest["api"]
            .as_array()
            .expect("api 数组")
            .iter()
            .map(|v| v.as_str().expect("api 字符串").to_string())
            .collect()
    }

    /// 测试用消息投递器：总线消息按 plugin_id 路由到测试持有的插件实例
    ///
    /// 生产环境由 PluginHost 实现 MessageDispatcher（with_wasm_plugin_call
    /// 加锁调用 + trap 自动重载）；互调测试无 PluginHost，等价实现：查实例表
    /// 加锁调用 on_message。is_activated 恒真（本测试全部实例均已 activate，
    /// 「未激活订阅者不投递」的语义由门禁/注销断言覆盖）。
    struct TestInstanceDispatcher {
        instances: Arc<RwLock<HashMap<String, Arc<Mutex<LoadedWasmPlugin>>>>>,
    }

    impl crate::wasm_core::bus::MessageDispatcher for TestInstanceDispatcher {
        fn dispatch_to_wasm(&self, plugin_id: &str, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            let instances = self.instances.clone();
            let plugin_id = plugin_id.to_string();
            let msg = msg.clone();
            block_on_async(async move {
                let instances = instances.read().await;
                let plugin = instances
                    .get(&plugin_id)
                    .ok_or_else(|| anyhow::anyhow!("TestInstanceDispatcher: no instance '{}'", plugin_id))?;
                let mut plugin = plugin.lock().await;
                // v11：按载荷格式路由（与生产 PluginHost 的 dispatch_to_wasm 同语义）
                if let Some(bytes) = &msg.payload_binary {
                    plugin
                        .on_message_binary(&msg.topic, &msg.sender, bytes)
                        .map_err(|e| anyhow::Error::from(e))
                } else {
                    plugin
                        .on_message(&msg.topic, &msg.sender, &msg.payload)
                        .map_err(|e| anyhow::Error::from(e))
                }
            })
        }

        /// ABI v14：WS 帧投递（`events-ws`）——与 `dispatch_to_wasm` 同桥，
        /// 生产环境由 PluginHost 实现（本实现等价：查实例表加锁调用）
        fn dispatch_ws_frame(
            &self,
            plugin_id: &str,
            frame: &crate::wasm_core::bus::WsFrameDispatch,
        ) -> anyhow::Result<bool> {
            let instances = self.instances.clone();
            let plugin_id = plugin_id.to_string();
            let frame = frame.clone();
            block_on_async(async move {
                let instances = instances.read().await;
                let plugin = instances
                    .get(&plugin_id)
                    .ok_or_else(|| anyhow::anyhow!("TestInstanceDispatcher: no instance '{}'", plugin_id))?;
                let mut plugin = plugin.lock().await;
                plugin.on_ws_frame(&frame).map_err(anyhow::Error::from)
            })
        }

        fn is_activated(&self, _plugin_id: &str) -> bool {
            true
        }
    }
}
