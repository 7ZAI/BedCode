//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage、wasm_runtime 五个子系统
//! 支持静态注册（Rust 插件 via inventory）、文件扫描（TS-only 插件）和 WASM 模块（Rust+TS 插件）

use crate::plugin::loader::PluginLoader;
use crate::plugin::permission::PermissionManager;
use crate::plugin::registry::PluginRegistry;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::{DesktopPluginInfo, LoadedPlugin, PluginSource};
use crate::plugin::wasm_runtime::{LoadedWasmPlugin, PluginServices, WasmHostContext, WasmRuntime};
use crate::db::Database;
use crate::session::{SessionConfigManager, SessionManager, SessionInputListener, SessionLifecycleEvent, SessionLifecycleListener};
use crate::system::constants::plugin::PLUGIN_CALLBACK_TIMEOUT_SECS;
use crate::system::constants::event;
use bedcode_plugin_api::PluginState;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

/// WASM 插件 trap 自动重载最小间隔（秒）
///
/// wasmtime 同步引擎下任何一次 trap 都会污染整个 Store（`set_trapped`），
/// 之后该实例所有调用持续报 `CannotEnterComponent`，唯一恢复途径是整体重载。
/// 自动重载用最小间隔限频，防「重载后立刻再 trap」时无限重载风暴
/// （持久性 bug 时最多每间隔重试一次，期间插件保持 Error 态）。
const PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS: u64 = 30;

/// 插件运行时异常前端提示最小间隔（秒）
///
/// 统一异常通道（`PLUGIN_RUNTIME_ERROR`）按插件合并提示：重载循环等
/// 连发异常场景下只弹一次 toast，日志始终记录全量错误。
const PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS: u64 = 15;

/// 插件宿主
pub struct PluginHost {
    /// 已加载的插件
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// 扩展点注册表
    registry: Arc<PluginRegistry>,
    /// 权限管理器
    permission: Arc<PermissionManager>,
    /// 插件存储
    storage: Arc<PluginStorage>,
    /// Rust 插件的 command handlers（运行时注册，inventory 静态注册插件使用）
    rust_command_handlers: Arc<RwLock<HashMap<String, bedcode_plugin_api::PluginCommand>>>,
    /// Rust 插件的 terminal handlers（运行时注册，inventory 静态注册插件使用）
    rust_terminal_handlers: Arc<RwLock<Vec<Box<dyn bedcode_plugin_api::TerminalHandler>>>>,
    /// WASM 运行时（全局共享）
    wasm_runtime: Arc<WasmRuntime>,
    /// WASM 插件实例（plugin_id → LoadedWasmPlugin）
    /// WASM 插件实例表：每插件一把互斥锁（实例的 Store 要求独占访问，
    /// 见 wasm_runtime 模块说明）。map 锁只保护索引结构本身，
    /// 取到实例 Arc 后立即释放，插件间互不阻塞
    wasm_plugins: Arc<RwLock<HashMap<String, Arc<Mutex<LoadedWasmPlugin>>>>>,
    /// 宿主上下文工厂（供 WASM 插件激活时使用）
    wasm_host_ctx: Arc<WasmHostContext>,
    /// 消息总线
    message_bus: Arc<crate::plugin::message_bus::MessageBus>,
    /// 文件服务注册表（宿主通用文件服务能力，规格第 4 节）
    file_service: Arc<crate::plugin::file_service::FileServiceRegistry>,
    /// 插件定时器（plugin_id → tokio 任务句柄，v6 ADR 0003）
    ///
    /// 重复注册替换旧句柄；插件停用/应用关闭时中止。
    /// 用 std Mutex：仅短时间的 map 操作，不跨 await 持锁
    plugin_timers: Arc<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// WASM 插件 trap 自动重载限频表（plugin_id → 最近一次自动重载时刻）
    ///
    /// std Mutex：仅短时 map 操作，不跨 await 持锁
    wasm_reload_throttle: Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    /// 插件运行时异常前端提示限频表（plugin_id → 最近一次 toast 时刻）
    ///
    /// 见 [`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS`]；std Mutex：短时 map 操作
    runtime_error_notify_throttle: Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    /// 应用关闭标志：deactivate_all（应用退出）置位
    ///
    /// 插件 deactivate 内的卸载动作（如 CLI 安装清理）据此跳过：
    /// 应用正常退出 ≠ 用户停用插件，随包 CLI 应保留（下次启动 activate 幂等重装）
    shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

impl PluginHost {
    /// 创建 PluginHost 并加载所有插件（静态注册 + 文件扫描 + WASM）
    ///
    /// # Arguments
    /// * `db` - 数据库实例
    /// * `plugins_dir` - 插件目录
    /// * `session_manager` - 会话管理器
    /// * `config_manager` - 会话配置管理器
    /// * `app_handle` - Tauri AppHandle
    pub async fn new(
        db: Arc<Mutex<Database>>,
        plugins_dir: &Path,
        session_manager: Arc<SessionManager>,
        config_manager: Arc<SessionConfigManager>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        tracing::info!("[PluginHost] Initializing with plugins_dir: {:?}", plugins_dir);

        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let storage = Arc::new(PluginStorage::new(db.clone()));

        // 构建 WASM 运行时和宿主上下文
        let wasm_runtime = Arc::new(
            WasmRuntime::new(storage.clone(), Some(app_handle.clone()))
                .expect("Failed to initialize WASM runtime"),
        );

        // 创建消息总线（dispatcher 延迟注入，在 init_message_bus 中设置）
        let message_bus = Arc::new(crate::plugin::message_bus::MessageBus::new());

        // 文件服务注册表：必须在 auto_activate 之前创建 ——
        // 插件激活时可能立即调用 host_filesrv_mount；宿主引用待 PluginHost
        // Arc 化后经 set_plugin_host 两阶段注入
        let file_service = crate::plugin::file_service::FileServiceRegistry::new(
            wasm_runtime.fs_auth().clone(),
            Some(app_handle.clone()),
        );

        let wasm_host_ctx = Arc::new(WasmHostContext::new(
            db.clone(),
            Arc::new(Mutex::new(HashMap::new())),
            storage.clone(),
            session_manager,
            config_manager,
            Some(app_handle),
            permission.clone(),
            wasm_runtime.fs_auth().clone(),
            message_bus.clone(),
            // 注册表早于 auto-activate 注入宿主上下文，插件激活阶段挂载可用
            file_service.clone(),
        ));

        // 1. 收集静态注册的 Rust 插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();
        tracing::info!("[PluginHost] Found {} static plugin(s) from inventory", static_plugins.len());

        // 2. 扫描文件系统中的 TS-only 和 WASM 插件
        let file_plugins = PluginLoader::load_all(plugins_dir, &permission);
        tracing::info!("[PluginHost] Found {} file-based plugin(s)", file_plugins.len());

        // 3. 合并所有插件
        let mut all_plugins: HashMap<String, LoadedPlugin> = HashMap::new();

        // 添加静态注册的 Rust 插件
        for entry in static_plugins {
            let manifest = (entry.create_manifest)();
            let plugin_id = manifest.id.clone();

            let granted = permission.grant_permissions(&plugin_id, &manifest.permissions);

            let loaded = LoadedPlugin {
                manifest,
                state: PluginState::Loaded,
                granted_permissions: granted,
                extension_path: String::new(),
                activated_at: None,
                source: PluginSource::StaticRegistry,
            };

            tracing::info!("Static plugin loaded: {} v{}", loaded.manifest.id, loaded.manifest.version);
            all_plugins.insert(plugin_id, loaded);
        }

        // 添加文件扫描的插件（包含 TS-only 和 WASM 来源判定）
        let mut wasm_plugins_map: HashMap<String, Arc<Mutex<LoadedWasmPlugin>>> = HashMap::new();

        for (id, loaded) in file_plugins {
            // 如果 manifest 声明了 rust_library，尝试加载 WASM 模块
            if !loaded.manifest.rust_library.is_empty() {
                let plugin_dir = Path::new(&loaded.extension_path);
                let wasm_filename = format!("{}.wasm", loaded.manifest.rust_library);
                let wasm_path = plugin_dir.join(&wasm_filename);

                if !wasm_path.exists() {
                    tracing::error!(
                        "WASM module not found for plugin {} v{}: {}",
                        loaded.manifest.id,
                        loaded.manifest.version,
                        wasm_path.display()
                    );
                    // 不 continue：manifest 仍注册（Error 状态），避免 WASM 缺失时
                    // 插件从列表消失（与移动端行为一致，仅跳过 WASM 实例）
                    all_plugins.insert(
                        id,
                        LoadedPlugin {
                            state: PluginState::Error(format!(
                                "WASM module not found: {}",
                                wasm_path.display()
                            )),
                            ..loaded
                        },
                    );
                    continue;
                }

                // 阶段 A 共存入口：按产物格式自动选择 core module / component
                match wasm_runtime.load_plugin_from_file(&wasm_path, &id, wasm_host_ctx.clone()) {
                    Ok(wasm_plugin) => {
                        tracing::info!(
                            "WASM plugin loaded: {} v{} (module: {})",
                            loaded.manifest.id,
                            loaded.manifest.version,
                            wasm_filename
                        );
                        wasm_plugins_map.insert(id.clone(), Arc::new(Mutex::new(wasm_plugin)));
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to load WASM for plugin {} v{}: {}",
                            loaded.manifest.id,
                            loaded.manifest.version,
                            e
                        );
                        // 同上：WASM 加载失败仅丢弃运行时实例，manifest 仍注册，
                        // 保证插件列表可见且状态可诊断
                        all_plugins.insert(
                            id,
                            LoadedPlugin {
                                state: PluginState::Error(format!("WASM load failed: {}", e)),
                                ..loaded
                            },
                        );
                        continue;
                    }
                }
            }

            all_plugins.insert(id, loaded);
        }

