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
use bedcode_plugin_api::{PluginState, PluginCommandEntry};
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

    /// 停用所有已激活的插件
    pub async fn deactivate_all(&self) -> crate::Result<()> {
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

        // WASM 插件：调用 __bedcode_activate
        if loaded.source == PluginSource::Wasm {
            let wasm_plugins = self.wasm_plugins.read().await;
            if let Some(wasm_plugin) = wasm_plugins.get(plugin_id).cloned() {
                drop(wasm_plugins);
                let mut wasm_plugin = wasm_plugin.lock().await;
                match wasm_plugin.activate() {
                    Ok(0) => {
                        tracing::info!("[PluginHost] Plugin '{}' activated", plugin_id);
                    }
                    Ok(code) => {
                        tracing::error!("[PluginHost] Plugin '{}' activate() returned error code {}", plugin_id, code);
                        loaded.state = PluginState::Error(
                            format!("activate() returned error code {}", code)
                        );
                        return Err(crate::AppError::Plugin(format!(
                            "Plugin {} activate() returned error code {}", plugin_id, code
                        )));
                    }
                    Err(e) => {
                        tracing::error!("[PluginHost] Plugin '{}' activate() failed: {}", plugin_id, e);
                        loaded.state = PluginState::Error(format!("activate() failed: {}", e));
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
            } else {
                tracing::error!("WASM plugin {} not found in wasm_plugins map", plugin_id);
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} WASM module not loaded", plugin_id
                )));
            }
        }

        loaded.state = PluginState::Activated;
        loaded.activated_at = Some(Utc::now());

        // 注册 manifest 中声明的 topic 订阅
        let subscribes = loaded.manifest.contributes.subscribes.clone();
        let plugin_id_owned = plugin_id.to_string();
        drop(plugins);

        if !subscribes.is_empty() {
            for topic in &subscribes {
                self.message_bus.subscribe_wasm(&plugin_id_owned, topic).await;
            }
            tracing::info!(
                "[PluginHost] Plugin {} subscribed to {} topic(s): {:?}",
                plugin_id_owned,
                subscribes.len(),
                subscribes
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

    // ==================== Rust Command Dispatch ====================

    /// 执行 Rust 插件的 command handler
    ///
    /// 路由逻辑：
    /// - WASM 插件：通过 WASM 导出函数调用 invoke_command
    /// - 静态注册插件：通过运行时注册表查找 handler
    pub async fn invoke_rust_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        if !self.is_activated(plugin_id).await {
            return Err(crate::AppError::Plugin(format!(
                "Plugin {} is not activated", plugin_id
            )));
        }

        let source = {
            let plugins = self.plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| p.source.clone())
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?
        };

        match source {
            PluginSource::Wasm => {
                self.invoke_wasm_command(plugin_id, command_name, args).await
            }
            PluginSource::StaticRegistry => {
                self.invoke_static_command(plugin_id, command_name, args).await
            }
            PluginSource::FileScan => {
                Err(crate::AppError::Plugin(format!(
                    "Plugin {} is TS-only, cannot invoke Rust command", plugin_id
                )))
            }
        }
    }

    /// 获取 WASM 插件实例句柄（map 读锁仅在取 Arc 期间持有，随即释放，
    /// 实例串行化由各插件自己的 Mutex 承担，插件间互不阻塞）
    async fn get_wasm_plugin(
        &self,
        plugin_id: &str,
    ) -> Option<Arc<Mutex<LoadedWasmPlugin>>> {
        let wasm_plugins = self.wasm_plugins.read().await;
        wasm_plugins.get(plugin_id).cloned()
    }

    /// WASM 插件调用失败（trap / store 中毒）后的自动恢复
    ///
    /// wasmtime 同步引擎下任何一次 trap 都会 `set_trapped()` 污染 Store，
    /// 之后该实例所有调用持续报 `CannotEnterComponent`，唯一恢复途径是整体重载
    /// （deactivate → 重新实例化 → activate，即 [`reload_wasm_plugin`]）。
    /// 本方法只做：限频（防重载风暴）+ 后台调度 + 失败时置 Error 态。
    ///
    /// 同步上下文可调用（内部 spawn 不阻塞）；调用方须先释放插件实例锁。
    pub fn schedule_plugin_reload_after_trap(&self, plugin_id: &str) {
        let plugin_id = plugin_id.to_string();

        // 限频：距上次自动重载不足最小间隔则跳过（已在上次恢复或仍属持久性故障）
        {
            let mut throttle = self
                .wasm_reload_throttle
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(last) = throttle.get(&plugin_id) {
                if last.elapsed()
                    < std::time::Duration::from_secs(PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS)
                {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        "plugin trap recovery throttled (recent reload), keeping error state"
                    );
                    return;
                }
            }
            throttle.insert(plugin_id.clone(), std::time::Instant::now());
        }

        let host = self.clone();
        tracing::warn!(
            plugin_id = %plugin_id,
            "plugin WASM trap detected, scheduling auto reload"
        );
        tokio::spawn(async move {
            // 恢复窗口内用户已停用（或正在停用）时不擅自重载
            if !host.is_activated(&plugin_id).await {
                tracing::info!(
                    plugin_id = %plugin_id,
                    "plugin no longer activated, skip auto reload"
                );
                return;
            }
            match host.reload_wasm_plugin(&plugin_id).await {
                Ok(()) => {
                    tracing::info!(plugin_id = %plugin_id, "plugin auto reloaded after trap");
                }
                Err(e) => {
                    tracing::error!(
                        plugin_id = %plugin_id,
                        error = %e,
                        "plugin auto reload after trap failed"
                    );
                    // 置 Error 态：UI 可见原因，且 is_activated 门禁停止后续分发
                    host.mark_error(
                        &plugin_id,
                        format!("auto reload after trap failed: {}", e),
                    )
                    .await;
                }
            }
        });
    }

    /// 持锁调用 WASM 插件导出并统一处理失败恢复
    ///
    /// 调用失败（trap / 导出绑定失败 / store 中毒）时：先释放实例锁（防死锁），
    /// 再调度自动重载（见 [`schedule_plugin_reload_after_trap`]），最后返回 Err 给调用方。
    /// 调用方只需把 Err 转成自己的错误形态（anyhow / 日志 / fail-closed 决定）。
    async fn with_wasm_plugin_call<T>(
        &self,
        plugin_id: &str,
        call: impl FnOnce(&mut LoadedWasmPlugin) -> crate::Result<T>,
    ) -> crate::Result<T> {
        let Some(wasm_plugin) = self.get_wasm_plugin(plugin_id).await else {
            return Err(crate::AppError::Plugin(format!(
                "WASM plugin {} not found in loaded instances",
                plugin_id
            )));
        };
        let result = {
            let mut guard = wasm_plugin.lock().await;
            call(&mut guard)
        };
        if result.is_err() {
            // 实例已不可用 → 自动重载恢复（锁已释放，无死锁）
            self.schedule_plugin_reload_after_trap(plugin_id);
        }
        result
    }

    /// 调用 WASM 插件的 command
    async fn invoke_wasm_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        // 为需要 resource_dir 的命令自动注入插件 extension_path
        // （剥离 verbatim 前缀，保证插件侧正斜杠拼接可用，见 loader.rs strip_verbatim_prefix）
        let mut enriched_args = args;
        if enriched_args.get("resource_dir").is_none() {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(plugin_id) {
                enriched_args.as_object_mut().map(|obj| {
                    obj.insert(
                        "resource_dir".to_string(),
                        serde_json::Value::String(crate::plugin::loader::strip_verbatim_prefix(
                            &loaded.extension_path,
                        )),
                    );
                });
            }
        }

        let args_str = serde_json::to_string(&enriched_args)
            .map_err(|e| crate::AppError::Plugin(format!(
                "Failed to serialize command args: {}", e
            )))?;

        // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
        let result_str = self
            .with_wasm_plugin_call(plugin_id, |plugin| {
                plugin.invoke_command(command_name, &args_str)
            })
            .await?;

        let value: serde_json::Value = serde_json::from_str(&result_str)
            .map_err(|e| crate::AppError::Plugin(format!(
                "WASM plugin {} invoke_command() returned invalid JSON: {}", plugin_id, e
            )))?;

        Ok(value)
    }

    /// 调用静态注册插件的 command handler
    async fn invoke_static_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        let handlers = self.rust_command_handlers.read().await;
        let full_name = format!("{}::{}", plugin_id, command_name);
        let cmd = handlers.get(&full_name).ok_or_else(|| {
            crate::AppError::Plugin(format!("Command not found: {}", full_name))
        })?;

        let result = (cmd.handler)(args).await
            .map_err(|e| crate::AppError::Plugin(format!("Command execution error: {}", e)))?;

        Ok(result)
    }

    /// 获取所有 Rust 插件的 command 列表
    pub async fn list_rust_commands(&self) -> Vec<PluginCommandEntry> {
        let handlers = self.rust_command_handlers.read().await;
        handlers.iter().map(|(full_name, cmd)| {
            let parts: Vec<&str> = full_name.splitn(2, "::").collect();
            let plugin_id = parts.first().map(|s| s.to_string()).unwrap_or_default();
            let command_name = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
            PluginCommandEntry {
                plugin_id,
                command_name,
                title: cmd.title.clone(),
            }
        }).collect()
    }

    // ==================== Terminal Handler Pipeline ====================

    /// 通过插件 TerminalHandler 管道处理终端输入
    pub async fn process_terminal_input(&self, session_id: &str, text: &str) -> String {
        let handlers = self.rust_terminal_handlers.read().await;
        let mut result = text.to_string();
        for handler in handlers.iter() {
            if let Some(modified) = handler.on_input(session_id, &result) {
                tracing::debug!(
                    "Terminal input modified by plugin handler: session_id={}, original_len={}, modified_len={}",
                    session_id, result.len(), modified.len()
                );
                result = modified;
            }
        }
        result
    }

    /// 通过插件 TerminalHandler 管道处理终端输出
    pub async fn process_terminal_output(&self, session_id: &str, data: &str) -> String {
        let handlers = self.rust_terminal_handlers.read().await;
        let mut result = data.to_string();
        for handler in handlers.iter() {
            if let Some(modified) = handler.on_output(session_id, &result) {
                tracing::debug!(
                    "Terminal output modified by plugin handler: session_id={}, original_len={}, modified_len={}",
                    session_id, result.len(), modified.len()
                );
                result = modified;
            }
        }
        result
    }

    /// 将提交输入行分发给 Rust 插件的 TerminalHandler 观察回调（见 ADR 0001）
    ///
    /// 与 `process_terminal_input`（逐块同步修改）互补：纯观察、不修改、
    /// 由 SessionManager 在异步错误隔离任务中调用
    pub async fn process_input_submitted(&self, session_id: &str, text: &str) {
        let handlers = self.rust_terminal_handlers.read().await;
        tracing::debug!(
            "process_input_submitted session_id={}, text_len={}, rust_handlers={}",
            session_id,
            text.len(),
            handlers.len()
        );
        for handler in handlers.iter() {
            handler.on_input_submitted(session_id, text);
        }
    }
}

