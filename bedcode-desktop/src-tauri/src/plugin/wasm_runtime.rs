//! WASM 插件运行时
//!
//! 基于 wasmtime 的 WASM 模块加载、实例化、调用
//! 管理 Engine/Linker/Store/Instance 生命周期
//! 注册宿主 Host Functions 供 WASM 插件调用
//!
//! Host Functions 的具体实现位于 [`host_functions`] 子模块，
//! 本模块只负责运行时生命周期管理与宿主上下文定义
//!
//! Component Model 共存（迁移阶段 A）见 [`component`] 子模块：
//! `load_plugin_from_file` 按产物格式自动选择 core module / component 路径

mod component;
mod host_functions;

use crate::db::Database;
use crate::plugin::file_service::FileServiceRegistry;
use crate::plugin::fs_auth::FsAuthChecker;
use crate::plugin::permission::PermissionManager;
use crate::plugin::storage::PluginStorage;
use crate::session::{SessionConfigManager, SessionManager};
use bedcode_plugin_api::abi;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Instance, Linker, Memory, Module, ResourceLimiter, Store,
};

// ==================== Resource Limits & Interruption ====================

/// epoch 递增周期（毫秒）：后台线程每周期推进一次全局纪元
const EPOCH_TICK_MILLIS: u64 = 500;
/// 每次 wasm 调用允许的 epoch tick 数（超时窗口 ≈ EPOCH_TICK_MILLIS × EPOCH_GRACE_TICKS，
/// 放宽至 2s 避免误杀合法重计算；纯 guest 死循环最迟 2s 被 trap）
const EPOCH_GRACE_TICKS: u64 = 4;
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
pub(crate) fn block_on_async<F, R>(fut: F) -> R
where
    F: std::future::Future<Output = R> + Send,
    R: Send + 'static,
{
    let handle = tokio::runtime::Handle::current();
    match handle.runtime_flavor() {
        tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| handle.block_on(fut))
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
    linker: Linker<WasmPluginState>,
    /// Component 形态插件的 linker（迁移阶段 A：协议共存）
    ///
    /// 与 core 路径的 [`linker`] 平行，已接线的 import 接口见
    /// [`component::add_to_linker`]，实例化导入未接线接口的组件会报
    /// unknown import 错误
    component_linker: wasmtime::component::Linker<WasmPluginState>,
    /// 文件系统访问校验器
    fs_auth: Arc<FsAuthChecker>,
    /// AOT 编译产物（`.cwasm`）缓存目录（宿主 cache 目录，非插件目录）
    ///
    /// 插件目录可被安装方/插件自身写入，若把反序列化产物放回插件目录，
    /// 能写插件目录的攻击者可投放伪造产物触发宿主进程 UB
    /// （`Module::deserialize_file` 是 unsafe，假定数据可信）。
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
}

/// 已加载的 WASM 插件（core module 形态，自研 ABI）
///
/// 持有 Instance、Store 和 Memory 引用
/// Store 必须与 Instance 一起持有，否则 Instance 的导出函数无法调用
pub struct LoadedCorePlugin {
    pub(crate) instance: Instance,
    pub(crate) store: Store<WasmPluginState>,
    pub(crate) memory: Memory,
}

/// 已加载的 WASM 插件（按产物形态分派）
///
/// 迁移阶段 A 的共存形态：`Core` 为现状自研 ABI 插件，`Component` 为
/// Component Model 插件（见 [`component::ComponentWasmPlugin`]）。
/// 加载入口 `WasmRuntime::load_plugin_from_file` 自动选择；
/// 阶段 C 清理完成后仅保留 `Component` 变体。
pub enum LoadedWasmPlugin {
    /// core module（wasm32-unknown-unknown，`__bedcode_*` 导出）
    Core(LoadedCorePlugin),
    /// component（WIT 契约，bindgen 类型化调用）
    Component(component::ComponentWasmPlugin),
}

impl LoadedWasmPlugin {
    /// 调用插件的 activate 导出函数
    pub fn activate(&mut self) -> crate::Result<i32> {
        match self {
            Self::Core(p) => p.activate(),
            Self::Component(p) => p.activate(),
        }
    }

    /// 调用插件的 deactivate 导出函数
    pub fn deactivate(&mut self) -> crate::Result<i32> {
        match self {
            Self::Core(p) => p.deactivate(),
            Self::Component(p) => p.deactivate(),
        }
    }

    /// 调用插件的 invoke_command 导出函数
    pub fn invoke_command(&mut self, command_name: &str, args_json: &str) -> crate::Result<String> {
        match self {
            Self::Core(p) => p.invoke_command(command_name, args_json),
            Self::Component(p) => p.invoke_command(command_name, args_json),
        }
    }

