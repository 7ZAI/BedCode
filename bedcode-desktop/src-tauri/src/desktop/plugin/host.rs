//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage 四个子系统
//! 支持静态注册（Rust 插件）和文件扫描（TS-only 插件）

use crate::desktop::plugin::loader::PluginLoader;
use crate::desktop::plugin::permission::PermissionManager;
use crate::desktop::plugin::registry::PluginRegistry;
use crate::desktop::plugin::storage::PluginStorage;
use crate::desktop::plugin::types::{DesktopPluginInfo, LoadedPlugin, PluginSource};
use crate::shared::db::Database;
use bedcode_plugin_api::{PluginState, PluginCommandEntry};
use chrono::Utc;
use std::collections::HashMap;
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
    /// Rust 插件的 command handlers（运行时注册）
    rust_command_handlers: Arc<RwLock<HashMap<String, bedcode_plugin_api::PluginCommand>>>,
}

impl PluginHost {
    /// 创建 PluginHost 并加载所有插件（静态注册 + 文件扫描）
    ///
    /// 改为 async 方法，避免 block_on 在 Tokio runtime 中的潜在风险
    pub async fn new(db: Arc<Mutex<Database>>, plugins_dir: &Path) -> Self {
        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let storage = Arc::new(PluginStorage::new(db));

        // 1. 收集静态注册的 Rust 插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();

        // 2. 扫描文件系统中的 TS-only 插件
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
                extension_path: String::new(), // Rust 插件无文件路径
                activated_at: None,
                source: PluginSource::StaticRegistry,
            };

            tracing::info!("Static plugin loaded: {} v{}", loaded.manifest.id, loaded.manifest.version);
            all_plugins.insert(plugin_id, loaded);
        }

        // 添加文件扫描的 TS-only 插件
        for (id, loaded) in file_plugins {
            all_plugins.insert(id, loaded);
        }

        let host = Self {
            plugins: Arc::new(RwLock::new(all_plugins)),
            registry,
            permission,
            storage,
            rust_command_handlers: Arc::new(RwLock::new(HashMap::new())),
        };

        // 注册所有已加载插件的 manifest contributes 到 registry
        host.register_manifest_contributions().await;

        // 注册 Rust 插件的 command handlers
        host.register_rust_command_handlers().await;

        let count = host.plugins.read().await.len();
        tracing::info!("PluginHost initialized with {} plugin(s)", count);
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

    /// 注册 Rust 插件的 command handlers 到运行时注册表
    async fn register_rust_command_handlers(&self) {
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>.into_iter().collect();

        let mut handlers = self.rust_command_handlers.write().await;
        for entry in static_plugins {
            let commands = (entry.register_commands)();
            let plugin_id = entry.id;
            for cmd in commands {
                // 使用 plugin_id::command_name 作为 key，避免冲突
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

    /// 激活插件（标记状态为 Activated）
    ///
    /// Rust 插件通过此方法调用 BedcodePlugin::activate()
    /// TS-only 插件的前端模块加载在 PluginLoader 中完成
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

        loaded.state = PluginState::Activated;
        loaded.activated_at = Some(Utc::now());

        tracing::info!("Plugin activated: {}", plugin_id);
        Ok(())
    }

    /// 停用插件
    pub async fn deactivate_plugin(&self, plugin_id: &str) -> crate::Result<()> {
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
        }
    }
}
