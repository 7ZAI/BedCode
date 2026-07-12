//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage、cdylib_loader 五个子系统
//! 支持静态注册（Rust 插件 via inventory）、文件扫描（TS-only 插件）和 cdylib 动态库（Rust+TS 插件）

use crate::plugin::cdylib_loader::{CdylibLoader, LoadedCdylibPlugin};
use crate::plugin::host_context::HostContextFns;
use crate::plugin::loader::PluginLoader;
use crate::plugin::permission::PermissionManager;
use crate::plugin::registry::PluginRegistry;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::{DesktopPluginInfo, LoadedPlugin, PluginSource};
use crate::db::Database;
use crate::session::SessionManager;
use crate::system::constants::plugin::PLUGIN_CALLBACK_TIMEOUT_SECS;
use crate::system::constants::event;
use bedcode_plugin_api::{PluginState, PluginCommandEntry};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

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
    /// cdylib 插件句柄（plugin_id → LoadedCdylibPlugin）
    cdylib_plugins: Arc<RwLock<HashMap<String, LoadedCdylibPlugin>>>,
    /// HostContext 函数实现（共享引用，所有 cdylib 插件共用）
    host_context_fns: Arc<HostContextFns>,
}

impl PluginHost {
    /// 创建 PluginHost 并加载所有插件（静态注册 + 文件扫描 + cdylib）
    ///
    /// # Arguments
    /// * `db` - 数据库实例
    /// * `plugins_dir` - 插件目录
    /// * `session_manager` - 会话管理器（供 cdylib HostContext 使用）
    /// * `app_handle` - Tauri AppHandle（供 cdylib HostContext 发送事件使用）
    pub async fn new(
        db: Arc<Mutex<Database>>,
        plugins_dir: &Path,
        session_manager: Arc<SessionManager>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let storage = Arc::new(PluginStorage::new(db.clone()));

        // 构建 HostContextFns 工厂，供 cdylib 插件激活时构建 HostContext
        let host_context_fns = Arc::new(HostContextFns::new(
            db.clone(),
            storage.clone(),
            session_manager,
            app_handle,
            permission.clone(),
        ));

        // 1. 收集静态注册的 Rust 插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();

        // 2. 扫描文件系统中的 TS-only 和 cdylib 插件
        let file_plugins = PluginLoader::load_all(plugins_dir, &permission);

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

        // 添加文件扫描的插件（包含 TS-only 和 cdylib 来源判定）
        let mut cdylib_plugins_map: HashMap<String, LoadedCdylibPlugin> = HashMap::new();

        for (id, loaded) in file_plugins {
            // 如果 manifest 声明了 rust_library，尝试加载 cdylib 动态库
            if !loaded.manifest.rust_library.is_empty() {
                let plugin_dir = Path::new(&loaded.extension_path);
                match CdylibLoader::load(plugin_dir, &loaded.manifest.rust_library) {
                    Ok(cdylib_plugin) => {
                        tracing::info!(
                            "Cdylib plugin loaded: {} v{} (library: {})",
                            loaded.manifest.id,
                            loaded.manifest.version,
                            loaded.manifest.rust_library
                        );
                        cdylib_plugins_map.insert(id.clone(), cdylib_plugin);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to load cdylib for plugin {} v{}: {}",
                            loaded.manifest.id,
                            loaded.manifest.version,
                            e
                        );
                        // cdylib 加载失败，跳过该插件，不插入 all_plugins
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
            cdylib_plugins: Arc::new(RwLock::new(cdylib_plugins_map)),
            host_context_fns,
        };

        // 注册所有已加载插件的 manifest contributes 到 registry
        host.register_manifest_contributions().await;

        // 注册 Rust 插件的 command handlers（inventory 静态注册）
        host.register_rust_command_handlers().await;

        // 注册 Rust 插件的 terminal handlers（inventory 静态注册）
        host.register_rust_terminal_handlers().await;

        // 4. 根据持久化状态自动激活之前已激活的插件
        host.auto_activate_from_persisted_state().await;

        let count = host.plugins.read().await.len();
        let cdylib_count = host.cdylib_plugins.read().await.len();
        tracing::info!(
            "PluginHost initialized with {} plugin(s), {} cdylib plugin(s)",
            count,
            cdylib_count
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

    pub fn registry(&self) -> &Arc<PluginRegistry> {
        &self.registry
    }

    pub fn permission(&self) -> &Arc<PermissionManager> {
        &self.permission
    }

    pub fn storage(&self) -> &Arc<PluginStorage> {
        &self.storage
    }

    // ==================== Lifecycle ====================

    /// 获取所有已加载插件的信息列表
    pub async fn list_plugins(&self) -> Vec<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.values().map(DesktopPluginInfo::from).collect()
    }

    /// 获取单个插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> Option<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(DesktopPluginInfo::from)
    }

    /// 检查插件是否处于激活状态（用于 API 调用的调用者身份校验）
    pub async fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id)
            .map(|p| matches!(p.state, PluginState::Activated))
            .unwrap_or(false)
    }