// ==================== PluginLifecycleListener ====================

/// 插件专属的会话生命周期监听器
///
/// 每个 WASM 插件在 activate 时通过 host_session_lifecycle_register 注册。
/// 收到生命周期事件后，将事件序列化为 JSON payload，调用插件的
/// __bedcode_on_session_lifecycle 导出函数。
pub struct PluginLifecycleListener {
    /// 插件 ID
    plugin_id: String,
    /// 插件宿主（Arc 内部，Clone 成本低）
    plugin_host: PluginHost,
}

impl PluginLifecycleListener {
    /// 创建插件生命周期监听器
    pub fn new(plugin_id: String, plugin_host: PluginHost) -> Self {
        Self { plugin_id, plugin_host }
    }

    /// 获取插件 ID
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

impl SessionLifecycleListener for PluginLifecycleListener {
    fn on_session_lifecycle(&self, event: &SessionLifecycleEvent) {
        // 获取插件的 extension_path 作为 resource_dir 注入 payload
        // （剥离 verbatim 前缀，保证插件侧正斜杠拼接可用，见 loader.rs strip_verbatim_prefix）
        let plugins = self.plugin_host.plugins.clone();
        let plugin_id = self.plugin_id.clone();
        let resource_dir = crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins
                .get(&plugin_id)
                .map(|p| crate::plugin::loader::strip_verbatim_prefix(&p.extension_path))
                .unwrap_or_default()
        });

