//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 组件（Component Model）加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//!
//! 宿主能力实现位于 [`host_impl`] 子模块（权限校验 + 宿主服务调用），
//! 组件绑定与实例化位于 [`component`] 子模块，本模块只负责运行时生命周期
//! 管理与宿主上下文定义

mod component;
pub(crate) mod host_impl;

pub use component::LoadedWasmPlugin;
/// WASI 预打开目录解析（激活时重建实例判定用，见 host.rs `rebuild_wasm_instance`）
pub(crate) use component::resolve_preopen_dirs;
/// 声明展开（不过滤授权，preauthorize 收集弹窗候选用，见 host.rs `preauthorize_plugin`）
pub(crate) use component::expand_preopen_declarations;

use crate::db::Database;
use crate::plugin::security::fs_auth::FsAuthChecker;
use crate::plugin::permission::PermissionManager;
use crate::plugin::manager::storage::PluginStorage;
use crate::session::{SessionConfigManager, SessionManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use wasmtime::{Cache, CacheConfig, Config, Engine, ResourceLimiter, WasmBacktraceDetails};

// ==================== Resource Limits & Interruption ====================

// 运行参数单一事实源在配置模块（core-config，见 `crate::plugin::config`）：
// Engine 构建参数与 Store 资源上限的默认值即历史生产常量（`config::defaults`），
// 支持配置文件加载与运行时覆盖；燃料看门狗的语义说明随默认值一并迁入。
pub(crate) use crate::plugin::config::plugin_debug_mode;
use crate::plugin::config::{CoreConfig, StoreLimits};
use bedcode_plugin_api::ResourceOverrides;
use crate::plugin::monitor::{LifecycleEvent, MetricsRegistry, PluginMetrics};

/// 从 catch_unwind 的 panic 载荷提取人类可读消息（统一 panic 诊断文案）
pub fn panic_payload_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

// ==================== Async Blocking Helper ====================

thread_local! {
    /// 当前线程是否已处于 block_in_place 让出后的阻塞上下文
    static IN_BLOCK_IN_PLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 无当前 runtime handle 的线程（spawn_blocking / 纯 std 线程）执行 block_on 时
/// 的全局收益运行时：与 wasmtime-wasi 的 ambient runtime 同策略，供宿主函数在
/// 无 handle 线程上仍可阻塞执行（WASI 预打开模式下插件调用跑在阻塞线程上）
static AMBIENT_RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .expect("create ambient tokio runtime")
});

/// 重入标志的 RAII 守卫：作用域退出（含 block_in_place panic 穿透）时复位标志，
/// 避免线程残留 `true` 导致后续调用恒走新线程路径（正确但多一次线程切换）
struct BlockInPlaceGuard;

impl BlockInPlaceGuard {
    /// 进入阻塞上下文：重入时返回 None（调用方改走新线程路径）
    fn enter() -> Option<Self> {
        if IN_BLOCK_IN_PLACE.with(|f| f.get()) {
            return None;
        }
        IN_BLOCK_IN_PLACE.with(|f| f.set(true));
        Some(BlockInPlaceGuard)
    }
}

impl Drop for BlockInPlaceGuard {
    fn drop(&mut self) {
        IN_BLOCK_IN_PLACE.with(|f| f.set(false));
    }
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

/// 在同步上下文中执行 async 闭包，兼容多线程和 current_thread 运行时
///
/// WASM host functions 是同步的，但需要调用 async Tokio 代码（数据库、锁等）。
/// 标准做法 `block_in_place(|| block_on(...))` 仅在多线程运行时上可用，
/// Actix Web 的 `actix-rt` 使用 `current_thread` 运行时，会导致 panic。
///
/// 策略：
/// - 多线程运行时：`block_in_place` + `block_on`（不阻塞 worker 线程）
/// - current_thread 运行时或非运行时线程：`std::thread::spawn` + `block_on`（新线程上运行）
///
/// 重入安全：`dispatch_to_wasm` → 插件 on_message → host http_fetch 的调用链会
/// 嵌套调用本函数。嵌套 `block_in_place` 在已让出的线程上会 panic；而嵌套
/// `handle.block_on` 同样 panic——外层 `block_in_place(|| handle.block_on(...))`
/// 的 tokio enter 守卫仍挂在当前线程上（block_in_place 只是把线程让出 worker 池，
/// 守卫不释放），实证见 panic.log 的 wasm_runtime.rs:82 FATAL
/// （"Cannot start a runtime from within a runtime"）。两种 panic 都会穿透污染
/// wasmtime Store、插件永久不可用，故用线程局部标志检测重入，重入时改在
/// **新线程上 block_on**：新线程无 enter 守卫、非 worker，任意 flavor 均合法，
/// 外层线程 join 等待（runtime 其他 worker 推进 IO，无死锁）。
pub(crate) fn block_on_async<F, R>(fut: F) -> R
where
    F: std::future::Future<Output = R> + Send,
    R: Send + 'static,
{
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // 无当前 runtime 上下文（spawn_blocking 阻塞线程 / 纯 std 线程）：
        // 在全局 ambient multi-thread 运行时上阻塞执行。
        // 与 wasmtime-wasi 的 ambient runtime 同策略——这是 WASI 预打开模式的关键：
        // 插件调用被搬到无 handle 线程后，wasi 同步绑定（in_tokio）走其自身 ambient
        // runtime，宿主函数经此 ambient runtime 阻塞执行，两者互不冲突。
        return AMBIENT_RT.block_on(fut);
    };
    match handle.runtime_flavor() {
        tokio::runtime::RuntimeFlavor::MultiThread => {
            if let Some(_guard) = BlockInPlaceGuard::enter() {
                // guard 持有期间当前线程在 worker 池外阻塞；退出（含 panic）时复位重入标志
                tokio::task::block_in_place(|| handle.block_on(fut))
            } else {
                // 重入：当前线程已被外层 block_in_place + handle.block_on 占据
                // （enter 守卫仍生效），嵌套 handle.block_on 必然 panic
                // （Cannot start a runtime from within a runtime）。
                // 新线程无 enter 守卫，block_on 合法；外层同步 join 等待结果。
                std::thread::scope(|s| {
                    s.spawn(|| handle.block_on(fut))
                        .join()
                        .expect("block_on_async: spawned thread panicked")
                })
            }
        }
        _ => {
            // current_thread 运行时（#[tokio::test] / Actix-rt worker）：当前线程
            // 已在 runtime context 内，两条路都走不通：
            // - `handle.block_on`（current_thread 调度器由 owner 线程独占驱动，
            //   本线程即 owner 线程，直接调用必然死锁；跨线程驱动 IO/process
            //   future 同样永久空转——历史死锁：process_kill 测试）；
            // - `AMBIENT_RT.block_on`（本线程）：重入检查 panic
            //   （"Cannot start a runtime from within a runtime"）。
            // 方案：在 scoped 新线程（无 runtime 上下文、支持非 'static future）
            // 上 AMBIENT_RT.block_on。ambient runtime 是 multi_thread + enable_all，
            // IO/process/time 驱动齐全，multi_thread 的 block_on 契约本就允许任意
            // 线程调用（future 在调用线程内执行、spawned 任务进线程池）。
            //
            // ⚠️ 本分支会阻塞调用线程直到 future 完成：**调用方必须是「不驱动
            // future 所依赖资源」的线程**。actix arbiter 是反例——宿主 WS 原语要
            // await arbiter 上的连接 actor，投递任务若在 arbiter 线程上同步等待即
            // 自锁（见 `ambient_handle` 说明）。
            std::thread::scope(|s| {
                s.spawn(|| AMBIENT_RT.block_on(fut))
                    .join()
                    .expect("block_on_async: spawned thread panicked")
            })
        }
    }
}

/// 在全局 ambient runtime 上同步阻塞驱动 future（供无 handle 的阻塞线程使用）
///
/// 与 [`block_on_async`] 的 ambient 兜底同 runtime，但**不要求** future/
/// 输出满足 `'static`——仅同步驱动当前 future 并返回结果，不把 future
/// 交给其它执行器接管。`run_guest_call` 在 `spawn_blocking` 线程驱动 tokio
/// Mutex 锁获取用（借用闭包内 Arc，无法满足 `'static` 约束）。
pub(crate) fn block_on_ambient<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send,
{
    AMBIENT_RT.block_on(fut)
}

/// ambient runtime 句柄：在「调用方线程不可被占用」的场景派生后台任务
///
/// 典型场景是 **actix arbiter**：它是 `current_thread` 运行时、由本线程独占驱动，
/// 而插件投递用的是同步桥 [`block_on_async`]（会阻塞调用线程）。若投递任务跑在
/// arbiter 上，客人回调里的宿主原语（如 WS 端点的 `send-text-to-client`）需要
/// await arbiter 上的连接 actor —— arbiter 被投递自己占住，双方互等形成自锁
/// （实证：插件端点回显帧）。故此类投递改在 ambient runtime 上派生，arbiter 保持
/// 空闲以推进 actor。
pub(crate) fn ambient_handle() -> tokio::runtime::Handle {
    AMBIENT_RT.handle().clone()
}

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
            // v11 / v14：可选导出在实例化后动态探测（verify_abi 内写入，见 component.rs）
            on_message_binary: None,
            on_ws_message: None,
            on_ws_client_message: None,
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

