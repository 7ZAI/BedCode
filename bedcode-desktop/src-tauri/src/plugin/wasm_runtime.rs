//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 组件（Component Model）加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//!
//! 宿主能力实现位于 [`host_impl`] 子模块（权限校验 + 宿主服务调用），
//! 组件绑定与实例化位于 [`component`] 子模块，本模块只负责运行时生命周期
//! 管理与宿主上下文定义

mod component;
mod host_impl;

pub use component::LoadedWasmPlugin;

use crate::db::Database;
use crate::plugin::file_service::FileServiceRegistry;
use crate::plugin::fs_auth::FsAuthChecker;
use crate::plugin::permission::PermissionManager;
use crate::plugin::storage::PluginStorage;
use crate::session::{SessionConfigManager, SessionManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use wasmtime::{Cache, CacheConfig, Config, Engine, ResourceLimiter};

// ==================== Resource Limits & Interruption ====================

/// 单次 wasm 导出调用允许消耗的燃料（指令数）——防失控/恶意插件无限执行
///
/// 用燃料（fuel）而非 epoch 墙钟窗口做看门狗：
/// - 燃料只计 guest 指令数，宿主调用阻塞期间（授权弹窗、目录扫描、网络）
///   guest 零消耗——慢宿主调用无论多久都不会被误杀；epoch 按墙钟计，
///   宿主阻塞期间照走，正是历史上误杀慢调用的根因（见组件迁移期间
///   filesrv_mount 阻塞 >2s 被 trap 的回归）
/// - 纯 guest 死循环持续烧燃料，必然耗尽被 trap（确定性，不受宿主负载影响）
/// - 每次导出调用前重置燃料（见 component::exports），预算只约束单次调用内
///   guest 计算量，与宿主延迟彻底解耦
/// 64G 指令 ≈ 数十秒纯 guest 计算（wasm32 release 约 1-3G 指令/秒），
/// 覆盖大 JSON 解析等重活；死循环最迟烧完被 trap
const FUEL_PER_CALL: u64 = 64_000_000_000;
/// 单插件线性内存上限（字节）——防失控/恶意插件耗尽宿主内存
const MAX_PLUGIN_MEMORY_BYTES: usize = 256 * 1024 * 1024;
/// 单插件表元素上限
const MAX_PLUGIN_TABLE_ENTRIES: usize = 1_000_000;

// ==================== Async Blocking Helper ====================

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
thread_local! {
    /// 当前线程是否已处于 block_in_place 让出后的阻塞上下文
    static IN_BLOCK_IN_PLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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

pub(crate) fn block_on_async<F, R>(fut: F) -> R
where
    F: std::future::Future<Output = R> + Send,
    R: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
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
            // current_thread 运行时（如 Actix-rt）或未来新增变体：
            // 在新线程上执行 block_on，避免 block_in_place panic
            std::thread::scope(|s| {
                s.spawn(|| handle.block_on(fut))
                    .join()
                    .expect("block_on_async: spawned thread panicked")
            })
        }
    }
}

/// WASM 插件运行时（全局共享）
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
}

/// 插件实例资源限制器
///
/// 直接借用 Store 状态（`Store::limiter` 的闭包返回本状态的可变引用），
/// 限制单插件线性内存与表大小，防止失控/恶意插件耗尽宿主内存。
impl ResourceLimiter for WasmPluginState {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > MAX_PLUGIN_MEMORY_BYTES {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_bytes = desired,
                max_bytes = MAX_PLUGIN_MEMORY_BYTES,
                "WASM memory growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }

    fn table_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > MAX_PLUGIN_TABLE_ENTRIES {
            tracing::warn!(
                plugin_id = %self.plugin_id,
                desired_entries = desired,
                max_entries = MAX_PLUGIN_TABLE_ENTRIES,
                "WASM table growth denied by resource limiter"
            );
            Ok(false)
        } else {
            Ok(true)
        }
    }
}

