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

/// 声明展开（不过滤授权，preauthorize 收集弹窗候选用，见 host.rs `preauthorize_plugin`）
pub(crate) use component::expand_preopen_declarations;
/// WASI 预打开目录解析（激活时重建实例判定用，见 host.rs `rebuild_wasm_instance`）
pub(crate) use component::resolve_preopen_dirs;
pub use component::LoadedWasmPlugin;

use crate::db::Database;
use crate::plugin::manager::storage::PluginStorage;
use crate::plugin::permission::PermissionManager;
use crate::plugin::security::fs_auth::FsAuthChecker;
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
use crate::plugin::monitor::{LifecycleEvent, MetricsRegistry, PluginMetrics};
use bedcode_plugin_api::ResourceOverrides;

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

    /// 分发宿主并发任务事件到插件（host-task，v20）
    ///
    /// 由 core-task 消费派发任务调用（每插件单线程串行，实例锁天然要求）：
    /// 经可选导出 `events-task#on-task-event` 投递 `{ jobId, phase, ... }`；
    /// 未导出（旧 SDK 产物）时降级 `Ok(false)` → 事件丢弃 + 计数（宿主不缓存，
    /// `status` / `list-jobs` 自愈）。插件未激活/已卸载时调用失败仅记日志
    /// （尽力而为，同 dispatch_process_done）。
    fn dispatch_task_event(&self, plugin_id: String, event: serde_json::Value);

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
    /// 插件私有库根目录覆盖（布局 `<root>/<plugin_id>/plugin.db`）
    ///
    /// 生产为 `None`：走 `app_handle` 的 `app_data_dir()/plugins/<plugin_id>`；
    /// 无头测试经此注入（tao 事件循环不允许在测试线程建 AppHandle，故测试只能
    /// 经注入点拿到真实私有库——票 08 的 S1 闭环需要它）。
    plugin_db_root: Option<PathBuf>,
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
            // 私有库根目录覆盖：生产 None（走 app_handle 的 app_data_dir），
            // 无头测试在构造后注入（见 setup_wasm_runtime）
            plugin_db_root: None,
        }
    }

    /// 获取进程注册表引用（host-process）
    /// 文件系统访问校验器（票 07：闭环用例预置「已记住」授权记录的唯一入口）
    pub(crate) fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }

    /// 权限管理器（core-task 单元预检读声明闸门用，见 `manager::task`）
    pub(crate) fn permission(&self) -> &Arc<crate::plugin::permission::PermissionManager> {
        &self.permission
    }

    pub fn process_registry(&self) -> &Arc<ProcessRegistry> {
        &self.process_registry
    }

    /// 获取主库句柄（宿主侧读写内核表的入口）
    ///
    /// 生产路径经各 host_impl 域函数访问（权限门 + 属主校验在域函数内）；
    /// 本访问器供宿主装配/测试直接读写内核真源（如 `pairings` 表播种与断言，
    /// 见 `host::tests::test_server_auth_policy_closed_loop`）。
    pub(crate) fn database(&self) -> &Arc<Mutex<Database>> {
        &self.db
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
        host_impl::api::api_call(
            self,
            host_impl::api::HOST_API_CALLER_ID,
            request_topic,
            payload_json,
            timeout_ms,
        )
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

        // 慢路径：创建数据库。数据目录来源二选一（`aot_cache_dir` 同模式）：
        // - 生产：`app_handle` 派生 `app_data_dir()/plugins/<plugin_id>`
        // - 无头测试：`plugin_db_root` 注入（tao 事件循环不允许在测试线程建
        //   AppHandle，故无头上下文必须显式给根目录才能测插件私有库）
        let plugin_dir = match (&self.app_handle, &self.plugin_db_root) {
            (Some(app_handle), _) => {
                let app_data_dir = app_handle
                    .path()
                    .app_data_dir()
                    .map_err(|e| crate::AppError::Plugin(format!("Failed to get app data dir: {}", e)))?;
                app_data_dir.join("plugins").join(plugin_id)
            }
            (None, Some(root)) => root.join(plugin_id),
            (None, None) => {
                return Err(crate::AppError::Plugin(
                    "plugin database unavailable in headless context (no app_handle)".to_string(),
                ))
            }
        };

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

    // A0-3 前置探针（P1/P2/P5）：async store 兼容性 + 资源限制 async 语义 + 性能基线。
    // 文档：.scratch/2026-09-21-a0-3-host-async/spec.md + report.md（只读探针，不碰生产路径）
    mod a03_probe;
    // 终端输出消费插件化性能前置验证（P1-P3，只读探针；文档 .scratch/2026-09-21-terminal-output-consumer-perf/）
    mod terminal_output_perf;
    // 域拆分（P0）：测试函数自本文件拆至 wasm_runtime/tests/，共享脚手架留在下方；
    // 各域文件 `use super::*` 复用，fixture 互斥与产物构建语义不变
    mod component_e2e;
    mod engine_limits;
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
            let session_manager = Arc::new(SessionManager::new_with_handlers(Arc::new(
                crate::pty::PtySessionHandler::new(),
            )));

            let config_manager = Arc::new(SessionConfigManager::new(kernel_db));

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
                session_manager,
                config_manager,
                None,
                permission,
                wasm_runtime.fs_auth().clone(),
                message_bus,
            );
            // 注入插件私有库根目录（无头上下文无 AppHandle，见字段文档）：
            // 票 08 的 S1 闭环需要真实私有库（host-plugin-database）
            host_ctx.plugin_db_root = Some(plugin_db_root());
            let host_ctx = Arc::new(host_ctx);

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
        fn register_session_lifecycle_listener(&self, _plugin_id: String, _session_manager: Arc<SessionManager>) {}
        fn register_session_input_listener(&self, _plugin_id: String, _session_manager: Arc<SessionManager>) {}
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
}