    /// 调用插件的 on_terminal_input 导出函数
    pub fn on_terminal_input(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> crate::Result<Option<String>> {
        match self {
            Self::Core(p) => p.on_terminal_input(session_id, text),
            Self::Component(p) => p.on_terminal_input(session_id, text),
        }
    }

    /// 调用插件的 on_terminal_output 导出函数
    pub fn on_terminal_output(
        &mut self,
        session_id: &str,
        data: &str,
    ) -> crate::Result<Option<String>> {
        match self {
            Self::Core(p) => p.on_terminal_output(session_id, data),
            Self::Component(p) => p.on_terminal_output(session_id, data),
        }
    }

    /// 调用插件的 on_startup 导出函数（可选）
    pub fn on_startup(&mut self) -> crate::Result<()> {
        match self {
            Self::Core(p) => p.on_startup(),
            Self::Component(p) => p.on_startup(),
        }
    }

    /// 调用插件的 on_shutdown 导出函数（可选）
    pub fn on_shutdown(&mut self) -> crate::Result<()> {
        match self {
            Self::Core(p) => p.on_shutdown(),
            Self::Component(p) => p.on_shutdown(),
        }
    }

    /// 调用插件的消息总线消息接收导出函数（可选）
    pub fn on_message(
        &mut self,
        topic: &str,
        sender: &str,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        match self {
            Self::Core(p) => p.on_message(topic, sender, payload),
            Self::Component(p) => p.on_message(topic, sender, payload),
        }
    }

    /// 调用插件的会话生命周期事件导出函数（可选）
    pub fn on_session_lifecycle(&mut self, payload: &serde_json::Value) -> crate::Result<()> {
        match self {
            Self::Core(p) => p.on_session_lifecycle(payload),
            Self::Component(p) => p.on_session_lifecycle(payload),
        }
    }

    /// 调用插件的提交输入行事件导出函数（可选，见 ADR 0001）
    pub fn on_input_submitted(&mut self, payload: &serde_json::Value) -> crate::Result<()> {
        match self {
            Self::Core(p) => p.on_input_submitted(payload),
            Self::Component(p) => p.on_input_submitted(payload),
        }
    }

    /// 调用插件的上传策略钩子导出函数（可选，ABI v5）
    pub fn on_upload_request(&mut self, meta_json: &str) -> crate::Result<String> {
        match self {
            Self::Core(p) => p.on_upload_request(meta_json),
            Self::Component(p) => p.on_upload_request(meta_json),
        }
    }

    /// 获取插件的 manifest JSON
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        match self {
            Self::Core(p) => p.get_manifest(),
            Self::Component(p) => p.get_manifest(),
        }
    }

    /// 获取 core 形态内部引用（仅供内存操作测试使用；component 形态 panic）
    #[cfg(test)]
    pub(crate) fn as_core_mut(&mut self) -> &mut LoadedCorePlugin {
        match self {
            Self::Core(p) => p,
            Self::Component(_) => panic!("as_core_mut called on component plugin"),
        }
    }
}

