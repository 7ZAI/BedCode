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
use bedcode_plugin_api::{PluginState, PluginCommandEntry};
use chrono::Utc;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::Arc;
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
            cdylib_plugins: Arc::new(RwLock::new(cdylib_plugins_map)),
            host_context_fns,
        };

        // 注册所有已加载插件的 manifest contributes 到 registry
        host.register_manifest_contributions().await;

        // 注册 Rust 插件的 command handlers（inventory 静态注册）
        host.register_rust_command_handlers().await;

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

    /// 激活插件
    ///
    /// - 静态注册插件：仅标记状态
    /// - cdylib 插件：调用 exports.activate() 传入 HostContext
    /// - TS-only 插件：前端模块加载在 PluginLoader 中完成
    pub async fn activate_plugin(&self, plugin_id: &str) -> crate::Result<()> {
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
        Ok(())
    }

    /// 停用插件
    ///
    /// cdylib 插件：先调用 exports.deactivate()，再执行现有清理流程
    pub async fn deactivate_plugin(&self, plugin_id: &str) -> crate::Result<()> {
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
        Ok(())
    }

    /// 标记插件为错误状态
    pub async fn mark_error(&self, plugin_id: &str, error: String) {
        let mut plugins = self.plugins.write().await;
        if let Some(loaded) = plugins.get_mut(plugin_id) {
            loaded.state = PluginState::Error(error);
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
            cdylib_plugins: self.cdylib_plugins.clone(),
            host_context_fns: self.host_context_fns.clone(),
        }
    }
}