    /// 通知所有已激活的 Rust 插件应用启动完成
    ///
    /// 遍历静态注册插件调用 `on_startup`，遍历 cdylib 插件调用 FFI `on_startup`。
    /// 同时通过 Tauri 事件 `lifecycle:startup` 通知 TS-only 插件。
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

        // cdylib 插件
        let cdylib_plugins = self.cdylib_plugins.read().await;
        for (id, cdylib) in cdylib_plugins.iter() {
            if self.is_activated(id).await {
                if let Some(on_startup) = cdylib.exports().on_startup {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        unsafe { on_startup() }
                    }));
                    tracing::debug!("Cdylib plugin {} on_startup called", id);
                }
            }
        }

        // TS-only 插件：通过 Tauri 事件通知
        let ctx = crate::system::app_context::AppContext::global();
        let _ = ctx.app_handle().emit(event::LIFECYCLE_STARTUP, serde_json::json!({}));

        tracing::info!("PluginHost notify_startup completed");
    }

    /// 通知所有已激活的插件应用即将关闭
    ///
    /// 在 deactivate 之前触发，此时插件仍处于激活状态。
    /// 同时通过 Tauri 事件 `lifecycle:shutdown` 通知 TS-only 插件。
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

        // cdylib 插件
        let cdylib_plugins = self.cdylib_plugins.read().await;
        for (id, cdylib) in cdylib_plugins.iter() {
            if self.is_activated(id).await {
                if let Some(on_shutdown) = cdylib.exports().on_shutdown {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        unsafe { on_shutdown() }
                    }));
                    tracing::debug!("Cdylib plugin {} on_shutdown called", id);
                }
            }
        }

        // TS-only 插件：通过 Tauri 事件通知
        let ctx = crate::system::app_context::AppContext::global();
        let _ = ctx.app_handle().emit(event::LIFECYCLE_SHUTDOWN, serde_json::json!({}));

        tracing::info!("PluginHost notify_shutdown completed");
    }

    /// 停用所有已激活的插件
    ///
    /// 遍历所有 Activated 状态的插件，逐个调用 deactivate_plugin()。
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
    /// - cdylib 插件：调用 exports.activate() 传入 HostContext
    /// - TS-only 插件：前端模块加载在 PluginLoader 中完成
    ///
    /// # Arguments
    /// * `plugin_id` - 插件 ID
    /// * `persist` - 是否持久化激活状态（用户操作传 true，启动自动激活/热重载传 false）
    pub async fn activate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        let mut plugins = self.plugins.write().await;
        let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
        })?;

        match &loaded.state {
            PluginState::Activated => {
                tracing::debug!("Plugin {} already activated", plugin_id);
                return Ok(());
            }
            PluginState::Error(e) => {
                tracing::warn!("Plugin {} in error state: {}, attempting re-activation", plugin_id, e);
            }
            _ => {}
        }

        // 重新授权：deactivate 会 revoke_all，再次激活时必须重新授予
        let permissions = loaded.manifest.permissions.clone();
        let granted = self.permission.grant_permissions(plugin_id, &permissions);
        loaded.granted_permissions = granted;

        // cdylib 插件：调用 exports.activate() 并传入 HostContext
        if loaded.source == PluginSource::Cdylib {
            let cdylib_plugins = self.cdylib_plugins.read().await;
            if let Some(cdylib_plugin) = cdylib_plugins.get(plugin_id) {
                let host_context = self.host_context_fns.build_host_context(plugin_id);
                let exports = cdylib_plugin.exports();

                let activate_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // SAFETY: exports 函数指针由 libloading 从已加载的动态库解析，
                    // Library 句柄由 LoadedCdylibPlugin.library 持有，生命周期与插件一致
                    unsafe { (exports.activate)(&host_context as *const _) }
                }));

                match activate_result {
                    Ok(0) => {
                        tracing::info!("Cdylib plugin activate() succeeded: {}", plugin_id);
                    }
                    Ok(code) => {
                        // activate 返回非零表示初始化失败
                        tracing::error!(
                            "Cdylib plugin activate() returned error code {}: {}",
                            code,
                            plugin_id
                        );
                        loaded.state = PluginState::Error(
                            format!("activate() returned error code {}", code)
                        );
                        return Err(crate::AppError::Plugin(format!(
                            "Plugin {} activate() returned error code {}", plugin_id, code
                        )));
                    }
                    Err(_) => {
                        tracing::error!("Cdylib plugin activate() panicked: {}", plugin_id);
                        loaded.state = PluginState::Error("activate() panicked".to_string());
                        return Err(crate::AppError::Plugin(format!(
                            "Plugin {} activate() panicked", plugin_id
                        )));
                    }
                }
            } else {
                tracing::error!(
                    "Cdylib plugin {} not found in cdylib_plugins map (library not loaded)",
                    plugin_id
                );
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} cdylib library not loaded", plugin_id
                )));
            }
        }

        loaded.state = PluginState::Activated;
        loaded.activated_at = Some(Utc::now());

        tracing::info!("Plugin activated: {}", plugin_id);

        if persist {
            self.persist_activation_state().await;
        }

        Ok(())
    }

    /// 停用插件
    ///
    /// cdylib 插件：先调用 exports.deactivate()，再执行现有清理流程
    ///
    /// # Arguments
    /// * `plugin_id` - 插件 ID
    /// * `persist` - 是否持久化激活状态（用户操作传 true，shutdown/热重载传 false）
    pub async fn deactivate_plugin(&self, plugin_id: &str, persist: bool) -> crate::Result<()> {
        // cdylib 插件：调用 exports.deactivate()
        {
            let plugins = self.plugins.read().await;
            if let Some(loaded) = plugins.get(plugin_id) {
                if loaded.source == PluginSource::Cdylib {
                    let cdylib_plugins = self.cdylib_plugins.read().await;
                    if let Some(cdylib_plugin) = cdylib_plugins.get(plugin_id) {
                        let exports = cdylib_plugin.exports();

                        let deactivate_result = std::panic::catch_unwind(
                            std::panic::AssertUnwindSafe(|| {
                                // SAFETY: exports 函数指针由 libloading 从已加载的动态库解析
                                unsafe { (exports.deactivate)() }
                            }),
                        );

                        match deactivate_result {
                            Ok(0) => {
                                tracing::info!(
                                    "Cdylib plugin deactivate() succeeded: {}",
                                    plugin_id
                                );
                            }
                            Ok(code) => {
                                // deactivate 返回非零，记录警告但不阻止停用流程
                                tracing::warn!(
                                    "Cdylib plugin deactivate() returned error code {}: {}",
                                    code,
                                    plugin_id
                                );
                            }
                            Err(_) => {
                                tracing::error!(
                                    "Cdylib plugin deactivate() panicked: {}",
                                    plugin_id
                                );
                            }
                        }
                    }
                }
            }
        }

        // 统一清理：取消注册和撤销权限
        self.registry.unregister_plugin(plugin_id).await;
        self.permission.revoke_all(plugin_id);

        let mut plugins = self.plugins.write().await;
        let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
        })?;

        loaded.state = PluginState::Deactivated;
        loaded.activated_at = None;
        tracing::info!("Plugin deactivated: {}", plugin_id);

        if persist {
            self.persist_activation_state().await;
        }

        Ok(())
    }

    /// 热重载 cdylib 插件（开发模式）
    ///
    /// 执行完整的卸载-重载-激活循环：
    /// 1. 停用插件（调用 deactivate + 清理注册/权限）
    /// 2. 卸载旧 cdylib（从 map 移除触发 Library drop / FreeLibrary）
    /// 3. 重新加载 cdylib（shadow copy 新 DLL + 解析符号）
    /// 4. 重新激活插件
    ///
    /// 仅在开发模式下可用，生产构建中调用返回错误
    pub async fn reload_cdylib_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        // 验证插件存在且为 cdylib 来源
        let (rust_library, extension_path) = {
            let plugins = self.plugins.read().await;
            let loaded = plugins.get(plugin_id).ok_or_else(|| {
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;
            if loaded.source != PluginSource::Cdylib {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin {} is not a cdylib plugin, cannot hot-reload",
                    plugin_id
                )));
            }
            (loaded.manifest.rust_library.clone(), loaded.extension_path.clone())
        };

        tracing::info!("Hot-reloading cdylib plugin: {}", plugin_id);

        // 1. 停用插件（不持久化，热重载后立即重新激活）
        self.deactivate_plugin(plugin_id, false).await?;

        // 2. 卸载旧 cdylib：从 map 移除，触发 Library drop（FreeLibrary）
        let old_cdylib = {
            let mut cdylib = self.cdylib_plugins.write().await;
            cdylib.remove(plugin_id)
        };
        // 显式 drop 确保 Library 在重新加载前释放
        drop(old_cdylib);

        // 3. 重新加载 cdylib（CdylibLoader::load 会自动 shadow copy）
        let plugin_dir = Path::new(&extension_path);
        let new_cdylib = CdylibLoader::load(plugin_dir, &rust_library)?;

        // 存入 cdylib_plugins map
        self.cdylib_plugins.write().await.insert(plugin_id.to_string(), new_cdylib);

        // 4. 重新注册 manifest contributes（deactivate 时已 unregister）
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

        // 5. 重新激活（不持久化，保持原有激活状态记录）
        self.activate_plugin(plugin_id, false).await?;

        tracing::info!("Cdylib plugin hot-reloaded successfully: {}", plugin_id);
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
            map.insert(id.clone(), matches!(loaded.state, PluginState::Activated));
        }
        map
    }

    /// 持久化当前激活状态到 SQLite
    ///
    /// 仅记录非 StaticRegistry 插件的状态，Rust-only 插件始终激活无需记录
    async fn persist_activation_state(&self) {
        let activated_map = self.get_activated_state().await;
        if let Err(e) = self.storage.save_activated_plugins(&activated_map).await {
            tracing::warn!("Failed to persist plugin activation state: {}", e);
        }
    }

    /// 根据持久化状态自动激活之前已激活的插件
    ///
    /// 在 PluginHost::new() 末尾调用，跳过 StaticRegistry 插件，
    /// 清理已不存在的插件 ID
    async fn auto_activate_from_persisted_state(&self) {
        let activated_map = match self.storage.load_activated_plugins().await {
            Ok(map) => map,
            Err(e) => {
                tracing::warn!("Failed to load persisted activation state, skipping auto-activation: {}", e);
                return;
            }
        };

        if activated_map.is_empty() {
            return;
        }

        // 收集需要激活的插件 ID（跳过 StaticRegistry）
        let to_activate: Vec<String> = {
            let plugins = self.plugins.read().await;
            activated_map.iter()
                .filter(|(id, &is_active)| {
                    if !is_active { return false; }
                    // 跳过 Rust-only 插件（始终激活，不可控）
                    plugins.get(*id)
                        .map(|p| p.source != PluginSource::StaticRegistry)
                        .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect()
        };

        for plugin_id in &to_activate {
            if let Err(e) = self.activate_plugin(plugin_id, false).await {
                tracing::warn!("Failed to auto-activate plugin {}: {}", plugin_id, e);
            }
        }

        // 清理已不存在的插件 ID（磁盘已删除）
        let current_ids: HashSet<String> = self.plugins.read().await.keys().cloned().collect();
        let original_len = activated_map.len();
        let mut cleaned_map = activated_map;
        cleaned_map.retain(|id, _| current_ids.contains(id));
        if cleaned_map.len() != original_len {
            if let Err(e) = self.storage.save_activated_plugins(&cleaned_map).await {
                tracing::warn!("Failed to clean up stale activation entries: {}", e);
            }
        }

        if !to_activate.is_empty() {
            tracing::info!("Auto-activated {} plugin(s) from persisted state", to_activate.len());
        }
    }

    /// 判断插件是否应该按需激活
    pub async fn should_lazy_activate(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        if let Some(loaded) = plugins.get(plugin_id) {
            // Rust 插件（static registry）不懒激活，由 PluginHost 统一管理生命周期
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
    /// - cdylib 插件：通过 FFI 调用 exports.invoke_command()
    /// - 静态注册插件：通过运行时注册表查找 handler
    pub async fn invoke_rust_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        // 权限校验：插件必须处于激活状态
        if !self.is_activated(plugin_id).await {
            return Err(crate::AppError::Plugin(format!(
                "Plugin {} is not activated", plugin_id
            )));
        }

        // 读取插件来源，决定路由方式
        let source = {
            let plugins = self.plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| p.source.clone())
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?
        };

        match source {
            PluginSource::Cdylib => {
                self.invoke_cdylib_command(plugin_id, command_name, args).await
            }
            PluginSource::StaticRegistry => {
                self.invoke_static_command(plugin_id, command_name, args).await
            }
            PluginSource::FileScan => {
                // TS-only 插件不应有 Rust command 调用
                Err(crate::AppError::Plugin(format!(
                    "Plugin {} is TS-only, cannot invoke Rust command", plugin_id
                )))
            }
        }
    }

    /// 调用 cdylib 插件的 command
    ///
    /// 将 command_name 和 args 转为 C 字符串，通过 FFI 调用 exports.invoke_command()，
    /// 解析返回的 JSON 字符串，并通过 CString::from_raw 释放插件分配的内存
    async fn invoke_cdylib_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args: serde_json::Value,
    ) -> crate::Result<serde_json::Value> {
        let cdylib_plugins = self.cdylib_plugins.read().await;
        let cdylib_plugin = cdylib_plugins.get(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!(
                "Cdylib plugin {} not found in loaded libraries", plugin_id
            ))
        })?;

        let exports = cdylib_plugin.exports();

        // 将参数转为 C 字符串
        let name_cstr = CString::new(command_name)
            .map_err(|e| crate::AppError::Plugin(format!(
                "Command name contains null bytes: {}", e
            )))?;
        let args_str = serde_json::to_string(&args)
            .map_err(|e| crate::AppError::Plugin(format!(
                "Failed to serialize command args: {}", e
            )))?;
        let args_cstr = CString::new(args_str)
            .map_err(|e| crate::AppError::Plugin(format!(
                "Command args contain null bytes: {}", e
            )))?;

        // 调用 cdylib 的 invoke_command，catch_unwind 防止 panic 传播
        let result_ptr = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: exports 函数指针由 libloading 从已加载的动态库解析
            unsafe {
                (exports.invoke_command)(name_cstr.as_ptr(), args_cstr.as_ptr())
            }
        }));

        let ptr = match result_ptr {
            Ok(p) => p,
            Err(_) => {
                return Err(crate::AppError::Plugin(format!(
                    "Cdylib plugin {} invoke_command() panicked", plugin_id
                )));
            }
        };

        // 解析返回值：null 表示调用失败
        if ptr.is_null() {
            return Err(crate::AppError::Plugin(format!(
                "Cdylib plugin {} invoke_command() returned null", plugin_id
            )));
        }

        // SAFETY: ptr 由插件通过 CString::into_raw() 或等价方式分配，
        // 我们通过 CString::from_raw 回收内存（同一 allocator）
        let result_string = unsafe {
            let cstr = CStr::from_ptr(ptr);
            let s = cstr.to_string_lossy().into_owned();
            // 释放插件分配的内存
            let _ = CString::from_raw(ptr);
            s
        };

        // 解析 JSON 结果
        let value: serde_json::Value = serde_json::from_str(&result_string)
            .map_err(|e| crate::AppError::Plugin(format!(
                "Cdylib plugin {} invoke_command() returned invalid JSON: {}", plugin_id, e
            )))?;

        Ok(value)
    }

    /// 调用静态注册插件的 command handler（inventory 静态注册）
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
    ///
    /// 依次调用所有已注册的 Rust terminal handler 的 `on_input`，
    /// 如果任一 handler 返回 `Some(modified)`，后续 handler 使用修改后的文本。
    /// 返回最终处理后的文本（如果没有 handler 修改，返回原始输入）
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
    ///
    /// 依次调用所有已注册的 Rust terminal handler 的 `on_output`，
    /// 如果任一 handler 返回 `Some(modified)`，后续 handler 使用修改后的数据。
    /// 返回最终处理后的数据（如果没有 handler 修改，返回原始数据）
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
            cdylib_plugins: self.cdylib_plugins.clone(),
            host_context_fns: self.host_context_fns.clone(),
        }
    }
}