/// 根据 wasm 路径生成 AOT 缓存文件名（稳定 hash，避免路径字符/长度问题）
fn aot_cache_key(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
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
        // 启用 epoch 中断：后台线程周期推进纪元，防止插件死循环无限阻塞宿主
        config.epoch_interruption(true);
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
        let mut linker = Linker::new(&engine);

        // 注册所有 Host Functions 到 "bedcode" 命名空间（实现见 host_functions 子模块）
        host_functions::register_host_functions(&mut linker)?;

        // Component 形态插件（阶段 A）：已接线接口见 component::add_to_linker
        let mut component_linker = wasmtime::component::Linker::new(&engine);
        component::add_to_linker(&mut component_linker)?;

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

        // 后台线程周期递增 epoch：任何进行中的 wasm 调用超过超时窗口即被中断（trap）。
        // spawn 失败降级为无中断（与未启用 epoch 时行为一致），不 panic
        let epoch_engine = engine.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("wasmtime-epoch".to_string())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(EPOCH_TICK_MILLIS));
                epoch_engine.increment_epoch();
            })
        {
            tracing::warn!(error = %e, "Failed to spawn epoch thread, interruption disabled");
        }

        Ok(Self { engine, linker, component_linker, fs_auth, aot_cache_dir })
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

        let cache_path = cache_dir.join(format!("c{:016x}.cwasm", aot_cache_key(path)));

        // 产物存在且不旧于 wasm 源时尝试直接反序列化
        let cache_fresh = std::fs::metadata(path)
            .and_then(|w| w.modified())
            .ok()
            .zip(std::fs::metadata(&cache_path).and_then(|c| c.modified()).ok())
            .map(|(wasm_mtime, cache_mtime)| cache_mtime >= wasm_mtime)
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

    /// 从文件加载 WASM 插件（阶段 A 共存入口：按产物格式自动选择）
    ///
    /// core module 走现有 `compile_module_from_file` + `instantiate` 路径；
    /// component 走 `compile_component_from_file` + `instantiate_component`。
    /// 现有插件（wasm32-unknown-unknown 产物）行为完全不变。
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
        match component::detect_artifact_kind(&bytes)? {
            component::ArtifactKind::Core => {
                let module = self.compile_module(&bytes)?;
                self.instantiate(&module, plugin_id, host_ctx)
            }
            component::ArtifactKind::Component => {
                let component = self.compile_component(&bytes)?;
                self.instantiate_component(&component, plugin_id, host_ctx)
            }
        }
    }

    /// 实例化 WASM 组件（Component Model，迁移阶段 A）
    ///
    /// 创建 Store + WasmPluginState，通过 component linker 实例化，
    /// 校验 ABI 版本与形态字段（见 [`component::ComponentWasmPlugin::new`]）
    pub fn instantiate_component(
        &self,
        component: &wasmtime::component::Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<LoadedWasmPlugin> {
        Ok(LoadedWasmPlugin::Component(component::ComponentWasmPlugin::new(
            &self.engine,
            &self.component_linker,
            component,
            plugin_id,
            host_ctx,
        )?))
    }

    /// 从字节流编译 WASM 模块
    pub fn compile_module(&self, bytes: &[u8]) -> crate::Result<Module> {
        Module::from_binary(&self.engine, bytes).map_err(|e| {
            crate::AppError::Plugin(format!("Failed to compile WASM module: {}", e))
        })
    }

    /// 从文件编译 WASM 模块（带 AOT 缓存）
    ///
    /// 优先加载宿主 cache 目录中的 `.cwasm` 编译产物（wasm 源未变时跳过编译）；
    /// 产物缺失/过期/与当前 Engine 不兼容（版本或特性变化）时重新编译并写回。
    ///
    /// 缓存文件以 wasm 路径 hash 命名，位于宿主 cache 目录而非插件目录：
    /// 插件目录对安装方/插件可写，反序列化产物放在那里可被投毒
    /// （`Module::deserialize_file` 是 unsafe，假定数据可信）。
    pub fn compile_module_from_file(&self, path: &Path) -> crate::Result<Module> {
        // 无 AOT 缓存目录（无头/测试上下文）时退化为纯编译
        let Some(cache_dir) = &self.aot_cache_dir else {
            return Module::from_file(&self.engine, path).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to compile WASM module from '{}': {}",
                    path.display(),
                    e
                ))
            });
        };

        let cache_path = cache_dir.join(format!("{:016x}.cwasm", aot_cache_key(path)));

        // 产物存在且不旧于 wasm 源时尝试直接反序列化
        let cache_fresh = std::fs::metadata(path)
            .and_then(|w| w.modified())
            .ok()
            .zip(std::fs::metadata(&cache_path).and_then(|c| c.modified()).ok())
            .map(|(wasm_mtime, cache_mtime)| cache_mtime >= wasm_mtime)
            .unwrap_or(false);

        if cache_fresh {
            // unsafe：产物为本机自写缓存；Engine 版本/特性不匹配时 deserialize 失败，
            // 回退到完整编译路径
            if let Ok(module) = unsafe { Module::deserialize_file(&self.engine, &cache_path) } {
                tracing::debug!(
                    path = %cache_path.display(),
                    "Loaded WASM module from AOT cache"
                );
                return Ok(module);
            }
        }

        let module = Module::from_file(&self.engine, path).map_err(|e| {
            crate::AppError::Plugin(format!(
                "Failed to compile WASM module from '{}': {}",
                path.display(),
                e
            ))
        })?;

        // 写回 AOT 缓存：先写临时文件再 rename（原子替换，避免崩溃留半截产物）；
        // 失败不阻断加载（下次启动重新编译）
        match module.serialize() {
            Ok(bytes) => {
                // 目录可能尚未创建（无头/测试路径注入时），写前确保存在
                if let Err(e) = std::fs::create_dir_all(cache_dir) {
                    tracing::warn!(
                        path = %cache_dir.display(),
                        error = %e,
                        "Failed to create AOT cache dir, will recompile next time"
                    );
                    return Ok(module);
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
                tracing::warn!(error = %e, "Failed to serialize module for AOT cache");
            }
        }

        Ok(module)
    }

    /// 获取文件系统访问校验器引用
    pub fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }
    /// 获取 Host Function 注册表引用（仅测试用，用于校验 abi 签名契约）
    #[cfg(test)]
    pub fn linker(&self) -> &Linker<WasmPluginState> {
        &self.linker
    }

    /// 实例化 WASM 模块
    ///
    /// 创建 Store + WasmPluginState，通过 Linker 实例化模块
    pub fn instantiate(
        &self,
        module: &Module,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
    ) -> crate::Result<LoadedWasmPlugin> {
        let state = WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx,
        };
        let mut store = Store::new(&self.engine, state);

        // 注册资源限制（内存/表超限拒绝增长）并配置 epoch 中断（wasm 死循环超时 trap）
        store.limiter(|state| state as &mut dyn ResourceLimiter);
        store.epoch_deadline_trap();
        store.set_epoch_deadline(EPOCH_GRACE_TICKS);

        let instance = self
            .linker
            .instantiate(&mut store, module)
            .map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to instantiate WASM module for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;

        // 获取线性内存引用（Host Functions 读写插件内存需要）
        let memory = instance
            .get_memory(&mut store, abi::MEMORY)
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module for plugin '{}' has no exported 'memory'",
                    plugin_id
                ))
            })?;

        // ABI 版本协商：插件导出 __bedcode_abi_version（v2+）时校验不超过宿主支持版本；
        // 未导出视为 v1 遗留插件，兼容加载（v1 的全部 host functions 宿主均支持）
        if let Some(func) = instance.get_func(&mut store, abi::export::ABI_VERSION) {
            let mut results = [wasmtime::Val::I32(0)];
            func.call(&mut store, &[], &mut results).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "WASM __bedcode_abi_version() call failed for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;
            let plugin_abi = results[0].unwrap_i32() as u32;
            if plugin_abi > abi::ABI_VERSION {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin '{}' requires ABI v{} but host supports v{} — please upgrade BedCode",
                    plugin_id, plugin_abi, abi::ABI_VERSION
                )));
            }
        }

        Ok(LoadedWasmPlugin::Core(LoadedCorePlugin {
            instance,
            store,
            memory,
        }))
    }
}

impl LoadedCorePlugin {
    /// 调用插件的 activate 导出函数
    pub fn activate(&mut self) -> crate::Result<i32> {
        let func = self.get_export_func(abi::export::ACTIVATE)?;
        let mut results = [wasmtime::Val::I32(0)];
        func.call(&mut self.store, &[], &mut results).map_err(|e| {
            crate::AppError::Plugin(format!("WASM activate() call failed: {}", e))
        })?;
        Ok(results[0].unwrap_i32())
    }

    /// 调用插件的 deactivate 导出函数
    pub fn deactivate(&mut self) -> crate::Result<i32> {
        let func = self.get_export_func(abi::export::DEACTIVATE)?;
        let mut results = [wasmtime::Val::I32(0)];
        func.call(&mut self.store, &[], &mut results).map_err(|e| {
            crate::AppError::Plugin(format!("WASM deactivate() call failed: {}", e))
        })?;
        Ok(results[0].unwrap_i32())
    }

