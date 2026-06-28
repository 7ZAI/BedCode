//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage 四个子系统

use crate::desktop::plugin::loader::PluginLoader;
use crate::desktop::plugin::permission::PermissionManager;
use crate::desktop::plugin::registry::PluginRegistry;
use crate::desktop::plugin::storage::PluginStorage;
use crate::desktop::plugin::types::{LoadedPlugin, PluginInfo, PluginState};
use crate::shared::db::Database;
use chrono::Utc;
use std::collections::HashMap;
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
}

impl PluginHost {
    /// 创建 PluginHost 并扫描加载插件
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let storage = Arc::new(PluginStorage::new(db));

        // 扫描并加载所有 plugin.json
        let plugins = PluginLoader::load_all(&permission);
        let count = plugins.len();

        let host = Self {
            plugins: Arc::new(RwLock::new(plugins)),
            registry,
            permission,
            storage,
        };

        // 将 manifest 中的 contributes 注册到 registry
        let host_clone = host.clone();
        tauri::async_runtime::block_on(async {
            host_clone.register_manifest_contributions().await;
        });

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
    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.values().map(PluginInfo::from).collect()
    }

    /// 获取单个插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(PluginInfo::from)
    }

    /// 激活插件（标记状态为 Activated）
    ///
    /// 实际的 JS 模块加载在前端 PluginLoader 中完成，
    /// Rust 端只负责状态管理和权限授予
    pub async fn activate_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        let mut plugins = self.plugins.write().await;
        let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!("插件不存在: {}", plugin_id))
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

        loaded.state = PluginState::Activated;
        loaded.activated_at = Some(Utc::now());
        tracing::info!("Plugin activated: {}", plugin_id);
        Ok(())
    }

    /// 停用插件
    pub async fn deactivate_plugin(&self, plugin_id: &str) -> crate::Result<()> {
        // 先清理 registry 和权限（不需要写锁）
        self.registry.unregister_plugin(plugin_id).await;
        self.permission.revoke_all(plugin_id);

        let mut plugins = self.plugins.write().await;
        let loaded = plugins.get_mut(plugin_id).ok_or_else(|| {
            crate::AppError::Plugin(format!("插件不存在: {}", plugin_id))
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

// 通过 Arc 共享内部状态实现 Clone
impl Clone for PluginHost {
    fn clone(&self) -> Self {
        Self {
            plugins: self.plugins.clone(),
            registry: self.registry.clone(),
            permission: self.permission.clone(),
            storage: self.storage.clone(),
        }
    }
}