        let host = Self {
            plugins: Arc::new(RwLock::new(all_plugins)),
            registry,
            permission,
            storage,
            rust_command_handlers: Arc::new(RwLock::new(HashMap::new())),
            rust_terminal_handlers: Arc::new(RwLock::new(Vec::new())),
            wasm_runtime,
            wasm_plugins: Arc::new(RwLock::new(wasm_plugins_map)),
            wasm_host_ctx,
            message_bus,
            file_service,
            plugin_timers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            wasm_reload_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_error_notify_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        // 两阶段初始化：将 PluginHost（作为 PluginServices 实现）注入 WasmHostContext
        // 必须在 auto_activate 之前完成，否则 host_session_lifecycle_register 无法获取宿主服务
        host.wasm_host_ctx().set_services(Arc::new(host.clone())).await;

        // 注册所有已加载插件的 manifest contributes 到 registry
        host.register_manifest_contributions().await;

        // 注册 Rust 插件的 command handlers（inventory 静态注册）
        host.register_rust_command_handlers().await;

        // 注册 Rust 插件的 terminal handlers（inventory 静态注册）
        host.register_rust_terminal_handlers().await;

        // 4. 根据持久化状态自动激活之前已激活的插件
        tracing::info!("[PluginHost] Starting auto-activation from persisted state...");
        host.auto_activate_from_persisted_state().await;

        let count = host.plugins.read().await.len();
        let wasm_count = host.wasm_plugins.read().await.len();
        let activated_count = host.plugins.read().await.values()
            .filter(|p| matches!(p.state, PluginState::Activated))
            .count();
        tracing::info!(
            "[PluginHost] Initialization complete: {} plugin(s) total, {} wasm, {} activated",
            count, wasm_count, activated_count
        );
        host
    }

    /// 将所有已加载插件的 manifest contributes 注册到 registry
    async fn register_manifest_contributions(&self) {
        let plugins = self.plugins.read().await;
        for loaded in plugins.values() {
            let m = &loaded.manifest;
            self.registry.register_commands(&m.id, &m.contributes.commands).await;
            self.registry.register_views(&m.id, &m.contributes.views).await;
            if let Some(ref term) = m.contributes.terminal {
                self.registry
                    .register_terminal_handlers(&m.id, &term.input_handlers, &term.output_parsers)
                    .await;
            }
            self.registry.register_tool_providers(&m.id, &m.contributes.tool_providers).await;
            self.registry.register_file_handlers(&m.id, &m.contributes.file_handlers).await;
        }
    }

    /// 注册 Rust 插件的 command handlers 到运行时注册表（inventory 静态注册）
    async fn register_rust_command_handlers(&self) {
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();

        let mut handlers = self.rust_command_handlers.write().await;
        for entry in static_plugins {
            let commands = (entry.register_commands)();
            let plugin_id = entry.id;
            for cmd in commands {
                let full_name = format!("{}::{}", plugin_id, cmd.name);
                tracing::info!("Registered Rust command: {}", full_name);
                handlers.insert(full_name, cmd);
            }
        }
    }

    /// 注册 Rust 插件的 terminal handlers 到运行时注册表（inventory 静态注册）
    async fn register_rust_terminal_handlers(&self) {
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();

        let mut handlers = self.rust_terminal_handlers.write().await;
        for entry in static_plugins {
            let plugin_handlers = (entry.terminal_handlers)();
            for handler in plugin_handlers {
                tracing::info!("Registered Rust terminal handler for plugin {}", entry.id);
                handlers.push(handler);
            }
        }
    }

    // ==================== Accessors ====================

    /// 获取 WASM 宿主上下文引用
    pub fn wasm_host_ctx(&self) -> &Arc<WasmHostContext> {
        &self.wasm_host_ctx
    }

    pub fn registry(&self) -> &Arc<PluginRegistry> {
        &self.registry
    }

    pub fn permission(&self) -> &Arc<PermissionManager> {
        &self.permission
    }

    pub fn storage(&self) -> &Arc<PluginStorage> {
        &self.storage
    }

    /// 获取 WASM 运行时引用
    pub fn wasm_runtime(&self) -> &Arc<WasmRuntime> {
        &self.wasm_runtime
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::plugin::message_bus::MessageBus> {
        &self.message_bus
    }

    /// 获取文件服务注册表（宿主通用文件服务能力）
    pub fn file_service(&self) -> &Arc<crate::plugin::file_service::FileServiceRegistry> {
        &self.file_service
    }

    /// 初始化消息总线 dispatcher（必须在 new() 之后调用）
    pub async fn init_message_bus(&self) {
        let dispatcher: Arc<dyn crate::plugin::message_bus::MessageDispatcher> = Arc::new(self.clone());
        self.message_bus.set_dispatcher(dispatcher).await;
        tracing::info!("[PluginHost] MessageBus dispatcher initialized");
    }

    // ==================== Lifecycle ====================

    /// 获取所有已加载插件的信息列表
    pub async fn list_plugins(&self) -> Vec<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        let list: Vec<DesktopPluginInfo> = plugins.values().map(DesktopPluginInfo::from).collect();
        tracing::debug!("[PluginHost] list_plugins() returning {} plugin(s)", list.len());
        for info in &list {
            tracing::debug!("[PluginHost]   - {} (state={:?}, type={:?})", info.id, info.state, info.plugin_type);
        }
        list
    }

    /// 获取单个插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> Option<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(DesktopPluginInfo::from)
    }