        // 宿主事件 → SDK 类型化枚举（与插件侧 on_session_lifecycle 接收的类型一致），
        // 穷尽 match：任一端新增变体时编译失败，强制同步
        use bedcode_plugin_api::events::SessionLifecycleEvent as SdkLifecycleEvent;

        let sdk_event = match event {
            SessionLifecycleEvent::Creating { config_id, command, working_dir, source_device } => {
                SdkLifecycleEvent::Creating {
                    config_id: config_id.clone(),
                    command: command.clone(),
                    working_dir: working_dir.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Created { session_id, config_id, name, working_dir } => {
                SdkLifecycleEvent::Created {
                    session_id: session_id.clone(),
                    config_id: config_id.clone(),
                    name: name.clone(),
                    working_dir: working_dir.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Stopping { session_id, source_device } => {
                SdkLifecycleEvent::Stopping {
                    session_id: session_id.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
            SessionLifecycleEvent::Stopped { session_id, source_device } => {
                SdkLifecycleEvent::Stopped {
                    session_id: session_id.clone(),
                    source_device: source_device.clone(),
                    resource_dir: resource_dir.clone(),
                }
            }
        };

        // serde 表示即线协议；序列化失败（理论上不可能）退化为空对象，插件侧按协议错误处理
        let payload = serde_json::to_value(&sdk_event).unwrap_or_else(|_| serde_json::json!({}));

        self.plugin_host.dispatch_lifecycle_to_plugin(&self.plugin_id, &payload);
    }

    fn plugin_id(&self) -> Option<&str> {
        Some(&self.plugin_id)
    }
}

// ==================== PluginInputListener ====================

/// 插件专属的提交输入行监听器（见 ADR 0001）
///
/// 每个 WASM 插件在 activate 时通过 host_session_input_register 注册
///（需 `terminal:observe` 权限）。收到提交输入行后，构造 SDK 类型化
/// `InputSubmittedEvent`（serde 表示即线协议），调用插件的
/// `__bedcode_on_input_submitted` 导出函数。
pub struct PluginInputListener {
    /// 插件 ID
    plugin_id: String,
    /// 插件宿主（Arc 内部，Clone 成本低）
    plugin_host: PluginHost,
}

impl PluginInputListener {
    /// 创建插件输入监听器
    pub fn new(plugin_id: String, plugin_host: PluginHost) -> Self {
        Self { plugin_id, plugin_host }
    }
}

impl SessionInputListener for PluginInputListener {
    fn on_input_submitted(&self, session_id: &str, text: &str) {
        tracing::debug!(
            "PluginInputListener on_input_submitted plugin_id={}, session_id={}, text_len={}",
            self.plugin_id,
            session_id,
            text.len()
        );
        // 宿主事件 → SDK 类型化结构体（与插件侧 on_input_submitted 接收的类型一致），
        // serde 表示即线协议；序列化失败（理论上不可能）退化为空对象
        let event = bedcode_plugin_api::events::InputSubmittedEvent {
            session_id: session_id.to_string(),
            text: text.to_string(),
        };
        let payload = serde_json::to_value(&event).unwrap_or_else(|_| serde_json::json!({}));

        self.plugin_host.dispatch_input_to_plugin(&self.plugin_id, &payload);
    }

    fn plugin_id(&self) -> Option<&str> {
        Some(&self.plugin_id)
    }
}

impl PluginHost {
    /// 将会话生命周期事件分发给指定插件的 on_session_lifecycle 回调
    pub fn dispatch_lifecycle_to_plugin(&self, plugin_id: &str, payload: &serde_json::Value) {
        if !self.is_activated_block(plugin_id) {
            return;
        }

        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let payload = payload.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            if let Err(e) = host
                .with_wasm_plugin_call(&plugin_id, |plugin| plugin.on_session_lifecycle(&payload))
                .await
            {
                tracing::error!(
                    "SessionLifecycle: dispatch to plugin '{}' failed: {}",
                    plugin_id, e
                );
            }
        });
    }

    /// 将提交输入行事件分发给指定插件的 on_input_submitted 回调（见 ADR 0001）
    ///
    /// 由 PluginInputListener 在 SessionManager spawn 的错误隔离任务中调用；
    /// 分发失败仅记录日志，不影响输入本身
    pub fn dispatch_input_to_plugin(&self, plugin_id: &str, payload: &serde_json::Value) {
        if !self.is_activated_block(plugin_id) {
            // 插件未处于 Activated 状态（Loaded/Deactivated/Error）：事件被此门禁静默丢弃，
            // 是输入分发链路上唯一无日志的断点，记录 debug 便于定位
            tracing::debug!(
                "InputSubmitted: drop event for plugin '{}': plugin not in Activated state",
                plugin_id
            );
            return;
        }

        tracing::debug!(
            "InputSubmitted: dispatch to plugin '{}', payload={}",
            plugin_id,
            payload
        );

        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let payload = payload.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            if let Err(e) = host
                .with_wasm_plugin_call(&plugin_id, |plugin| plugin.on_input_submitted(&payload))
                .await
            {
                tracing::error!(
                    "InputSubmitted: dispatch to plugin '{}' failed: {}",
                    plugin_id, e
                );
            }
        });
    }

    /// 阻塞式检查插件是否已激活（用于同步分发场景）
    fn is_activated_block(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| matches!(p.state, PluginState::Activated))
                .unwrap_or(false)
        })
    }
}