    /// 调用插件的 invoke_command 导出函数
    ///
    /// 传入 (name_ptr, name_len, args_ptr, args_len, out_ptr)，结果通过 out_ptr 写入
    /// 宿主通过线性内存读写字符串
    pub fn invoke_command(
        &mut self,
        command_name: &str,
        args_json: &str,
    ) -> crate::Result<String> {
        let (name_ptr, name_len) = self.write_string_to_memory(command_name)?;
        let (args_ptr, args_len) = self.write_string_to_memory(args_json)?;
        let out_ptr = self.allocate_memory(8)?;

        let func = self.get_export_func(abi::export::INVOKE_COMMAND)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(name_ptr as i32),
                wasmtime::Val::I32(name_len as i32),
                wasmtime::Val::I32(args_ptr as i32),
                wasmtime::Val::I32(args_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM invoke_command() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len);
        // 读取完毕，回收插件分配的结果缓冲区与 out_ptr（RESULT_PAIR_SIZE 字节）本身，
        // 防止长驻插件线性内存随调用次数单调增长
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, abi::RESULT_PAIR_SIZE as u32);
        result
    }

    /// 调用插件的 on_terminal_input 导出函数
    pub fn on_terminal_input(
        &mut self,
        session_id: &str,
        text: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (text_ptr, text_len) = self.write_string_to_memory(text)?;
        let out_ptr = self.allocate_memory(8)?;

        let func = self.get_export_func(abi::export::ON_TERMINAL_INPUT)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(text_ptr as i32),
                wasmtime::Val::I32(text_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_input() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        self.dealloc_plugin_memory(out_ptr, abi::RESULT_PAIR_SIZE as u32);

        if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            let result = self.read_string_from_memory(ptr, len);
            self.dealloc_plugin_memory(ptr, len);
            result.map(Some)
        }
    }

    /// 调用插件的 on_terminal_output 导出函数
    pub fn on_terminal_output(
        &mut self,
        session_id: &str,
        data: &str,
    ) -> crate::Result<Option<String>> {
        let (sid_ptr, sid_len) = self.write_string_to_memory(session_id)?;
        let (data_ptr, data_len) = self.write_string_to_memory(data)?;
        let out_ptr = self.allocate_memory(8)?;

        let func = self.get_export_func(abi::export::ON_TERMINAL_OUTPUT)?;
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(sid_ptr as i32),
                wasmtime::Val::I32(sid_len as i32),
                wasmtime::Val::I32(data_ptr as i32),
                wasmtime::Val::I32(data_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_terminal_output() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        self.dealloc_plugin_memory(out_ptr, abi::RESULT_PAIR_SIZE as u32);

        if ptr == 0 && len == 0 {
            Ok(None)
        } else {
            let result = self.read_string_from_memory(ptr, len);
            self.dealloc_plugin_memory(ptr, len);
            result.map(Some)
        }
    }

    /// 调用插件的 on_startup 导出函数（可选）
    pub fn on_startup(&mut self) -> crate::Result<()> {
        if let Ok(func) = self.get_export_func(abi::export::ON_STARTUP) {
            func.call(&mut self.store, &[], &mut []).map_err(|e| {
                crate::AppError::Plugin(format!("WASM on_startup() call failed: {}", e))
            })?;
        }
        Ok(())
    }

    /// 调用插件的 on_shutdown 导出函数（可选）
    pub fn on_shutdown(&mut self) -> crate::Result<()> {
        if let Ok(func) = self.get_export_func(abi::export::ON_SHUTDOWN) {
            func.call(&mut self.store, &[], &mut []).map_err(|e| {
                crate::AppError::Plugin(format!("WASM on_shutdown() call failed: {}", e))
            })?;
        }
        Ok(())
    }

    /// 调用插件的消息总线消息接收导出函数（可选）
    pub fn on_message(
        &mut self,
        topic: &str,
        sender: &str,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        // 可选导出：如果插件未导出 on_message，跳过
        let Ok(func) = self.get_export_func(abi::export::ON_MESSAGE) else {
            return Ok(());
        };

        let (topic_ptr, topic_len) = self.write_string_to_memory(topic)?;
        let (sender_ptr, sender_len) = self.write_string_to_memory(sender)?;
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = self.write_string_to_memory(&payload_str)?;

        let mut results = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(topic_ptr as i32),
                wasmtime::Val::I32(topic_len as i32),
                wasmtime::Val::I32(sender_ptr as i32),
                wasmtime::Val::I32(sender_len as i32),
                wasmtime::Val::I32(payload_ptr as i32),
                wasmtime::Val::I32(payload_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_message() call failed: {}", e))
        })?;

        let status = results[0].unwrap_i32();
        if status != 0 {
            tracing::warn!("WASM on_message() returned non-zero status: {}", status);
        }
        Ok(())
    }

    /// 调用插件的会话生命周期事件导出函数（可选）
    pub fn on_session_lifecycle(
        &mut self,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        let Ok(func) = self.get_export_func(abi::export::ON_SESSION_LIFECYCLE) else {
            return Ok(());
        };

        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = self.write_string_to_memory(&payload_str)?;

        let mut results = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(payload_ptr as i32),
                wasmtime::Val::I32(payload_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_session_lifecycle() call failed: {}", e))
        })?;

        let status = results[0].unwrap_i32();
        if status != 0 {
            tracing::warn!("WASM on_session_lifecycle() returned non-zero status: {}", status);
        }
        Ok(())
    }

    /// 调用插件的提交输入行事件导出函数（可选，见 ADR 0001）
    ///
    /// 纯观察通知：由 SessionManager 在异步错误隔离任务中经
    /// PluginInputListener 触发，调用失败仅记录日志，不影响输入本身
    pub fn on_input_submitted(
        &mut self,
        payload: &serde_json::Value,
    ) -> crate::Result<()> {
        let Ok(func) = self.get_export_func(abi::export::ON_INPUT_SUBMITTED) else {
            // 旧版 WASM 产物（b5449a6 之前）不含此导出，原实现静默 no-op 会掩盖
            // "hooks 正常但输入不触发"的产物版本不匹配问题，记录 debug 便于排查
            tracing::debug!(
                "WASM plugin missing export '{}', skip on_input_submitted",
                abi::export::ON_INPUT_SUBMITTED
            );
            return Ok(());
        };

        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let (payload_ptr, payload_len) = self.write_string_to_memory(&payload_str)?;

        let mut results = [wasmtime::Val::I32(0)];
        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(payload_ptr as i32),
                wasmtime::Val::I32(payload_len as i32),
            ],
            &mut results,
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_input_submitted() call failed: {}", e))
        })?;

        let status = results[0].unwrap_i32();
        if status != 0 {
            tracing::warn!("WASM on_input_submitted() returned non-zero status: {}", status);
        }
        Ok(())
    }

    /// 调用插件的上传策略钩子导出函数（可选，ABI v5）
    ///
    /// 返回插件的 UploadHookDecision JSON。插件未导出该函数、返回空结果
    /// 或调用失败时返回 Err，由调用方按 fail-closed 语义拒绝上传
    pub fn on_upload_request(&mut self, meta_json: &str) -> crate::Result<String> {
        // 可选导出：未实现钩子的插件（或旧版 ABI 产物）返回错误 → 宿主拒绝上传
        let func = self.get_export_func(abi::export::ON_UPLOAD_REQUEST)?;

        let (meta_ptr, meta_len) = self.write_string_to_memory(meta_json)?;
        let out_ptr = self.allocate_memory(8)?;

        func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(meta_ptr as i32),
                wasmtime::Val::I32(meta_len as i32),
                wasmtime::Val::I32(out_ptr as i32),
            ],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM on_upload_request() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        self.dealloc_plugin_memory(out_ptr, abi::RESULT_PAIR_SIZE as u32);

        if ptr == 0 && len == 0 {
            return Err(crate::AppError::Plugin(
                "WASM on_upload_request() returned empty decision".to_string(),
            ));
        }

        let result = self.read_string_from_memory(ptr, len);
        self.dealloc_plugin_memory(ptr, len);
        result
    }

    /// 获取插件的 manifest JSON
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        let out_ptr = self.allocate_memory(8)?;

        let func = self.get_export_func(abi::export::MANIFEST)?;
        func.call(
            &mut self.store,
            &[wasmtime::Val::I32(out_ptr as i32)],
            &mut [],
        )
        .map_err(|e| {
            crate::AppError::Plugin(format!("WASM manifest() call failed: {}", e))
        })?;

        let (ptr, len) = self.read_result_from_out_ptr(out_ptr)?;
        let result = self.read_string_from_memory(ptr, len);
        self.dealloc_plugin_memory(ptr, len);
        self.dealloc_plugin_memory(out_ptr, abi::RESULT_PAIR_SIZE as u32);
        result
    }

    // ==================== Memory Helpers ====================

    /// 回收插件线性内存中由 `__bedcode_allocate` / `wasm_alloc_string` 分配的缓冲区
    ///
    /// 与 write_string_to_memory / 插件结果缓冲配对调用，
    /// 防止长驻插件线性内存随调用次数单调增长。
    /// v2 之前的旧插件未导出 `__bedcode_deallocate` — 跳过回收，退化 v1 行为
    fn dealloc_plugin_memory(&mut self, ptr: u32, len: u32) {
        if ptr == 0 || len == 0 {
            return;
        }
        let Some(func) = self.instance.get_func(&mut self.store, abi::export::DEALLOCATE) else {
            return;
        };
        // 回收函数不会再回调 host，无递归风险；失败时静默降级（仅泄漏 guest 内存）
        // 刷新 epoch 超时窗口：guest 回收函数也可能死循环
        self.store.set_epoch_deadline(EPOCH_GRACE_TICKS);
        let _ = func.call(
            &mut self.store,
            &[
                wasmtime::Val::I32(ptr as i32),
                wasmtime::Val::I32(len as i32),
            ],
            &mut [],
        );
    }

    /// 获取导出函数
    fn get_export_func(
        &mut self,
        name: &str,
    ) -> crate::Result<wasmtime::Func> {
        // 刷新 epoch 超时窗口：本次调用最多执行 EPOCH_GRACE_TICKS 个 tick
        self.store.set_epoch_deadline(EPOCH_GRACE_TICKS);
        self.instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module missing required export '{}'",
                    name
                ))
            })
    }

    /// 将字符串写入插件线性内存
    ///
    /// 通过插件的 __bedcode_allocate 导出函数分配内存，
    /// 然后写入字符串字节，返回 (ptr, len)
    pub fn write_string_to_memory(&mut self, s: &str) -> crate::Result<(u32, u32)> {
        if s.is_empty() {
            return Ok((0, 0));
        }

        let bytes = s.as_bytes();
        let len = bytes.len();

        let ptr = self.allocate_memory(len)?;

        let memory = self.memory;
        let memory_data = memory.data_mut(&mut self.store);
        let start = ptr as usize;
        let end = start + len;
        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM allocate() returned ptr {} + len {} exceeds memory size {}",
                ptr,
                len,
                memory_data.len()
            )));
        }
        memory_data[start..end].copy_from_slice(bytes);

        Ok((ptr, len as u32))
    }

    /// 通过插件的 allocate 导出函数分配内存
    ///
    /// 返回分配的内存起始地址
    pub fn allocate_memory(&mut self, size: usize) -> crate::Result<u32> {
        let alloc_func = self
            .instance
            .get_func(&mut self.store, abi::export::ALLOCATE)
            .ok_or_else(|| {
                crate::AppError::Plugin(format!(
                    "WASM module missing required export '{}'",
                    abi::export::ALLOCATE
                ))
            })?;

        let mut alloc_results = [wasmtime::Val::I32(0)];
        // 刷新 epoch 超时窗口：guest 分配器也可能死循环
        self.store.set_epoch_deadline(EPOCH_GRACE_TICKS);
        alloc_func
            .call(&mut self.store, &[wasmtime::Val::I32(size as i32)], &mut alloc_results)
            .map_err(|e| {
                crate::AppError::Plugin(format!("WASM allocate() call failed: {}", e))
            })?;

        let ptr = alloc_results[0].unwrap_i32() as u32;
        if ptr == 0 {
            return Err(crate::AppError::Plugin(
                "WASM allocate() returned null pointer".to_string(),
            ));
        }

        Ok(ptr)
    }

    /// 从插件线性内存读取字符串
    pub fn read_string_from_memory(&self, ptr: u32, len: u32) -> crate::Result<String> {
        if ptr == 0 && len == 0 {
            return Ok(String::new());
        }
        if len == 0 {
            return Ok(String::new());
        }

        let memory = self.memory;
        let memory_data = memory.data(&self.store);
        let start = ptr as usize;
        let end = start + len as usize;

        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM read_string: ptr {} + len {} exceeds memory size {}",
                ptr,
                len,
                memory_data.len()
            )));
        }

        let bytes = &memory_data[start..end];
        String::from_utf8(bytes.to_vec()).map_err(|e| {
            crate::AppError::Plugin(format!("WASM read_string: invalid UTF-8: {}", e))
        })
    }

    /// 从 out_ptr 位置读取 (ptr, len) 结果（8 字节: ptr:u32 + len:u32）
    pub fn read_result_from_out_ptr(&self, out_ptr: u32) -> crate::Result<(u32, u32)> {
        let memory = self.memory;
        let memory_data = memory.data(&self.store);
        let start = out_ptr as usize;
        let end = start + 8;

        if end > memory_data.len() {
            return Err(crate::AppError::Plugin(format!(
                "WASM read_result_from_out_ptr: out_ptr {} + 8 exceeds memory size {}",
                out_ptr,
                memory_data.len()
            )));
        }

        let ptr = u32::from_le_bytes(memory_data[start..start + 4].try_into().unwrap());
        let len = u32::from_le_bytes(memory_data[start + 4..end].try_into().unwrap());
        Ok((ptr, len))
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
        }
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

    /// 编译测试用 WASM 插件并返回字节
    ///
    /// 测试插件为独立 crate（packages/plugin-test），不再内嵌于 SDK
    fn build_test_wasm() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-test");
        let sdk_dir = packages_dir.join("plugin-sdk-desktop/rust");

        let output_dir = plugin_dir.join("target/wasm32-unknown-unknown/release");
        let wasm_path = output_dir.join("bedcode_plugin_test.wasm");

        if wasm_path.exists() {
            // 源码变更检测：插件本体 + SDK 的 ABI/胶水层相关源文件
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                sdk_dir.join("src/lib.rs"),
                sdk_dir.join("src/abi.rs"),
                sdk_dir.join("src/wasm.rs"),
                sdk_dir.join("src/wasm_host.rs"),
                sdk_dir.join("src/events.rs"),
            ];
            let wasm_modified = std::fs::metadata(&wasm_path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            let needs_rebuild = src_files.iter().any(|f| {
                std::fs::metadata(f)
                    .and_then(|m| m.modified())
                    .map(|t| t > wasm_modified)
                    .unwrap_or(true)
            });

            if !needs_rebuild {
                return std::fs::read(&wasm_path).expect("Failed to read test WASM");
            }
        }

        // 新版 cargo 的 --manifest-path 必须指向 Cargo.toml 文件，不再接受目录
        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .args([
                "build",
                "--target", "wasm32-unknown-unknown",
                "--release",
                "--manifest-path", manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for test plugin");
        assert!(status.success(), "Test plugin WASM build failed");

        std::fs::read(&wasm_path).expect("Failed to read test WASM after build")
    }

    /// 仅用 wasmtime Engine 加载 WASM 模块（不注册 host function，不实例化）
    /// 用于 ABI 签名验证
    fn load_wasm_module() -> (wasmtime::Engine, wasmtime::Module) {
        let wasm_bytes = build_test_wasm();
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::from_binary(&engine, &wasm_bytes)
            .expect("Failed to compile test WASM module");
        (engine, module)
    }

    /// 创建 WasmRuntime + 完整 host function + 实例化插件
    /// 用于连通性测试
    ///
    /// 返回 (运行时, 插件实例)：运行时供 Linker 注册签名校验测试使用。
    /// 无头构建（app_handle = None）：tao 事件循环不允许在测试线程创建，
    /// emit/数据目录类能力在测试中不被调用路径覆盖
    fn setup_wasm_plugin() -> (WasmRuntime, LoadedWasmPlugin) {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let wasm_bytes = build_test_wasm();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let module = wasm_runtime.compile_module(&wasm_bytes).unwrap();
            let plugin = wasm_runtime.instantiate(&module, TEST_PLUGIN_ID, host_ctx).unwrap();
            (wasm_runtime, plugin)
        })
    }

    /// 创建 WasmRuntime + 完整 host function + 宿主上下文（不实例化插件）
    ///
    /// 供需要独立编译/实例化模块的测试（如 AOT 缓存）复用。
    /// 无头构建（app_handle = None）：tao 事件循环不允许在测试线程创建，
    /// emit/数据目录类能力在测试中不被调用路径覆盖；
    /// AOT 缓存目录注入到系统临时目录，保证 compile_module_from_file 走缓存路径。
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

    // ==================== ABI 签名验证测试 ====================

    #[test]
    fn test_wasm_export_signatures() {
        let (_engine, module) = load_wasm_module();

        // 直接从 module 检查导出函数的签名，不需要实例化
        // 签名表定义在 SDK abi 模块（单一事实来源），与 wasm_entry! 宏生成结果比对
        for &(name, expected_params, expected_results) in bedcode_plugin_api::abi::PLUGIN_EXPORT_SIGNATURES {
            let export = module.get_export(name);
            assert!(export.is_some(), "Missing export: {}", name);

            let func_type = match export.unwrap() {
                wasmtime::ExternType::Func(ty) => ty,
                other => panic!("Export {} is not a function, got: {:?}", name, other),
            };

            let actual_params = func_type.params().len();
            let actual_results = func_type.results().len();

            assert_eq!(
                actual_params, expected_params,
                "ABI mismatch for {}: expected {} params, got {}",
                name, expected_params, actual_params
            );
            assert_eq!(
                actual_results, expected_results,
                "ABI mismatch for {}: expected {} results, got {}",
                name, expected_results, actual_results
            );
        }
    }

    /// 校验宿主注册的 host function 签名与 abi 契约表逐一匹配
    ///
    /// 防止 register_host_functions 的注册与 SDK 声明漂移：
    /// 签名不匹配过去只在插件 instantiate 时以运行期 panic 暴露，现在测试期即失败
    #[test]
    fn test_host_fn_registration_matches_abi() {
        let (wasm_runtime, mut plugin) = setup_wasm_plugin();

        for &(name, expected_params, expected_results) in bedcode_plugin_api::abi::HOST_FN_SIGNATURES {
            let export = wasm_runtime
                .linker()
                .get(&mut plugin.as_core_mut().store, bedcode_plugin_api::abi::NAMESPACE, name)
                .unwrap_or_else(|e| panic!("Host function '{}' not registered: {}", name, e));

            let func_type = match export.ty(&plugin.as_core_mut().store) {
                wasmtime::ExternType::Func(ty) => ty,
                other => panic!("Host function '{}' is not a function, got: {:?}", name, other),
            };

            assert_eq!(
                func_type.params().len(),
                expected_params,
                "ABI mismatch for {}: expected {} params, got {}",
                name,
                expected_params,
                func_type.params().len()
            );
            assert_eq!(
                func_type.results().len(),
                expected_results,
                "ABI mismatch for {}: expected {} results, got {}",
                name,
                expected_results,
                func_type.results().len()
            );
        }
    }

    // ==================== AOT 缓存测试 ====================

    #[test]
    fn test_compile_module_from_file_aot_cache() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();

        // 用临时目录模拟插件 wasm 文件
        let temp_dir = std::env::temp_dir().join(format!(
            "bedcode_aot_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("test_plugin.wasm");
        let cache_path = std::env::temp_dir()
            .join(format!("bedcode_aot_{}", std::process::id()))
            .join(format!("{:016x}.cwasm", aot_cache_key(&wasm_path)));
        std::fs::write(&wasm_path, build_test_wasm()).unwrap();

        // 首次编译：生成 .cwasm 产物
        let module = wasm_runtime
            .compile_module_from_file(&wasm_path)
            .expect("first compile should succeed");
        assert!(cache_path.exists(), "AOT cache file should be written");

        // 再次加载：应命中缓存（mtime 未变）
        let cached = wasm_runtime
            .compile_module_from_file(&wasm_path)
            .expect("cached load should succeed");

        // 两个 Module 都可正常实例化（功能等价）
        for m in [module, cached] {
            wasm_runtime
                .instantiate(&m, TEST_PLUGIN_ID, host_ctx.clone())
                .expect("module from cache should instantiate");
        }

        // 清理临时产物
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_compile_module_from_file_recompiles_on_stale() {
        let (wasm_runtime, _plugin) = setup_wasm_plugin();

        let temp_dir = std::env::temp_dir().join(format!(
            "bedcode_aot_test_stale_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("test_plugin.wasm");
        let cache_path = std::env::temp_dir()
            .join(format!("bedcode_aot_{}", std::process::id()))
            .join(format!("{:016x}.cwasm", aot_cache_key(&wasm_path)));
        std::fs::write(&wasm_path, build_test_wasm()).unwrap();

        // 首次编译生成缓存
        wasm_runtime.compile_module_from_file(&wasm_path).unwrap();

        // 篡改缓存为无效字节：deserialize 应失败并回退到完整编译
        std::fs::write(&cache_path, b"not a valid cwasm").unwrap();
        let module = wasm_runtime
            .compile_module_from_file(&wasm_path)
            .expect("invalid cache should fall back to full compile");
        assert!(module.get_export("memory").is_some());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ==================== 内存操作测试 ====================

    #[test]
    fn test_write_read_string_roundtrip() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let test_str = "Hello, WASM!";
        let (ptr, len) = plugin.as_core_mut().write_string_to_memory(test_str).unwrap();
        assert_ne!(ptr, 0);
        assert_eq!(len as usize, test_str.len());

        let read_back = plugin.as_core_mut().read_string_from_memory(ptr, len).unwrap();
        assert_eq!(read_back, test_str);
    }

    #[test]
    fn test_write_read_empty_string() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let (ptr, len) = plugin.as_core_mut().write_string_to_memory("").unwrap();
        assert_eq!(ptr, 0);
        assert_eq!(len, 0);

        let read_back = plugin.as_core_mut().read_string_from_memory(0, 0).unwrap();
        assert_eq!(read_back, "");
    }

    #[test]
    fn test_write_read_unicode_string() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let test_str = "你好世界 🦀 wasm";
        let (ptr, len) = plugin.as_core_mut().write_string_to_memory(test_str).unwrap();
        assert_ne!(ptr, 0);

        let read_back = plugin.as_core_mut().read_string_from_memory(ptr, len).unwrap();
        assert_eq!(read_back, test_str);
    }

    #[test]
    fn test_allocate_memory_returns_valid_ptr() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let ptr = plugin.as_core_mut().allocate_memory(64).unwrap();
        assert_ne!(ptr, 0);

        let ptr2 = plugin.as_core_mut().allocate_memory(128).unwrap();
        assert_ne!(ptr2, 0);
        assert_ne!(ptr, ptr2);
    }

    #[test]
    fn test_read_result_from_out_ptr() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let out_ptr = plugin.as_core_mut().allocate_memory(8).unwrap();

        let core = plugin.as_core_mut();
        let memory = core.memory;
        let memory_data = memory.data_mut(&mut core.store);
        let start = out_ptr as usize;
        memory_data[start..start + 4].copy_from_slice(&0x1000u32.to_le_bytes());
        memory_data[start + 4..start + 8].copy_from_slice(&42u32.to_le_bytes());

        let (ptr, len) = plugin.as_core_mut().read_result_from_out_ptr(out_ptr).unwrap();
        assert_eq!(ptr, 0x1000);
        assert_eq!(len, 42);
    }

    #[test]
    fn test_out_ptr_null_result() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let out_ptr = plugin.as_core_mut().allocate_memory(8).unwrap();
        let core = plugin.as_core_mut();
        let memory = core.memory;
        let memory_data = memory.data_mut(&mut core.store);
        let start = out_ptr as usize;
        memory_data[start..start + 4].copy_from_slice(&0u32.to_le_bytes());
        memory_data[start + 4..start + 8].copy_from_slice(&0u32.to_le_bytes());

        let (ptr, len) = plugin.as_core_mut().read_result_from_out_ptr(out_ptr).unwrap();
        assert_eq!(ptr, 0);
        assert_eq!(len, 0);
    }

    // ==================== Host Function 连通性测试 ====================

    #[test]
    fn test_invoke_command_echo() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let result = plugin.invoke_command("test.echo", r#"{"hello":"world"}"#);
        assert!(result.is_ok(), "invoke_command failed: {:?}", result.err());

        let result_json: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(result_json["hello"], "world");
    }

    #[test]
    fn test_on_terminal_input_output() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let result = plugin.on_terminal_input("session-1", "hello input");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("HELLO INPUT".to_string()));

        let result = plugin.on_terminal_output("session-1", "hello output");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("HELLO OUTPUT".to_string()));
    }

    #[test]
    fn test_activate_deactivate_manifest() {
        let (_rt, mut plugin) = setup_wasm_plugin();

        let result = plugin.activate();
        assert!(result.is_ok(), "activate failed: {:?}", result.err());
        assert_eq!(result.unwrap(), 0);

        let result = plugin.deactivate();
        assert!(result.is_ok(), "deactivate failed: {:?}", result.err());
        assert_eq!(result.unwrap(), 0);

        let result = plugin.get_manifest();
        assert!(result.is_ok(), "get_manifest failed: {:?}", result.err());
        let manifest: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(manifest["id"], TEST_PLUGIN_ID);
    }

    // ==================== Component Model 测试（迁移阶段 A） ====================

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

    /// 创建已实例化的组件插件（复用 setup_wasm_runtime 的宿主上下文）
    fn setup_component_plugin() -> (WasmRuntime, LoadedWasmPlugin) {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let component = wasm_runtime
            .compile_component(&build_test_component())
            .expect("compile test component");
        let plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx)
            .expect("instantiate test component");
        (wasm_runtime, plugin)
    }

    /// 产物形态检测：魔法字节区分 core module / component
    #[test]
    fn test_detect_artifact_kind() {
        use super::component::{detect_artifact_kind, ArtifactKind};

        // core module 版本字：01 00 00 00
        assert_eq!(
            detect_artifact_kind(&[0, b'a', b's', b'm', 1, 0, 0, 0]).unwrap(),
            ArtifactKind::Core
        );
        // component 版本字：0d 00 01 00
        assert_eq!(
            detect_artifact_kind(&[0, b'a', b's', b'm', 0x0d, 0, 1, 0]).unwrap(),
            ArtifactKind::Component
        );
        // 非法输入
        assert!(detect_artifact_kind(b"not wasm").is_err());
        assert!(detect_artifact_kind(&[0, b'a', b's', b'm', 9, 9, 9, 9]).is_err());
        assert!(detect_artifact_kind(&[0, b'a']).is_err());
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

    /// 共存入口：load_plugin_from_file 按产物格式自动选择 component 路径
    #[test]
    fn test_load_plugin_from_file_detects_component() {
        let (wasm_runtime, host_ctx) = setup_wasm_runtime();
        let temp_dir = std::env::temp_dir()
            .join(format!("bedcode_component_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let wasm_path = temp_dir.join("plugin.wasm");
        std::fs::write(&wasm_path, build_test_component()).unwrap();

        let plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, TEST_PLUGIN_ID, host_ctx)
            .expect("auto-detect should load component");
        assert!(matches!(plugin, LoadedWasmPlugin::Component(_)));

        let _ = std::fs::remove_dir_all(&temp_dir);
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
        let cache_path = std::env::temp_dir()
            .join(format!("bedcode_aot_{}", std::process::id()))
            .join(format!("c{:016x}.cwasm", aot_cache_key(&wasm_path)));
        std::fs::write(&wasm_path, build_test_component()).unwrap();

        // 首次编译：生成缓存产物
        let component = wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("first compile should succeed");
        assert!(cache_path.exists(), "component AOT cache file should be written");

        // 再次加载：命中缓存（mtime 未变）
        let cached = wasm_runtime
            .compile_component_from_file(&wasm_path)
            .expect("cached load should succeed");

        for c in [component, cached] {
            wasm_runtime
                .instantiate_component(&c, TEST_PLUGIN_ID, host_ctx.clone())
                .expect("component from cache should instantiate");
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