/// 插件宿主服务抽象 — 解耦 WasmHostContext 与 PluginHost 的循环依赖
///
/// WasmHostContext 需要回调插件宿主（注册会话生命周期监听器），
/// 而 PluginHost 持有 WasmHostContext —— 通过 trait 对象 + 两阶段注入打破类型互引：
/// 本模块只依赖此 trait，`PluginHost` 在 `plugin::manager::host` 模块中实现它
pub trait PluginServices: Send + Sync + 'static {
    /// 为指定插件创建并注册会话生命周期监听器到 SessionManager
    fn register_session_lifecycle_listener(&self, plugin_id: String, session_manager: Arc<SessionManager>);

    /// 为指定插件创建并注册提交输入行监听器到 SessionManager（见 ADR 0001）
    fn register_session_input_listener(&self, plugin_id: String, session_manager: Arc<SessionManager>);

    /// 标记插件为错误状态
    ///
    /// 仅通知前端弹窗提示，不改变插件状态（保持激活，会话照常运行）。
    /// 由 `host_mark_plugin_error` Host Function 转发，插件自身检测到
    /// 配置失败（如 hooks 脚本拷贝失败）时调用。
    fn mark_plugin_error(&self, plugin_id: String, error: String);

    /// 为指定插件注册宿主周期定时器（v6，ADR 0003）
    ///
    /// 宿主按 interval_secs 到点调用插件的 command（附当前时间参数），
    /// 幂等判断归插件。重复注册替换该插件已有定时器。
    fn register_plugin_timer(&self, plugin_id: String, interval_secs: u64, command: String);

    /// 分发进程执行完成事件到插件（host-process，v8）
    ///
    /// 由 host_impl/process.rs 在进程结束时调用：经插件 export
    /// `on_process_done` 投递 `{ run_id, exit_code, timed_out }`。
    /// 插件未激活/已卸载时调用失败，仅记日志（尽力而为）。
    fn dispatch_process_done(&self, plugin_id: String, event: serde_json::Value);

    /// 安装插件随包 CLI（host-app，v8）：复制到用户 bin 目录 + 注册 PATH（幂等）
    ///
    /// 源 = 插件包目录 `cli/<file-name>`（Windows 自动补 .exe）；
    /// `bin_dir` 为空用平台默认。返回安装后的 bin 目录绝对路径。
    /// 由 host_impl/app.rs 经 block_on_async 驱动（宿主侧注册表/PATH 实现）。
    /// 返回 Box<dyn Future> 保持 trait dyn 兼容（async fn 会破坏 Arc<dyn>）。
    fn install_cli(
        &self,
        plugin_id: String,
        file_name: String,
        bin_dir: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>>;

    /// 卸载插件随包 CLI（host-app，v8）：删文件 + 移除仅本插件的 PATH 条目（幂等）
    ///
    /// 应用关闭流程（deactivate_all 置位 shutting_down）中调用时自动跳过，
    /// CLI 随下次激活重新安装。
    fn uninstall_cli(
        &self,
        plugin_id: String,
        file_name: String,
        bin_dir: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;
}

/// 宿主上下文（注入到 WasmPluginState）
///
/// 持有宿主子系统引用，Host Functions 通过此上下文访问宿主能力
/// plugin_services 使用两阶段初始化：new() 时为 None，PluginHost 构造完成后通过 set_services() 注入
pub struct WasmHostContext {
    db: Arc<Mutex<Database>>,
    /// 插件独立数据库池 — 每插件一个独立 .db 文件和连接
    plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
    storage: Arc<PluginStorage>,
    /// 密钥托管读缓存（v15 host-auth）：read-through（get 命中直接返回，
    /// set/delete 失效对应键）；真源为主库 plugin_secrets 表（重启后一致）
    secrets_cache: Arc<std::sync::RwLock<std::collections::HashMap<(String, String), String>>>,
    session_manager: Arc<SessionManager>,
    /// 会话配置管理器 — 用于获取所有会话配置（working_dir 等）
    config_manager: Arc<SessionConfigManager>,
    /// Tauri AppHandle（无头/测试上下文为 None，emit/路径类宿主能力降级）
    app_handle: Option<Arc<tauri::AppHandle>>,
    permission: Arc<PermissionManager>,
    fs_auth: Arc<FsAuthChecker>,
    message_bus: Arc<crate::plugin::bus::MessageBus>,
    /// 插件宿主服务（两阶段初始化，避免 PluginHost 与 WasmHostContext 类型互引）
    plugin_services: Arc<RwLock<Option<Arc<dyn PluginServices>>>>,
    /// 运行中进程注册表（host-process，v8）：run_id → 进程句柄
    ///
    /// host_impl/process.rs 注册/移除；kill（超时/取消）经此查找句柄。
    process_registry: Arc<ProcessRegistry>,
    /// 插件互调 api 注册表（ADR-0017）：激活登记 / 停用注销，
    /// `bus_publish` 对 `bedcode.api.*` 请求 topic 做目标校验
    api_registry: Arc<crate::plugin::security::api_registry::ApiRegistry>,
    /// 统一授权框架（core-security）：三段决策管线 + 仲裁器注册表
    security: crate::plugin::security::SecurityFramework,
    /// 能力注册表（core-plugin-manager 票据 06）：能力名 → 宿主原语/系统组件
    /// 实例（二选一装配）；host_impl 宿主函数内经此路由
    capabilities: crate::plugin::manager::capability::CapabilityRegistry,
}

/// 运行中的进程（记录 pid 供进程组 kill）
///
/// Child 句柄由执行任务（host_impl/process.rs）独占持有：`Child::wait`
/// 在整个进程生命周期内独占 `&mut self`，注册表若同时持句柄，kill 路径
/// 将阻塞到进程自然退出（死锁）；按 pid 杀进程组则与 wait 无冲突。
struct RunningProcess {
    /// 发起执行的插件 ID（完成事件分发目标）
    plugin_id: String,
    /// 子进程 pid（process_group(0)/CREATE_NEW_PROCESS_GROUP 后为进程组组长）
    pid: u32,
}

/// 进程注册表（run_id → 运行中进程）
///
/// 生命周期：`process_run` 注册 → 进程结束/kill 后移除。
/// 应用退出时进程由 OS 回收（孤儿进程随宿主进程终止）。
pub struct ProcessRegistry {
    runs: std::sync::RwLock<HashMap<String, RunningProcess>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            runs: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 注册运行中进程（run_id 由调用方预生成，UUID）
    ///
    /// 同步锁：临界区仅 map 操作（无 await），wasm host 调用栈内直接可用
    pub fn register(&self, run_id: String, plugin_id: String, pid: u32) {
        let mut runs = self.runs.write().unwrap_or_else(|e| e.into_inner());
        runs.insert(run_id, RunningProcess { plugin_id, pid });
    }

    /// 移除并返回进程的发起插件 ID（进程结束/kill 后调用）
    pub fn remove(&self, run_id: &str) -> Option<String> {
        let mut runs = self.runs.write().unwrap_or_else(|e| e.into_inner());
        runs.remove(run_id).map(|p| p.plugin_id)
    }

    /// 终止进程组（尽力而为）：找到记录则按 pid 杀进程组，返回是否找到
    pub async fn kill(&self, run_id: &str) -> bool {
        let pid = {
            let runs = self.runs.read().unwrap_or_else(|e| e.into_inner());
            match runs.get(run_id) {
                Some(proc) => proc.pid,
                None => return false,
            }
        };
        kill_process_group(pid).await;
        true
    }

    /// 运行中进程数（并发限制/诊断用）
    pub fn running_count(&self) -> usize {
        let runs = self.runs.read().unwrap_or_else(|e| e.into_inner());
        runs.len()
    }
}

/// 终止进程组（尽力而为，超时 kill 与插件取消共用）
///
/// - unix：`kill -9 -<pgid>`（`process_group(0)` 保证 pgid == pid）
/// - Windows：`taskkill /F /T /PID`（/T 连带子进程树）
///
/// 返回是否成功发起（进程已退出 / pid 无效返回 false，属预期内竞态）。
pub(crate) async fn kill_process_group(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        // CREATE_NO_WINDOW：taskkill 为控制台程序，避免超时杀进程时黑窗闪烁
        cmd.creation_flags(0x0800_0000);
        match cmd.output().await {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                tracing::warn!(
                    pid,
                    output = %String::from_utf8_lossy(&o.stderr),
                    "kill_process_group: taskkill reported failure"
                );
                false
            }
            Err(e) => {
                tracing::warn!(pid, error = %e, "kill_process_group: taskkill failed");
                false
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 直接系统调用杀进程组（负 pgid = 组；process_group(0) 后组 id == 进程 pid）。
        // 不使用外部 kill 命令：命令进程是第二个 tokio child，在
        // current_thread（sh 的 wait）与 ambient multi_thread（kill 命令）双 runtime
        // 共享全局 SIGCHLD handler 的场景下，kill 命令退出与目标被杀同时发生时，
        // 两个 reaper 竞争 waitpid(-1) 回收 zombie，可能吞掉 sh 的退出通知导致
        // wait() 永久挂起（CI flaky：process_kill_terminates_process_group）。
        // 同步系统调用不产生 child，从根上消除该竞争。
        let rc = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        tracing::warn!(
            pid,
            error = %err,
            "kill_process_group: kill(-pid) failed"
        );
        false
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

        Ok(Self {
            engine,
            linker,
            fs_auth,
            aot_cache_dir,
            config: Arc::new(std::sync::RwLock::new(core_config)),
            monitor: Arc::new(MetricsRegistry::new()),
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
    /// 支持 ${home}；实例化时经展开+授权过滤，见 component::build_wasi_ctx）
    pub fn load_plugin_from_file(
        &self,
        path: &Path,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        declared_preopen_dirs: &[String],
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
    /// 作为本 Store 的限额（见 [`crate::plugin::security::SecurityFramework::resolve_store_limits`]）。
    pub fn instantiate_component(
        &self,
        component: &wasmtime::component::Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        declared_preopen_dirs: &[String],
        resource_overrides: Option<&ResourceOverrides>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let (limits, fuel_enabled) = {
            let cfg = self.config.read().expect("core config lock poisoned");
            (
                host_ctx.security().resolve_store_limits(plugin_id, &cfg.store, resource_overrides),
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

impl WasmHostContext {
    /// 创建宿主上下文
    ///
    /// `app_handle` 为 None 时（无头/测试上下文）emit、数据目录等能力不可用
    pub fn new(
        db: Arc<Mutex<Database>>,
        plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
        storage: Arc<PluginStorage>,
        session_manager: Arc<SessionManager>,
        config_manager: Arc<SessionConfigManager>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        permission: Arc<PermissionManager>,
        fs_auth: Arc<FsAuthChecker>,
        message_bus: Arc<crate::plugin::bus::MessageBus>,
    ) -> Self {
        let api_registry = Arc::new(crate::plugin::security::api_registry::ApiRegistry::new());
        // 统一授权框架：注册既有资源的仲裁器（fs 三层校验 / api-call 互调门）
        let security = crate::plugin::security::SecurityFramework::new();
        security.register(Arc::new(crate::plugin::security::framework::FsAuthorizer::new(
            permission.clone(),
            fs_auth.clone(),
        )));
        security.register(Arc::new(crate::plugin::security::framework::ApiCallAuthorizer::new(
            api_registry.clone(),
        )));
        Self {
            db,
            plugin_dbs,
            storage,
            secrets_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            session_manager,
            config_manager,
            app_handle,
            permission,
            fs_auth,
            message_bus,
            plugin_services: Arc::new(RwLock::new(None)),
            process_registry: Arc::new(ProcessRegistry::new()),
            api_registry,
            security,
            capabilities: crate::plugin::manager::capability::CapabilityRegistry::new(),
        }
    }

    /// 获取进程注册表引用（host-process）
    pub fn process_registry(&self) -> &Arc<ProcessRegistry> {
        &self.process_registry
    }

    /// 两阶段初始化：PluginHost 构造完成后注入宿主服务
    ///
    /// 必须在 PluginHost::new() 返回后、任何插件 activate 之前调用
    pub async fn set_services(&self, services: Arc<dyn PluginServices>) {
        *self.plugin_services.write().await = Some(services);
    }

    /// 获取宿主服务引用
    ///
    /// 在两阶段初始化完成前返回 None
    pub async fn services(&self) -> Option<Arc<dyn PluginServices>> {
        self.plugin_services.read().await.clone()
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::plugin::bus::MessageBus> {
        &self.message_bus
    }

    /// 获取插件互调 api 注册表引用（ADR-0017 门禁）
    pub fn api_registry(&self) -> &Arc<crate::plugin::security::api_registry::ApiRegistry> {
        &self.api_registry
    }

    /// 获取统一授权框架引用（core-security 三段决策管线）
    pub fn security(&self) -> &crate::plugin::security::SecurityFramework {
        &self.security
    }

    /// 宿主侧互调调用（票 11 命令面桥接）：以宿主虚拟身份发布 JSON-RPC 请求到
    /// `bedcode.api.<api>` topic 并等待回复。
    ///
    /// 与插件 mutual 调用的区别：调用方身份为 [`host_impl::api::HOST_API_CALLER_ID`]
    /// （宿主不是插件，reply topic 路由 `bedcode.api.reply.<caller>.<id>` 用）；
    /// 互调门禁（ADR 0017 层 1）只校验目标 api 已声明、不校验调用方——未激活
    /// 插件在注册表无登记 → 门禁拒绝 → 调用方降级宿主实现。
    pub fn call_plugin_api_host(
        &self,
        request_topic: &str,
        payload_json: &str,
        timeout_ms: u64,
    ) -> Result<String, String> {
        host_impl::api::api_call(self, host_impl::api::HOST_API_CALLER_ID, request_topic, payload_json, timeout_ms)
    }

    /// 获取能力注册表引用（core-plugin-manager：系统组件装配与能力路由）
    pub fn capabilities(&self) -> &crate::plugin::manager::capability::CapabilityRegistry {
        &self.capabilities
    }

    /// 丢弃插件独立数据库连接（卸载时调用，dev 合入的卸载完整性）
    ///
    /// 仅从连接池移除（不删库文件）：删除插件目录前须先释放文件句柄，
    /// 否则在部分平台（Windows）会因文件仍被占用而删不掉
    pub async fn drop_plugin_db(&self, plugin_id: &str) {
        let dropped = self.plugin_dbs.lock().await.remove(plugin_id).is_some();
        if dropped {
            tracing::debug!(plugin_id = %plugin_id, "Plugin database connection dropped");
        }
    }

    /// 获取 SessionManager 的 Arc 引用
    pub fn session_manager_arc(&self) -> Arc<SessionManager> {
        self.session_manager.clone()
    }

    /// 获取或懒加载插件独立数据库
    ///
    /// 首次调用时创建目录 + 打开/创建 plugin.db + 缓存连接
    /// 后续调用直接返回缓存的连接
    pub async fn get_or_create_plugin_db(&self, plugin_id: &str) -> crate::Result<Arc<Mutex<Database>>> {
        // 快速路径：已缓存
        {
            let dbs = self.plugin_dbs.lock().await;
            if let Some(db) = dbs.get(plugin_id) {
                return Ok(db.clone());
            }
        }

        // 慢路径：创建数据库
        let app_handle = self.app_handle.as_ref().ok_or_else(|| {
            crate::AppError::Plugin("plugin database unavailable in headless context (no app_handle)".to_string())
        })?;
        let app_data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| crate::AppError::Plugin(format!("Failed to get app data dir: {}", e)))?;
        let plugin_dir = app_data_dir.join("plugins").join(plugin_id);

        // 创建插件数据目录
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to create plugin data dir '{}': {}",
                    plugin_dir.display(),
                    e
                ))
            })?;
        }

        let db_path = plugin_dir.join("plugin.db");
        let db = Database::new(&db_path)?;

        // 缓存连接
        let db_arc = Arc::new(Mutex::new(db));
        {
            let mut dbs = self.plugin_dbs.lock().await;
            // 双重检查：另一个线程可能已插入
            if let Some(existing) = dbs.get(plugin_id) {
                return Ok(existing.clone());
            }
            dbs.insert(plugin_id.to_string(), db_arc.clone());
        }

        tracing::info!(plugin_id = %plugin_id, path = %db_path.display(), "Plugin database created/opened");
        Ok(db_arc)
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::db::Database;
        use crate::plugin::bus::MessageBus;
        use crate::plugin::manager::storage::PluginStorage;
        use crate::plugin::permission::PermissionManager;
        use crate::session::{SessionConfigManager, SessionManager};
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

            let resource_dir = Arc::new(std::path::PathBuf::from("."));
            let session_manager = Arc::new(SessionManager::from_database(
                Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
                resource_dir.clone(),
            ));

            let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
                Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
            ))));

            let permission = Arc::new(PermissionManager::new());
            permission.grant_permissions(
                TEST_PLUGIN_ID,
                &all_permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );

            let message_bus = Arc::new(MessageBus::new());

            // 无头构建：不创建 AppHandle（tao 事件循环不允许在测试线程初始化）
            let mut wasm_runtime = WasmRuntime::new(storage.clone(), None).unwrap();
            // 注入 AOT 缓存目录（生产由 app_handle 派生，测试无头上下文手动注入）
            wasm_runtime.aot_cache_dir = Some(std::env::temp_dir().join(format!("bedcode_aot_{}", std::process::id())));

            let host_ctx = Arc::new(WasmHostContext::new(
                db,
                Arc::new(Mutex::new(std::collections::HashMap::new())),
                storage,
                session_manager,
                config_manager,
                None,
                permission,
                wasm_runtime.fs_auth().clone(),
                message_bus,
            ));

            (wasm_runtime, host_ctx)
        })
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
        let status = std::process::Command::new("cargo")
            .args(&args)
            .status()
            .expect("Failed to run cargo build for test component");
        assert!(status.success(), "Test component WASM build failed");

        std::fs::read(&module_path).expect("Failed to read test component after build")
    }

    /// 组件完整往返：实例化、ABI 协商、生命周期、命令（guest 内 import 往返）、
    /// 终端钩子、事件回调、上传钩子、manifest
    #[test]
    fn test_component_roundtrip() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        // 组件内 import 调用经 block_on_async 走 tokio（与 core 路径同机制），
        // 测试体整体在运行时上下文中执行
        rt.block_on(async {
            // 预写 storage key：验证 guest 内 host_storage import 读回（JSON 值往返）
            host_ctx
                .storage
                .set(TEST_PLUGIN_ID, "component-test-key", serde_json::json!({"k": "v"}))
                .await
                .expect("preset storage key");

            let mut plugin = wasm_runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
                .expect("instantiate test component");

            // 生命周期
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);

            // manifest
            let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
            assert_eq!(manifest["id"], "com.bedcode.component-test");

            // 命令调用：guest 内 host_storage.get 往返
            let result = plugin
                .invoke_command("test.echo", r#"{"hello":"component"}"#)
                .expect("invoke_command");
            let result_json: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(result_json["name"], "test.echo");
            assert_eq!(result_json["stored"]["k"], "v");

            // 主库往返：前缀校验通过 + 建表 + 插入 + 查询
            let db_rows = result_json["dbRows"].as_array().expect("dbRows array");
            assert_eq!(db_rows.len(), 1);
            assert_eq!(db_rows[0]["val"], "hello");

            // 插件独立库：无头测试上下文无 app_handle，宿主按设计返回不可用错误
            // （真实运行环境有 app_handle，独立库正常打开）。此处验证 import 接线
            // 与错误透传链路正确，而非 SQL 执行本身（主库往返已覆盖 SQL 语义）。
            let pdb_err = result_json["pdbQueryError"]
                .as_str()
                .expect("pdbQueryError should be present");
            assert!(pdb_err.contains("headless"), "unexpected pdbQueryError: {}", pdb_err);

            // 会话列表（权限 session:read，空列表）
            assert_eq!(result_json["sessions"], serde_json::json!([]));

            // 消息总线发布（同步投递）
            assert_eq!(result_json["busPublished"], serde_json::json!(true));

            // 终端钩子（与 core 形态 plugin-test 同语义：大写转换）
            assert_eq!(
                plugin.on_terminal_input("session-1", "hello input").unwrap(),
                Some("HELLO INPUT".to_string())
            );
            assert_eq!(
                plugin.on_terminal_output("session-1", "hello output").unwrap(),
                Some("HELLO OUTPUT".to_string())
            );

            // 事件回调 + 启动/关闭
            plugin
                .on_message("topic", "sender", &serde_json::json!({"a": 1}))
                .expect("on_message");
            plugin
                .on_session_lifecycle(&serde_json::json!({"type": "created"}))
                .expect("on_session_lifecycle");
            plugin
                .on_input_submitted(&serde_json::json!({"sessionId": "s1"}))
                .expect("on_input_submitted");
            plugin.on_startup().expect("on_startup").expect("plugin on_startup returned Err");
            plugin.on_shutdown().expect("on_shutdown").expect("plugin on_shutdown returned Err");
        });
    }

    /// v14：`events-ws` 可选导出的探测与投递
    ///
    /// - SDK 产物（`wasm_entry!` 无条件导出 `events-ws`）→ 探测命中：
    ///   `on_ws_frame` 投递成功（`Ok(true)`），客户端域与服务端域两条回调都可达；
    /// - 手写绑定产物（`plugin-component-test`，未导出 `events-ws`）→ 探测为
    ///   None：`on_ws_frame` 返回 `Ok(false)`（调用方按 spec §2.2 降级：丢弃 +
    ///   首次 warn + 计数，宿主不缓存），**不影响加载与其余导出**
    #[test]
    fn test_ws_events_export_probe_and_dispatch() {
        use crate::plugin::bus::WsFrameDispatch;

        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let sdk_component = wasm_runtime
            .compile_component(&build_sdk_test_component())
            .expect("compile SDK test component");
        let legacy_component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile component-test");

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut sdk_plugin = wasm_runtime
                .instantiate_component(&sdk_component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate SDK component");
            let client_frame = WsFrameDispatch::Client {
                handle: "wsc-test".to_string(),
                kind: "text".to_string(),
                payload: b"hello ws".to_vec(),
            };
            assert!(
                sdk_plugin.on_ws_frame(&client_frame).expect("deliver client frame"),
                "SDK 产物必须导出 events-ws（wasm_entry! 无条件导出）"
            );
            // 同一接口的第二个函数：服务端域回调同样命中
            let server_frame = WsFrameDispatch::EndpointClient {
                endpoint_id: "wse-test".to_string(),
                client_id: "wsc-peer".to_string(),
                kind: "binary".to_string(),
                payload: vec![0xff, 0x00, 0x7f],
            };
            assert!(
                sdk_plugin.on_ws_frame(&server_frame).expect("deliver server frame"),
                "服务端域回调必须可投递"
            );

            // 旧产物（v13 及更早，未导出 events-ws）：探测 None → 降级 Ok(false)
            let mut legacy_plugin = wasm_runtime
                .instantiate_component(
                    &legacy_component,
                    "com.bedcode.component-test",
                    host_ctx.clone(),
                    &[],
                    None,
                )
                .expect("未导出 events-ws 的产物不得影响加载");
            assert!(
                !legacy_plugin.on_ws_frame(&client_frame).expect("legacy probe"),
                "未导出 events-ws 的产物必须走降级路径（Ok(false)）"
            );
            // 降级不得影响其余导出
            assert!(legacy_plugin.get_manifest().is_ok(), "降级后其余导出照常");
        });
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
            Err(_) => panic!(
                "{label}: 超过 {WS_E2E_TIMEOUT_SECS}s 未完成（疑似死锁；检查投递任务是否占住 actix arbiter）"
            ),
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

    /// host-websocket 客户端域端到端（ABI v14）
    ///
    /// fixture 插件（`packages/plugin-ws-test`）→ 宿主 `connect`（**真握手**）→
    /// 文本 / 二进制回文经 `events-ws` 回灌 → owner 作用域状态事件
    /// （`ws:open` / `ws:close`）经 host-bus 投递 → `close` 后 `is-connected`
    /// 立即为 false（spec D3 时序）。
    ///
    /// mock echo 服务**进程内**（随机端口 + `accept_async`），不引入外部进程，
    /// 测试结束随 runtime 关闭（无残留进程与端口）。
    /// 总线与帧投递共用 `TestInstanceDispatcher`（生产 = PluginHost）。
    ///
    /// 运行时形态与同文件其它用例一致（`Runtime::new()` + `block_on`）：
    /// guest 调用需在 `block_on` 体内执行，`host_impl` 的 `block_on_async`
    /// 桥在这一形态下已验证可用（如 `test_session_list`）。
    #[test]
    fn test_ws_client_outbound_roundtrip() {
        // `setup_wasm_runtime` 内部自建 runtime 并 block_on（建库/建上下文），
        // 必须在 `rt.block_on` **之外**调用：嵌套 block_on 会 panic
        // `Cannot start a runtime from within a runtime`
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let _e2e_guard = lock_ws_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("ws 客户端域 e2e", async {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        const PLUGIN_ID: &str = "com.bedcode.ws-test";
        let open_topic = format!("ws:open.{PLUGIN_ID}");
        let close_topic = format!("ws:close.{PLUGIN_ID}");

        // ==================== mock echo server（进程内） ====================
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind in-process echo server");
        let port = listener.local_addr().expect("local addr").port();
        let echo = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while let Some(Ok(msg)) = ws.next().await {
                match msg {
                    Message::Text(text) => {
                        if ws.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Message::Binary(payload) => {
                        if ws.send(Message::Binary(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => {
                        // 对端 Close 的应答：tungstenite 收到 Close 时已把回帧（echo 收到的 code）
                        // 排入 `additional_send`，用 `SinkExt::close` 驱动 flush 即完成握手。
                        // 注意：不能用 `WebSocketStream::close(Some(..))`——其内部走
                        // `write(Message::Close)`，而在 `ClosedByPeer` 状态下
                        // `WebSocketContext::write` 直接返回 `SendAfterClosing`，回帧不会发出，
                        // 对端只能读到 EOF（wasClean 判定因此失真）。
                        let _ = futures_util::SinkExt::close(&mut ws).await;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // ==================== 加载 fixture 并接线 dispatcher ====================
        // 单测不走 manifest 授权路径：显式授予（storage 由 SDK 默认授予）
        host_ctx
            .permission
            .grant_permissions(PLUGIN_ID, &["storage".to_string(), "ws:client".to_string()]);
        let component = wasm_runtime
            .compile_component(&build_ws_test_component())
            .expect("compile ws fixture component");
        let plugin = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate ws fixture"),
        ));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher {
                instances: Arc::new(RwLock::new(HashMap::from([(
                    PLUGIN_ID.to_string(),
                    plugin.clone(),
                )]))),
            }))
            .await;

        plugin.lock().await.activate().expect("activate = 0");
        // 订阅为异步投递（bus_subscribe 内部 spawn）：等其落地再发 connect
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // ==================== connect（同步阻塞至握手完成） ====================
        let connected = plugin
            .lock()
            .await
            .invoke_command(
                "ws-connect",
                &serde_json::json!({ "url": format!("ws://127.0.0.1:{port}/") }).to_string(),
            )
            .expect("ws-connect");
        let handle = serde_json::from_str::<serde_json::Value>(&connected).expect("connect json")["handle"]
            .as_str()
            .expect("handle")
            .to_string();
        assert!(handle.starts_with("wsc-"), "连接句柄形状应为 wsc-<uuid>，got: {handle}");

        // ws:open（owner 作用域 topic，activate 期已订阅）必须投递且带 handle
        let state = ws_poll_state(&plugin, |s| ws_event_payload(s, &open_topic).is_some(), std::time::Duration::from_secs(3)).await;
        let open_payload = ws_event_payload(&state, &open_topic).unwrap_or_else(|| panic!("ws:open 必须投递，got: {state}"));
        assert_eq!(open_payload["handle"], handle, "ws:open payload 应带连接句柄");
        assert!(
            open_payload["url"].as_str().unwrap_or_default().starts_with("ws://127.0.0.1:"),
            "ws:open payload 应带 url，got: {open_payload}"
        );

        let connected_state = plugin
            .lock()
            .await
            .invoke_command("ws-is-connected", &serde_json::json!({ "handle": handle }).to_string())
            .expect("ws-is-connected");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&connected_state).unwrap()["connected"],
            true,
            "握手完成后 is-connected 必须为 true"
        );

        // ==================== 文本回文（events-ws 回灌） ====================
        plugin
            .lock()
            .await
            .invoke_command(
                "ws-send-text",
                &serde_json::json!({ "handle": handle, "text": "ping-text" }).to_string(),
            )
            .expect("ws-send-text");
        let state = ws_poll_state(
            &plugin,
            |s| ws_has_frame(s, "text", Some("ping-text")),
            std::time::Duration::from_secs(3),
        )
        .await;
        assert!(
            ws_has_frame(&state, "text", Some("ping-text")),
            "文本回文必须经 events-ws 回灌，got: {state}"
        );

        // ==================== 二进制回文（含非 UTF-8 字节） ====================
        let bytes = serde_json::json!([0, 1, 255, 254]);
        plugin
            .lock()
            .await
            .invoke_command(
                "ws-send-binary",
                &serde_json::json!({ "handle": handle, "bytes": bytes }).to_string(),
            )
            .expect("ws-send-binary");
        let state = ws_poll_state(
            &plugin,
            |s| ws_frame_len(s, "binary") == Some(4),
            std::time::Duration::from_secs(3),
        )
        .await;
        assert_eq!(
            ws_frame_len(&state, "binary"),
            Some(4),
            "二进制回文长度一致（非 UTF-8 直通，零 JSON 转义），got: {state}"
        );
        assert!(
            state["frames"]
                .as_array()
                .map(|frames| frames.iter().all(|f| f["target"] == handle))
                .unwrap_or(false),
            "客户端域帧标识即连接句柄，got: {state}"
        );

        // ==================== close → ws:close（对端回 1000 → wasClean=true） ====================
        let closed = plugin
            .lock()
            .await
            .invoke_command(
                "ws-close",
                &serde_json::json!({ "handle": handle, "code": 1000 }).to_string(),
            )
            .expect("ws-close");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&closed).unwrap()["hit"],
            true,
            "close 应命中句柄"
        );
        let state = ws_poll_state(
            &plugin,
            |s| ws_event_payload(s, &close_topic).is_some(),
            std::time::Duration::from_secs(3),
        )
        .await;
        let close_payload = ws_event_payload(&state, &close_topic).unwrap_or_else(|| panic!("ws:close 必须上报，got: {state}"));
        assert_eq!(
            close_payload["wasClean"], true,
            "对端回复 Close(1000) → wasClean=true（spec §4.5 / D11），got: {close_payload}"
        );
        assert_eq!(close_payload["handle"], handle, "ws:close payload 应带连接句柄");

        // 关闭后 is-connected 立即 false（快照自愈路径）
        let after = plugin
            .lock()
            .await
            .invoke_command("ws-is-connected", &serde_json::json!({ "handle": handle }).to_string())
            .expect("ws-is-connected");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&after).unwrap()["connected"],
            false,
            "关闭后 is-connected 必须为 false"
        );

        plugin.lock().await.deactivate().expect("deactivate = 0");
        echo.abort();
        }));
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
    async fn ws_client_recv(client: &mut WsTestClient, timeout: std::time::Duration) -> Option<tokio_tungstenite::tungstenite::Message> {
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

    /// host-websocket 服务端域端到端（ABI v14，票 05）
    ///
    /// 真实宿主 WS 服务器（进程内随机端口）+ 真实 tokio-tungstenite 客户端 +
    /// fixture 插件（`packages/plugin-ws-test`）一次贯通：
    ///
    /// 1. `register-endpoint`（`auth: none`，`maxClients: 1`）→ 通配路由挂载
    ///    `/ws/plugin/<owner>/echo`；
    /// 2. 客户端连入 → `ws:client-connect` 事件（先于首帧）+ `list-clients` 认证态；
    /// 3. 入站文本 / 二进制帧经 `events-ws` 投给插件 → 插件回显原样回客户端
    ///    （宿主零业务语义，回显是插件行为）；
    /// 4. 单播 / 广播 / 踢出（缺省 4004）/ 注销端点（4005）逐条验证，并在
    ///    `ws:client-disconnect` 上核对 code 与 `wasClean`；
    /// 5. 上限与门禁：`maxClients` 超限在升级前 503、注销后握手 404；
    /// 6. 关闭码与「恰好一次」：每次断开都有且仅有一条 disconnect 事件。
    ///
    /// 运行时形态与同文件其它用例一致（`Runtime::new()` + `block_on`）。
    #[test]
    fn test_ws_endpoint_server_domain_roundtrip() {
        // `setup_wasm_runtime` 内部自建 runtime 并 block_on：必须在 `rt.block_on` 之外
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let _e2e_guard = lock_ws_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("ws 服务端域 e2e", async {
            use futures_util::SinkExt;
            use tokio_tungstenite::tungstenite::Message;

            const PLUGIN_ID: &str = "com.bedcode.ws-test";
            let connect_topic = format!("ws:client-connect.{PLUGIN_ID}");
            let disconnect_topic = format!("ws:client-disconnect.{PLUGIN_ID}");

            // ==================== 宿主服务器 + fixture 装载 ====================
            let (server_handle, server_task, port) = {
                let config = crate::system::config::AppConfig::default().network;
                let port = ws_pick_free_port();
                let (handle, server) = crate::server::app::start_http_server(port, &config)
                    .await
                    .expect("start host http+ws server");
                (handle, tokio::spawn(server), port)
            };

            host_ctx
                .permission
                .grant_permissions(PLUGIN_ID, &["storage".to_string(), "ws:server".to_string()]);
            let component = wasm_runtime
                .compile_component(&build_ws_test_component())
                .expect("compile ws fixture component");
            let plugin = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate ws fixture"),
            ));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher {
                    instances: Arc::new(RwLock::new(HashMap::from([(
                        PLUGIN_ID.to_string(),
                        plugin.clone(),
                    )]))),
                }))
                .await;
            plugin.lock().await.activate().expect("activate = 0");
            // 订阅为异步投递（bus_subscribe 内部 spawn）：等其落地再注册端点
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // ==================== 1. 注册端点 + 打开回显 ====================
            let endpoint_id = {
                let mut guard = plugin.lock().await;
                let raw = guard
                    .invoke_command("ws-register-endpoint", r#"{"path":"echo","maxClients":1}"#)
                    .expect("register-endpoint");
                serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                    .as_str()
                    .expect("endpointId")
                    .to_string()
            };
            assert!(endpoint_id.starts_with("wse-"), "端点句柄前缀，got: {endpoint_id}");
            plugin
                .lock()
                .await
                .invoke_command("ws-endpoint-echo", r#"{"enabled":true}"#)
                .expect("echo on");

            // 端点清单：注册即可见（clientCount 0）
            {
                let raw = plugin.lock().await.invoke_command("ws-list-endpoints", "{}").expect("list-endpoints");
                let listed: serde_json::Value = serde_json::from_str(&raw).expect("list json");
                let entries = listed["endpoints"].as_array().expect("endpoints array");
                assert_eq!(entries.len(), 1, "本插件恰好一个端点，got: {listed}");
                assert_eq!(entries[0]["path"], "echo");
                assert_eq!(entries[0]["clientCount"], 0);
            }

            // ==================== 2. 客户端连入 → 接入事件 + 认证态 ====================
            let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/echo");
            let (mut client_a, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("client A connect via host route");

            let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
            assert_eq!(clients.len(), 1, "client A 应出现在 list-clients");
            let client_a_id = clients[0]["clientId"].as_str().expect("clientId").to_string();
            assert_eq!(
                clients[0]["authenticated"], false,
                "auth:none 下注册表认证态保持 false（连接可用 ≠ 已认证）"
            );
            assert!(clients[0]["addr"].as_str().is_some_and(|a| !a.is_empty()));

            let state = ws_poll_state(
                &plugin,
                |s| ws_event_payload(s, &connect_topic).is_some(),
                std::time::Duration::from_secs(5),
            )
            .await;
            let connect = ws_event_payload(&state, &connect_topic).expect("ws:client-connect 事件");
            assert_eq!(connect["endpointId"], endpoint_id);
            assert_eq!(connect["clientId"], client_a_id, "事件标识与 list-clients 同源");

            // ==================== 3. 入站帧 → 插件回显（文本 + 二进制） ====================
            client_a
                .send(Message::Text("hello-endpoint".to_string()))
                .await
                .expect("send text");
            let state = ws_poll_state(
                &plugin,
                |s| ws_has_frame(s, "text", Some("hello-endpoint")),
                std::time::Duration::from_secs(5),
            )
            .await;
            let frame = state["frames"]
                .as_array()
                .and_then(|f| f.iter().find(|f| f["text"] == "hello-endpoint"))
                .cloned()
                .expect("fixture 应收到入站文本帧");
            assert!(
                frame["target"]
                    .as_str()
                    .is_some_and(|t| t.starts_with(&format!("{endpoint_id}/"))),
                "服务端域帧标识应为 endpoint/client，got: {}",
                frame["target"]
            );
            match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
                Some(Message::Text(text)) => assert_eq!(text, "hello-endpoint", "插件回显原样回客户端"),
                other => panic!("期望文本回显，got: {other:?}"),
            }

            let binary_payload: Vec<u8> = vec![0x00, 0xff, 0x7f, 0x41];
            client_a
                .send(Message::Binary(binary_payload.clone()))
                .await
                .expect("send binary");
            let state = ws_poll_state(
                &plugin,
                |s| ws_has_frame(s, "binary", None),
                std::time::Duration::from_secs(5),
            )
            .await;
            assert_eq!(
                ws_frame_len(&state, "binary"),
                Some(binary_payload.len() as u64),
                "非 UTF-8 二进制帧长度必须一致（零 JSON 转义）"
            );
            match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
                Some(Message::Binary(bytes)) => assert_eq!(bytes, binary_payload, "二进制原样回显"),
                other => panic!("期望二进制回显，got: {other:?}"),
            }

            // ==================== 4. 广播 + 上限（升级前 503） ====================
            let sent = {
                let args = serde_json::json!({ "endpointId": endpoint_id, "text": "broadcast" }).to_string();
                let raw = plugin.lock().await.invoke_command("ws-broadcast-text", &args).expect("broadcast");
                serde_json::from_str::<serde_json::Value>(&raw).expect("broadcast json")["sent"]
                    .as_u64()
                    .expect("sent")
            };
            assert_eq!(sent, 1, "广播成功入队数 = 在线客户端数");
            match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
                Some(Message::Text(text)) => assert_eq!(text, "broadcast"),
                other => panic!("期望广播文本，got: {other:?}"),
            }

            // maxClients=1 已满：第二条连接在协议升级前被拒（503，不产生连接事件）
            let rejected = tokio_tungstenite::connect_async(&url).await;
            assert!(rejected.is_err(), "超限连接必须在升级前被拒");

            // ==================== 5. 踢出（缺省 4004）====================
            let hit = {
                let args = serde_json::json!({ "endpointId": endpoint_id, "clientId": client_a_id }).to_string();
                let raw = plugin.lock().await.invoke_command("ws-close-client", &args).expect("close-client");
                serde_json::from_str::<serde_json::Value>(&raw).expect("close json")["hit"]
                    .as_bool()
                    .expect("hit")
            };
            assert!(hit, "踢出应命中在线客户端");
            match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
                Some(Message::Close(Some(frame))) => {
                    assert_eq!(u16::from(frame.code), 4004, "踢出缺省关闭码 4004（spec §4.5）");
                }
                other => panic!("期望 Close(4004)，got: {other:?}"),
            }
            let state = ws_poll_state(
                &plugin,
                |s| ws_event_payload(s, &disconnect_topic).is_some(),
                std::time::Duration::from_secs(5),
            )
            .await;
            let disconnect = ws_event_payload(&state, &disconnect_topic).expect("ws:client-disconnect 事件");
            assert_eq!(disconnect["clientId"], client_a_id);
            assert_eq!(disconnect["code"], 4004, "宿主主动断开须上报关闭码");
            assert_eq!(
                disconnect["wasClean"], false,
                "宿主主动断开恒 wasClean=false（spec §4.5）"
            );
            assert!(
                ws_wait_clients(&plugin, &endpoint_id, 0).await.is_empty(),
                "踢出后句柄已回收"
            );
            // 每次断开恰好一条 disconnect 事件（再做一次投递等待后计数）
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let state = ws_fixture_state(&plugin).await;
            let disconnect_events = state["events"]
                .as_array()
                .map(|events| events.iter().filter(|e| e["topic"] == disconnect_topic.as_str()).count())
                .unwrap_or(0);
            assert_eq!(disconnect_events, 1, "断开事件每连接恰好一次");

            // ==================== 6. 注销端点（4005）→ 握手 404 ====================
            let (mut client_b, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("client B connect after slot freed");
            let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
            assert_eq!(clients.len(), 1, "腾出名额后新连接可接入");

            let unregistered = {
                let args = serde_json::json!({ "endpointId": endpoint_id }).to_string();
                let raw = plugin
                    .lock()
                    .await
                    .invoke_command("ws-unregister-endpoint", &args)
                    .expect("unregister-endpoint");
                serde_json::from_str::<serde_json::Value>(&raw).expect("unregister json")["hit"]
                    .as_bool()
                    .expect("hit")
            };
            assert!(unregistered, "注销命中已注册端点");
            match ws_client_recv(&mut client_b, std::time::Duration::from_secs(5)).await {
                Some(Message::Close(Some(frame))) => {
                    assert_eq!(u16::from(frame.code), 4005, "端点注销关闭码 4005");
                }
                other => panic!("期望 Close(4005)，got: {other:?}"),
            }

            // 端点已摘除 → 清单空 + 新握手 404（未注册端点）
            let raw = plugin.lock().await.invoke_command("ws-list-endpoints", "{}").expect("list-endpoints");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&raw).expect("list json")["endpoints"],
                serde_json::json!([]),
                "注销后本插件无端点"
            );
            assert!(
                tokio_tungstenite::connect_async(&url).await.is_err(),
                "未注册端点的握手必须被拒（404）"
            );

            // ==================== 7. 属主停用路径（4005 + 恰好一次）====================
            // 重挂同一后缀端点 → 新客户端连入 → 模拟宿主停用回收
            // （生产调用点：`PluginHost::deactivate_plugin_inner` → `ws::purge_for_plugin`）
            let endpoint_id = {
                let mut guard = plugin.lock().await;
                let raw = guard
                    .invoke_command("ws-register-endpoint", r#"{"path":"echo"}"#)
                    .expect("re-register endpoint after unregister");
                serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                    .as_str()
                    .expect("endpointId")
                    .to_string()
            };
            plugin
                .lock()
                .await
                .invoke_command("ws-endpoint-echo", r#"{"enabled":true}"#)
                .expect("echo on");
            let (mut client_c, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("client C connect before deactivation");
            let clients = ws_wait_clients(&plugin, &endpoint_id, 1).await;
            assert_eq!(clients.len(), 1, "client C 已登记");
            let client_c_id = clients[0]["clientId"].as_str().expect("clientId").to_string();

            crate::plugin::manager::wasm_runtime::host_impl::ws::purge_for_plugin(PLUGIN_ID);

            match ws_client_recv(&mut client_c, std::time::Duration::from_secs(5)).await {
                Some(Message::Close(Some(frame))) => {
                    assert_eq!(u16::from(frame.code), 4005, "属主停用关闭码 4005");
                }
                other => panic!("期望 Close(4005)，got: {other:?}"),
            }
            assert!(
                crate::server::ws::endpoint::get(&endpoint_id).is_none(),
                "停用回收端点表条目（只碰本人）"
            );
            // 停用路径同样恰好一条 disconnect 事件
            //
            // 同一 topic 上多条事件共存（A 踢出 4004 / B 注销 4005 / C 停用 4005），
            // 轮询与计数都必须按 clientId 收敛：只按 code 计数会被前一条同码事件
            // （B 的注销）提前满足，形成竞态误判
            let disconnect_events_for = |s: &serde_json::Value, client_id: &str| -> Vec<serde_json::Value> {
                s["events"]
                    .as_array()
                    .map(|events| {
                        events
                            .iter()
                            .filter(|e| {
                                e["topic"] == disconnect_topic.as_str() && e["payload"]["clientId"] == client_id
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let state = ws_poll_state(
                &plugin,
                |s| disconnect_events_for(s, &client_c_id).len() == 1,
                std::time::Duration::from_secs(5),
            )
            .await;
            let deactivated_disconnects = disconnect_events_for(&state, &client_c_id);
            assert_eq!(
                deactivated_disconnects.len(),
                1,
                "停用断开事件恰好一次，got: {state}"
            );
            assert_eq!(
                deactivated_disconnects[0]["payload"]["code"], 4005,
                "属主停用关闭码 4005，got: {}",
                deactivated_disconnects[0]
            );
            assert_eq!(
                deactivated_disconnects[0]["payload"]["wasClean"], false,
                "宿主主动断开恒 wasClean=false（spec §4.5）"
            );

            // ==================== 8. auth:"jwt"：未认证帧丢弃 + 认证失败 4001 ====================
            // `none` 路径已在上文贯通；本段补 jwt 策略的失败分支与「未认证连接不产生
            // 接入事件」契约（jwt 成功分支需真实签发 token，属遗留项，见票 05 Comments）
            let secure_endpoint = {
                let mut guard = plugin.lock().await;
                let raw = guard
                    .invoke_command(
                        "ws-register-endpoint",
                        r#"{"path":"secure","auth":"jwt","maxClients":1}"#,
                    )
                    .expect("register jwt endpoint");
                serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                    .as_str()
                    .expect("endpointId")
                    .to_string()
            };
            let secure_url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_ID}/secure");
            let (mut client_d, _) = tokio_tungstenite::connect_async(&secure_url)
                .await
                .expect("jwt endpoint connect");
            let clients = ws_wait_clients(&plugin, &secure_endpoint, 1).await;
            assert_eq!(clients.len(), 1, "jwt 端点连接已登记");
            assert_eq!(clients[0]["authenticated"], false, "未认证期注册表认证态为 false");

            // 未认证期业务帧：丢弃 + warn（不缓存，spec §4.3）→ 插件不得收到
            client_d
                .send(Message::Text("before-auth".to_string()))
                .await
                .expect("send pre-auth frame");
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let state = ws_fixture_state(&plugin).await;
            assert!(
                !ws_has_frame(&state, "text", Some("before-auth")),
                "未认证期业务帧必须丢弃（不缓存），got: {state}"
            );

            // 非法 token → 认证失败 → close 4001
            client_d
                .send(Message::Text(r#"{"type":"auth","token":"not-a-jwt"}"#.to_string()))
                .await
                .expect("send bad auth frame");
            match ws_client_recv(&mut client_d, std::time::Duration::from_secs(5)).await {
                Some(Message::Close(Some(frame))) => {
                    assert_eq!(u16::from(frame.code), 4001, "认证失败关闭码 4001（spec D8）");
                }
                other => panic!("期望 Close(4001)，got: {other:?}"),
            }

            // 认证失败的连接从未「接入」→ 不得产生 client-connect 事件
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let state = ws_fixture_state(&plugin).await;
            let secure_connect_events = state["events"]
                .as_array()
                .map(|events| {
                    events
                        .iter()
                        .filter(|e| {
                            e["topic"] == connect_topic.as_str()
                                && e["payload"]["endpointId"] == secure_endpoint.as_str()
                        })
                        .count()
                })
                .unwrap_or(0);
            assert_eq!(
                secure_connect_events, 0,
                "认证失败连接不得产生 client-connect 事件，got: {state}"
            );

            // ==================== 收尾：优雅停机 + 实例停用 ====================
            server_handle.stop(true).await;
            server_task.abort();
            plugin.lock().await.deactivate().expect("deactivate = 0");
            // 全局端点表在本进程内跨用例共享：显式清理（deactivate 不触达宿主侧回收）
            crate::server::ws::endpoint::purge_for_plugin(PLUGIN_ID);
        }));
    }

    /// 票据 06 双 fixture 隔离 demo：A 挂入站端点、B 连外部服务
    ///
    /// 一次贯通两组隔离断言：
    ///
    /// - **零可见**：B 的 `list-endpoints` / `list-clients` 看不到 A 的端点；
    ///   A 对 B 的出站句柄调用被拒（跨插件属主仲裁）；
    /// - **零影响**：A 停用回收（`purge_for_plugin(A)`）后 B 的外部连接仍然在线，
    ///   而 A 的入站对端收到 4005 下线关闭帧。
    #[test]
    fn test_ws_two_plugin_isolation() {
        // `setup_wasm_runtime` 内部自建 runtime 并 block_on（建库/建上下文），
        // 必须在 `rt.block_on` **之外**调用：嵌套 block_on 会 panic
        // `Cannot start a runtime from within a runtime`
        let (runtime_a, ctx_a) = setup_wasm_runtime();
        let (runtime_b, ctx_b) = setup_wasm_runtime();
        let _e2e_guard = lock_ws_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("ws 双 fixture 隔离 e2e", async {
            use futures_util::StreamExt;
            use tokio_tungstenite::tungstenite::Message;

            const PLUGIN_A: &str = "com.bedcode.ws-test";
            // 第二个实例用独立 id：属主域完全隔离（端点命名空间 / 句柄 / 事件 topic）
            const PLUGIN_B: &str = "com.bedcode.ws-test.peer";

            // ==================== B 的外部对端（进程内 mock echo） ====================
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind mock peer");
            let peer_port = listener.local_addr().expect("peer addr").port();
            let peer = tokio::spawn(async move {
                let Ok((stream, _)) = listener.accept().await else { return };
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else { return };
                while let Some(Ok(msg)) = ws.next().await {
                    if matches!(msg, Message::Close(_)) {
                        break;
                    }
                }
            });

            // ==================== 两个独立宿主上下文（各自的 bus 与实例表） ====================
            // A 两个域都授权：跨插件负向断言必须先过权限门，才落到属主仲裁
            ctx_a.permission.grant_permissions(
                PLUGIN_A,
                &["storage".to_string(), "ws:server".to_string(), "ws:client".to_string()],
            );
            let component = runtime_a
                .compile_component(&build_ws_test_component())
                .expect("compile ws fixture component");
            let plugin_a = Arc::new(Mutex::new(
                runtime_a
                    .instantiate_component(&component, PLUGIN_A, ctx_a.clone(), &[], None)
                    .expect("instantiate plugin A"),
            ));
            ctx_a
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher {
                    instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_A.to_string(), plugin_a.clone())]))),
                }))
                .await;
            plugin_a.lock().await.activate().expect("activate A");

            // B 同样两个域都授权：跨插件负向断言必须落在**属主仲裁**而非权限门，
            // 否则 B 对 A 端点的调用会被 `permission denied: ws:server` 短路，
            // 证明不了属主隔离（B 实际只需出站能力，此处为断言口径而授权）
            ctx_b.permission.grant_permissions(
                PLUGIN_B,
                &["storage".to_string(), "ws:client".to_string(), "ws:server".to_string()],
            );
            // B 必须用 runtime_b 自己编译的组件：wasmtime 不支持跨 `Engine` 实例化
            let component_b = runtime_b
                .compile_component(&build_ws_test_component())
                .expect("compile ws fixture component for B");
            let plugin_b = Arc::new(Mutex::new(
                runtime_b
                    .instantiate_component(&component_b, PLUGIN_B, ctx_b.clone(), &[], None)
                    .expect("instantiate plugin B"),
            ));
            ctx_b
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher {
                    instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_B.to_string(), plugin_b.clone())]))),
                }))
                .await;
            plugin_b.lock().await.activate().expect("activate B");

            // ==================== A 挂端点 + B 连外部服务（互不相干） ====================
            let (server_handle, server_task, port) = {
                let config = crate::system::config::AppConfig::default().network;
                let port = ws_pick_free_port();
                let (handle, server) = crate::server::app::start_http_server(port, &config)
                    .await
                    .expect("start host http+ws server");
                (handle, tokio::spawn(server), port)
            };

            let endpoint_a = {
                let mut guard = plugin_a.lock().await;
                let raw = guard
                    .invoke_command("ws-register-endpoint", r#"{"path":"iso"}"#)
                    .expect("register endpoint on A");
                serde_json::from_str::<serde_json::Value>(&raw).expect("register json")["endpointId"]
                    .as_str()
                    .expect("endpointId")
                    .to_string()
            };
            let handle_b = {
                let args = serde_json::json!({ "url": format!("ws://127.0.0.1:{peer_port}/") }).to_string();
                let raw = plugin_b
                    .lock()
                    .await
                    .invoke_command("ws-connect", &args)
                    .expect("B connect outbound");
                serde_json::from_str::<serde_json::Value>(&raw).expect("connect json")["handle"]
                    .as_str()
                    .expect("handle")
                    .to_string()
            };

            let url = format!("ws://127.0.0.1:{port}/ws/plugin/{PLUGIN_A}/iso");
            let (mut client_a, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("inbound client connects to A endpoint");
            let clients = ws_wait_clients(&plugin_a, &endpoint_a, 1).await;
            assert_eq!(clients.len(), 1, "A 的端点客户端已登记");

            // ==================== 零可见 ====================
            assert_eq!(
                crate::plugin::manager::wasm_runtime::host_impl::ws::ws_list_endpoints(&ctx_b, PLUGIN_B).unwrap(),
                "[]",
                "B 看不到 A 的端点"
            );
            assert_eq!(
                crate::plugin::manager::wasm_runtime::host_impl::ws::ws_list_clients(&ctx_b, PLUGIN_B, &endpoint_a)
                    .unwrap_err(),
                "not owner of ws endpoint",
                "B 不得查询 A 的端点客户端"
            );
            assert_eq!(
                crate::plugin::manager::wasm_runtime::host_impl::ws::ws_is_connected(&ctx_a, PLUGIN_A, &handle_b)
                    .unwrap_err(),
                "not owner of ws handle",
                "A 不得操作 B 的出站句柄"
            );

            // ==================== A 停用回收：零影响 B；A 的对端收到 4005 ====================
            crate::plugin::manager::wasm_runtime::host_impl::ws::purge_for_plugin(PLUGIN_A);
            match ws_client_recv(&mut client_a, std::time::Duration::from_secs(5)).await {
                Some(Message::Close(Some(frame))) => {
                    assert_eq!(u16::from(frame.code), 4005, "属主停用关闭码 4005");
                }
                other => panic!("期望 Close(4005)，got: {other:?}"),
            }
            assert!(
                crate::server::ws::endpoint::get(&endpoint_a).is_none(),
                "A 的端点随停用回收"
            );
            assert!(
                crate::plugin::manager::wasm_runtime::host_impl::ws::ws_is_connected(&ctx_b, PLUGIN_B, &handle_b)
                    .expect("B 句柄仍可查询"),
                "A 停用不得影响 B 的外部连接"
            );
            // B 的对端仍在线：可继续发送（fail-visible 之外的正向断言）
            assert!(
                crate::plugin::manager::wasm_runtime::host_impl::ws::ws_send_text(&ctx_b, PLUGIN_B, &handle_b, "still-alive")
                    .is_ok(),
                "A 停用后 B 仍可发送"
            );

            // ==================== 收尾 ====================
            server_handle.stop(true).await;
            server_task.abort();
            plugin_a.lock().await.deactivate().expect("deactivate A");
            plugin_b.lock().await.deactivate().expect("deactivate B");
            crate::plugin::manager::wasm_runtime::host_impl::ws::purge_for_plugin(PLUGIN_A);
            crate::plugin::manager::wasm_runtime::host_impl::ws::purge_for_plugin(PLUGIN_B);
            crate::server::ws::endpoint::purge_for_plugin(PLUGIN_A);
            crate::server::ws::endpoint::purge_for_plugin(PLUGIN_B);
            peer.abort();
        }));
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
            .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
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
            .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
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
    async fn pty_fetch_until(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        pty_id: &str,
        want: &str,
    ) -> serde_json::Value {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let fetched = pty_fixture_fetch(plugin, pty_id, 0).await;
            if pty_fetched_text(&fetched).contains(want) || std::time::Instant::now() >= deadline {
                return fetched;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// 端到端（最高 seam）：WIT 契约 → 宿主实现 → 组件接线 → 权限两域 → SDK → 真 PTY 输出
    ///
    /// 覆盖票 02 主干：spawn 真实命令拿句柄 → `ring-fetch` 拉到输出字节 → 二次按
    /// `next-offset` 续拉不重复 → spawn 不发布任何事件 → 跨插件属主隔离 →
    /// 未授权 `pty:spawn` 被宿主拒绝（Rust 端最终仲裁）。
    #[test]
    fn test_pty_spawn_ring_fetch_roundtrip() {
        // `setup_wasm_runtime` 内部自建 runtime 并 block_on，必须在 `rt.block_on`
        // 之外调用（嵌套 block_on 会 panic "Cannot start a runtime from within a runtime"）
        let (runtime_a, ctx_a) = setup_wasm_runtime();
        let (runtime_b, ctx_b) = setup_wasm_runtime();
        let _e2e_guard = lock_pty_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("host-pty spawn→ring-fetch e2e", async {
            const PLUGIN_A: &str = "com.bedcode.pty-test";
            // 同一产物以第二个属主 id 实例化：属主域完全隔离
            const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

            ctx_a
                .permission
                .grant_permissions(PLUGIN_A, &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()]);
            // B 只授数据域：跨插件负向断言必须先过权限门才落到属主仲裁；
            // 同时它没有 pty:spawn，正好端到端验证两域独立
            ctx_b
                .permission
                .grant_permissions(PLUGIN_B, &["storage".to_string(), "pty:io".to_string()]);

            let component_a = runtime_a
                .compile_component(&build_pty_test_component())
                .expect("compile pty fixture component for A");
            let plugin_a = Arc::new(Mutex::new(
                runtime_a
                    .instantiate_component(&component_a, PLUGIN_A, ctx_a.clone(), &[], None)
                    .expect("instantiate pty fixture A"),
            ));
            ctx_a
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher {
                    instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_A.to_string(), plugin_a.clone())]))),
                }))
                .await;
            plugin_a.lock().await.activate().expect("activate A = 订阅 pty:exit.<A>");

            // wasmtime 不支持跨 Engine 实例化，B 用自己 runtime 编译的组件
            let component_b = runtime_b
                .compile_component(&build_pty_test_component())
                .expect("compile pty fixture component for B");
            let plugin_b = Arc::new(Mutex::new(
                runtime_b
                    .instantiate_component(&component_b, PLUGIN_B, ctx_b.clone(), &[], None)
                    .expect("instantiate pty fixture B"),
            ));
            ctx_b
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher {
                    instances: Arc::new(RwLock::new(HashMap::from([(PLUGIN_B.to_string(), plugin_b.clone())]))),
                }))
                .await;
            plugin_b.lock().await.activate().expect("activate B");

            // ==================== A：spawn 真实命令 → 句柄 ====================
            let marker = format!(
                "BEDCODE_PTY_E2E_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let spawned = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        // 载体常驻（`echo` 后阻塞在 `read`）：票 04 起进程一退出即摘环，
                        // 短命命令会让后续断言撞上「句柄已不存在」而不是它要测的行为
                        &serde_json::json!({ "command": "/bin/sh", "args": ["-c", format!("echo {marker}; read go")] })
                            .to_string(),
                    )
                    .expect("pty-spawn")
            };
            let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            assert!(pty_id.starts_with("pty-"), "句柄形状应为 pty-<uuid>，got: {pty_id}");

            // ==================== ring-fetch：输出字节流到插件侧 ====================
            let first = pty_fetch_until(&plugin_a, &pty_id, &marker).await;
            let text = pty_fetched_text(&first);
            assert!(text.contains(&marker), "真 PTY 输出必须经环流到插件侧，got: {text}");
            assert_eq!(first["truncated"], false, "首次全量拉取不存在缺口: {first}");
            let next_offset = first["nextOffset"].as_u64().unwrap_or(0);
            assert_eq!(
                next_offset,
                first["data"].as_array().map(|a| a.len()).unwrap_or(0) as u64,
                "nextOffset 应等于本次返回区间末端: {first}"
            );

            // 续拉不重复（游标追平 → none）
            let second = pty_fixture_fetch(&plugin_a, &pty_id, next_offset).await;
            assert_eq!(second["none"], true, "游标追平时不得重复投递已消费字节: {second}");

            // spawn 不发任何事件（成功面无事件；退出事件须先授权/订阅时序契约，见票 04）
            let state_a = {
                let mut guard = plugin_a.lock().await;
                guard.invoke_command("pty-state", "{}").expect("pty-state")
            };
            let events_a: serde_json::Value = serde_json::from_str(&state_a).expect("pty-state json");
            assert_eq!(
                events_a["events"].as_array().map(|a| a.len()).unwrap_or(0),
                0,
                "spawn 不得发布任何总线事件: {state_a}"
            );

            // ==================== 属主隔离（WIT 端到端） ====================
            let denied_by_owner = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command(
                        "pty-ring-fetch",
                        &serde_json::json!({ "ptyId": pty_id, "fromOffset": 0, "maxBytes": 4096 }).to_string(),
                    )
                    .expect("跨插件调用以 error 载荷回传")
            };
            assert!(
                pty_command_error(&denied_by_owner).contains("not owner of pty handle"),
                "B 拉 A 的句柄必须被属主仲裁拒绝，got: {denied_by_owner}"
            );

            // ==================== 权限两域独立（Rust 端最终仲裁） ====================
            let denied_by_permission = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command("pty-spawn", &serde_json::json!({ "command": "/bin/true" }).to_string())
                    .expect("未授权调用以 error 载荷回传")
            };
            assert!(
                pty_command_error(&denied_by_permission).contains("permission denied: pty:spawn"),
                "未声明 pty:spawn 的插件不得创建 PTY，got: {denied_by_permission}"
            );

            // 收尾：杀掉常驻载体，避免测试进程退出前挂着无用子进程（AGENTS §3）
            {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                    .expect("pty-kill 收尾");
            }
        }));
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

    /// 端到端（票 03 数据面）：spawn 交互进程 → write 输入 → ring-fetch 拉回**进程响应**
    /// → resize 生效 → is-running 快照，全程跨 WIT / SDK 边界
    #[test]
    fn test_pty_interactive_io_loop_roundtrip() {
        let (runtime, ctx) = setup_wasm_runtime();
        let _e2e_guard = lock_pty_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("host-pty 数据面 e2e", async {
            const PLUGIN_ID: &str = "com.bedcode.pty-test";
            ctx.permission.grant_permissions(
                PLUGIN_ID,
                &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
            );
            let plugin = pty_activate_fixture(&runtime, &ctx, PLUGIN_ID).await;

            // sed 由进程自己加前缀：拉到的 `OUT:` 只能来自进程，而非 tty 本地回显
            let marker = format!(
                "BEDCODE_PTY_IO_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let spawned = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({ "command": "/bin/sed", "args": ["s/^/OUT:/"] }).to_string(),
                    )
                    .expect("pty-spawn")
            };
            let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            // ==================== write：输入进进程，响应回环到环 ====================
            let input = format!("in-{marker}\n");
            let written = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command(
                        "pty-write",
                        &serde_json::json!({
                            "ptyId": pty_id,
                            "bytes": input.as_bytes().to_vec(),
                        })
                        .to_string(),
                    )
                    .expect("pty-write")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&written).expect("write json")["len"],
                input.len() as u64,
                "pty-write 应回报实际写入字节数"
            );
            let response = pty_fetch_until(&plugin, &pty_id, &format!("OUT:in-{marker}")).await;
            assert!(
                pty_fetched_text(&response).contains(&format!("OUT:in-{marker}")),
                "写进 PTY 的输入必须被进程消费并以其输出回环: {response}"
            );

            // ==================== resize：无错即生效（时序不承诺） ====================
            let resized = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command("pty-resize", &serde_json::json!({ "ptyId": pty_id, "cols": 90, "rows": 25 }).to_string())
                    .expect("pty-resize")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&resized).expect("resize json")["ok"],
                true,
                "resize 成功必须回 ok"
            );

            // ==================== is-running：进程存活期快照 ====================
            let state = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command("pty-is-running", &serde_json::json!({ "ptyId": pty_id }).to_string())
                    .expect("pty-is-running")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&state).expect("running json")["running"],
                true,
                "sed 仍在等待下一行输入时应为 running: {state}"
            );

            // 收尾：杀掉常驻 sed（其退出事件同时是票 04 的一次真实投递，不断言只清场）
            {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                    .expect("pty-kill 收尾");
            }
        }));
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
        assert!(
            value.get("error").is_none(),
            "{command} 期望成功载荷，got: {value}"
        );
        value
    }

    /// 读 fixture 已收事件列表（`pty:exit.<owner>` 投递事实源）
    async fn pty_fixture_events(plugin: &Arc<Mutex<LoadedWasmPlugin>>) -> Vec<serde_json::Value> {
        let state = pty_fixture_call(plugin, "pty-state", serde_json::json!({})).await;
        state["events"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// 轮询 fixture 事件直至出现指定 ptyId 的退出事件（宿主→guest 投递异步）
    async fn pty_wait_exit_event(
        plugin: &Arc<Mutex<LoadedWasmPlugin>>,
        pty_id: &str,
    ) -> serde_json::Value {
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

    /// 端到端（票 04 生命面）：kill / 自然退出 / 停用回收三条路径的退出事件与摘除
    ///
    /// 覆盖 WIT → 宿主 → SDK → guest 事件回调的完整一圈：`pty-kill` 后属主经
    /// `on_message` 收到 `pty:exit.<owner>`（reason=killed）且句柄不可再寻址；进程
    /// 自然退出带出真实退出码；`purge_for_plugin`（deactivate 路径调用的同一函数）
    /// 只回收本人，它插件的 PTY 与其事件流不受影响。
    #[test]
    fn test_pty_exit_event_and_purge_roundtrip() {
        let (runtime_a, ctx_a) = setup_wasm_runtime();
        let (runtime_b, ctx_b) = setup_wasm_runtime();
        let _e2e_guard = lock_pty_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("host-pty 生命面 e2e", async {
            const PLUGIN_A: &str = "com.bedcode.pty-test";
            const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

            // 接线锁：停用路径必须调用插件 PTY 回收（本夹具没有 PluginHost，行为侧由
            // 下面的 purge 断言兜住，调用点存在性在此锁死——AGENTS §7 停用回收契约）
            let host_src = include_str!("host.rs");
            assert!(
                host_src.contains("pty::purge_for_plugin(plugin_id, &self.message_bus)"),
                "deactivate_plugin_inner 未接线 host-pty 停用回收"
            );

            for (ctx, id) in [(&ctx_a, PLUGIN_A), (&ctx_b, PLUGIN_B)] {
                ctx.permission.grant_permissions(
                    id,
                    &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
                );
            }
            let plugin_a = pty_activate_fixture(&runtime_a, &ctx_a, PLUGIN_A).await;
            let plugin_b = pty_activate_fixture(&runtime_b, &ctx_b, PLUGIN_B).await;

            // ==================== kill：终止 + 摘除 + killed 事件 ====================
            let killed = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                    )
                    .expect("pty-spawn")
            };
            let killed_id = serde_json::from_str::<serde_json::Value>(&killed).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let kill_result = pty_fixture_call(&plugin_a, "pty-kill", serde_json::json!({ "ptyId": killed_id })).await;
            assert_eq!(kill_result["ok"], true, "kill 必须回报成功");

            let event = pty_wait_exit_event(&plugin_a, &killed_id).await;
            assert_eq!(event["topic"], format!("pty:exit.{PLUGIN_A}"), "topic 内嵌属主");
            assert_eq!(event["sender"], "host", "事件由宿主发布");
            assert_eq!(event["payload"]["reason"], "killed", "kill 路径 reason 固定");

            // 摘除即不可寻址（句柄与环一并释放）
            let after_kill = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command(
                        "pty-is-running",
                        &serde_json::json!({ "ptyId": killed_id }).to_string(),
                    )
                    .expect("以 error 载荷回传")
            };
            assert!(
                pty_command_error(&after_kill).contains("not found"),
                "kill 摘除后句柄必须不可寻址，got: {after_kill}"
            );

            // ==================== 自然退出：stopped + 真实退出码 ====================
            let exited = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "exit 3"] }).to_string(),
                    )
                    .expect("pty-spawn")
            };
            let exited_id = serde_json::from_str::<serde_json::Value>(&exited).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let event = pty_wait_exit_event(&plugin_a, &exited_id).await;
            assert_eq!(event["payload"]["reason"], "stopped", "自然退出不得报 killed");
            assert_eq!(
                event["payload"]["exitCode"], 3,
                "退出码必须经 WIT/bus 原样送达插件: {event}"
            );

            // ==================== 停用回收：只碰本人 ====================
            let mine = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                    )
                    .expect("pty-spawn")
            };
            let mine_id = serde_json::from_str::<serde_json::Value>(&mine).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let theirs = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }).to_string(),
                    )
                    .expect("pty-spawn")
            };
            let theirs_id = serde_json::from_str::<serde_json::Value>(&theirs).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            // A 停用：宿主回收其全部在册 PTY（此刻 A 有 1 条活着的那条 + 已终态的两条已摘除）
            // 走 spawn_blocking：`purge_for_plugin` 内含 `block_on_async`（kill 要 await
            // 引擎），在 `rt.block_on` 的驱动线程上直接调用会撞 block_in_place 约束——
            // 无 handle 的阻塞线程才是它的真实调用形态（同生产 deactivate 路径）
            let purged = {
                let bus = Arc::clone(&ctx_a.message_bus);
                tokio::task::spawn_blocking(move || {
                    crate::plugin::manager::wasm_runtime::host_impl::pty::purge_for_plugin(PLUGIN_A, &bus)
                })
                .await
                .expect("purge 任务不得 panic")
            };
            assert_eq!(purged, 1, "回收数应为 A 当前在册的 PTY 数（已终态者早被摘除）");

            let event = pty_wait_exit_event(&plugin_a, &mine_id).await;
            assert_eq!(
                event["payload"]["reason"], "killed",
                "停用回收对插件表现为一次被宿主终止: {event}"
            );
            assert_eq!(
                pty_fixture_events(&plugin_a)
                    .await
                    .iter()
                    .filter(|e| e["payload"]["ptyId"] == serde_json::Value::String(mine_id.clone()))
                    .count(),
                1,
                "恰好一次：停用回收与退出监听不得各发一条"
            );

            // B 完全不受影响：句柄可查、事件流里没有 A 的任何一条
            let peer_state =
                pty_fixture_call(&plugin_b, "pty-is-running", serde_json::json!({ "ptyId": theirs_id })).await;
            assert_eq!(peer_state["running"], true, "它插件的 PTY 不得被连带终止");
            let peer_events = pty_fixture_events(&plugin_b).await;
            assert!(
                peer_events.is_empty(),
                "非属主物理上收不到他人的退出事件: {peer_events:?}"
            );

            // 收尾：清场 B 的常驻进程（同一回收函数对 B 亦只碰本人）
            let purged_peer = {
                let bus = Arc::clone(&ctx_b.message_bus);
                tokio::task::spawn_blocking(move || {
                    crate::plugin::manager::wasm_runtime::host_impl::pty::purge_for_plugin(PLUGIN_B, &bus)
                })
                .await
                .expect("peer purge 任务不得 panic")
            };
            assert_eq!(purged_peer, 1, "B 的回收同样只清自己那一条");
        }));
    }

    /// 端到端（票 05 限额与背压）：插件声明的 `ringBytes` 经 WIT 生效，落后游标得到
    /// `truncated` 并可按 `next-offset` 续拉；超上限的声明被宿主拒绝（不静默降级）
    #[test]
    fn test_pty_declared_ring_backpressure_roundtrip() {
        let (runtime, ctx) = setup_wasm_runtime();
        let _e2e_guard = lock_pty_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("host-pty 背压 e2e", async {
            const PLUGIN_ID: &str = "com.bedcode.pty-test";
            ctx.permission.grant_permissions(
                PLUGIN_ID,
                &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
            );
            let plugin = pty_activate_fixture(&runtime, &ctx, PLUGIN_ID).await;

            // ==================== 声明超上限：宿主拒绝且不静默夹取 ====================
            let oversized = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({
                            "command": "/bin/sh",
                            "args": ["-c", "read go"],
                            // 宿主上限 4 MiB（PLUGIN_PTY_RING_MAX_BYTES）
                            "ringBytes": 4 * 1024 * 1024 + 1,
                        })
                        .to_string(),
                    )
                    .expect("以 error 载荷回传")
            };
            let err = pty_command_error(&oversized);
            assert!(err.contains("ringBytes") && err.contains("too large"), "声明超上限必须可见: {err}");

            // ==================== 小环 + 持续产出：落后游标 truncated + 可续拉 ====================
            let spawned = {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command(
                        "pty-spawn",
                        &serde_json::json!({
                            "command": "/bin/sh",
                            // 关掉 tty 回显：环内内容即进程产出；每 ~1ms 一行
                            "args": ["-c", "stty -echo; while true; do echo line; sleep 0.001; done"],
                            "ringBytes": 512,
                        })
                        .to_string(),
                    )
                    .expect("pty-spawn")
            };
            let pty_id = serde_json::from_str::<serde_json::Value>(&spawned).expect("spawn json")["ptyId"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            // 消费者（本用例）先不拉取，让产出远超 512 字节
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let stale = pty_fixture_fetch(&plugin, &pty_id, 0).await;
            assert_eq!(stale["truncated"], true, "游标 0 必已落后于驻留起点: {stale}");
            let first_end = stale["nextOffset"].as_u64().unwrap_or(0);
            assert!(first_end > 512, "产出必须已远超声明容量（源侧未被拖住）: {stale}");
            assert!(
                first_end - pty_fetched_text(&stale).len() as u64 > 0,
                "返回段必须从产出中段起（此前字节已淘汰）: {stale}"
            );
            assert!(pty_fetched_text(&stale).contains("line"), "返回的必须是真实输出: {stale}");

            // 续拉不重复、不再报缺口
            let resume = pty_fixture_fetch(&plugin, &pty_id, first_end).await;
            if resume.get("none").is_none() {
                assert_eq!(
                    resume["truncated"], false,
                    "从 nextOffset 起续拉不得再报缺口: {resume}"
                );
                assert!(
                    resume["nextOffset"].as_u64().unwrap_or(0) > first_end,
                    "游标必须前进: {resume}"
                );
            }

            // 收尾：杀掉流式载体
            {
                let mut guard = plugin.lock().await;
                guard
                    .invoke_command("pty-kill", &serde_json::json!({ "ptyId": pty_id }).to_string())
                    .expect("pty-kill 收尾");
            }
        }));
    }

    /// 端到端契约矩阵（票 06，最高 seam 固化为回归基线）
    ///
    /// 一条用例冻结五类断言，**每个分格独立可定位**（不打包成大 assert）：
    /// ① 正路径闭环 spawn → ring-fetch → write → resize → is-running → kill → exit 事件；
    /// ② 属主隔离矩阵——他人句柄上**每一个**带句柄入参的函数都验一次 `not owner`
    ///    （漏一个函数即契约破口），并验拒绝零副作用；
    /// ③ 事件定向——非属主的事件流里不得出现他人事件；
    /// ④ ADR 0017——`api: []` 的插件被互调时宿主门禁拒绝；
    /// ⑤ 权限两域独立（未声明 `pty:spawn` 的插件不得创建）。
    /// 权限「完全未授权」分格与配额/回收分格分持在宿主层单测（`every_api_without_any_permission_is_denied_before_any_lookup`
    /// 与 `pty_quota_*` / `purge_for_plugin_*`），此处不重复造轮子。
    #[test]
    fn test_pty_isolation_and_contract_matrix_roundtrip() {
        let (runtime_a, ctx_a) = setup_wasm_runtime();
        let (runtime_b, ctx_b) = setup_wasm_runtime();
        let _e2e_guard = lock_pty_fixture_e2e();
        let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
        rt.block_on(ws_e2e_guard("host-pty 契约矩阵 e2e", async {
            const PLUGIN_A: &str = "com.bedcode.pty-test";
            const PLUGIN_B: &str = "com.bedcode.pty-test.peer";

            // A：双域；B：只授数据域——既做「越权创建」的负向载体，也让属主负向断言
            // 必须先过权限门（不致于把权限拒绝误当成属主拒绝）
            ctx_a.permission.grant_permissions(
                PLUGIN_A,
                &["storage".to_string(), "pty:spawn".to_string(), "pty:io".to_string()],
            );
            ctx_b.permission.grant_permissions(
                PLUGIN_B,
                &["storage".to_string(), "pty:io".to_string()],
            );
            let plugin_a = pty_activate_fixture(&runtime_a, &ctx_a, PLUGIN_A).await;
            let plugin_b = pty_activate_fixture(&runtime_b, &ctx_b, PLUGIN_B).await;

            // ==================== ① 正路径闭环 ====================
            // `-u` 让 sed 行缓冲（非 tty 输出下默认块缓冲会把响应压到最后一次吐出）；
            // `OUT:` 前缀只能由进程加上，故拉到的前缀即「输入真的进了进程」的证据
            let spawned = pty_fixture_call(
                &plugin_a,
                "pty-spawn",
                serde_json::json!({ "command": "/bin/sed", "args": ["-u", "s/^/OUT:/"], "cols": 100, "rows": 30 }),
            )
            .await;
            let pty_id = spawned["ptyId"].as_str().unwrap_or_default().to_string();
            assert!(pty_id.starts_with("pty-"), "①-a 句柄形状: {spawned}");

            let tag = format!(
                "MATRIX_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let written = pty_fixture_call(
                &plugin_a,
                "pty-write",
                serde_json::json!({ "ptyId": pty_id, "bytes": format!("in-{tag}\n").into_bytes() }),
            )
            .await;
            assert_eq!(
                written["len"],
                format!("in-{tag}\n").len() as u64,
                "①-b write 必须回报实际写入字节数"
            );
            let response = pty_fetch_until(&plugin_a, &pty_id, &format!("OUT:in-{tag}")).await;
            assert!(
                pty_fetched_text(&response).contains(&format!("OUT:in-{tag}")),
                "①-c ring-fetch 必须拉到进程响应: {response}"
            );

            let resized = pty_fixture_call(
                &plugin_a,
                "pty-resize",
                serde_json::json!({ "ptyId": pty_id, "cols": 120, "rows": 40 }),
            )
            .await;
            assert_eq!(resized["ok"], true, "①-d resize 生效必须回 ok: {resized}");

            let alive =
                pty_fixture_call(&plugin_a, "pty-is-running", serde_json::json!({ "ptyId": pty_id })).await;
            assert_eq!(alive["running"], true, "①-e 存活快照必须为 true: {alive}");

            let killed = pty_fixture_call(&plugin_a, "pty-kill", serde_json::json!({ "ptyId": pty_id })).await;
            assert_eq!(killed["ok"], true, "①-f kill 必须回 ok: {killed}");
            let event = pty_wait_exit_event(&plugin_a, &pty_id).await;
            assert_eq!(
                event["topic"],
                format!("pty:exit.{PLUGIN_A}"),
                "①-g 退出事件必须落在属主作用域 topic: {event}"
            );
            assert_eq!(event["payload"]["reason"], "killed", "①-h kill 路径 reason 固定: {event}");
            let after_kill = {
                let mut guard = plugin_a.lock().await;
                guard
                    .invoke_command("pty-is-running", &serde_json::json!({ "ptyId": pty_id }).to_string())
                    .expect("以 error 载荷回传")
            };
            assert!(
                pty_command_error(&after_kill).contains("not found"),
                "①-i 退出即摘除：句柄必须不再可寻址: {after_kill}"
            );

            // ==================== ② 属主隔离矩阵（每一个句柄型函数） ====================
            let foreign = pty_fixture_call(
                &plugin_a,
                "pty-spawn",
                serde_json::json!({ "command": "/bin/sh", "args": ["-c", "read go"] }),
            )
            .await;
            let foreign_id = foreign["ptyId"].as_str().unwrap_or_default().to_string();
            assert!(!foreign_id.is_empty(), "②-a 属主 spawn 应先成功");

            // 第 5 个句柄型函数 `kill` 属创建域：B 无 `pty:spawn`，其属主分格由宿主层
            // `kill_still_enforces_owner_before_its_own_gate` 锁，⑤ 在此锁它的权限门优先级
            let matrix: [(&str, serde_json::Value); 4] = [
                ("pty-write", serde_json::json!({ "ptyId": foreign_id, "bytes": b"ls\n".to_vec() })),
                ("pty-resize", serde_json::json!({ "ptyId": foreign_id, "cols": 80, "rows": 24 })),
                ("pty-ring-fetch", serde_json::json!({ "ptyId": foreign_id, "fromOffset": 0, "maxBytes": 4096 })),
                ("pty-is-running", serde_json::json!({ "ptyId": foreign_id })),
            ];
            for (command, args) in &matrix {
                let raw = {
                    let mut guard = plugin_b.lock().await;
                    guard
                        .invoke_command(command, &args.to_string())
                        .unwrap_or_else(|e| panic!("②-b {command} 调用应失败而非 trap: {e}"))
                };
                // 失败信息必须自带分格名（命令），成功载荷同样视为破口
                let err = pty_error_of(&raw)
                    .unwrap_or_else(|| panic!("②-b {command} 必须被属主仲裁拒绝，got 成功载荷: {raw}"));
                assert!(
                    err.contains("not owner of pty handle"),
                    "②-b {command} 拒绝文案应为 not owner（而非权限/查表），got: {err}"
                );
            }
            // 拒绝必须零副作用：属主的句柄照旧可用、内容照旧可拉
            let still_ours =
                pty_fixture_call(&plugin_a, "pty-is-running", serde_json::json!({ "ptyId": foreign_id })).await;
            assert_eq!(still_ours["running"], true, "②-c 越权拒绝不得影响属主句柄: {still_ours}");

            // ==================== ③ 事件定向（B 的物理订阅窗口里没有 A 的事件） ====================
            let peer_events = pty_fixture_events(&plugin_b).await;
            assert!(
                peer_events.iter().all(|e| e["payload"]["ptyId"] != serde_json::Value::String(foreign_id.clone())),
                "③ 非属主事件流里不得出现他人事件: {peer_events:?}"
            );
            assert!(peer_events.is_empty(), "③ B 全程未拥有 PTY，事件流必须为空: {peer_events:?}");

            // ==================== ④ ADR 0017：未声明 api 的互调被宿主门禁拒绝 ====================
            let call_undeclared = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command("pty-call-undeclared-api", &serde_json::json!({ "api": "pty-spawn" }).to_string())
                    .expect("以 error 载荷回传")
            };
            let gate_err = pty_command_error(&call_undeclared);
            assert!(
                gate_err.contains("not declared"),
                "④ fixture 的 manifest `api: []`，互调必须被门禁拒绝，got: {call_undeclared}"
            );

            // ==================== ⑤ 权限两域独立（无 pty:spawn 的插件不得创建） ====================
            let denied_spawn = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command("pty-spawn", &serde_json::json!({ "command": "/bin/true" }).to_string())
                    .expect("以 error 载荷回传")
            };
            assert!(
                pty_command_error(&denied_spawn).contains("permission denied: pty:spawn"),
                "⑤ 只授 pty:io 的插件不得创建 PTY，got: {denied_spawn}"
            );
            // 同属主 B 的 kill（创建域）同样先撞权限门——与 ② 的属主判据区分开
            let denied_kill = {
                let mut guard = plugin_b.lock().await;
                guard
                    .invoke_command("pty-kill", &serde_json::json!({ "ptyId": foreign_id }).to_string())
                    .expect("以 error 载荷回传")
            };
            assert!(
                pty_command_error(&denied_kill).contains("permission denied: pty:spawn"),
                "⑤ kill 属创建域：B 缺 pty:spawn 时必须先被权限门拒绝: {denied_kill}"
            );

            // 收尾：A 的在册句柄回收（B 无在册句柄）
            let bus = Arc::clone(&ctx_a.message_bus);
            let purged = tokio::task::spawn_blocking(move || {
                crate::plugin::manager::wasm_runtime::host_impl::pty::purge_for_plugin(PLUGIN_A, &bus)
            })
            .await
            .expect("purge 任务不得 panic");
            assert_eq!(purged, 1, "收尾回收应只剩 A 的那条常驻 PTY");
        }));
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
            .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
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
            Some(String::from_utf8_lossy(&out.stdout).lines().any(|l| l == "wasm32-wasip3"))
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

    /// WASI 预打开端到端：wasip2 插件经 std::fs 直写宿主预打开目录
    ///
    /// 验证链路：manifest 声明 wasiPreopenDirs（已授权）→
    /// 实例化时宿主 preopen /data → 插件 std::fs::write("/data/demo.txt") →
    /// 宿主侧校验文件落盘 + 读回 + 沙箱边界（根外路径不可达）。
    #[test]
    fn test_wasi_preopen_std_fs_e2e() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_wasi_test_component())
            .expect("compile wasi test component");
        let pid = "com.bedcode.wasi-test";

        let rt = tokio::runtime::Runtime::new().unwrap();
        // 阶段 1（runtime 上下文）：宿主侧准备——授权 + 实例化。
        // 实例化需当前 handle（resolve_preopen_dirs 校验授权）；
        // 组件 ctor 不触发 wasi 文件访问，故此时有 handle 仍安全。
        let (mut plugin, dir) = rt.block_on(async {
            // 授权插件（storage 权限用于 seed fs_granted_paths）
            host_ctx.permission.grant_permissions(pid, &["storage".to_string()]);
            let dir = tempfile::tempdir().expect("tempdir");
            crate::plugin::manager::wasm_runtime::host_impl::storage::storage_set(
                &host_ctx,
                pid,
                "fs_granted_paths",
                serde_json::json!([dir.path().to_string_lossy()]),
            )
            .expect("seed granted path");

            // 实例化：组件导入 wasi 接口，宿主按声明（授权过滤后）preopen /data
            let declared = vec![dir.path().to_string_lossy().to_string()];
            let plugin = wasm_runtime
                .instantiate_component(&component, pid, host_ctx.clone(), &declared, None)
                .expect("instantiate wasi test component");
            (plugin, dir)
        });

        // 阶段 2（无 handle 阻塞线程）：guest 经 std::fs 访问 preopen 目录。
        // 与生产 run_guest_call 对齐——wasi 同步绑定（in_tokio）要求调用线程
        // 不处于任何 tokio runtime 内，否则 "Cannot start a runtime..." panic。
        std::thread::spawn(move || {
            // 1. 插件经 std::fs 直写 /data/demo.txt → 宿主侧落盘校验
            let r = plugin
                .invoke_command("wasi-test.write-file", "{}")
                .expect("write command");
            assert!(serde_json::from_str::<serde_json::Value>(&r)
                .unwrap()
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false));
            let host_file = dir.path().join("demo.txt");
            assert_eq!(
                std::fs::read_to_string(&host_file).expect("host must see the file"),
                "hello-from-wasi"
            );

            // 2. 读回（guest 内同路径）
            let r = plugin
                .invoke_command("wasi-test.read-file", "{}")
                .expect("read command");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&r)
                    .unwrap()
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                "hello-from-wasi"
            );

            // 3. 列举 preopen 根目录，demo.txt 可见
            let r = plugin.invoke_command("wasi-test.list", "{}").expect("list command");
            let entries = serde_json::from_str::<serde_json::Value>(&r)
                .unwrap()
                .get("entries")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            assert!(
                entries.iter().any(|e| e.as_str() == Some("demo.txt")),
                "preopen dir entries must include demo.txt, got {:?}",
                entries
            );

            // 4. 沙箱边界：preopen 根外路径不可达（WASI 能力沙箱）
            let r = plugin
                .invoke_command("wasi-test.outside-root", "{}")
                .expect("outside command");
            let leaked = serde_json::from_str::<serde_json::Value>(&r)
                .unwrap()
                .get("leaked")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            assert!(!leaked, "WASI sandbox must block access outside preopen root");
        })
        .join()
        .expect("guest call thread panicked");
    }

    /// 票 02 A1：wasip3 fixture async 闭环（本地装有 pinned nightly + wasm32-wasip3
    /// target 时执行；未装则跳过——CI stable 无该 target）
    ///
    /// 断言链（/tmp/wasip3-probe 场景 1-3 实证的机制落地到宿主测试）：
    /// - 实例化走 async 路径（instantiate_async）：wasip3 组件导入 async wasi 0.3
    ///   函数，Store 为 async-required，同步实例化会报错——能实例化即证明 async 化
    /// - `read-clock`：wasi:clocks 接口在 async 语义下可读（SystemTime 走 clocks）
    /// - `get-random`：async `wasi:random` get-random-bytes 返回熵——非空、非全零、
    ///   两次调用结果不同（同一实例两次独立调用，证明每次都是新鲜熵）
    #[test]
    fn test_wasip3_fixture_async_closure() {
        let Some(component_bytes) = build_wasip3_test_component() else {
            eprintln!(
                "[skip] wasip3 工具链（{} + wasm32-wasip3）未安装，先执行 scripts/wasip3-toolchain.sh install",
                WASIP3_NIGHTLY
            );
            return;
        };
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&component_bytes)
            .expect("compile wasip3 test component");
        let mut plugin = wasm_runtime
            .instantiate_component(
                &component,
                "com.bedcode.wasip3-test",
                host_ctx,
                &[],
                None,
            )
            .expect("instantiate wasip3 component (async store)");

        // 时钟（wasi:clocks，async 语义下可读）
        let r = plugin
            .invoke_command("wasip3-test.read-clock", "{}")
            .expect("clock command");
        let unix_ms = serde_json::from_str::<serde_json::Value>(&r)
            .unwrap()
            .get("unix_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(unix_ms > 0, "async clock must be readable, got {}", unix_ms);

        // 熵（async wasi:random get-random-bytes）
        let r1 = plugin
            .invoke_command("wasip3-test.get-random", "{}")
            .expect("random command 1");
        let hex1 = serde_json::from_str::<serde_json::Value>(&r1)
            .unwrap()
            .get("hex")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        assert_eq!(hex1.len(), 64, "32 字节 → 64 hex 字符");
        assert!(
            hex1.chars().any(|c| c != '0'),
            "entropy must not be all zeros"
        );

        // 同一实例第二次调用：结果必须不同（每次新鲜熵，非缓存/伪随机重复）
        let r2 = plugin
            .invoke_command("wasip3-test.get-random", "{}")
            .expect("random command 2");
        let hex2 = serde_json::from_str::<serde_json::Value>(&r2)
            .unwrap()
            .get("hex")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        assert_ne!(hex1, hex2, "two get-random calls must differ");
    }

    /// 回归保护：加载真实构建产物（resources 下 wasip3 版 ai-chatbox，票 03）
    /// 组件导入接口必须与宿主 linker 全部匹配（实例化成功即证明，含 wasi0.3 全套
    /// p3 接口）；产物缺失（未跑插件构建）时跳过——插件装配由真实构建 + 运行覆盖。
    #[test]
    fn test_ai_chatbox_wasip3_artifact_loads() {
        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm");
        if !wasm_path.exists() {
            eprintln!("[skip] ai-chatbox wasip3 artifact not built");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, "com.bedcode.ai-chatbox", host_ctx, &[], None)
            .expect("load wasip3 ai-chatbox: all imports must resolve");
        // manifest 往返（无副作用导出，验证 bindgen 接口工作）
        let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.ai-chatbox");
    }

    /// 认证中心插件（票 06）生命周期闭环：加载真实构建产物（resources 下
    /// wasip3 版 devices）→ 激活 → 命令调用 → 停用；manifest 声明（api +
    /// permissions）透传断言。产物缺失（未跑插件构建）时跳过。
    #[test]
    fn test_devices_plugin_artifact_lifecycle() {
        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm");
        if !wasm_path.exists() {
            eprintln!("[skip] devices wasip3 artifact not built");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        // 授权路径等价 PluginHost 装载（manifest permissions 登记）：认证中心
        // 声明 auth/storage/peer（票 09 起 peer 供 consent 取可信集）——缺省
        // 不授权则 host-auth/host-peer 调用被权限门拒绝（2026-09-19 修复：此前
        // 测试从未真正运行（产物路径未解析）→ 授权缺口被跳过掩盖）
        host_ctx.permission.grant_permissions(
            "com.bedcode.devices",
            &[
                "auth".to_string(),
                "storage".to_string(),
                "peer".to_string(),
            ],
        );
        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, "com.bedcode.devices", Arc::clone(&host_ctx), &[], None)
            .expect("load wasip3 devices: all imports must resolve");

        // 生命周期闭环
        assert_eq!(plugin.activate().expect("activate"), 0);

        // manifest 声明（api + permissions，票 06 验收：auth/storage + api 探活）
        let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.devices");
        let permissions: Vec<&str> = manifest["permissions"]
            .as_array()
            .expect("permissions array")
            .iter()
            .map(|v| v.as_str().expect("permission str"))
            .collect();
        assert!(permissions.contains(&"auth"), "auth permission declared");
        assert!(permissions.contains(&"storage"), "storage permission declared");
        assert!(permissions.contains(&"peer"), "peer permission declared (票 09 consent 需 host-peer 可信集)");
        assert_eq!(
            manifest["api"],
            serde_json::json!([
                "com.bedcode.devices.hello",
                "com.bedcode.devices.decide-consent",
                "com.bedcode.devices.list-trusted-devices",
                "com.bedcode.devices.pairing-code-generate",
                "com.bedcode.devices.pairing-code-status",
                "com.bedcode.devices.pairing-code-verify",
                "com.bedcode.devices.pairing-code-clear",
                "com.bedcode.devices.qr-code-generate",
                "com.bedcode.devices.qr-code-status",
                "com.bedcode.devices.qr-code-verify",
                "com.bedcode.devices.qr-code-clear"
            ]),
            "api declared (ADR 0017，票 09 互调 api + 票 11 命令面桥接 api)"
        );

        // 命令可调用（host 命令面探活）
        let result = plugin
            .invoke_command("devices.status", "{}")
            .expect("devices.status");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["plugin"], "com.bedcode.devices");
        assert_eq!(r["skeleton"], true);
        assert_eq!(r["permissions"], manifest["permissions"]);
        assert_eq!(r["api"], manifest["api"]);

        // 未知命令明确报错（命令面快速失败）：wasm_entry 将 Err 序列化为
        // `{"error": ...}` JSON 返回（宿主侧 Ok）——断言错误形状而非 is_err
        let ghost = plugin
            .invoke_command("devices.ghost", "{}")
            .expect("devices.ghost 返回 JSON");
        let r: serde_json::Value = serde_json::from_str(&ghost).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("Unknown command")).unwrap_or(false),
            "未知命令必须返回 error 形状, got: {ghost}"
        );

        // ==================== pairing 闭环（票 07） ====================
        // 配对码：生成 → 正确验证通过 → 一次性（二次失败）→ 错误码拒绝
        let r = plugin
            .invoke_command("devices.pairing.code.generate", "{}")
            .expect("code.generate");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let code = r["code"].as_str().expect("pairing code").to_string();
        assert_eq!(code.len(), 6, "配对码必须 6 位数字");
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert!(r["expires_in"].as_u64().unwrap() <= r["ttl"].as_u64().unwrap());

        let r = plugin
            .invoke_command("devices.pairing.code.verify", &format!(r#"{{"code":"{code}"}}"#))
            .expect("code.verify correct");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r).unwrap()["valid"], true);
        // 一次性语义：验证成功后 code 已消耗，二次验证失败
        let r = plugin
            .invoke_command("devices.pairing.code.verify", &format!(r#"{{"code":"{code}"}}"#))
            .expect("code.verify reuse");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r).unwrap()["valid"], false);
        // 错误码拒绝
        plugin.invoke_command("devices.pairing.code.generate", "{}").expect("code.generate 2");
        let r = plugin
            .invoke_command("devices.pairing.code.verify", r#"{"code":"000000"}"#)
            .expect("code.verify wrong");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r).unwrap()["valid"], false);

        // QR token：生成（32 hex）→ 正确验证通过 → 一次性（二次失败，且不匹配也报错）
        let r = plugin
            .invoke_command("devices.pairing.qr.generate", "{}")
            .expect("qr.generate");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let token = r["token"].as_str().expect("qr token").to_string();
        assert_eq!(token.len(), 32, "QR token 必须 128-bit → 32 hex 字符");
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));

        let r = plugin
            .invoke_command("devices.pairing.qr.verify", &format!(r#"{{"token":"{token}"}}"#))
            .expect("qr.verify correct");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r).unwrap()["valid"], true);
        // 一次性：消费后无活跃 token → 报错（wasm_entry 将 Err 序列化为
        // `{"error": ...}` JSON，断言错误形状）
        let reused = plugin
            .invoke_command("devices.pairing.qr.verify", &format!(r#"{{"token":"{token}"}}"#))
            .expect("qr.verify reuse returns JSON");
        let r: serde_json::Value = serde_json::from_str(&reused).unwrap();
        assert!(
            r["error"].as_str().is_some(),
            "QR token 消费后二次验证必须失败, got: {reused}"
        );

        // JWT：经 host-auth 密钥签发 → 验签闭环（claims 透传）
        let r = plugin
            .invoke_command(
                "devices.pairing.jwt.generate",
                r#"{"device_id":"pairing-test-dev","device_name":"Test Phone","fingerprint":"fp-1"}"#,
            )
            .expect("jwt.generate");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let jwt = r["token"].as_str().expect("jwt token").to_string();
        assert_eq!(jwt.split('.').count(), 3, "JWT 必须三段结构");
        assert!(r["exp"].as_u64().unwrap() > 0);

        let r = plugin
            .invoke_command("devices.pairing.jwt.verify", &format!(r#"{{"token":"{jwt}"}}"#))
            .expect("jwt.verify");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["valid"], true);
        assert_eq!(r["claims"]["sub"], "pairing-test-dev");
        assert_eq!(r["claims"]["device_name"], "Test Phone");
        assert_eq!(r["claims"]["iss"], "BedCode");
        // 篡改 token 必须验签失败（签名完整性；Err → `{"error": ...}` JSON）
        let tampered = format!("{}x", jwt);
        let r = plugin
            .invoke_command("devices.pairing.jwt.verify", &format!(r#"{{"token":"{tampered}"}}"#))
            .expect("jwt.verify tampered returns JSON");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert!(
            r["error"].as_str().is_some(),
            "篡改 token 必须拒绝, got: {r}"
        );

        // 密钥托管：host-auth secret-store 落库（真源 plugin_secrets，属主隔离），
        // 明文不出插件（只报长度）
        let r = plugin
            .invoke_command("devices.pairing.key.status", "{}")
            .expect("key.status");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["present"], true);
        assert_eq!(r["key_len"], 32, "JWT 密钥必须 32 字节（HS256 最小安全长度）");

        // 密钥经宿主落库断言：plugin_secrets 表存在属主行（hex 64 字符），
        // 且只能经 length 观测——明文不出宿主（票 04 日志红线）
        {
            let db = host_ctx.db.blocking_lock();
            let stored: String = db
                .conn()
                .query_row(
                    "SELECT value FROM plugin_secrets WHERE plugin_id = 'com.bedcode.devices' AND key = 'jwt.key'",
                    [],
                    |row| row.get(0),
                )
                .expect("jwt.key 必须已落库");
            assert_eq!(stored.len(), 64, "32 字节密钥 → 64 hex 字符落库");
            assert_eq!(
                hex::decode(&stored).expect("stored hex").len(),
                32,
                "落库值必须可解码回 32 字节"
            );
        }

        // ==================== trust 闭环（票 08） ====================
        // 统一视图：空列表 → 新增配对记录 → 列表可见（kind=pairing，字段对齐
        // 宿主 Pairing）→ 撤销 → 立即消失；持久化落库断言（plugin_storage
        // 键 trust.pairings）。
        //
        // 注：宿主测试为无头上下文（app_handle=None），host-peer 原语不可用
        // （require_app 失败），故 peer 段经 peerError 透出——此处断言 pairing
        // 段不受影响 + peerError 非空（显性降级，不静默吞错）。

        // 1. 空信任列表 → devices 空
        let r = plugin
            .invoke_command("devices.trust.list", "{}")
            .expect("trust.list empty");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["devices"].as_array().unwrap().len(), 0, "初始信任列表为空");
        // 无头上下文 peer-net 不可用：peerError 必须透出（不静默降级为空）
        let peer_err = r["peerError"].as_str().expect("peerError 必须透出");
        assert!(
            peer_err.contains("unavailable") || peer_err.contains("headless"),
            "无头上下文 peer 不可用错误透出，got: {peer_err}"
        );

        // 2. 新增配对记录（配对完成流写入入口）→ 列表可见
        let r = plugin
            .invoke_command(
                "devices.trust.add-pairing",
                r#"{"deviceName":"Trust Phone","deviceFingerprint":"fp-trust-1"}"#,
            )
            .expect("trust.add-pairing");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let pairing_id = r["id"].as_str().expect("pairing id").to_string();
        assert!(!pairing_id.is_empty());

        let r = plugin
            .invoke_command("devices.trust.list", "{}")
            .expect("trust.list after add");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let devices = r["devices"].as_array().expect("devices array");
        assert_eq!(devices.len(), 1, "新增后列表可见 1 条");
        assert_eq!(devices[0]["kind"], "pairing");
        assert_eq!(devices[0]["id"], pairing_id);
        assert_eq!(devices[0]["name"], "Trust Phone");
        assert_eq!(devices[0]["fingerprint"], "fp-trust-1");
        assert_eq!(devices[0]["active"], true);
        assert_eq!(devices[0]["connectCount"], 1, "宿主 add_pairing 初始 connect_count=1");
        // addedAt 必须 RFC3339（可解析）
        let added = devices[0]["addedAt"].as_str().expect("addedAt");
        assert!(
            crate::plugin::manager::wasm_runtime::tests::parse_rfc3339_for_test(added),
            "addedAt 必须 RFC3339: {added}"
        );

        // 3. 持久化断言：plugin_storage 表存在 trust.pairings 键，内容含新记录
        //    （重启一致真源在宿主存储，撤销软删后仍保留 active=false）
        {
            let db = host_ctx.db.blocking_lock();
            let stored: Option<String> = db
                .conn()
                .query_row(
                    "SELECT value FROM plugin_storage WHERE plugin_id = 'com.bedcode.devices' AND key = 'trust.pairings'",
                    [],
                    |row| row.get(0),
                )
                .ok();
            let stored = stored.expect("trust.pairings 必须已落库");
            let parsed: serde_json::Value = serde_json::from_str(&stored).expect("trust.pairings JSON");
            let arr = parsed.as_array().expect("pairing 记录数组");
            assert_eq!(arr.len(), 1, "落库 1 条记录");
            assert_eq!(arr[0]["id"], pairing_id);
            assert_eq!(arr[0]["active"], true, "撤销前 active=true");
        }

        // 4. 撤销 → 立即从统一视图消失 + 落库软删（active=false，保留记录）
        let r = plugin
            .invoke_command(
                "devices.trust.revoke",
                &format!(r#"{{"id":"{pairing_id}"}}"#),
            )
            .expect("trust.revoke");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(r["removed"], true);
        assert_eq!(r["kind"], "pairing");

        let r = plugin
            .invoke_command("devices.trust.list", "{}")
            .expect("trust.list after revoke");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(
            r["devices"].as_array().unwrap().len(),
            0,
            "撤销后立即从统一视图消失（active 过滤语义）"
        );

        // 5. 撤销未命中 id：pairing 幂等 false 由插件单测覆盖（MockPeer）；
        // 无头上下文下 ghost-id 落空 pairing → 落入 peer 段 → host-peer 不可用
        // → 显性上抛（trust 模块文档「不静默吞错，不做看似成功的假撤销」）
        let r = plugin
            .invoke_command("devices.trust.revoke", r#"{"id":"ghost-id"}"#)
            .expect("trust.revoke unknown returns JSON");
        let r: serde_json::Value = serde_json::from_str(&r).unwrap();
        let err = r["error"].as_str().expect("无头上下文 peer 不可用必须上抛, got: {r}");
        assert!(
            err.contains("unavailable") || err.contains("headless"),
            "peer 引擎不可用错误透出, got: {err}"
        );

        // 6. 撤销后落库断言：记录保留但 active=false（软删持久化，重启一致）
        {
            let db = host_ctx.db.blocking_lock();
            let stored: String = db
                .conn()
                .query_row(
                    "SELECT value FROM plugin_storage WHERE plugin_id = 'com.bedcode.devices' AND key = 'trust.pairings'",
                    [],
                    |row| row.get(0),
                )
                .expect("trust.pairings 撤销后仍落库");
            let parsed: serde_json::Value = serde_json::from_str(&stored).expect("trust.pairings JSON");
            let arr = parsed.as_array().expect("pairing 记录数组");
            assert_eq!(arr.len(), 1, "软删保留记录（宿主 is_active=0 语义）");
            assert_eq!(arr[0]["active"], false, "撤销后 active=false");
        }

        assert_eq!(plugin.deactivate().expect("deactivate"), 0);
    }

    /// 票 09 互调闭环：消费方（sdk-test caller 角色）经互调调用认证中心真实产物的
    /// `auth.decide-consent` / `auth.list-trusted-devices` 成功返回；未声明 api 被
    /// 宿主门禁拒绝（ADR 0017「未声明 api 不可调」）。产物缺失时跳过。
    ///
    /// 决策断言（headless 宿主：host-peer require_app 失败 → 信任不可验证 →
    /// fail-closed 按未知处理，见 consent/ops.rs 模块文档）：
    /// - 阶段 1（仅 peer 信息，无用户意向）→ ask
    /// - 阶段 2（回传 userDecision）→ accept/explicit、deny、accept/one_time
    /// 已信任免确认的自动放行路径由插件 native 单测覆盖（mock 可信集注入）。
    #[test]
    fn test_devices_consent_api_closed_loop() {
        const CONSUMER_ID: &str = "com.bedcode.consent-consumer";
        const DEVICE_ID: &str = "com.bedcode.devices";
        const NODE: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";

        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm");
        if !wasm_path.exists() {
            eprintln!("[skip] devices wasip3 artifact not built");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let devices_component = wasm_runtime
            .compile_component(&std::fs::read(&wasm_path).expect("read devices artifact"))
            .expect("compile devices artifact");
        let consumer_component = wasm_runtime
            .compile_component(&build_sdk_test_component())
            .expect("compile sdk-test consumer");

        // 授权路径等价 PluginHost 装载（manifest permissions 登记）：认证中心
        // 声明 auth/storage/peer（票 09 起 peer 供 consent 取可信集）
        host_ctx.permission.grant_permissions(
            DEVICE_ID,
            &[
                "auth".to_string(),
                "storage".to_string(),
                "peer".to_string(),
            ],
        );
        // 登记目标插件声明的 api（等价 PluginHost::activate_plugin 的登记）
        host_ctx.api_registry().register(
            DEVICE_ID,
            &[
                "com.bedcode.devices.hello".to_string(),
                "com.bedcode.devices.decide-consent".to_string(),
                "com.bedcode.devices.list-trusted-devices".to_string(),
                "com.bedcode.devices.pairing-code-generate".to_string(),
                "com.bedcode.devices.pairing-code-status".to_string(),
                "com.bedcode.devices.pairing-code-verify".to_string(),
                "com.bedcode.devices.pairing-code-clear".to_string(),
                "com.bedcode.devices.qr-code-generate".to_string(),
                "com.bedcode.devices.qr-code-status".to_string(),
                "com.bedcode.devices.qr-code-verify".to_string(),
                "com.bedcode.devices.qr-code-clear".to_string(),
            ],
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let devices = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&devices_component, DEVICE_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate devices"),
            ));
            let consumer = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&consumer_component, CONSUMER_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate consumer"),
            ));

            let instances = Arc::new(RwLock::new(HashMap::from([
                (DEVICE_ID.to_string(), devices.clone()),
                (CONSUMER_ID.to_string(), consumer.clone()),
            ])));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
                .await;

            devices.lock().await.activate().expect("devices activate");
            consumer.lock().await.activate().expect("consumer activate");

            // 阶段 1：仅 peer 信息（无用户意向）→ 无头上下文 host-peer 不可用
            // （require_app 失败）→ fail-closed ask（信任不可验证按未知处理）
            let result = consumer
                .lock()
                .await
                .invoke_command(
                    "test_devices_decide_consent",
                    &format!(
                        r#"{{"peerInfo":{{"requestId":"req-c1","nodeId":"{NODE}","deviceName":"模拟对端"}}}}"#
                    ),
                )
                .expect("decide-consent phase 1");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(
                r["decision"]["decision"], "ask",
                "无头上下文信任不可验证 → fail-closed ask, got: {result}"
            );
            assert_eq!(r["decision"]["requestId"], "req-c1");
            assert!(r["decision"].get("reason").is_none(), "ask 无 reason, got: {result}");

            // 阶段 2（允许）：回传 userDecision=accept → accept / reason=explicit
            let result = consumer
                .lock()
                .await
                .invoke_command(
                    "test_devices_decide_consent_explicit",
                    &format!(
                        r#"{{"userDecision":"accept","peerInfo":{{"requestId":"req-c2","nodeId":"{NODE}"}}}}"#
                    ),
                )
                .expect("decide-consent explicit accept");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["decision"]["decision"], "accept", "got: {result}");
            assert_eq!(r["decision"]["reason"], "explicit", "用户显式允许 reason=explicit, got: {result}");

            // 阶段 2（拒绝）：userDecision=deny → deny / reason=explicit
            let result = consumer
                .lock()
                .await
                .invoke_command(
                    "test_devices_decide_consent_explicit",
                    &format!(
                        r#"{{"userDecision":"deny","peerInfo":{{"requestId":"req-c3","nodeId":"{NODE}"}}}}"#
                    ),
                )
                .expect("decide-consent explicit deny");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["decision"]["decision"], "deny", "got: {result}");

            // 阶段 2（一次性确认）：userDecision=one_time → accept / reason=one_time
            // （放行但不改变信任状态——消费方据此不落信任，下次首连仍询问）
            let result = consumer
                .lock()
                .await
                .invoke_command(
                    "test_devices_decide_consent_explicit",
                    &format!(
                        r#"{{"userDecision":"one_time","peerInfo":{{"requestId":"req-c4","nodeId":"{NODE}"}}}}"#
                    ),
                )
                .expect("decide-consent explicit one_time");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["decision"]["decision"], "accept", "got: {result}");
            assert_eq!(r["decision"]["reason"], "one_time", "一次性确认 reason=one_time, got: {result}");

            // 零参 api：list-trusted-devices → 统一视图（pairing 空 + 无头上下文
            // peerError 透出——trust 模块显性降级语义）
            let result = consumer
                .lock()
                .await
                .invoke_command("test_devices_list_trusted", "{}")
                .expect("list-trusted-devices");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["devices"].as_array().unwrap().len(), 0, "初始信任列表为空, got: {result}");
            let peer_err = r["peerError"].as_str().expect("peerError 必须透出");
            assert!(
                peer_err.contains("unavailable") || peer_err.contains("headless"),
                "无头上下文 peer 不可用错误透出, got: {peer_err}"
            );

            // 未声明 api：com.bedcode.devices.ghost-api 不在 manifest → 宿主门禁拒绝
            // （ADR 0017「未声明 api 不可调」——请求在发布前被拒，不进入插件）
            let result = consumer
                .lock()
                .await
                .invoke_command("test_devices_undeclared", "{}")
                .expect("test_devices_undeclared");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
                "未声明 api 必须被门禁拒绝, got: {result}"
            );

            devices.lock().await.deactivate().expect("devices deactivate");
            consumer.lock().await.deactivate().expect("consumer deactivate");
        });
    }

    /// 互调 wire 捕获器（票 10 闭环）：静态订阅认证中心的请求 topic，记录
    /// file-transfer → auth center 的 JSON-RPC 请求（含 params 原样）
    struct AuthCenterCaptureHandler {
        captures: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl crate::plugin::bus::BusMessageHandler for AuthCenterCaptureHandler {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            // std Mutex 短临界区（总线消费任务内，禁止阻塞锁/await）
            self.captures
                .lock()
                .expect("capture lock")
                .push((msg.topic.clone(), msg.payload.clone()));
            Ok(())
        }
    }

    /// 票 10 互调闭环：真实 file-transfer 产物消费真实认证中心（devices）产物
    /// ——consent 两阶段流（阶段 1 信任预检 / 阶段 2 用户意向）与统一信任视图
    /// 均经总线互调；并在 wire 层捕获断言 file-transfer 发出的 JSON-RPC 请求
    /// 形状（requestId / nodeId / userDecision 映射正确），证明消费迁移生效。
    ///
    /// 约束：无头上下文 host-peer 不可用（require_app 失败）——
    /// - decide-consent 返回 ask（fail-closed）；accept/deny 决策后的宿主应答
    ///   以错误形状（headless）透出，恰证「决策已应用到宿主应答路径」
    /// - list-trusted 经认证中心视图（peerError 透出）错误形状
    /// 降级路径（认证中心不可用）：respond-consent / list-trusted 直答宿主
    /// （与迁移前行为等价，双轨无单点）。决策本身的正确性由票 09 闭环覆盖，
    /// 本测试聚焦消费方 wire 契约与降级。产物缺失时跳过。
    #[test]
    fn test_filetransfer_consumes_auth_center_closed_loop() {
        const FT_ID: &str = "com.bedcode.file-transfer";
        const DEVICE_ID: &str = "com.bedcode.devices";
        const NODE: &str = "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344";
        const DECIDE_TOPIC: &str = "bedcode.api.com.bedcode.devices.decide-consent";
        const LIST_TOPIC: &str = "bedcode.api.com.bedcode.devices.list-trusted-devices";
        const DEVICE_APIS: &[&str] = &[
            "com.bedcode.devices.hello",
            "com.bedcode.devices.decide-consent",
            "com.bedcode.devices.list-trusted-devices",
            "com.bedcode.devices.pairing-code-generate",
            "com.bedcode.devices.pairing-code-status",
            "com.bedcode.devices.pairing-code-verify",
            "com.bedcode.devices.pairing-code-clear",
            "com.bedcode.devices.qr-code-generate",
            "com.bedcode.devices.qr-code-status",
            "com.bedcode.devices.qr-code-verify",
            "com.bedcode.devices.qr-code-clear",
        ];

        let ft_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.file-transfer/bedcode_plugin_file_transfer.wasm");
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm");
        if !ft_path.exists() || !dev_path.exists() {
            eprintln!("[skip] file-transfer / devices wasip3 artifacts not built");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let ft_component = wasm_runtime
            .compile_component(&std::fs::read(&ft_path).expect("read file-transfer artifact"))
            .expect("compile file-transfer artifact");
        let dev_component = wasm_runtime
            .compile_component(&std::fs::read(&dev_path).expect("read devices artifact"))
            .expect("compile devices artifact");

        // 授权路径等价 PluginHost 装载（manifest permissions 登记）：
        // - 认证中心声明 auth/storage/peer（ail 09 起 peer 供 consent 取可信集）
        // - file-transfer 声明 peer（宿主应答/降级直查路径需权限门放行）
        host_ctx.permission.grant_permissions(
            DEVICE_ID,
            &["auth".to_string(), "storage".to_string(), "peer".to_string()],
        );
        host_ctx
            .permission
            .grant_permissions(FT_ID, &["peer".to_string()]);
        host_ctx.api_registry().register(
            DEVICE_ID,
            &DEVICE_APIS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );

        // wire 捕获（静态订阅，与 devices 的 wasm 订阅共存 fan-out）
        let captures: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            host_ctx
                .message_bus
                .subscribe_static("capture", DECIDE_TOPIC, Box::new(AuthCenterCaptureHandler { captures: Arc::clone(&captures) }))
                .await;
            host_ctx
                .message_bus
                .subscribe_static("capture", LIST_TOPIC, Box::new(AuthCenterCaptureHandler { captures: Arc::clone(&captures) }))
                .await;

            let filetransfer = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&ft_component, FT_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate file-transfer"),
            ));
            let devices = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&dev_component, DEVICE_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate devices"),
            ));

            let instances = Arc::new(RwLock::new(HashMap::from([
                (FT_ID.to_string(), filetransfer.clone()),
                (DEVICE_ID.to_string(), devices.clone()),
            ])));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
                .await;

            devices.lock().await.activate().expect("devices activate");
            filetransfer.lock().await.activate().expect("file-transfer activate");

            // 等待捕获数（线程安全轮询；事件/互调均异步派发）
            async fn wait_captures(
                captures: &Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
                topic: &str,
                n: usize,
            ) {
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
                loop {
                    let count = {
                        let c = captures.lock().expect("capture lock");
                        c.iter().filter(|(t, _)| t == topic).count()
                    };
                    if count >= n {
                        return;
                    }
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "timeout waiting for {n} captures on '{topic}' (got {count})"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                }
            }
            async fn decide_count(
                captures: &Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
                topic: &str,
            ) -> usize {
                let c = captures.lock().expect("capture lock");
                c.iter().filter(|(t, _)| t == topic).count()
            }

            // ==================== consent：阶段 1 信任预检 ====================
            // 发布 peer:consent（宿主引擎事件桥语义）→ file-transfer 经
            // auth.decide-consent（无 userDecision）预检信任
            host_ctx.message_bus.publish(
                "peer:consent",
                "test-host",
                serde_json::json!({
                    "requestId": "req-c1",
                    "nodeId": NODE,
                    "fingerprintShort": &NODE[..8],
                    "deviceName": "消费方测试对端",
                }),
            );
            wait_captures(&captures, DECIDE_TOPIC, 1).await;
            {
                let c = captures.lock().expect("capture lock");
                let (_, req) = c.iter().find(|(t, _)| t == DECIDE_TOPIC).unwrap();
                // 阶段 1：无用户意向（仅对端信息），requestId/nodeId 沿事件桥原样
                assert!(
                    req["params"].get("userDecision").is_none(),
                    "阶段 1 不得携带用户意向, got: {}",
                    req["params"]
                );
                assert_eq!(req["params"]["requestId"], "req-c1");
                assert_eq!(req["params"]["nodeId"], NODE);
                assert_eq!(req["params"]["deviceName"], "消费方测试对端");
            }

            // ==================== consent：阶段 2 用户接受 ====================
            // 回传 accepted=true → auth.decide-consent userDecision=accept →
            // 决策 accept → 应答宿主（无头上下文 headless 错误透出——恰证决策
            // 被应用到宿主应答路径，而非静默丢弃）
            let result = filetransfer
                .lock()
                .await
                .invoke_command(
                    "file-transfer.respond-consent",
                    r#"{"requestId":"req-c1","accepted":true}"#,
                )
                .expect("respond-consent accept");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("unavailable") || e.contains("headless"))
                    .unwrap_or(false),
                "accept 决策后应答宿主（无头上下文报错透出）, got: {result}"
            );
            wait_captures(&captures, DECIDE_TOPIC, 2).await;
            {
                let c = captures.lock().expect("capture lock");
                let (_, req) = c
                    .iter()
                    .filter(|(t, _)| t == DECIDE_TOPIC)
                    .last()
                    .unwrap();
                assert_eq!(
                    req["params"]["userDecision"], "accept",
                    "阶段 2 必须携带 userDecision=accept, got: {}",
                    req["params"]
                );
                assert_eq!(req["params"]["requestId"], "req-c1");
                assert_eq!(
                    req["params"]["nodeId"], NODE,
                    "对端信息沿事件桥登记传递"
                );
            }

            // ==================== consent：阶段 2 用户拒绝 ====================
            host_ctx.message_bus.publish(
                "peer:consent",
                "test-host",
                serde_json::json!({
                    "requestId": "req-c2",
                    "nodeId": NODE,
                    "fingerprintShort": &NODE[..8],
                }),
            );
            wait_captures(&captures, DECIDE_TOPIC, 3).await; // req-c2 阶段 1
            let result = filetransfer
                .lock()
                .await
                .invoke_command(
                    "file-transfer.respond-consent",
                    r#"{"requestId":"req-c2","accepted":false}"#,
                )
                .expect("respond-consent deny");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("unavailable") || e.contains("headless"))
                    .unwrap_or(false),
                "deny 决策后应答宿主（无头上下文报错透出）, got: {result}"
            );
            wait_captures(&captures, DECIDE_TOPIC, 4).await;
            {
                let c = captures.lock().expect("capture lock");
                let (_, req) = c
                    .iter()
                    .filter(|(t, _)| t == DECIDE_TOPIC)
                    .last()
                    .unwrap();
                assert_eq!(
                    req["params"]["userDecision"], "deny",
                    "accepted=false 映射 userDecision=deny, got: {}",
                    req["params"]
                );
            }

            // ==================== 降级：认证中心不可用（门禁拒绝） ====================
            // 注销 devices 声明 → 互调请求被门禁拦下。无登记请求（未发布
            // peer:consent）的迟到应答 → file-transfer 直答宿主（迁移前行为
            // 等价，双轨无单点），不发起任何互调。
            // （阶段 1 降级弹窗的编排路径由插件 native 单测
            // phase1_auth_center_down_falls_back_to_ask 覆盖——闭环聚焦可确定的
            // wire 断言，避免异步派发时序竞态。）
            host_ctx.api_registry().unregister(DEVICE_ID);
            let result = filetransfer
                .lock()
                .await
                .invoke_command(
                    "file-transfer.respond-consent",
                    r#"{"requestId":"req-unknown","accepted":true}"#,
                )
                .expect("respond-consent degraded");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("unavailable") || e.contains("headless"))
                    .unwrap_or(false),
                "降级直答宿主（无头上下文报错透出）, got: {result}"
            );
            assert_eq!(
                decide_count(&captures, DECIDE_TOPIC).await,
                4,
                "认证中心不可用：不得发出版互调请求（门禁拦下 / 无登记直答）"
            );

            // ==================== 信任列表：经认证中心统一视图 ====================
            host_ctx.api_registry().register(
                DEVICE_ID,
                &DEVICE_APIS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );
            let result = filetransfer
                .lock()
                .await
                .invoke_command("file-transfer.list-trusted", "{}")
                .expect("list-trusted");
            wait_captures(&captures, LIST_TOPIC, 1).await;
            {
                let c = captures.lock().expect("capture lock");
                let (_, req) = c.iter().find(|(t, _)| t == LIST_TOPIC).unwrap();
                assert_eq!(req["method"], "list-trusted-devices");
            }
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("unavailable") || e.contains("headless"))
                    .unwrap_or(false),
                "认证中心 peerError 透出（不静默空列表）, got: {result}"
            );

            // ==================== 撤销：维持宿主原语（语义不变） ====================
            let result = filetransfer
                .lock()
                .await
                .invoke_command(
                    "file-transfer.revoke-trusted",
                    &format!(r#"{{"nodeId":"{NODE}"}}"#),
                )
                .expect("revoke-trusted");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("unavailable") || e.contains("headless"))
                    .unwrap_or(false),
                "revoke 宿主原语（无头上下文报错透出）, got: {result}"
            );
            assert_eq!(
                decide_count(&captures, DECIDE_TOPIC).await,
                4,
                "revoke 不走互调（无声明 api）"
            );

            devices.lock().await.deactivate().expect("devices deactivate");
            filetransfer
                .lock()
                .await
                .deactivate()
                .expect("file-transfer deactivate");
        });
    }

    /// 票 11 宿主命令面桥接闭环：宿主桥接层（`utils/auth/auth_center.rs`）复用
    /// 真实 wasip3 devices 产物 —— 认证中心激活时配对码 / QR 生命周期经互调
    /// 转发（状态以认证中心为准、与前端命令面/server 端点同源）；注销注册表
    /// （模拟停用）后降级宿主 `PairingService` / `QrTokenManager`（双轨并存
    /// 期无单点）。产物缺失时跳过。
    #[test]
    fn test_host_pairing_bridge_closed_loop() {
        use crate::utils::auth::auth_center as bridge;

        const DEVICE_ID: &str = "com.bedcode.devices";
        const DEVICE_APIS: &[&str] = &[
            "com.bedcode.devices.hello",
            "com.bedcode.devices.decide-consent",
            "com.bedcode.devices.list-trusted-devices",
            "com.bedcode.devices.pairing-code-generate",
            "com.bedcode.devices.pairing-code-status",
            "com.bedcode.devices.pairing-code-verify",
            "com.bedcode.devices.pairing-code-clear",
            "com.bedcode.devices.qr-code-generate",
            "com.bedcode.devices.qr-code-status",
            "com.bedcode.devices.qr-code-verify",
            "com.bedcode.devices.qr-code-clear",
        ];

        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.devices/bedcode_plugin_devices.wasm");
        if !wasm_path.exists() {
            eprintln!("[skip] devices wasip3 artifact not built");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let dev_component = wasm_runtime
            .compile_component(&std::fs::read(&wasm_path).expect("read devices artifact"))
            .expect("compile devices artifact");

        // 授权路径等价 PluginHost 装载（manifest permissions 登记）
        host_ctx.permission.grant_permissions(
            DEVICE_ID,
            &["auth".to_string(), "storage".to_string(), "peer".to_string()],
        );
        host_ctx.api_registry().register(
            DEVICE_ID,
            &DEVICE_APIS.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let devices = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&dev_component, DEVICE_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate devices"),
            ));
            let instances = Arc::new(RwLock::new(HashMap::from([(DEVICE_ID.to_string(), devices.clone())])));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
                .await;
            devices.lock().await.activate().expect("devices activate");

            // 真实宿主服务（降级路径目标，迁移前行为）
            let pairing_service = crate::server::services::pairing_service::PairingService::new();
            let qr_manager = crate::utils::auth::QrTokenManager::new();

            // ==================== 激活路径：配对码经认证中心 ====================
            let code = bridge::generate_pairing_code(&host_ctx, &pairing_service, 300)
                .await
                .expect("bridge generate");
            assert_eq!(code.code.len(), 6, "配对码 6 位数字");
            assert!(code.code.chars().all(|c| c.is_ascii_digit()));

            let current = bridge::current_pairing_code(&host_ctx, &pairing_service)
                .await
                .expect("bridge current");
            assert_eq!(
                current.expect("current code").code,
                code.code,
                "生成后当前码一致（认证中心状态为准）"
            );

            assert!(
                bridge::verify_pairing_code(&host_ctx, &pairing_service, &code.code)
                    .await
                    .expect("bridge verify")
            );
            assert!(
                !bridge::verify_pairing_code(&host_ctx, &pairing_service, &code.code)
                    .await
                    .expect("bridge verify reuse"),
                "一次性：二次验证失败"
            );
            assert!(
                bridge::current_pairing_code(&host_ctx, &pairing_service)
                    .await
                    .expect("bridge current after consume")
                    .is_none(),
                "验证成功后当前码为空（认证中心已消耗）"
            );

            // ==================== 激活路径：QR token 经认证中心 ====================
            let token = bridge::generate_qr_code(&host_ctx, &qr_manager, 300)
                .await
                .expect("bridge qr generate");
            assert_eq!(token.len(), 32, "QR token 32 hex 字符");
            let (active_token, remaining) = bridge::qr_conn_info(&host_ctx, &qr_manager)
                .await
                .expect("bridge qr info")
                .expect("active token");
            assert_eq!(active_token, token);
            assert!(remaining <= 300);

            let outcome = bridge::verify_qr_token(&host_ctx, &qr_manager, &token)
                .await
                .expect("bridge qr verify");
            assert!(
                matches!(outcome, bridge::QrVerifyOutcome::Valid),
                "首次验证通过"
            );
            let reuse = bridge::verify_qr_token(&host_ctx, &qr_manager, &token)
                .await
                .expect("bridge qr verify reuse");
            match reuse {
                bridge::QrVerifyOutcome::Rejected(reason) => {
                    // 成功消费即清除（宿主同语义）→ 二次验证命中 NoActiveToken
                    assert_eq!(reason, "No active QR token", "一次性：成功消费后 token 已清除")
                }
                _ => panic!("QR token 必须一次性"),
            }
            bridge::clear_qr_code(&host_ctx, &qr_manager)
                .await
                .expect("bridge qr clear");
            assert!(
                bridge::qr_conn_info(&host_ctx, &qr_manager)
                    .await
                    .expect("bridge qr info after clear")
                    .is_none(),
                "清除后无活跃 token"
            );

            // ==================== 降级路径：注销（模拟停用）→ 宿主实现 ====================
            host_ctx.api_registry().unregister(DEVICE_ID);
            assert!(!bridge::auth_center_active(&host_ctx), "注销后认证中心不可用");

            let fallback_code = bridge::generate_pairing_code(&host_ctx, &pairing_service, 300)
                .await
                .expect("fallback generate");
            assert_eq!(fallback_code.code.len(), 6);
            assert!(
                bridge::verify_pairing_code(&host_ctx, &pairing_service, &fallback_code.code)
                    .await
                    .expect("fallback verify"),
                "降级后宿主 PairingService 验证（迁移前行为）"
            );

            let fallback_token = bridge::generate_qr_code(&host_ctx, &qr_manager, 300)
                .await
                .expect("fallback qr generate");
            assert_eq!(fallback_token.len(), 32);
            let outcome = bridge::verify_qr_token(&host_ctx, &qr_manager, &fallback_token)
                .await
                .expect("fallback qr verify");
            assert!(
                matches!(outcome, bridge::QrVerifyOutcome::Valid),
                "降级后宿主 QrTokenManager 验证"
            );

            devices.lock().await.deactivate().expect("devices deactivate");
        });
    }

    /// SDK 组件插件完整往返：真实 SDK（wasm_entry! 宏 + WasmHost）产物的组件
    /// 加载、ABI 协商、生命周期、WasmHost 各 trait 经组件 import 的能力往返
    #[test]
    fn test_sdk_plugin_component_roundtrip() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_sdk_test_component())
            .expect("compile SDK test component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut plugin = wasm_runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
                .expect("instantiate SDK test component");

            // 生命周期（宏生成的 lifecycle::Guest）
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);

            // manifest（宏生成的 manifest::Guest）
            let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
            assert_eq!(manifest["id"], "com.bedcode.sdk-test");

            // storage 往返（WasmHost::storage_set/get 经组件 import）
            let result = plugin
                .invoke_command("test_storage", r#"{"key":"sdk-key","value":{"k":"v"}}"#)
                .expect("test_storage");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["got"]["k"], "v");

            // 主库往返（权限 + 表名前缀校验）
            let result = plugin.invoke_command("test_db", "{}").expect("test_db");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            let rows = r["rows"].as_array().expect("rows array");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["val"], "sdk-db");

            // 配置读取（AppConfig 测试初始化 port=8765）
            let result = plugin.invoke_command("test_config", "{}").expect("test_config");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["port"], "8765");

            // 会话列表（权限 session:read，空列表）
            let result = plugin
                .invoke_command("test_session_list", "{}")
                .expect("test_session_list");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["sessions"], serde_json::json!([]));

            // 事件 emit（无头上下文幂等 Ok）
            let result = plugin.invoke_command("test_emit", "{}").expect("test_emit");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["emitted"], true);

            // 消息总线发布（同步投递）
            let result = plugin.invoke_command("test_bus", "{}").expect("test_bus");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["published"], true);

            // notify：无头上下文无 AppHandle，宿主错误经 WIT result 透传
            let result = plugin.invoke_command("test_notify", "{}").expect("test_notify");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"]
                    .as_str()
                    .map(|e| e.contains("headless") || e.contains("app_handle"))
                    .unwrap_or(false),
                "unexpected notify error: {}",
                r["error"]
            );

            // v11 二进制回调（票据 06）：`wasm_entry!` 产物必须暴露可选导出
            // events-binary，宿主按 ItemName 路径语法动态探测命中且调用成功
            // （此前平名 `iface#func` 探测恒不命中，二进制回调实际从未接线）
            plugin
                .on_message_binary("binary-topic", "com.test.sender", b"\x00\xff\x01binary")
                .expect("SDK component must expose events-binary on_message_binary export");

            // 终端钩子（宏生成的 terminal_hooks::Guest，大写转换语义）
            assert_eq!(
                plugin.on_terminal_input("session-1", "sdk input").unwrap(),
                Some("SDK INPUT".to_string())
            );
        });
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

    impl crate::plugin::bus::MessageDispatcher for TestInstanceDispatcher {
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
            frame: &crate::plugin::bus::WsFrameDispatch,
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

    /// 插件互调端到端（issue 04，ADR-0017）：同一 sdk-test 组件以两个实例加载
    /// —— caller（com.bedcode.api-caller）+ 目标（com.bedcode.sdk-test），
    /// 覆盖：请求/响应配对成功、错误传播、超时（模拟无响应目标）、
    /// 门禁拒绝（未声明 api）、停用注销后目标被拒。
    ///
    /// 请求投递依赖 MessageBus 的 dispatcher 路由（生产 = PluginHost），
    /// 本测试注入 TestInstanceDispatcher 把总线消息转发到共享实例。
    #[test]
    fn test_sdk_plugin_api_call_roundtrip() {
        const CALLER_ID: &str = "com.bedcode.api-caller";
        const TARGET_ID: &str = "com.bedcode.sdk-test";

        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_sdk_test_component())
            .expect("compile SDK test component");

        // 登记目标插件声明的 api（等价 PluginHost::activate_plugin 的登记）
        host_ctx.api_registry().register(
            TARGET_ID,
            &[
                "com.bedcode.sdk-test.echo".to_string(),
                "com.bedcode.sdk-test.fail".to_string(),
            ],
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let target = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, TARGET_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate target"),
            ));
            let caller = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, CALLER_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate caller"),
            ));

            // 注入消息投递器（生产为 PluginHost）：总线消息 → 插件实例 on_message
            let instances = Arc::new(RwLock::new(HashMap::from([
                (TARGET_ID.to_string(), target.clone()),
                (CALLER_ID.to_string(), caller.clone()),
            ])));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
                .await;

            // 激活两实例：宏生成的 register() 订阅请求 topic（宿主订阅去重）
            target.lock().await.activate().expect("target activate");
            caller.lock().await.activate().expect("caller activate");

            // 请求/响应配对成功：caller 经 JSON-RPC 调目标 echo
            let result = caller
                .lock()
                .await
                .invoke_command("test_api_echo", r#"{"text":"hi"}"#)
                .expect("test_api_echo");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["echo"], "echo: hi", "got: {}", result);

            // 错误传播：目标方法返回 error → JSON-RPC error 对象 → 调用方报错
            let result = caller
                .lock()
                .await
                .invoke_command("test_api_fail", "{}")
                .expect("test_api_fail");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"].as_str().map(|e| e.contains("boom")).unwrap_or(false),
                "fail error must propagate, got: {}",
                result
            );

            // 门禁拒绝：未声明的 api（ghost 不在注册表）在发布前被拒，不等待
            let result = caller
                .lock()
                .await
                .invoke_command("test_api_undeclared", "{}")
                .expect("test_api_undeclared");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
                "undeclared api must be rejected by gate, got: {}",
                result
            );

            // 超时：目标声明并订阅了 no-response topic（模拟构建期不可能出现的
            // 声明未实现场景），分派器不处理 → 不回复 → 调用方 800ms 超时
            host_ctx
                .api_registry()
                .register(TARGET_ID, &["com.bedcode.sdk-test.no-response".to_string()]);
            host_ctx
                .message_bus
                .subscribe_wasm(TARGET_ID, "bedcode.api.com.bedcode.sdk-test.no-response")
                .await;
            let result = caller
                .lock()
                .await
                .invoke_command("test_api_timeout", "{}")
                .expect("test_api_timeout");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"].as_str().map(|e| e.contains("timeout")).unwrap_or(false),
                "no-reply target must time out, got: {}",
                result
            );

            // 停用注销：目标 api 从注册表移除后，调用被门禁拒绝（验收「未激活
            // 插件目标调用被拒」；注销由 PluginHost::deactivate_plugin 执行，
            // 此处等价手动注销）
            host_ctx.api_registry().unregister(TARGET_ID);
            let result = caller
                .lock()
                .await
                .invoke_command("test_api_echo", r#"{"text":"again"}"#)
                .expect("test_api_echo after unregister");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
                "unregistered target must be rejected, got: {}",
                result
            );
        });
    }

    /// v11 二进制总线端到端（票据 06 补齐 SDK 二进制发布/订阅缺口后覆盖）：
    /// 同一 SDK 组件以两个实例加载——发布方经 SDK `bus_publish_binary` 发字节列，
    /// 订阅方 activate 内以 SDK `bus_subscribe_binary` 声明二进制偏好；断言订阅方
    /// `on_message_binary` 回调收到的 topic/sender/字节列与发布完全一致（含非
    /// UTF-8）。覆盖「SDK 通道 → 总线格式过滤 → 宿主 dispatcher → guest
    /// events-binary 回调」全链。
    #[test]
    fn test_sdk_plugin_binary_bus_roundtrip() {
        const PUB_ID: &str = "com.bedcode.bin-pub";
        const SUB_ID: &str = "com.bedcode.bin-sub";
        const TOPIC: &str = "sdk:binary-topic";

        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_sdk_test_component())
            .expect("compile SDK test component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let publisher = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, PUB_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate publisher"),
            ));
            let subscriber = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, SUB_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate subscriber"),
            ));

            let instances = Arc::new(RwLock::new(HashMap::from([
                (PUB_ID.to_string(), publisher.clone()),
                (SUB_ID.to_string(), subscriber.clone()),
            ])));
            host_ctx
                .message_bus
                .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
                .await;

            // 激活：SDK activate 内以二进制偏好订阅 TOPIC（订阅为异步投递，稍候生效）
            publisher.lock().await.activate().expect("publisher activate");
            subscriber.lock().await.activate().expect("subscriber activate");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            // 非 UTF-8 + 边界字节（0x00 / 0xff / 0x80）
            let payload: Vec<u8> = vec![0x00, 0xff, 0x80, b'b', b'i', b'n', 0x7f];
            let result = publisher
                .lock()
                .await
                .invoke_command(
                    "test_binary_publish",
                    &serde_json::json!({ "topic": TOPIC, "bytes": payload }).to_string(),
                )
                .expect("test_binary_publish");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(r["published"], payload.len(), "publish result: {}", result);

            // 总线投递为异步：轮询订阅方记录直到命中（上限 2s）
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            let received = loop {
                let result = subscriber
                    .lock()
                    .await
                    .invoke_command("test_binary_received", "{}")
                    .expect("test_binary_received");
                let r: serde_json::Value = serde_json::from_str(&result).unwrap();
                if !r["received"].is_null() {
                    break r["received"].clone();
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "subscriber never received binary message, last: {}",
                    result
                );
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            };

            assert_eq!(received["topic"], TOPIC, "got: {}", received);
            assert_eq!(received["sender"], PUB_ID, "got: {}", received);
            assert_eq!(
                received["bytes"],
                serde_json::json!(payload),
                "guest-received bytes must match published payload exactly, got: {}",
                received
            );
        });
    }

    /// 加载入口：load_plugin_from_file 直接走组件路径（阶段 C 起仅组件形态）
    #[test]
    fn test_load_plugin_from_file() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let temp_dir = std::env::temp_dir().join(format!("bedcode_component_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("plugin.wasm");
        std::fs::write(&wasm_path, build_test_component()).unwrap();

        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("load_plugin_from_file should load component");
        // 加载成功即可调用：激活 + manifest 往返验证组件路径
        assert_eq!(plugin.activate().expect("activate"), 0);
        let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.component-test");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 缓存 key：源码大小变化必须换 key（防解压器保留旧 mtime 时误加载旧产物）
    #[test]
    fn test_aot_cache_key_factors_source_size() {
        let path = std::path::Path::new("plugin.wasm");
        assert_eq!(aot_cache_key(path, 100), aot_cache_key(path, 100));
        assert_ne!(aot_cache_key(path, 100), aot_cache_key(path, 200));
    }

    /// 组件 AOT 缓存：产物写入、缓存命中、两次实例化等价
    #[test]
    fn test_compile_component_from_file_aot_cache() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();

        let temp_dir = std::env::temp_dir().join(format!("bedcode_component_aot_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("test_component.wasm");
        // 组件缓存文件名带 c 前缀（与 core module 产物区分）
        let wasm_bytes = build_test_component();
        let cache_path = std::env::temp_dir()
            .join(format!("bedcode_aot_{}", std::process::id()))
            .join(format!(
                "c{:016x}.cwasm",
                aot_cache_key(&wasm_path, wasm_bytes.len() as u64)
            ));
        std::fs::write(&wasm_path, &wasm_bytes).unwrap();

        // 首次编译：生成缓存产物
        let component = wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("first compile should succeed");
        assert!(cache_path.exists(), "component AOT cache file should be written");

        // 再次加载：命中缓存（产物不被重写，mtime 不变）——重编译路径会重写产物
        let cache_mtime_before = std::fs::metadata(&cache_path).unwrap().modified().unwrap();
        let cached = wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("cached load should succeed");
        let cache_mtime_after = std::fs::metadata(&cache_path).unwrap().modified().unwrap();
        assert_eq!(
            cache_mtime_before, cache_mtime_after,
            "cache hit should not rewrite artifact"
        );

        for c in [component, cached] {
            wasm_runtime
                .instantiate_component(&c, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("component from cache should instantiate");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 组件 AOT 缓存：产物损坏时反序列化失败并回退到完整编译
    ///
    /// 对应 core 路径的 `recompiles_on_stale` 测试；组件缓存文件名带 c 前缀
    #[test]
    fn test_compile_component_from_file_recompiles_on_stale() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();

        let temp_dir = std::env::temp_dir().join(format!("bedcode_component_aot_stale_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("test_component.wasm");
        let cache_path = std::env::temp_dir()
            .join(format!("bedcode_aot_{}", std::process::id()))
            .join(format!(
                "c{:016x}.cwasm",
                aot_cache_key(&wasm_path, build_test_component().len() as u64)
            ));
        std::fs::write(&wasm_path, build_test_component()).unwrap();

        // 首次编译生成缓存
        wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("first compile should succeed");

        // 篡改缓存为无效字节：deserialize 应失败并回退到完整编译
        std::fs::write(&cache_path, b"not a valid cwasm").unwrap();
        let component = wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("invalid cache should fall back to full compile");
        wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("component from full compile should instantiate");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }


    /// 燃料看门狗：guest 执行必须消耗燃料（组件形态下 fuel 生效），
    /// 且每次导出调用前重置预算（预算不跨调用累积）
    #[test]
    fn test_component_fuel_watchdog() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");

        // 调用前剩余燃料 ≈ 单次预算（实例化/ABI 校验的消耗可忽略）
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let before = {
                let (store, _) = plugin.raw_store();
                store.get_fuel().expect("get fuel")
            };
            plugin
                .invoke_command("test.echo", r#"{"hello":"x"}"#)
                .expect("invoke_command");
            let after = {
                let (store, _) = plugin.raw_store();
                store.get_fuel().expect("get fuel")
            };
            assert!(
                after < before,
                "guest execution must consume fuel (before={}, after={})",
                before,
                after
            );

            // 预算重置：人为耗尽燃料后再调用——exports() 必须自动续费使其成功
            {
                let (store, _) = plugin.raw_store();
                store.set_fuel(1000).expect("drain fuel");
            }
            plugin
                .invoke_command("test.echo", r#"{"hello":"z"}"#)
                .expect("refueled invoke must succeed");
            let after2 = {
                let (store, _) = plugin.raw_store();
                store.get_fuel().expect("get fuel")
            };
            assert!(
                after2 > StoreLimits::default().fuel_per_call / 2,
                "fuel must be refilled per export call, got {}",
                after2
            );
        });
    }

    /// 票据 02 验收：运行时覆盖生效——覆盖后新建立的 Store 按新上限运行；
    /// 资源限制器行为等价：超限增长被拒绝，限额来自配置
    #[test]
    fn test_runtime_config_override_applies_to_new_stores() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");

        // 覆盖：收紧内存上限到 4MiB（须高于测试组件最小内存 17 页 ≈ 1.1MiB）
        let mut cfg = CoreConfig::default();
        cfg.store.max_memory_bytes = 4 * 1024 * 1024;
        wasm_runtime.set_config(cfg).expect("set config");

        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate under overridden limits");
        let (store, _) = plugin.raw_store();
        // 限额内允许、超限拒绝——限额来自运行时覆盖的配置
        assert!(store.data_mut().memory_growing(0, 3 * 1024 * 1024, None).expect("within limit"));
        assert!(!store.data_mut().memory_growing(0, 5 * 1024 * 1024, None).expect("over limit"));
        assert_eq!(store.data().limits.max_memory_bytes, 4 * 1024 * 1024);
    }

    /// 票据 02 验收：非法配置（燃料为 0）被 set_config 拒绝且带上下文；旧配置不受影响
    #[test]
    fn test_set_config_rejects_invalid() {
        let (wasm_runtime, _host_ctx) = setup_wasm_runtime();
        let mut cfg = CoreConfig::default();
        cfg.store.fuel_per_call = 0;
        let err = wasm_runtime.set_config(cfg).expect_err("zero fuel must be rejected");
        assert!(format!("{err}").contains("fuel_per_call"));
        assert_eq!(wasm_runtime.config(), CoreConfig::default());
    }

    /// 票据 07 验收：manifest 资源覆盖经安全模块仲裁后作用于新建 Store——
    /// 收紧请求生效（限额来自仲裁结果）、放宽请求钳回内核配置
    #[test]
    fn test_plugin_resource_overrides_apply_to_new_stores() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let base = StoreLimits::default();

        // 收紧请求：内存上限 4MiB（须高于测试组件最小内存 17 页 ≈ 1.1MiB）→ 生效
        let tightened = ResourceOverrides {
            max_memory_bytes: Some(4 * 1024 * 1024),
            ..ResourceOverrides::default()
        };
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], Some(&tightened))
            .expect("instantiate with tightened limits");
        {
            let (store, _) = plugin.raw_store();
            assert_eq!(
                store.data().limits.max_memory_bytes,
                4 * 1024 * 1024,
                "Store 限额必须来自仲裁后的插件覆盖值"
            );
            // 限额内允许、超限拒绝——限额来自仲裁结果
            assert!(store.data_mut().memory_growing(0, 3 * 1024 * 1024, None).expect("within limit"));
            assert!(!store.data_mut().memory_growing(0, 5 * 1024 * 1024, None).expect("over limit"));
        }

        // 放宽请求：钳回内核配置（插件不得借 manifest 突破运维上限）
        let relaxed = ResourceOverrides {
            max_memory_bytes: Some(base.max_memory_bytes * 2),
            ..ResourceOverrides::default()
        };
        let mut relaxed_plugin = wasm_runtime
            .instantiate_component(&component, "com.bedcode.relaxed", host_ctx, &[], Some(&relaxed))
            .expect("instantiate with relaxed limits");
        let (store, _) = relaxed_plugin.raw_store();
        assert_eq!(
            store.data().limits.max_memory_bytes,
            base.max_memory_bytes,
            "放宽请求必须被钳回内核配置值"
        );
    }

    /// 票据 03 验收：调用聚合（次数/燃料/耗时）+ 生命周期事件计数 + 内存记账
    #[test]
    fn test_monitor_metrics_end_to_end() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");
        let monitor = wasm_runtime.monitor();

        plugin.activate().expect("activate");
        plugin.invoke_command("test.echo", r#"{"n":1}"#).expect("invoke 1");
        plugin.invoke_command("test.echo", r#"{"n":2}"#).expect("invoke 2");

        // 内存记账：limiter 批准路径写入当前值/峰值（在真实增长之后注入，
        // 避免后续真实增长把 current 刷回实际值——真实内存 ~1.1MiB < 2MiB）
        {
            let (store, _) = plugin.raw_store();
            use wasmtime::ResourceLimiter;
            store
                .data_mut()
                .memory_growing(0, 2 * 1024 * 1024, None)
                .expect("within limit");
        }

        let snap = monitor.snapshot();
        let m = &snap["plugins"][TEST_PLUGIN_ID];
        assert_eq!(m["lifecycle"]["instantiate"].as_u64().unwrap(), 1);
        assert_eq!(m["lifecycle"]["activate_ok"].as_u64().unwrap(), 1);
        assert_eq!(m["calls_total"].as_u64().unwrap(), 3, "activate + 2×invoke = 3 次导出调用");
        assert!(m["fuel_consumed_total"].as_u64().unwrap() > 0, "燃料消耗必须有记录");
        assert_eq!(
            m["call_duration_buckets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_u64().unwrap())
                .sum::<u64>(),
            3,
            "直方图桶计数必须等于调用数"
        );
        assert_eq!(m["memory_current_bytes"].as_u64().unwrap(), 2 * 1024 * 1024);
        assert_eq!(m["memory_peak_bytes"].as_u64().unwrap(), 2 * 1024 * 1024);
    }

    /// 燃料耗尽必须 trap：绕过 exports() 的自动续费，直接以小预算调用导出
    #[test]
    fn test_component_fuel_exhaustion_traps() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");

        let (store, instance) = plugin.raw_store();
        store.set_fuel(1).expect("set tiny fuel");
        let binding = super::component::Plugin::new(&mut *store, instance).expect("bind exports");
        // 票 02：导出绑定全部 async（bindgen `default: async`），燃料耗尽 trap 经
        // call_async 在 async 语义下生效（/tmp/wasip3-probe 场景 3 实证）
        let result = block_on_async(async {
            binding
                .bedcode_plugin_command()
                .call_invoke(store, "test.echo", r#"{"a":1}"#)
                .await
        });
        assert!(result.is_err(), "fuel exhausted must trap: {:?}", result);
    }

    /// ticket 01（wasm backtrace）：trap 错误串携带 WASM 内部函数调用栈
    ///
    /// 显式 panic 走生产 Engine（WasmRuntime::new 已开 wasm_backtrace_max_frames
    /// 32 帧）——错误串必须含 `wasm backtrace:` 且栈穿透到业务函数 invoke
    /// （names section，release 构建即有），AI agent 无需重跑即可从错误串
    /// 定位插件内部故障点
    #[test]
    fn test_component_trap_error_includes_wasm_backtrace() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(async { plugin.invoke_command("test.panic", "{}").expect_err("panic must trap").to_string() });
        assert!(
            err.contains("wasm backtrace:"),
            "trap error must include wasm backtrace marker, got: {}",
            err
        );
        // 栈内含插件业务函数名（invoke 是 command 导出实现），证明函数级可读
        assert!(
            err.contains("invoke"),
            "trap backtrace must include plugin function name, got: {}",
            err
        );
    }

    /// 回归：开启 backtrace 不改变正常调用行为（非 trap 路径零影响）
    #[test]
    fn test_component_backtrace_enabled_normal_calls_unaffected() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let echo = rt.block_on(async {
            plugin
                .invoke_command("test.echo", r#"{"hello":"backtrace-on"}"#)
                .expect("normal invoke must succeed with backtrace enabled")
        });
        assert!(echo.contains("backtrace-on"), "got: {}", echo);
        // 正常返回的 JSON 载荷不应混入 backtrace 文本（非 trap 路径零影响）
        assert!(!echo.contains("wasm backtrace:"), "got: {}", echo);
    }

    /// ticket 03（调试模式端到端冒烟）：后台日志行号栈
    ///
    /// 仅当宿主持有 `BEDCODE_PLUGIN_DEBUG=1` 时有效（此时构建链路以 debug
    /// profile 产出带 DWARF 的测试组件，WasmRuntime 也已置 WASMTIME_BACKTRACE_DETAILS
    /// 开行号解析）——断言 trap 错误串含 `file:line` 行号而非仅函数名。
    /// 未设该开关的正常测试环境自动跳过（SKIP 输出，不失败）
    #[test]
    fn test_debug_mode_trap_includes_line_info() {
        if !plugin_debug_mode() {
            eprintln!("SKIP: BEDCODE_PLUGIN_DEBUG 未设置，跳过行号冒烟（调试模式是手工开关）");
            return;
        }
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile debug test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate debug test component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(async { plugin.invoke_command("test.panic", "{}").expect_err("panic must trap").to_string() });
        assert!(
            err.contains(".rs:"),
            "debug backtrace should include file:line symbols, got: {}",
            err
        );
    }

    /// ticket 02（trap 宿主日志）：trap 时宿主侧产生含 plugin_id 的 error 级记录
    ///
    /// 即使调用方静默忽略返回错误，崩溃证据也经 tracing error 落盘；
    /// trap 详情（含 wasm backtrace）作为结构化字段随日志携带
    #[test]
    fn test_component_trap_emits_host_error_log() {
        use crate::plugin::manager::wasm_runtime::host_impl::log::capture::{capture, CapturedEvent};
        use tracing::Level;

        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");

        let captured = capture(|| {
            // 通过 invoke_command 触发显式 panic（确定性 trap，栈穿透到 invoke）
            let _ = plugin.invoke_command("test.panic", "{}");
        });

        let errors: Vec<&CapturedEvent> = captured.iter().filter(|e| e.level == Level::ERROR).collect();
        assert!(
            !errors.is_empty(),
            "trap must emit host error log, captured: {:?}",
            captured
        );
        let host_error = errors[0];
        let fields: std::collections::HashMap<&str, &str> = host_error
            .fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(fields.get("plugin_id"), Some(&TEST_PLUGIN_ID), "got: {:?}", fields);
        assert_eq!(fields.get("export"), Some(&"invoke_command"));
        assert!(
            fields.get("trap").map(|t| t.contains("wasm backtrace:")).unwrap_or(false),
            "trap field should carry wasm backtrace, got: {:?}",
            fields
        );
    }

    /// ticket 02：guest 自报失败（内层 Err）不升级为宿主 error
    ///
    /// 双层 Result 语义：Ok(Err(msg)) 是插件自己报告的失败，按既有级别（warn/返回）
    /// 记录，不产生宿主 error 日志——只有真 trap（外层 Err）才走 error 证据路径
    #[test]
    fn test_component_guest_self_reported_failure_no_host_error() {
        use crate::plugin::manager::wasm_runtime::host_impl::log::capture::{capture, CapturedEvent};
        use tracing::Level;

        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");

        let captured = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                // 预写 storage key：guest on_startup 读到后返回 Err（见测试插件实现）
                host_ctx
                    .storage
                    .set(
                        TEST_PLUGIN_ID,
                        "component-test-fail-startup",
                        serde_json::json!("x"),
                    )
                    .await
                    .expect("preset failing-startup key");
                let mut plugin = wasm_runtime
                    .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
                    .expect("instantiate test component");
                capture(|| {
                    let result = plugin.on_startup();
                    assert!(
                        matches!(result, Ok(Err(_))),
                        "guest should self-report startup failure, got: {:?}",
                        result
                    );
                })
            })
        };

        assert!(
            !captured.iter().any(|e| e.level == Level::ERROR),
            "guest self-reported failure must not emit host error, captured: {:?}",
            captured
        );
    }

    /// trap 后 Store 被污染：同一实例后续调用持续报 `cannot enter component instance`
    /// （wasmtime 同步引擎 `set_trapped` 语义，宿主 trap 自动重载机制的立论依据）
    #[test]
    fn test_component_trap_poisons_store_and_reinstantiate_recovers() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");

        // 1. 实例 A：制造一次 trap（燃料耗尽）
        let mut plugin_a = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("instantiate component A");
        {
            let (store, instance) = plugin_a.raw_store();
            store.set_fuel(1).expect("set tiny fuel");
            let binding = super::component::Plugin::new(&mut *store, instance).expect("bind exports");
            let result = block_on_async(async {
                binding
                    .bedcode_plugin_command()
                    .call_invoke(store, "test.echo", r#"{"a":1}"#)
                    .await
            });
            assert!(result.is_err(), "fuel exhausted must trap: {:?}", result);
        }

        // 2. 同一实例再次调用：必须持续失败且报 cannot enter component instance
        //    （不能自愈 —— 这正是宿主必须整体重载的原因）
        let err = {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                plugin_a
                    .invoke_command("test.echo", r#"{"a":2}"#)
                    .expect_err("poisoned store must keep failing")
            })
        };
        assert!(
            err.to_string().contains("cannot enter component instance"),
            "poisoned store error should be CannotEnterComponent, got: {}",
            err
        );

        // 3. 重新实例化（等价宿主 reload_wasm_plugin 的重建）→ 新实例正常可用
        let mut plugin_b = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("re-instantiate after trap");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let echo = rt.block_on(async {
            plugin_b
                .invoke_command("test.echo", r#"{"hello":"recovered"}"#)
                .expect("fresh instance must work")
        });
        assert!(echo.contains("recovered"), "got: {}", echo);
    }

    #[test]
    fn block_on_async_reentrant_nested_call_no_panic() {
        // 回归（panic.log 实证 wasm_runtime.rs:82 FATAL）：
        // 插件分发路径 dispatch_*_to_plugin → block_on_async（block_in_place +
        // handle.block_on）包着插件调用，插件回调里的宿主函数（session_get /
        // config_get / db 查询等）再调 block_on_async 构成重入。旧实现重入分支
        // 直接 handle.block_on —— 外层 block_on 的 enter 守卫仍挂在当前线程上，
        // 必然 panic（Cannot start a runtime from within a runtime），panic 穿透
        // 污染 wasmtime Store 导致插件 trap → 重载循环 → 插件整体失效。
        // 修复：重入分支改在新线程上 block_on，此处验证重入可返回且嵌套 future
        // 真正挂起（sleep）时也能被 runtime 唤醒（无死锁）。
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // tokio::spawn：模拟真实分发在 worker 线程执行（block_in_place 前置条件）
            tokio::spawn(async move {
                // 外层 block_on_async：模拟 dispatch_*_to_plugin 的同步桥接
                let outer = block_on_async(async {
                    // 内层 block_on_async：模拟插件回调内的宿主函数调用（重入分支）
                    let inner = block_on_async(async {
                        // 真实挂起：验证新线程上的 block_on 能被 runtime 定时器唤醒
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        42u32
                    });
                    inner * 2
                });
                assert_eq!(outer, 84);
            })
            .await
            .expect("spawned task must not panic");
        });
    }
}