// ==================== PluginServices Implementation ====================

impl PluginServices for PluginHost {
    fn register_session_lifecycle_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    ) {
        // host function 处于同步上下文，通过 block_on_async 完成异步注册
        let listener = PluginLifecycleListener::new(plugin_id, self.clone());
        crate::plugin::wasm_runtime::block_on_async(
            session_manager.register_lifecycle_listener(Arc::new(listener)),
        );
    }

    fn register_session_input_listener(
        &self,
        plugin_id: String,
        session_manager: Arc<SessionManager>,
    ) {
        // host function 处于同步上下文，通过 block_on_async 完成异步注册
        let listener = PluginInputListener::new(plugin_id, self.clone());
        crate::plugin::wasm_runtime::block_on_async(
            session_manager.register_input_listener(Arc::new(listener)),
        );
    }

    fn mark_plugin_error(&self, plugin_id: String, error: String) {
        crate::plugin::wasm_runtime::block_on_async(async move {
            // 仅通知前端弹窗提示：不置 Error、不持久化，插件保持激活，会话照常运行。
            // hooks 安装失败等自检错误属可恢复/局部问题，不应因此禁用整个插件。
            tracing::error!("[PluginHost] Plugin {} self-check failed: {}", plugin_id, error);

            let _ = crate::system::app_context::AppContext::global()
                .app_handle()
                .emit(
                    crate::system::constants::event::PLUGIN_ERROR,
                    serde_json::json!({
                        "plugin_id": plugin_id,
                        "error": error,
                    }),
                );
        });
    }

    fn register_plugin_timer(&self, plugin_id: String, interval_secs: u64, command: String) {
        // 重复注册替换旧定时器：先中止旧任务再插入新句柄，
        // 同一插件仅保留一个定时器实例
        let mut timers = self
            .plugin_timers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(old) = timers.remove(&plugin_id) {
            old.abort();
        }

        let host = self.clone();
        let pid = plugin_id.clone();
        let cmd = command.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            // 首个 tick 立即触发：跳过，从下一个周期开始（避免注册瞬间就回调）
            interval.tick().await;
            loop {
                interval.tick().await;

                let now = chrono::Utc::now();
                let args = serde_json::json!({
                    "now_ms": now.timestamp_millis(),
                    // 与 SQLite datetime('now') 同格式（UTC，无时区后缀），
                    // 便于插件在 SQL 中直接字符串比较到期时间
                    "now_utc": now.format("%Y-%m-%d %H:%M:%S").to_string(),
                });

                // 到点调用插件 command；插件未激活/已卸载时返回 Err，
                // 属预期内路径（定时器中止前的空窗期），仅记 debug 日志
                match host.invoke_rust_command(&pid, &cmd, args).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            plugin_id = %pid,
                            command = %cmd,
                            error = %e,
                            "[PluginHost] timer tick skipped"
                        );
                    }
                }
            }
        });

        timers.insert(plugin_id.clone(), handle);
        drop(timers);

        tracing::info!(
            "[PluginHost] Timer started for '{}': interval={}s command={}",
            plugin_id, interval_secs, command
        );
    }
}

