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

/// 已加载的 WASM 插件（迁移阶段 C：组件形态唯一）
///
/// 类型别名 `pub use component::LoadedWasmPlugin`（见文件头）保留历史名称：
/// 宿主各模块（host.rs 等）以 `LoadedWasmPlugin` 引用插件实例，
/// 方法接口与迁移前枚举完全一致。

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
        Ok(component::LoadedWasmPlugin::new(
            &self.engine,
            &self.linker,
            component,
            plugin_id,
            host_ctx,
        )?)
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
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/src/wasm_host.rs"),
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
            .join(format!("c{:016x}.cwasm", aot_cache_key(&wasm_path)));
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
}