/// 插件宿主服务抽象 — 解耦 WasmHostContext 与 PluginHost 的循环依赖
///
/// WasmHostContext 需要回调插件宿主（注册会话生命周期监听器），
/// 而 PluginHost 持有 WasmHostContext —— 通过 trait 对象 + 两阶段注入打破类型互引：
/// 本模块只依赖此 trait，`PluginHost` 在 `plugin::host` 模块中实现它
pub trait PluginServices: Send + Sync + 'static {
    /// 为指定插件创建并注册会话生命周期监听器到 SessionManager
    fn register_session_lifecycle_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    );

    /// 为指定插件创建并注册提交输入行监听器到 SessionManager（见 ADR 0001）
    fn register_session_input_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    );

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
    session_manager: Arc<SessionManager>,
    /// 会话配置管理器 — 用于获取所有会话配置（working_dir 等）
    config_manager: Arc<SessionConfigManager>,
    /// Tauri AppHandle（无头/测试上下文为 None，emit/路径类宿主能力降级）
    app_handle: Option<Arc<tauri::AppHandle>>,
    permission: Arc<PermissionManager>,
    fs_auth: Arc<FsAuthChecker>,
    message_bus: Arc<crate::plugin::message_bus::MessageBus>,
    /// 文件服务注册表（挂载/沙箱/上传会话/钩子分发）
    ///
    /// 在 PluginHost::new() 中早于插件 auto-activate 创建并注入，插件激活阶段
    /// （AppContext 全局可能尚未初始化）host_filesrv_mount 即可用
    file_service: Arc<FileServiceRegistry>,
    /// 插件宿主服务（两阶段初始化，避免 PluginHost 与 WasmHostContext 类型互引）
    plugin_services: Arc<RwLock<Option<Arc<dyn PluginServices>>>>,
    /// 运行中进程注册表（host-process，v8）：run_id → 进程句柄
    ///
    /// host_impl/process.rs 注册/移除；kill（超时/取消）经此查找句柄。
    process_registry: Arc<ProcessRegistry>,
    /// 插件互调 api 注册表（ADR-0017）：激活登记 / 停用注销，
    /// `bus_publish` 对 `bedcode.api.*` 请求 topic 做目标校验
    api_registry: Arc<crate::plugin::api_registry::ApiRegistry>,
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
        runs.insert(
            run_id,
            RunningProcess { plugin_id, pid },
        );
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
        match cmd.output().await
        {
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
        // 负 pid 表示进程组；process_group(0) 后组 id == 进程 pid
        match tokio::process::Command::new("kill")
            .args(["-9", &format!("-{}", pid)])
            .output()
            .await
        {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                tracing::warn!(
                    pid,
                    output = %String::from_utf8_lossy(&o.stderr),
                    "kill_process_group: kill reported failure"
                );
                false
            }
            Err(e) => {
                tracing::warn!(pid, error = %e, "kill_process_group: kill failed");
                false
            }
        }
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
    pub fn new(
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> crate::Result<Self> {
        let mut config = Config::new();
        // 燃料看门狗：guest 指令计数耗尽即 trap（宿主调用阻塞不消耗，见 FUEL_PER_CALL）
        config.consume_fuel(true);
        // 编译缓存：跨进程复用已编译产物（初始化失败降级为不缓存，不阻断运行时）
        match Cache::new(CacheConfig::new()) {
            Ok(cache) => {
                config.cache(Some(cache));
            }
            Err(e) => {
                tracing::warn!(error = %e, "WASM compile cache disabled");
            }
        }
        let engine = Engine::new(&config).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to initialize WASM engine: {}", e))
        })?;
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

        Ok(Self { engine, linker, fs_auth, aot_cache_dir })
    }

    /// 从字节流编译 WASM 组件（Component Model，迁移阶段 A）
    pub fn compile_component(&self, bytes: &[u8]) -> crate::Result<wasmtime::component::Component> {
        wasmtime::component::Component::from_binary(&self.engine, bytes).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to compile WASM component: {}", e))
        })
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
            .zip(
                std::fs::metadata(&cache_path)
                    .ok()
                    .and_then(|c| c.modified().ok()),
            )
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
                let write_result = std::fs::write(&tmp_path, &bytes)
                    .and_then(|_| std::fs::rename(&tmp_path, &cache_path));
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
    pub fn load_plugin_from_file(
        &self,
        path: &Path,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let bytes = std::fs::read(path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to read WASM artifact '{}': {}",
                path.display(),
                e
            ))
        })?;
        let component = self.compile_component(&bytes)?;
        self.instantiate_component(&component, plugin_id, host_ctx)
    }

    /// 实例化 WASM 组件
    ///
    /// 创建 Store + WasmPluginState，通过 linker 实例化，
    /// 校验 ABI 版本与形态字段（见 [`component::LoadedWasmPlugin::new`]）
    pub fn instantiate_component(
        &self,
        component: &wasmtime::component::Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let plugin = component::LoadedWasmPlugin::new(
            &self.engine,
            &self.linker,
            component,
            plugin_id,
            host_ctx,
        )?;
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
        message_bus: Arc<crate::plugin::message_bus::MessageBus>,
        file_service: Arc<FileServiceRegistry>,
    ) -> Self {
        Self {
            db,
            plugin_dbs,
            storage,
            session_manager,
            config_manager,
            app_handle,
            permission,
            fs_auth,
            message_bus,
            file_service,
            plugin_services: Arc::new(RwLock::new(None)),
            process_registry: Arc::new(ProcessRegistry::new()),
            api_registry: Arc::new(crate::plugin::api_registry::ApiRegistry::new()),
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
    pub fn message_bus(&self) -> &Arc<crate::plugin::message_bus::MessageBus> {
        &self.message_bus
    }

    /// 获取插件互调 api 注册表引用（ADR-0017 门禁）
    pub fn api_registry(&self) -> &Arc<crate::plugin::api_registry::ApiRegistry> {
        &self.api_registry
    }

    /// 获取文件服务注册表引用
    pub fn file_service(&self) -> &Arc<FileServiceRegistry> {
        &self.file_service
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
            crate::AppError::Plugin(
                "plugin database unavailable in headless context (no app_handle)".to_string(),
            )
        })?;
        let app_data_dir = app_handle.path().app_data_dir()
            .map_err(|e| crate::AppError::Plugin(format!("Failed to get app data dir: {}", e)))?;
        let plugin_dir = app_data_dir.join("plugins").join(plugin_id);

        // 创建插件数据目录
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir)
                .map_err(|e| crate::AppError::Plugin(format!(
                    "Failed to create plugin data dir '{}': {}",
                    plugin_dir.display(), e
                )))?;
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

    /// 创建 WasmRuntime + 宿主上下文（不实例化插件）
    ///
    /// 供需要独立编译/实例化组件的测试复用。
    /// 无头构建（app_handle = None）：tao 事件循环不允许在测试线程创建，
    /// emit/数据目录类能力在测试中不被调用路径覆盖；
    /// AOT 缓存目录注入到系统临时目录，保证 compile_component_from_file 走缓存路径。
    fn setup_wasm_runtime() -> (WasmRuntime, Arc<WasmHostContext>) {
        use crate::db::Database;
        use crate::plugin::file_service::FileServiceRegistry;
        use crate::plugin::message_bus::MessageBus;
        use crate::plugin::permission::PermissionManager;
        use crate::plugin::storage::PluginStorage;
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
            "storage", "broadcast", "terminal:input", "terminal:output",
            "session:read", "fs:read", "fs:write", "ui:sidebar",
        ];

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db = Database::new(&std::path::PathBuf::from(":memory:")).unwrap();
            db.init_schema().unwrap();
            let db = Arc::new(Mutex::new(db));

            let storage = Arc::new(PluginStorage::new(db.clone()));

            let resource_dir = Arc::new(std::path::PathBuf::from("."));
            let session_manager = Arc::new(
                SessionManager::from_database(
                    Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
                    resource_dir.clone(),
                )
            );

            let config_manager = Arc::new(
                SessionConfigManager::new(Arc::new(Mutex::new(
                    Database::new(&std::path::PathBuf::from(":memory:")).unwrap()
                )))
            );

            let permission = Arc::new(PermissionManager::new());
            permission.grant_permissions(TEST_PLUGIN_ID, &all_permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>());

            let message_bus = Arc::new(MessageBus::new());

            // 无头构建：不创建 AppHandle（tao 事件循环不允许在测试线程初始化）
            let mut wasm_runtime = WasmRuntime::new(storage.clone(), None).unwrap();
            // 注入 AOT 缓存目录（生产由 app_handle 派生，测试无头上下文手动注入）
            wasm_runtime.aot_cache_dir = Some(
                std::env::temp_dir().join(format!("bedcode_aot_{}", std::process::id())),
            );

            // 文件服务注册表与宿主上下文同步构造（headless：无 AppHandle）
            let file_service = FileServiceRegistry::new(wasm_runtime.fs_auth().clone(), None);

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
                file_service,
            ));

            (wasm_runtime, host_ctx)
        })
    }

    // ==================== Component Model 测试 ====================

    /// 将 wit-bindgen 产出的 core module 编码为组件
    ///
    /// 等价于 `wasm-tools component new`（WIT 元数据已由 wit-bindgen
    /// 嵌入 core module 的 component-type 自定义段）
    fn encode_component(module: &[u8]) -> Vec<u8> {
        let mut encoder = wit_component::ComponentEncoder::default();
        encoder
            .module(module)
            .expect("component encoder module")
            .encode()
            .expect("component encoder encode")
    }

    /// 构建测试用组件插件并编码为组件
    ///
    /// 测试插件为独立 crate（packages/plugin-component-test），基于
    /// WIT 契约（packages/plugin-sdk-desktop/rust/wit）生成绑定；
    /// 源码变更检测与 build_test_wasm 同策略
    fn build_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-component-test");

        let output_dir = plugin_dir.join("target/wasm32-unknown-unknown/release");
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
                return encode_component(
                    &std::fs::read(&module_path).expect("Failed to read test component module"),
                );
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .args([
                "build",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for test component");
        assert!(status.success(), "Test component WASM build failed");

        encode_component(
            &std::fs::read(&module_path).expect("Failed to read test component after build"),
        )
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
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
                .expect("instantiate test component");

            // 生命周期
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);

            // manifest
            let manifest: serde_json::Value =
                serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
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
            assert!(
                pdb_err.contains("headless"),
                "unexpected pdbQueryError: {}",
                pdb_err
            );

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
            plugin.on_startup().expect("on_startup");
            plugin.on_shutdown().expect("on_shutdown");

            // 上传钩子：fail-closed 决策 JSON
            let decision = plugin
                .on_upload_request(r#"{"name": "f.bin"}"#)
                .expect("on_upload_request");
            let decision_json: serde_json::Value = serde_json::from_str(&decision).unwrap();
            assert_eq!(decision_json["allow"], false);
        });
    }

    /// 构建 SDK 组件形态测试插件（packages/plugin-sdk-test）并编码为组件
    ///
    /// 与 build_test_component 的区别：插件经真实 SDK（wasm_entry! 宏 + WasmHost）
    /// 构建，验证迁移阶段 B 的 SDK 组件产物链路；源码变更检测覆盖 SDK 关键文件
    fn build_sdk_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-sdk-test");

        let output_dir = plugin_dir.join("target/wasm32-unknown-unknown/release");
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
                return encode_component(
                    &std::fs::read(&module_path)
                        .expect("Failed to read SDK test component module"),
                );
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .args([
                "build",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for SDK test component");
        assert!(status.success(), "SDK test component WASM build failed");

        encode_component(
            &std::fs::read(&module_path)
                .expect("Failed to read SDK test component after build"),
        )
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
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
                .expect("instantiate SDK test component");

            // 生命周期（宏生成的 lifecycle::Guest）
            assert_eq!(plugin.activate().expect("activate"), 0);
            assert_eq!(plugin.deactivate().expect("deactivate"), 0);

            // manifest（宏生成的 manifest::Guest）
            let manifest: serde_json::Value =
                serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
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

            // 终端钩子（宏生成的 terminal_hooks::Guest，大写转换语义）
            assert_eq!(
                plugin.on_terminal_input("session-1", "sdk input").unwrap(),
                Some("SDK INPUT".to_string())
            );

            // 上传钩子（宏生成的 upload_hook::Guest，默认 fail-closed）
            let decision = plugin
                .on_upload_request(r#"{"name": "f.bin"}"#)
                .expect("on_upload_request");
            let d: serde_json::Value = serde_json::from_str(&decision).unwrap();
            assert_eq!(d["allow"], false);
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

    impl crate::plugin::message_bus::MessageDispatcher for TestInstanceDispatcher {
        fn dispatch_to_wasm(
            &self,
            plugin_id: &str,
            msg: &bedcode_plugin_api::BusMessage,
        ) -> anyhow::Result<()> {
            let instances = self.instances.clone();
            let plugin_id = plugin_id.to_string();
            let msg = msg.clone();
            block_on_async(async move {
                let instances = instances.read().await;
                let plugin = instances.get(&plugin_id).ok_or_else(|| {
                    anyhow::anyhow!("TestInstanceDispatcher: no instance '{}'", plugin_id)
                })?;
                let mut plugin = plugin.lock().await;
                plugin
                    .on_message(&msg.topic, &msg.sender, &msg.payload)
                    .map_err(|e| anyhow::Error::from(e))
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
            &["com.bedcode.sdk-test.echo".to_string(), "com.bedcode.sdk-test.fail".to_string()],
        );

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let target = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, TARGET_ID, host_ctx.clone())
                    .expect("instantiate target"),
            ));
            let caller = Arc::new(Mutex::new(
                wasm_runtime
                    .instantiate_component(&component, CALLER_ID, host_ctx.clone())
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
            host_ctx.api_registry().register(
                TARGET_ID,
                &["com.bedcode.sdk-test.no-response".to_string()],
            );
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

    /// 加载入口：load_plugin_from_file 直接走组件路径（阶段 C 起仅组件形态）
    #[test]
    fn test_load_plugin_from_file() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let temp_dir = std::env::temp_dir()
            .join(format!("bedcode_component_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("plugin.wasm");
        std::fs::write(&wasm_path, build_test_component()).unwrap();

        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, TEST_PLUGIN_ID, host_ctx)
            .expect("load_plugin_from_file should load component");
        // 加载成功即可调用：激活 + manifest 往返验证组件路径
        assert_eq!(plugin.activate().expect("activate"), 0);
        let manifest: serde_json::Value =
            serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
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

        let temp_dir = std::env::temp_dir().join(format!(
            "bedcode_component_aot_{}",
            std::process::id()
        ));
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
                .instantiate_component(&c, TEST_PLUGIN_ID, host_ctx.clone())
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

        let temp_dir = std::env::temp_dir().join(format!(
            "bedcode_component_aot_stale_{}",
            std::process::id()
        ));
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
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
            .expect("component from full compile should instantiate");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ==================== 真实 file-transfer 组件端到端 ====================

    /// 加载真实 file-transfer 组件产物并预置设置（roots 指向临时目录）
    ///
    /// 返回 (插件实例, 共享根目录)；测试结束由调用方清理临时目录。
    /// 产物缺失时 panic（构建顺序依赖：先跑插件构建脚本再跑测试）
    fn load_real_file_transfer(
        wasm_runtime: &WasmRuntime,
        host_ctx: &Arc<WasmHostContext>,
    ) -> (LoadedWasmPlugin, std::path::PathBuf) {
        const FT_PLUGIN_ID: &str = "com.bedcode.file-transfer";

        // 授予与插件 manifest 一致的权限（activate 路径：storage/fileservice/bus）
        let permissions: &[&str] = &[
            "broadcast",
            "bus",
            "fileservice",
            "fs:read",
            "fs:write",
            "network:http",
            "storage",
            "transfer",
            "ui:sidebar",
        ];
        host_ctx.permission.grant_permissions(
            FT_PLUGIN_ID,
            &permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );

        // 预置插件设置：roots 非空才会走到挂载路径（空 roots 直接跳过）
        let root_dir = std::env::temp_dir().join(format!(
            "bedcode_ft_epoch_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root_dir).unwrap();
        let settings = serde_json::json!({
            "roots": [root_dir.to_string_lossy()],
            "downloadDir": "",
            "concurrency": 2,
        });
        let storage = host_ctx.storage.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            storage
                .set(FT_PLUGIN_ID, "file-transfer-settings", settings)
                .await
                .unwrap();
        });

        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/plugins/desktop/com.bedcode.file-transfer")
            .join("bedcode_plugin_file_transfer.wasm");
        assert!(
            wasm_path.exists(),
            "file-transfer wasm artifact missing: {}",
            wasm_path.display()
        );

        let plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, FT_PLUGIN_ID, host_ctx.clone())
            .expect("load real file-transfer component");
        (plugin, root_dir)
    }

    /// 真实组件快速路径：activate 端到端成功（设置加载 → 挂载 → 任务加载）
    #[test]
    fn test_real_file_transfer_activate_success() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let (mut plugin, root_dir) = load_real_file_transfer(&wasm_runtime, &host_ctx);

        // 宿主调用需 tokio 运行时上下文（block_on_async 依赖）
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            plugin
                .activate()
                .expect("real file-transfer activate should succeed");
        });

        let _ = std::fs::remove_dir_all(&root_dir);
    }

    /// 慢宿主调用与看门狗机制的回归测试
    ///
    /// 曾出现：宿主 filesrv_mount 阻塞超过 epoch 窗口（2s）后返回，guest 重新进入
    /// wasm 提升返回值时被中断 trap（backtrace 首帧 cabi_realloc），activate 整体
    /// 失败。修复为燃料看门狗：燃料只计 guest 指令数，宿主阻塞期间零消耗，
    /// 慢调用无论多久都不会被误杀（死循环则持续烧燃料必被 trap）。
    #[test]
    fn test_real_file_transfer_activate_slow_host_call() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let (mut plugin, root_dir) = load_real_file_transfer(&wasm_runtime, &host_ctx);

        // 模拟宿主调用阻塞 4s：宿主延迟不得计入 guest 燃料消耗
        let previous = std::env::var("BEDCODE_TEST_MOUNT_DELAY_MS").ok();
        std::env::set_var("BEDCODE_TEST_MOUNT_DELAY_MS", "4000");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { plugin.activate() });
        match previous {
            Some(v) => std::env::set_var("BEDCODE_TEST_MOUNT_DELAY_MS", v),
            None => std::env::remove_var("BEDCODE_TEST_MOUNT_DELAY_MS"),
        }
        result.expect(
            "activate must survive slow host calls (fuel counts guest instructions only)",
        );

        let _ = std::fs::remove_dir_all(&root_dir);
    }

    /// 真实组件：scheduler 插件加载 + activate + tick 命令路由冒烟
    ///
    /// 验证 bindgen world 与产物 export 一致（events 含 on_process_done）、
    /// activate 恢复路径无 panic（测试上下文无宿主 DB/services，相关调用
    /// 仅记日志降级）。完整调度行为属 issue 06 端到端验证。
    #[test]
    fn test_real_scheduler_plugin_loads_and_ticks() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        const SCHED_PLUGIN_ID: &str = "com.bedcode.scheduler";

        // 授予与插件 manifest 一致的权限（activate 的 timer_register/cli_install 路径）
        let permissions: &[&str] =
            &["app:cli", "broadcast", "process:run", "storage", "timer:schedule"];
        host_ctx.permission.grant_permissions(
            SCHED_PLUGIN_ID,
            &permissions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        );

        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/plugins/desktop/com.bedcode.scheduler")
            .join("bedcode_plugin_scheduler.wasm");
        assert!(
            wasm_path.exists(),
            "scheduler wasm artifact missing (run plugins/scheduler build first): {}",
            wasm_path.display()
        );

        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, SCHED_PLUGIN_ID, host_ctx.clone())
            .expect("load real scheduler component");

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            plugin
                .activate()
                .expect("real scheduler activate should succeed");
            // tick 命令路由：now_local 参数透传，宿主 DB 缺失时插件侧降级不 panic
            let result = plugin
                .invoke_command(
                    "task-scheduler.tick",
                    r#"{"now_local":"2026-08-14 12:00:00","now_utc":"2026-08-14 04:00:00"}"#,
                )
                .expect("tick command should return");
            assert!(
                result.contains("ticked"),
                "tick response should contain ticked: {}",
                result
            );
        });
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
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
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
                after2 > FUEL_PER_CALL / 2,
                "fuel must be refilled per export call, got {}",
                after2
            );
        });
    }

    /// 燃料耗尽必须 trap：绕过 exports() 的自动续费，直接以小预算调用导出
    #[test]
    fn test_component_fuel_exhaustion_traps() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
            .expect("instantiate test component");

        let (store, instance) = plugin.raw_store();
        store.set_fuel(1).expect("set tiny fuel");
        let binding = super::component::Plugin::new(&mut *store, instance).expect("bind exports");
        let result = binding
            .bedcode_plugin_command()
            .call_invoke(store, "test.echo", r#"{"a":1}"#);
        assert!(result.is_err(), "fuel exhausted must trap: {:?}", result);
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
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone())
            .expect("instantiate component A");
        {
            let (store, instance) = plugin_a.raw_store();
            store.set_fuel(1).expect("set tiny fuel");
            let binding = super::component::Plugin::new(&mut *store, instance).expect("bind exports");
            let result = binding
                .bedcode_plugin_command()
                .call_invoke(store, "test.echo", r#"{"a":1}"#);
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
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
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