// 通过 Arc 共享内部状态实现 Clone
impl Clone for PluginHost {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
            registry: self.registry.clone(),
            permission: self.permission.clone(),
            storage: self.storage.clone(),
            rust_command_handlers: self.rust_command_handlers.clone(),
            rust_terminal_handlers: self.rust_terminal_handlers.clone(),
            wasm_runtime: self.wasm_runtime.clone(),
            wasm_plugins: self.wasm_plugins.clone(),
            wasm_host_ctx: self.wasm_host_ctx.clone(),
            message_bus: self.message_bus.clone(),
            file_service: self.file_service.clone(),
            plugin_timers: self.plugin_timers.clone(),
            wasm_reload_throttle: self.wasm_reload_throttle.clone(),
        }
    }
}

// ==================== MessageDispatcher Implementation ====================

impl crate::plugin::message_bus::MessageDispatcher for PluginHost {
    fn dispatch_to_wasm(&self, plugin_id: &str, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        let host = self.clone();
        let plugin_id = plugin_id.to_string();
        let msg = msg.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            // 调用失败（trap/store 中毒）时自动重载恢复，见 with_wasm_plugin_call
            host.with_wasm_plugin_call(&plugin_id, |plugin| {
                plugin.on_message(&msg.topic, &msg.sender, &msg.payload)
            })
            .await
            .map_err(|e| anyhow::Error::from(e))
        })
    }

    fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.clone();
        crate::plugin::wasm_runtime::block_on_async(async move {
            let plugins = plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| matches!(p.state, PluginState::Activated))
                .unwrap_or(false)
        })
    }
}