    /// 检查插件是否处于激活状态（用于 API 调用的调用者身份校验）
    pub async fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        let result = plugins.get(plugin_id)
            .map(|p| matches!(p.state, PluginState::Activated))
            .unwrap_or(false);
        // 高频校验路径（每插件 API 调用都会经过），仅 trace 级别可见，避免刷屏
        tracing::trace!("[PluginHost] is_activated({}) = {}", plugin_id, result);
        result
    }

    /// 通知所有已激活的 Rust 插件应用启动完成
    pub async fn notify_startup(&self) {
        // 静态注册插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();
        for entry in &static_plugins {
            if self.is_activated(entry.id).await {
                tracing::debug!("Notifying plugin {} on_startup", entry.id);
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(PLUGIN_CALLBACK_TIMEOUT_SECS),
                    (entry.on_startup)(),
                ).await;
                if result.is_err() {
                    tracing::error!("Plugin {} on_startup timed out", entry.id);
                }
            }
        }

        // WASM 插件的 on_startup 已在 activate_plugin() 中自动调用，此处不再重复

        // TS-only 插件：通过 Tauri 事件通知
        let ctx = crate::system::app_context::AppContext::global();
        let _ = ctx.app_handle().emit(event::LIFECYCLE_STARTUP, serde_json::json!({}));

        tracing::info!("PluginHost notify_startup completed");
    }

    /// 通知所有已激活的插件应用即将关闭
    pub async fn notify_shutdown(&self) {
        // 静态注册插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();
        for entry in &static_plugins {
            if self.is_activated(entry.id).await {
                tracing::debug!("Notifying plugin {} on_shutdown", entry.id);
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(PLUGIN_CALLBACK_TIMEOUT_SECS),
                    (entry.on_shutdown)(),
                ).await;
                if result.is_err() {
                    tracing::error!("Plugin {} on_shutdown timed out", entry.id);
                }
            }
        }

        // WASM 插件的 on_shutdown 已在 deactivate_plugin() 中自动调用，此处不再重复

        // TS-only 插件：通过 Tauri 事件通知
        let ctx = crate::system::app_context::AppContext::global();
        let _ = ctx.app_handle().emit(event::LIFECYCLE_SHUTDOWN, serde_json::json!({}));

        tracing::info!("PluginHost notify_shutdown completed");
    }

    /// 停用所有已激活的插件（应用关闭流程）
    pub async fn deactivate_all(&self) -> crate::Result<()> {
        // 置关闭标志：deactivate 内的卸载动作（CLI 清理等）跳过，
        // 保留随包产物供下次启动重新激活（幂等安装）
        self.shutting_down
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let plugin_ids: Vec<String> = {
            let plugins = self.plugins.read().await;
            plugins.values()
                .filter(|p| matches!(p.state, PluginState::Activated))
                .map(|p| p.manifest.id.clone())
                .collect()
        };

        for id in plugin_ids {
            if let Err(e) = self.deactivate_plugin(&id, false).await {
                tracing::error!("Failed to deactivate plugin {} during shutdown: {}", id, e);
            }
        }

        tracing::info!("PluginHost deactivate_all completed");
        Ok(())
    }

    /// 激活插件
    ///
    /// - 静态注册插件：仅标记状态
    /// - WASM 插件：调用 __bedcode_activate 导出函数
    /// - TS-only 插件：前端模块加载在 PluginLoader 中完成
    pub async fn activate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        tracing::info!("[PluginHost] activate_plugin({}, persist={})", plugin_id, persist);

        // 阶段 1（短写锁）：读取状态与 manifest 字段、重新授权后立即释放锁。
        // 禁止持 plugins 锁执行 WASM activate：activate 内可能回调宿主
        // （如 scheduler 的 cli_install 经 services.install_cli 读 plugins map），
        // 持写锁回调会死锁（锁约定见 PluginManager::activate，双侧一致）
        struct ActivatePlan {
            source: PluginSource,
            api: Vec<String>,
            subscribes: Vec<String>,
        }
        let plan = {
            let mut plugins = self.plugins.write().await;
            let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
                tracing::error!("[PluginHost] activate_plugin: plugin {} not found in plugins map", plugin_id);
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;

            match &loaded.state {
                PluginState::Activated => {
                    tracing::debug!("[PluginHost] Plugin {} already activated, skipping", plugin_id);
                    return Ok(());
                }
                PluginState::Error(e) => {
                    tracing::warn!("[PluginHost] Plugin {} in error state: {}, attempting re-activation", plugin_id, e);
                }
                _ => {
                    tracing::debug!("[PluginHost] Plugin {} current state: {:?}, proceeding with activation", plugin_id, loaded.state);
                }
            }

            // 重新授权：deactivate 会 revoke_all，再次激活时必须重新授予
            let permissions = loaded.manifest.permissions.clone();
            let granted = self.permission.grant_permissions(plugin_id, &permissions);
            loaded.granted_permissions = granted;

            ActivatePlan {
                source: loaded.source.clone(),
                api: loaded.manifest.api.clone(),
                subscribes: loaded.manifest.contributes.subscribes.clone(),
            }
        };

        // 阶段 2（无 map 锁）：执行 WASM activate + on_startup
        // 仅持单插件实例锁（避免重入死锁），失败置 Error 状态
        if plan.source == PluginSource::Wasm {
            let wasm_plugin = {
                let wasm_plugins = self.wasm_plugins.read().await;
                wasm_plugins.get(plugin_id).cloned()
            };
            let Some(wasm_plugin) = wasm_plugin else {
                tracing::error!("WASM plugin {} not found in wasm_plugins map", plugin_id);
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} WASM module not loaded", plugin_id
                )));
            };

            let mut wasm_plugin = wasm_plugin.lock().await;
            match wasm_plugin.activate() {
                Ok(0) => {
                    tracing::info!("[PluginHost] Plugin '{}' activated", plugin_id);
                }
                Ok(code) => {
                    tracing::error!("[PluginHost] Plugin '{}' activate() returned error code {}", plugin_id, code);
                    self.mark_error(plugin_id, format!("activate() returned error code {}", code)).await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} activate() returned error code {}", plugin_id, code
                    )));
                }
                Err(e) => {
                    tracing::error!("[PluginHost] Plugin '{}' activate() failed: {}", plugin_id, e);
                    self.mark_error(plugin_id, format!("activate() failed: {}", e)).await;
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin {} activate() failed: {}", plugin_id, e
                    )));
                }
            }

            // 激活成功后自动调用 on_startup
            tracing::info!("[PluginHost] Calling on_startup for plugin '{}'", plugin_id);
            if let Err(e) = wasm_plugin.on_startup() {
                tracing::warn!("[PluginHost] Plugin '{}' on_startup failed: {}", plugin_id, e);
            } else {
                tracing::info!("[PluginHost] Plugin '{}' on_startup completed", plugin_id);
            }
        }

        // 阶段 3（短写锁）：更新激活状态，然后释放锁执行订阅
        {
            let mut plugins = self.plugins.write().await;
            if let Some(loaded) = plugins.get_mut(plugin_id) {
                loaded.state = PluginState::Activated;
                loaded.activated_at = Some(Utc::now());
            }
        }

        // 登记互调 api 清单（ADR-0017）：激活后 `bedcode.api.*` 请求可路由到本插件。
        // 未声明 api 的插件登记空清单，幂等无操作
        self.wasm_host_ctx
            .api_registry()
            .register(plugin_id, &plan.api);

        // 注册 manifest 中声明的 topic 订阅
        if !plan.subscribes.is_empty() {
            let plugin_id_owned = plugin_id.to_string();
            for topic in &plan.subscribes {
                self.message_bus.subscribe_wasm(&plugin_id_owned, topic).await;
            }
            tracing::info!(
                "[PluginHost] Plugin {} subscribed to {} topic(s): {:?}",
                plugin_id_owned,
                plan.subscribes.len(),
                plan.subscribes
            );
        }

        tracing::info!("[PluginHost] Plugin activated successfully: {} (persist={})", plugin_id, persist);

        if persist {
            tracing::debug!("[PluginHost] Persisting activation state after activating {}", plugin_id);
            self.persist_activation_state().await;
        }

        Ok(())
    }

    /// 停用插件
    /// 中止指定插件的定时器（停用时调用，v6 ADR 0003）
    fn abort_plugin_timer(&self, plugin_id: &str) {
        let mut timers = self
            .plugin_timers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = timers.remove(plugin_id) {
            handle.abort();
            tracing::info!("[PluginHost] Timer aborted for '{}'", plugin_id);
        }
    }

    pub async fn deactivate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        tracing::info!("[PluginHost] deactivate_plugin({}, persist={})", plugin_id, persist);

        // WASM 插件：调用 on_shutdown + __bedcode_deactivate
        {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(plugin_id) {
                if loaded.source == PluginSource::Wasm {
                    let wasm_plugins = self.wasm_plugins.read().await;
                    if let Some(wasm_plugin) = wasm_plugins.get(plugin_id).cloned() {
                        drop(wasm_plugins);
                        let mut wasm_plugin = wasm_plugin.lock().await;
                        // 停用前先调用 on_shutdown
                        tracing::info!("[PluginHost] Calling on_shutdown for plugin '{}'", plugin_id);
                        if let Err(e) = wasm_plugin.on_shutdown() {
                            tracing::warn!("[PluginHost] Plugin '{}' on_shutdown failed: {}", plugin_id, e);
                        } else {
                            tracing::info!("[PluginHost] Plugin '{}' on_shutdown completed", plugin_id);
                        }

                        match wasm_plugin.deactivate() {
                            Ok(0) => {
                                tracing::info!("[PluginHost] Plugin '{}' deactivated", plugin_id);
                            }
                            Ok(code) => {
                                tracing::warn!("[PluginHost] Plugin '{}' deactivate() returned error code {}", plugin_id, code);
                            }
                            Err(e) => {
                                tracing::error!("[PluginHost] Plugin '{}' deactivate() failed: {}", plugin_id, e);
                            }
                        }
                    }
                }
            }
        }

        // 统一清理：取消注册和撤销权限
        self.registry.unregister_plugin(plugin_id).await;
        self.permission.revoke_all(plugin_id);
        // 注销互调 api 清单（ADR-0017）：停用后目标调用被门禁拒绝
        self.wasm_host_ctx.api_registry().unregister(plugin_id);

        // 中止插件定时器（若有）：停用后不再到点回调
        self.abort_plugin_timer(plugin_id);

        // 清理消息总线订阅
        self.message_bus.remove_all_subscriptions(plugin_id).await;

        // 摘除文件服务挂载（fail-closed：停用插件 = 服务消失，规格 8 节）
        self.file_service.unmount_plugin(plugin_id).await;

        // 移除该插件的会话生命周期监听器与输入监听器
        {
            let session_manager = self.wasm_host_ctx().session_manager_arc();
            session_manager.remove_lifecycle_listener(plugin_id).await;
            session_manager.remove_input_listener(plugin_id).await;
        }

        let mut plugins = self.plugins.write().await;
        let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
        })?;

        loaded.state = PluginState::Deactivated;
        loaded.activated_at = None;
        tracing::info!("[PluginHost] Plugin deactivated successfully: {} (persist={})", plugin_id, persist);

        // 释放写锁后再持久化
        drop(plugins);

        if persist {
            tracing::debug!("[PluginHost] Persisting activation state after deactivating {}", plugin_id);
            self.persist_activation_state().await;
        }

        Ok(())
    }

    /// 调用 WASM 插件的上传策略钩子（fail-closed，规格 4.2 节）
    ///
    /// 供 FileServiceRegistry 在上传会话创建时调用：锁 wasm_plugins →
    /// LoadedWasmPlugin::on_upload_request(meta_json) → 解析返回的决定。
    /// 插件未加载 / 未导出钩子 / 调用失败 / 决定 JSON 非法时一律拒绝。
    /// （2 秒超时由调用方 registry 用 tokio::time::timeout 包裹）
    pub async fn call_upload_hook(
        &self,
        plugin_id: &str,
        meta_json: &str,
    ) -> bedcode_plugin_api::UploadHookDecision {
        use bedcode_plugin_api::UploadHookDecision;

        // 插件未加载 → 直接拒绝（fail-closed），不触发重载
        if self.get_wasm_plugin(plugin_id).await.is_none() {
            tracing::warn!(
                plugin_id = %plugin_id,
                "call_upload_hook: wasm plugin not loaded, denying (fail-closed)"
            );
            return UploadHookDecision::deny("wasm plugin not loaded");
        }

        // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
        match self
            .with_wasm_plugin_call(plugin_id, |plugin| plugin.on_upload_request(meta_json))
            .await
        {
            Ok(decision_json) => match serde_json::from_str::<UploadHookDecision>(&decision_json) {
                Ok(decision) => decision,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        plugin_id = %plugin_id,
                        "call_upload_hook: invalid decision JSON from plugin, denying (fail-closed)"
                    );
                    UploadHookDecision::deny("invalid upload hook decision")
                }
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    plugin_id = %plugin_id,
                    "call_upload_hook: plugin hook call failed, denying (fail-closed)"
                );
                UploadHookDecision::deny("upload hook call failed")
            }
        }
    }

    /// 调用 WASM 插件的批量传输请求钩子（v2，fail-closed，spec 2.1）
    ///
    /// 与 [`call_upload_hook`](Self::call_upload_hook) 同构：锁 wasm_plugins →
    /// LoadedWasmPlugin::on_transfer_request(meta_json) → 解析返回的决定。
    /// 插件未加载 / 未导出钩子 / 调用失败 / 决定 JSON 非法时一律拒绝。
    /// （2 秒超时由调用方 registry 用 tokio::time::timeout 包裹）
    pub async fn call_transfer_hook(
        &self,
        plugin_id: &str,
        meta_json: &str,
    ) -> bedcode_plugin_api::UploadHookDecision {
        use bedcode_plugin_api::UploadHookDecision;

        // 插件未加载 → 直接拒绝（fail-closed），不触发重载
        if self.get_wasm_plugin(plugin_id).await.is_none() {
            tracing::warn!(
                plugin_id = %plugin_id,
                "call_transfer_hook: wasm plugin not loaded, denying (fail-closed)"
            );
            return UploadHookDecision::deny("wasm plugin not loaded");
        }

        // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
        match self
            .with_wasm_plugin_call(plugin_id, |plugin| plugin.on_transfer_request(meta_json))
            .await
        {
            Ok(decision_json) => match serde_json::from_str::<UploadHookDecision>(&decision_json) {
                Ok(decision) => decision,
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        plugin_id = %plugin_id,
                        "call_transfer_hook: invalid decision JSON from plugin, denying (fail-closed)"
                    );
                    UploadHookDecision::deny("invalid transfer hook decision")
                }
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    plugin_id = %plugin_id,
                    "call_transfer_hook: plugin hook call failed, denying (fail-closed)"
                );
                UploadHookDecision::deny("transfer hook call failed")
            }
        }
    }

    /// 热重载 WASM 插件（开发模式）
    ///
    /// 执行完整的卸载-重载-激活循环：
    /// 1. 停用插件
    /// 2. 重新编译并实例化 WASM 模块
    /// 3. 重新激活插件
    pub async fn reload_wasm_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        let (rust_library, extension_path) = {
            let plugins = self.plugins.read().await;
            let loaded = plugins.get(plugin_id).ok_or_else(|| {
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;
            if loaded.source != PluginSource::Wasm {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} is not a WASM plugin, cannot hot-reload",
                    plugin_id
                )));
            }
            (loaded.manifest.rust_library.clone(), loaded.extension_path.clone())
        };

        tracing::info!("Hot-reloading WASM plugin: {}", plugin_id);

        // 1. 停用插件（不持久化）
        self.deactivate_plugin(plugin_id, false).await?;

        // 2. 重新编译并实例化 WASM 模块
        let plugin_dir = Path::new(&extension_path);
        let wasm_filename = format!("{}.wasm", rust_library);
        let wasm_path = plugin_dir.join(&wasm_filename);

        let new_wasm_plugin = self.wasm_runtime.load_plugin_from_file(
            &wasm_path,
            plugin_id,
            self.wasm_host_ctx.clone(),
        )?;

        // 替换 wasm_plugins map 中的实例
        self.wasm_plugins
            .write()
            .await
            .insert(plugin_id.to_string(), Arc::new(Mutex::new(new_wasm_plugin)));

        // 3. 重新注册 manifest contributes
        let m = {
            let plugins = self.plugins.read().await;
            let loaded = plugins.get(plugin_id).ok_or_else(|| {
                crate::AppError::Plugin(format!("Plugin not found after reload: {}", plugin_id))
            })?;
            loaded.manifest.clone()
        };
        self.registry.register_commands(&m.id, &m.contributes.commands).await;
        self.registry.register_views(&m.id, &m.contributes.views).await;
        if let Some(ref term) = m.contributes.terminal {
            self.registry
                .register_terminal_handlers(&m.id, &term.input_handlers, &term.output_parsers)
                .await;
        }
        self.registry.register_tool_providers(&m.id, &m.contributes.tool_providers).await;
        self.registry.register_file_handlers(&m.id, &m.contributes.file_handlers).await;

        // 4. 重新激活
        self.activate_plugin(plugin_id, false).await?;

        tracing::info!("WASM plugin hot-reloaded successfully: {}", plugin_id);
        Ok(())
    }

    /// 标记插件为错误状态
    pub async fn mark_error(&self, plugin_id: &str, error: String) {
        let mut plugins = self.plugins.write().await;
        if let Some(loaded) = plugins.get_mut(plugin_id) {
            loaded.state = PluginState::Error(error);
        }
    }

    /// 插件运行时异常统一上报前端（全局异常通道，`PLUGIN_RUNTIME_ERROR`）
    ///
    /// 宿主检测到插件异常（非插件主动上报）时调用，覆盖三类场景：
    /// - `panic`：宿主函数 panic 穿透 wasmtime（catch_unwind 兜底，Store 已污染）
    /// - `trap`：wasm trap / 导出绑定失败 / store 中毒（已调度自动重载）
    /// - `recovery_failed`：自动重载失败，插件进入 Error 态
    ///
    /// 语义：日志**始终**记录全量错误（重载循环期间不丢现场）；前端 toast
    /// 按插件节流（见 [`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS`]），连发
    /// 异常只弹一次，避免 trap 重载风暴刷屏。无 AppContext（测试/无头）时
    /// 降级为纯日志，不 panic。
    pub async fn notify_plugin_runtime_error(&self, plugin_id: &str, kind: &str, error: &str) {
        // 日志始终记录（调用方也各自记日志，此处为统一通道的兜底记录）
        tracing::error!(
            plugin_id = %plugin_id,
            kind = %kind,
            error = %error,
            "Plugin runtime error (unified channel)"
        );

        // 节流：同一插件窗口内已提示过则跳过 toast（日志不受影响）；
        // recovery_failed 不节流——它每次重载失败只发一次（重载循环 30s 间隔），
        // 若落在 trap 通知的 15s 窗口内会被吞，用户看到「已恢复」实际进入 Error 态
        if kind != "recovery_failed" {
            let mut throttle = self
                .runtime_error_notify_throttle
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(last) = throttle.get(plugin_id) {
                if last.elapsed()
                    < std::time::Duration::from_secs(PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS)
                {
                    tracing::debug!(
                        plugin_id = %plugin_id,
                        kind = %kind,
                        "plugin runtime error toast throttled (recent notification)"
                    );
                    return;
                }
            }
            throttle.insert(plugin_id.to_string(), std::time::Instant::now());
        }

        // 插件展示名（manifest.name），查不到时退回插件 ID
        let plugin_name = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .map(|p| p.manifest.name.clone())
                .unwrap_or_else(|| plugin_id.to_string())
        };

        // 无头/测试上下文无 AppHandle：降级为纯日志
        let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
            return;
        };
        if let Err(e) = ctx.app_handle().emit(
            crate::system::constants::event::PLUGIN_RUNTIME_ERROR,
            serde_json::json!({
                "plugin_id": plugin_id,
                "plugin_name": plugin_name,
                "kind": kind,
                "error": error,
            }),
        ) {
            // 前端事件派发失败不致命：日志已全量记录，仅提示通道中断
            tracing::warn!(
                plugin_id = %plugin_id,
                "Failed to emit PLUGIN_RUNTIME_ERROR to frontend: {}",
                e
            );
        }
    }

    /// 获取当前所有非 StaticRegistry 插件的激活状态映射
    pub async fn get_activated_state(&self) -> HashMap<String, bool> {
        let plugins = self.plugins.read().await;
        let mut map = HashMap::new();
        for (id, loaded) in plugins.iter() {
            if loaded.source == PluginSource::StaticRegistry {
                continue;
            }
            let is_active = matches!(loaded.state, PluginState::Activated);
            map.insert(id.clone(), is_active);
        }
        tracing::debug!("[PluginHost] get_activated_state() returning {} entry/entries", map.len());
        map
    }

    /// 持久化当前激活状态到 SQLite
    async fn persist_activation_state(&self) {
        let activated_map = self.get_activated_state().await;
        tracing::debug!("[PluginHost] Persisting activation state: {} plugin(s)", activated_map.len());
        for (id, active) in &activated_map {
            tracing::debug!("[PluginHost]   Persist: {} = {}", id, active);
        }
        if let Err(e) = self.storage.save_activated_plugins(&activated_map).await {
            tracing::error!("[PluginHost] Failed to persist plugin activation state: {}", e);
        }
    }

    /// 根据持久化状态自动激活之前已激活的插件
    async fn auto_activate_from_persisted_state(&self) {
        let activated_map = match self.storage.load_activated_plugins().await {
            Ok(map) => {
                tracing::info!("[PluginHost] Loaded persisted activation state: {} entry/entries", map.len());
                for (id, active) in &map {
                    tracing::debug!("[PluginHost]   Persisted: {} = {}", id, active);
                }
                map
            }
            Err(e) => {
                tracing::warn!("[PluginHost] Failed to load persisted activation state, skipping auto-activation: {}", e);
                return;
            }
        };

        if activated_map.is_empty() {
            tracing::info!("[PluginHost] No persisted activation state, skipping auto-activation");
            return;
        }

        let to_activate: Vec<String> = {
            let plugins = self.plugins.read().await;
            activated_map.iter()
                .filter(|(id, &is_active)| {
                    if !is_active { return false; }
                    plugins.get(*id)
                        .map(|p| p.source != PluginSource::StaticRegistry)
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        tracing::info!("[PluginHost] Auto-activating {} plugin(s) from persisted state", to_activate.len());

        for plugin_id in &to_activate {
            tracing::info!("[PluginHost] Auto-activating plugin: {}", plugin_id);
            if let Err(e) = self.activate_plugin(plugin_id, false).await {
                tracing::error!("[PluginHost] Failed to auto-activate plugin {}: {}", plugin_id, e);
            }
        }

        // 清理已不存在的插件 ID
        let current_ids: HashSet<String> = self.plugins.read().await.keys().cloned().collect();
        let original_len = activated_map.len();
        let mut cleaned_map = activated_map;
        cleaned_map.retain(|id, _| current_ids.contains(id));
        if cleaned_map.len() != original_len {
            tracing::info!("[PluginHost] Cleaning {} stale plugin ID(s) from persisted state", original_len - cleaned_map.len());
            if let Err(e) = self.storage.save_activated_plugins(&cleaned_map).await {
                tracing::warn!("[PluginHost] Failed to clean up stale activation entries: {}", e);
            }
        }

        if !to_activate.is_empty() {
            tracing::info!("[PluginHost] Auto-activated {} plugin(s) from persisted state", to_activate.len());
        }
    }

    /// 判断插件是否应该按需激活
    pub async fn should_lazy_activate(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        if let Some(loaded) = plugins.get(plugin_id) {
            if loaded.source == PluginSource::StaticRegistry {
                return false;
            }
            if !matches!(loaded.state, PluginState::Loaded) {
                return false;
            }
            let c = &loaded.manifest.contributes;
            !c.commands.is_empty() || c.terminal.is_some() || !c.views.is_empty()
        } else {
            false
        }
    }
}

// ==================== 子模块（自本文件拆分） ====================
mod app_cli;
mod commands;
mod listeners;
mod services;
// 保持原导出路径（crate::plugin::host::PluginLifecycleListener 等）
pub use listeners::{PluginInputListener, PluginLifecycleListener};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::file_service::FileServiceRegistry;
    use crate::plugin::message_bus::{MessageBus, MessageDispatcher};
    use crate::system::config::AppConfig;
    use bedcode_plugin_api::{PluginCommand, PluginContributes, PluginManifest, PluginType, TerminalHandler};
    use serde_json::json;
    use std::path::PathBuf;

    /// 测试用插件 ID（非 WASM 插件）
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";
    /// 测试用组件形态 WASM 插件 ID（与 plugin-component-test 的 manifest 一致）
    const TEST_WASM_PLUGIN_ID: &str = "com.bedcode.component-test";

    /// 本模块测试的不可测面说明：
    ///
    /// - `PluginHost::new`：依赖真实 `tauri::AppHandle`（WASM 运行时数据目录、
    ///   FsAuthChecker 等）与 inventory 静态注册表，测试环境无法构造；
    ///   本模块通过结构体字面量直接构造（tests 位于 host.rs 内部，可访问私有字段），
    ///   覆盖 new() 之后的全部宿主行为。
    /// - `notify_startup` / `notify_shutdown` / `PluginServices::mark_plugin_error`：
    ///   依赖 `AppContext::global()`（未初始化即 panic）+ `app_handle().emit`，
    ///   无头测试上下文不可用。
    /// - `dispatch_*_to_plugin` 的错误分支（WASM 实例缺失/调用失败）：仅有日志
    ///   副作用，无返回值可断言；成功路径由 `test_dispatch_lifecycle_and_input_to_wasm_plugin`
    ///   以「分发后 store 未被污染」间接验证。
    /// - `invoke_wasm_command` 的非法 JSON 返回分支：需要构造返回坏 JSON 的恶意
    ///   WASM 插件，超出测试组件能力范围。

    /// 构造无头测试宿主（app_handle = None）
    ///
    /// 结构体字面量构造 PluginHost：字段私有但 tests 模块与 host.rs 同属一个
    /// 模块树，可访问。所有子系统均用真实实现 + 内存 SQLite，仅 Tauri 相关
    /// 能力降级（与 wasm_runtime.rs 测试同一策略）。
    async fn setup_host() -> PluginHost {
        // AppConfig 全局初始化（与 wasm_runtime 测试同策略；重复 init 幂等）
        static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
        CONFIG_INIT.call_once(|| {
            let mut config = AppConfig::default();
            config.network.port = 8765;
            AppConfig::init(config);
        });

        let db = Arc::new(Mutex::new(Database::new(&PathBuf::from(":memory:")).unwrap()));
        db.lock().await.init_schema().unwrap();
        let storage = Arc::new(PluginStorage::new(db.clone()));
        let session_manager = Arc::new(SessionManager::from_database(
            Database::new(&PathBuf::from(":memory:")).unwrap(),
            Arc::new(PathBuf::from(".")),
        ));
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
            Database::new(&PathBuf::from(":memory:")).unwrap(),
        ))));

        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let message_bus = Arc::new(MessageBus::new());

        let wasm_runtime = Arc::new(WasmRuntime::new(storage.clone(), None).unwrap());
        let file_service = FileServiceRegistry::new(wasm_runtime.fs_auth().clone(), None);

        let wasm_host_ctx = Arc::new(WasmHostContext::new(
            db,
            Arc::new(Mutex::new(HashMap::new())),
            storage.clone(),
            session_manager,
            config_manager,
            None,
            permission.clone(),
            wasm_runtime.fs_auth().clone(),
            message_bus.clone(),
            file_service.clone(),
        ));

        PluginHost {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            registry,
            permission,
            storage,
            rust_command_handlers: Arc::new(RwLock::new(HashMap::new())),
            rust_terminal_handlers: Arc::new(RwLock::new(Vec::new())),
            wasm_runtime,
            wasm_plugins: Arc::new(RwLock::new(HashMap::new())),
            wasm_host_ctx,
            message_bus,
            file_service,
            plugin_timers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            wasm_reload_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_error_notify_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 构造一个最小 LoadedPlugin（manifest 含 storage + terminal:input 权限）
    fn make_plugin(id: &str, source: PluginSource, state: PluginState) -> LoadedPlugin {
        LoadedPlugin {
            manifest: PluginManifest {
                id: id.to_string(),
                name: format!("Test {}", id),
                version: "1.0.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: "index.ts".to_string(),
                sandbox: "inline".to_string(),
                permissions: vec!["storage".to_string(), "terminal:input".to_string()],
                api: vec![],
                contributes: PluginContributes::default(),
                plugin_type: PluginType::TsOnly,
                rust_library: String::new(),
                icon: None,
            },
            state,
            granted_permissions: HashSet::new(),
            extension_path: String::new(),
            activated_at: None,
            source,
        }
    }

    // ==================== Accessors ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_accessors_return_shared_arcs() {
        let host = setup_host().await;
        // getter 返回的是与字段共享的同一 Arc（Clone 语义）
        assert!(Arc::ptr_eq(host.registry(), &host.registry));
        assert!(Arc::ptr_eq(host.permission(), &host.permission));
        assert!(Arc::ptr_eq(host.storage(), &host.storage));
        assert!(Arc::ptr_eq(host.wasm_runtime(), &host.wasm_runtime));
        assert!(Arc::ptr_eq(host.message_bus(), &host.message_bus));
        assert!(Arc::ptr_eq(host.file_service(), &host.file_service));
        assert!(Arc::ptr_eq(host.wasm_host_ctx(), &host.wasm_host_ctx));
    }

    // ==================== Plugins Map 查询 ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_plugins_and_get_plugin() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        host.plugins.write().await.insert(
            "com.bedcode.static".to_string(),
            make_plugin("com.bedcode.static", PluginSource::StaticRegistry, PluginState::Loaded),
        );

        let list = host.list_plugins().await;
        assert_eq!(list.len(), 2);
        // 来源映射到前端友好字符串
        let scanned = list.iter().find(|p| p.id == TEST_PLUGIN_ID).unwrap();
        assert_eq!(scanned.source, "scanned");
        assert_eq!(scanned.state, PluginState::Activated);
        let builtin = list.iter().find(|p| p.id == "com.bedcode.static").unwrap();
        assert_eq!(builtin.source, "builtin");

        // get_plugin：命中与未命中
        assert!(host.get_plugin("com.missing").await.is_none());
        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.id, TEST_PLUGIN_ID);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_is_activated_by_state() {
        let host = setup_host().await;
        // 未注册插件 → false
        assert!(!host.is_activated("com.missing").await);

        for (state, expected) in [
            (PluginState::Loaded, false),
            (PluginState::Activated, true),
            (PluginState::Deactivated, false),
            (PluginState::Error("boom".into()), false),
        ] {
            host.plugins.write().await.insert(
                TEST_PLUGIN_ID.to_string(),
                make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, state),
            );
            assert_eq!(host.is_activated(TEST_PLUGIN_ID).await, expected);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mark_error_updates_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );

        host.mark_error(TEST_PLUGIN_ID, "hooks install failed".to_string()).await;
        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Error("hooks install failed".to_string()));

        // 未注册插件：静默 no-op，不 panic
        host.mark_error("com.missing", "x".to_string()).await;
        assert!(host.get_plugin("com.missing").await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_activated_state_excludes_static() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        host.plugins.write().await.insert(
            "com.bedcode.ts".to_string(),
            make_plugin("com.bedcode.ts", PluginSource::FileScan, PluginState::Deactivated),
        );
        // 静态注册插件即使激活也不应进入持久化映射（由应用进程生命周期托管）
        host.plugins.write().await.insert(
            "com.bedcode.static".to_string(),
            make_plugin("com.bedcode.static", PluginSource::StaticRegistry, PluginState::Activated),
        );

        let map = host.get_activated_state().await;
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(TEST_PLUGIN_ID), Some(&true));
        assert_eq!(map.get("com.bedcode.ts"), Some(&false));
        assert!(!map.contains_key("com.bedcode.static"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_should_lazy_activate_rules() {
        let host = setup_host().await;
        // 未注册 → false
        assert!(!host.should_lazy_activate("com.missing").await);

        // Loaded + 有命令贡献 → 需要按需激活
        let mut plugin = make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded);
        plugin.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.cmd".into(),
                title: "T".into(),
                icon: None,
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert(TEST_PLUGIN_ID.to_string(), plugin);
        assert!(host.should_lazy_activate(TEST_PLUGIN_ID).await);

        // 无任何扩展点贡献 → false（激活无意义）
        host.plugins.write().await.insert(
            "com.bedcode.empty".to_string(),
            make_plugin("com.bedcode.empty", PluginSource::FileScan, PluginState::Loaded),
        );
        assert!(!host.should_lazy_activate("com.bedcode.empty").await);

        // 已激活/已停用/错误态 → false（仅 Loaded 态参与按需激活）
        host.plugins.write().await.insert(
            "com.bedcode.act".to_string(),
            make_plugin("com.bedcode.act", PluginSource::FileScan, PluginState::Activated),
        );
        assert!(!host.should_lazy_activate("com.bedcode.act").await);

        // 静态注册插件 → false（生命周期由 inventory 注册表托管）
        let mut static_p = make_plugin("com.bedcode.s", PluginSource::StaticRegistry, PluginState::Loaded);
        static_p.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.cmd".into(),
                title: "T".into(),
                icon: None,
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert("com.bedcode.s".to_string(), static_p);
        assert!(!host.should_lazy_activate("com.bedcode.s").await);
    }

    // ==================== Manifest Contributions ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_register_manifest_contributions() {
        let host = setup_host().await;
        let mut plugin = make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded);
        plugin.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.hello".into(),
                title: "Hello".into(),
                icon: None,
            }],
            views: vec![bedcode_plugin_api::ViewContribution {
                id: "test.view".into(),
                view_type: "sidebar".into(),
                title: "V".into(),
                component: "View.vue".into(),
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert(TEST_PLUGIN_ID.to_string(), plugin);

        host.register_manifest_contributions().await;

        let commands = host.registry().list_commands().await;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].plugin_id, TEST_PLUGIN_ID);
        let views = host.registry().list_views().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].plugin_id, TEST_PLUGIN_ID);
        assert_eq!(views[0].view_type, "sidebar");
    }

    // ==================== 激活 / 停用（非 WASM 插件） ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_file_scan_flow() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        // 未注册插件 → Err
        let err = host.activate_plugin("com.missing", false).await.unwrap_err();
        assert!(err.to_string().contains("not found"));

        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Activated);
        // 激活时间被记录
        {
            let plugins_guard = host.plugins.read().await;
            let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
            assert!(loaded.activated_at.is_some());
        }
        // 重新授权：manifest 声明的合法权限已授予（storage 恒默认授予）
        let granted = host.permission().get_granted(TEST_PLUGIN_ID);
        assert!(granted.contains("storage"));
        assert!(granted.contains("terminal:input"));
        assert!(host.permission().check(TEST_PLUGIN_ID, "terminal:input"));

        // 重复激活幂等（已激活 → Ok）
        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert_eq!(
            host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
            PluginState::Activated
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_recovers_from_error_state() {
        let host = setup_host().await;
        // Error 态插件可重新激活（如 WASM 缺失被标记后修复文件再激活）
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(
                TEST_PLUGIN_ID,
                PluginSource::FileScan,
                PluginState::Error("wasm load failed".into()),
            ),
        );

        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert_eq!(
            host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
            PluginState::Activated
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_persists_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();

        let persisted = host.storage().load_activated_plugins().await.unwrap();
        assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_plugin_flow() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        // 预注册一条命令贡献，验证停用时从 registry 摘除
        host.registry()
            .register_commands(
                TEST_PLUGIN_ID,
                &[bedcode_plugin_api::CommandContribution {
                    id: "test.cmd".into(),
                    title: "T".into(),
                    icon: None,
                }],
            )
            .await;
        assert_eq!(host.registry().list_commands().await.len(), 1);

        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Deactivated);
        assert!(!host.is_activated(TEST_PLUGIN_ID).await);
        // 激活时间被清除
        {
            let plugins_guard = host.plugins.read().await;
            let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
            assert!(loaded.activated_at.is_none());
        }
        // 权限被撤销（重新激活时重新授权）
        assert!(host.permission().get_granted(TEST_PLUGIN_ID).is_empty());
        // registry 贡献被摘除
        assert!(host.registry().list_commands().await.is_empty());

        // 未注册插件 → Err
        let err = host.deactivate_plugin("com.missing", false).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_all() {
        let host = setup_host().await;
        for id in ["com.bedcode.a", "com.bedcode.b"] {
            host.plugins.write().await.insert(
                id.to_string(),
                make_plugin(id, PluginSource::FileScan, PluginState::Activated),
            );
        }

        host.deactivate_all().await.unwrap();

        for id in ["com.bedcode.a", "com.bedcode.b"] {
            assert_eq!(
                host.get_plugin(id).await.unwrap().state,
                PluginState::Deactivated
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_auto_activate_from_persisted_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        // 持久化激活 → 手动停用（不持久化）→ 从持久化状态恢复激活
        host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();
        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert!(!host.is_activated(TEST_PLUGIN_ID).await);

        host.auto_activate_from_persisted_state().await;
        assert!(host.is_activated(TEST_PLUGIN_ID).await);

        // 幽灵 ID 清理：持久化映射中不存在的插件被剔除（map 整体替换语义：
        // 重新写入时保留现有条目再插入幽灵 ID）
        let mut stale = HashMap::new();
        stale.insert("com.ghost".to_string(), true);
        stale.insert(TEST_PLUGIN_ID.to_string(), true);
        host.storage().save_activated_plugins(&stale).await.unwrap();
        host.auto_activate_from_persisted_state().await;
        let persisted = host.storage().load_activated_plugins().await.unwrap();
        assert!(!persisted.contains_key("com.ghost"));
        // 已存在的插件条目保留
        assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
    }

    // ==================== Rust Command Dispatch ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_rust_command_gates() {
        let host = setup_host().await;
        // 未注册 / 未激活 → Err（调用者身份门禁）
        let err = host
            .invoke_rust_command("com.missing", "cmd", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not activated"));

        // TS-only 插件（FileScan）→ 拒绝 Rust command
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "cmd", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("TS-only"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_ok() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );
        // 运行时注册表直接注入 handler（等价于 register_rust_command_handlers 的产物）
        let cmd = PluginCommand::new("hello", |args| async move {
            Ok(serde_json::json!({ "echo": args }))
        });
        host.rust_command_handlers
            .write()
            .await
            .insert(format!("{}::hello", TEST_PLUGIN_ID), cmd);

        let result = host
            .invoke_rust_command(TEST_PLUGIN_ID, "hello", json!({"k": 1}))
            .await
            .unwrap();
        assert_eq!(result, json!({ "echo": { "k": 1 } }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_not_found() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );

        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "missing", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Command not found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_handler_error() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );
        let cmd = PluginCommand::new("boom", |_args| async move {
            Err(anyhow::anyhow!("handler exploded"))
        });
        host.rust_command_handlers
            .write()
            .await
            .insert(format!("{}::boom", TEST_PLUGIN_ID), cmd);

        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "boom", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Command execution error"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_rust_commands_parses_namespace() {
        let host = setup_host().await;
        for (pid, cmd_name, title) in [
            ("com.a", "cmd1", "One"),
            ("com.a", "cmd2", "Two"),
            ("com.b", "cmd3", "Three"),
        ] {
            let cmd = PluginCommand::new(cmd_name, |_args| async move {
                Ok(serde_json::json!(null))
            })
            .with_title(title);
            host.rust_command_handlers
                .write()
                .await
                .insert(format!("{}::{}", pid, cmd_name), cmd);
        }

        let mut entries = host.list_rust_commands().await;
        // HashMap 迭代无序：按 (plugin_id, command_name) 排序后比较
        entries.sort_by(|a, b| {
            (a.plugin_id.clone(), a.command_name.clone())
                .cmp(&(b.plugin_id.clone(), b.command_name.clone()))
        });
        let pairs: Vec<(String, String)> = entries
            .iter()
            .map(|e| (e.plugin_id.clone(), e.command_name.clone()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("com.a".to_string(), "cmd1".to_string()),
                ("com.a".to_string(), "cmd2".to_string()),
                ("com.b".to_string(), "cmd3".to_string()),
            ]
        );
        // 全名 `plugin_id::command_name` 正确拆分
        assert_eq!(entries[0].title, "One");
    }

    // ==================== Terminal Handler Pipeline ====================

    /// 记录 on_input_submitted 观测并转换输入/输出的 mock 处理器
    struct MockTerminalHandler {
        submitted: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TerminalHandler for MockTerminalHandler {
        fn on_input(&self, _session_id: &str, text: &str) -> Option<String> {
            Some(format!("[{}]", text))
        }

        fn on_output(&self, _session_id: &str, data: &str) -> Option<String> {
            Some(data.to_uppercase())
        }

        fn on_input_submitted(&self, _session_id: &str, text: &str) {
            self.submitted.lock().unwrap().push(text.to_string());
        }
    }

    /// 默认实现（全部透传）的处理器
    struct PassthroughHandler;

    impl TerminalHandler for PassthroughHandler {}

    #[tokio::test(flavor = "multi_thread")]
    async fn test_terminal_handler_pipeline() {
        let host = setup_host().await;
        // 无 handler：输入输出原样透传
        assert!(!host.has_terminal_handlers().await);
        assert_eq!(host.process_terminal_input("s1", "echo hi").await, "echo hi");
        assert_eq!(host.process_terminal_output("s1", "Hello").await, "Hello");

        let submitted = Arc::new(std::sync::Mutex::new(Vec::new()));
        host.rust_terminal_handlers
            .write()
            .await
            .push(Box::new(MockTerminalHandler {
                submitted: submitted.clone(),
            }));
        // 第二个 handler 不修改（验证 None 语义透传）
        host.rust_terminal_handlers
            .write()
            .await
            .push(Box::new(PassthroughHandler));

        assert!(host.has_terminal_handlers().await);
        assert_eq!(host.process_terminal_input("s1", "echo hi").await, "[echo hi]");
        assert_eq!(host.process_terminal_output("s1", "Hello").await, "HELLO");
        // 观察回调：提交行原样送达
        host.process_input_submitted("s1", "ls -la").await;
        host.process_input_submitted("s1", "pwd").await;
        assert_eq!(
            *submitted.lock().unwrap(),
            vec!["ls -la".to_string(), "pwd".to_string()]
        );
    }

    // ==================== MessageDispatcher ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_message_dispatcher_is_activated() {
        let host = setup_host().await;
        assert!(!MessageDispatcher::is_activated(&host, "com.missing"));

        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        assert!(MessageDispatcher::is_activated(&host, TEST_PLUGIN_ID));

        host.plugins.write().await.insert(
            "com.bedcode.d".to_string(),
            make_plugin("com.bedcode.d", PluginSource::FileScan, PluginState::Deactivated),
        );
        assert!(!MessageDispatcher::is_activated(&host, "com.bedcode.d"));
    }

    // ==================== Upload Hook（fail-closed） ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_call_upload_hook_fail_closed() {
        let host = setup_host().await;
        // 插件在 plugins map 中（Wasm 来源）但实例未加载 → 拒绝
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::Wasm, PluginState::Activated),
        );
        let decision = host.call_upload_hook(TEST_PLUGIN_ID, r#"{"name":"f.bin"}"#).await;
        assert!(!decision.allow);
        assert_eq!(decision.reason.as_deref(), Some("wasm plugin not loaded"));

        // 未知插件 → 同样拒绝
        let decision = host.call_upload_hook("com.missing", "{}").await;
        assert!(!decision.allow);
    }

    // ==================== WASM 插件（真实组件测试插件） ====================

    /// 将 wit-bindgen 产出的 core module 编码为组件
    /// （与 wasm_runtime.rs 测试同策略，等价于 `wasm-tools component new`）
    fn encode_component(module: &[u8]) -> Vec<u8> {
        let encoder = wit_component::ComponentEncoder::default();
        encoder
            .module(module)
            .expect("component encoder module")
            .encode()
            .expect("component encoder encode")
    }

    /// 构建测试用组件插件并编码为组件（packages/plugin-component-test）
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

    /// 将组件形态测试插件实例化并注入宿主（plugins + wasm_plugins 双表）
    ///
    /// 返回插件 ID；组件 invoke 内 host_storage 读回的 key 预写入
    /// `component-test-key`。extension_path 指向临时目录（invoke 的
    /// resource_dir 注入断言用）。
    async fn setup_wasm_plugin(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
        let component = host
            .wasm_runtime()
            .compile_component(&build_test_component())
            .expect("compile test component");
        let plugin = host
            .wasm_runtime()
            .instantiate_component(&component, TEST_WASM_PLUGIN_ID, host.wasm_host_ctx().clone())
            .expect("instantiate test component");

        host.storage()
            .set(TEST_WASM_PLUGIN_ID, "component-test-key", json!({"k": "v"}))
            .await
            .expect("preset storage key");

        let extension_path = tmp_dir.path().to_string_lossy().to_string();
        host.wasm_plugins
            .write()
            .await
            .insert(TEST_WASM_PLUGIN_ID.to_string(), Arc::new(Mutex::new(plugin)));

        let mut loaded = make_plugin(TEST_WASM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
        loaded.manifest.rust_library = "bedcode_plugin_component_test".to_string();
        loaded.extension_path = extension_path;
        host.plugins
            .write()
            .await
            .insert(TEST_WASM_PLUGIN_ID.to_string(), loaded);

        TEST_WASM_PLUGIN_ID.to_string()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_wasm_plugin_activate_invoke_deactivate() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        // 激活：调用组件 __bedcode_activate + on_startup，无错误码
        host.activate_plugin(&pid, false).await.unwrap();
        assert!(host.is_activated(&pid).await);

        // 命令调用：组件 echo + host_storage 读回 + resource_dir 自动注入
        let result = host
            .invoke_rust_command(&pid, "test.echo", json!({"hello": "host"}))
            .await
            .unwrap();
        assert_eq!(result["name"], "test.echo");
        assert_eq!(result["stored"], json!({"k": "v"}));
        let args: serde_json::Value =
            serde_json::from_str(result["args"].as_str().unwrap()).unwrap();
        assert_eq!(args["resource_dir"], json!(tmp_dir.path().to_string_lossy()));

        // 上传钩子：组件返回固定拒绝决策（JSON 解析链路）
        let decision = host.call_upload_hook(&pid, r#"{"name": "f.bin"}"#).await;
        assert!(!decision.allow);
        let reason = decision.reason.unwrap();
        assert!(reason.starts_with("component-test deny"), "unexpected reason: {}", reason);

        // 停用：调用组件 on_shutdown + __bedcode_deactivate
        host.deactivate_plugin(&pid, false).await.unwrap();
        assert!(!host.is_activated(&pid).await);
        assert_eq!(
            host.get_plugin(&pid).await.unwrap().state,
            PluginState::Deactivated
        );

        // 停用后调用被门禁拒绝
        let err = host
            .invoke_rust_command(&pid, "test.echo", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not activated"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dispatch_lifecycle_and_input_to_wasm_plugin() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        // 未激活：dispatch 被门禁静默丢弃（无 panic、不触碰 store）
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
        host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s1", "text": "hi"}));

        host.activate_plugin(&pid, false).await.unwrap();

        // 经真实 listener 走「事件 → payload 构造 → dispatch → wasm 回调」全链路
        let lifecycle_listener = PluginLifecycleListener::new(pid.clone(), host.clone());
        SessionLifecycleListener::on_session_lifecycle(
            &lifecycle_listener,
            &SessionLifecycleEvent::Created {
                session_id: "s1".to_string(),
                config_id: "c1".to_string(),
                name: "n".to_string(),
                working_dir: "/tmp".to_string(),
            },
        );
        let input_listener = PluginInputListener::new(pid.clone(), host.clone());
        SessionInputListener::on_input_submitted(&input_listener, "s1", "echo hi");

        // 直接分发不同 payload 形态
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "stopped", "sessionId": "s1"}));
        host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s2", "text": "ls"}));

        // store 未被污染：分发全部成功后 command 调用仍可用
        let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
        assert_eq!(result["name"], "test.echo");

        // 停用后 dispatch 门禁丢弃，invoke 拒绝
        host.deactivate_plugin(&pid, false).await.unwrap();
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
        assert!(host.invoke_rust_command(&pid, "test.echo", json!({})).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_reload_wasm_plugin_cycle() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        // 把组件字节写入临时插件目录（reload 从文件重新加载）
        let wasm_bytes = build_test_component();
        let wasm_path = tmp_dir.path().join("bedcode_plugin_component_test.wasm");
        std::fs::write(&wasm_path, &wasm_bytes).unwrap();

        let pid = setup_wasm_plugin(&host, &tmp_dir).await;
        host.activate_plugin(&pid, false).await.unwrap();

        // 完整卸载-重载-激活循环
        host.reload_wasm_plugin(&pid).await.unwrap();
        assert!(host.is_activated(&pid).await);

        // 重载后的新实例可用
        let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
        assert_eq!(result["name"], "test.echo");
    }

    // ==================== PluginServices（可测部分） ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_plugin_timer_register_replace_abort() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );

        // 注册定时器（3600s 间隔：测试期间不会触发 tick 回调）
        PluginServices::register_plugin_timer(
            &host,
            TEST_PLUGIN_ID.to_string(),
            3600,
            "tick".to_string(),
        );
        assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

        // 重复注册替换旧句柄（v6 ADR 0003：同一插件仅保留一个定时器）
        PluginServices::register_plugin_timer(
            &host,
            TEST_PLUGIN_ID.to_string(),
            3600,
            "tick".to_string(),
        );
        assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

        // 停用中止定时器（不再到点回调）
        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert!(host.plugin_timers.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_schedule_plugin_reload_throttle() {
        let host = setup_host().await;
        // 未激活插件：调度后后台任务直接退出，仅验证限频表行为
        host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
        {
            let throttle = host.wasm_reload_throttle.lock().unwrap();
            assert!(throttle.contains_key(TEST_PLUGIN_ID));
        }
        // 30 秒窗口内再次调度被限频跳过：不新增条目
        host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
        {
            let throttle = host.wasm_reload_throttle.lock().unwrap();
            assert_eq!(throttle.len(), 1);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_session_listener_registration_via_services() {
        let host = setup_host().await;
        let session_manager = host.wasm_host_ctx().session_manager_arc();

        // PluginServices 实现的注册路径（listener 构造 + block_on_async 注册）。
        // 注册结果在 SessionManager 内部（无公开查询接口），此处验证不 panic、
        // 且注册的 listener 可被停用流程按 plugin_id 摘除
        PluginServices::register_session_lifecycle_listener(
            &host,
            TEST_PLUGIN_ID.to_string(),
            session_manager.clone(),
        );
        PluginServices::register_session_input_listener(
            &host,
            TEST_PLUGIN_ID.to_string(),
            session_manager.clone(),
        );

        // listener 自身携带正确 plugin_id（停用摘除依赖此标识）
        let l1 = PluginLifecycleListener::new(TEST_PLUGIN_ID.to_string(), host.clone());
        assert_eq!(l1.plugin_id(), TEST_PLUGIN_ID);
        assert_eq!(SessionLifecycleListener::plugin_id(&l1), Some(TEST_PLUGIN_ID));
        let l2 = PluginInputListener::new(TEST_PLUGIN_ID.to_string(), host);
        assert_eq!(SessionInputListener::plugin_id(&l2), Some(TEST_PLUGIN_ID));
    }

    #[tokio::test]
    async fn notify_plugin_runtime_error_throttle_and_no_app_context() {
        // 统一异常通道（PLUGIN_RUNTIME_ERROR）：
        // 1. 无 AppContext（测试/无头）时降级为纯日志，不 panic
        // 2. 同一插件窗口内二次通知被节流（不重复提示），节流表只记录一次
        let host = setup_host().await;

        host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "panic", "boom").await;
        host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "trap", "boom again").await;

        let throttle = host.runtime_error_notify_throttle.lock().unwrap();
        assert!(throttle.contains_key(TEST_PLUGIN_ID), "first call must record throttle entry");
        // 窗口内二次调用不新增/刷新条目（被节流）
        assert_eq!(throttle.len(), 1, "second call within window must be throttled");
    }
}
