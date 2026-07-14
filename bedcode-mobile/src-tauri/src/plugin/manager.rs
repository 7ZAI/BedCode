//! Mobile Plugin Manager
//!
//! 插件生命周期管理 — 加载、激活、停用、状态持久化

use crate::plugin::host::PluginHost;
use crate::plugin::registry::{builtin_manifests, PluginHostContext};
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use crate::system::constants::plugin::PLUGIN_ENABLED_KEY_PREFIX;
use crate::system::settings::SettingsManager;
use crate::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 插件生命周期管理器
pub struct PluginManager {
    /// 已加载的插件
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// Rust 插件宿主
    rust_host: PluginHost,
    /// 插件键值存储
    storage: Arc<PluginStorage>,
    /// 设置管理器（用于持久化启用状态）
    settings: Arc<SettingsManager>,
}

impl PluginManager {
    /// 创建插件管理器
    pub fn new(app_data_dir: &PathBuf, settings: Arc<SettingsManager>) -> Self {
        let rust_host = PluginHost::empty();
        let storage = Arc::new(PluginStorage::new(app_data_dir));

        // 从 builtin_manifests() 加载所有内置插件
        let mut plugins = HashMap::new();
        for manifest in builtin_manifests() {
            let id = manifest.id.clone();
            let permissions: std::collections::HashSet<String> =
                manifest.permissions.iter().cloned().collect();
            plugins.insert(
                id,
                LoadedPlugin {
                    manifest,
                    state: PluginState::Loaded,
                    granted_permissions: permissions,
                },
            );
        }

        Self {
            plugins: Arc::new(RwLock::new(plugins)),
            rust_host,
            storage,
            settings,
        }
    }

    /// 应用启动时调用：读取持久化启用状态，自动激活之前启用的插件
    pub async fn load_all(&self, app_handle: &tauri::AppHandle) {
        let plugins = self.plugins.read().await;
        let plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        drop(plugins);

        for id in plugin_ids {
            if let Ok(Some(value)) = self.settings.get(&format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, &id)).await {
                if value == "true" {
                    if let Err(e) = self.activate(&id, app_handle).await {
                        tracing::warn!(plugin_id = %id, error = %e, "Failed to auto-activate plugin on startup");
                    }
                }
            }
        }
    }

    /// 激活插件
    pub async fn activate(&self, plugin_id: &str, app_handle: &tauri::AppHandle) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        if plugin.state == PluginState::Activated {
            return Ok(());
        }

        // 激活 Rust 插件
        let ctx = PluginHostContext {
            app_handle: app_handle.clone(),
        };
        if let Err(e) = self.rust_host.activate(plugin_id, &ctx) {
            plugin.state = PluginState::Error {
                error: e.to_string(),
            };
            return Err(e);
        }

        plugin.state = PluginState::Activated;
        tracing::info!(plugin_id = %plugin_id, "Plugin activated");
        Ok(())
    }

    /// 停用插件
    pub async fn deactivate(&self, plugin_id: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        if plugin.state != PluginState::Activated {
            return Ok(());
        }

        // 停用 Rust 插件
        if let Err(e) = self.rust_host.deactivate(plugin_id) {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "Rust plugin deactivate failed");
        }

        plugin.state = PluginState::Deactivated;
        tracing::info!(plugin_id = %plugin_id, "Plugin deactivated");
        Ok(())
    }

    /// 返回所有已加载插件信息
    pub async fn list_loaded(&self) -> Vec<MobilePluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| MobilePluginInfo::from(p)).collect()
    }

    /// 返回单个插件信息
    pub async fn get_info(&self, plugin_id: &str) -> Option<MobilePluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(|p| MobilePluginInfo::from(p))
    }

    /// 查询插件启用状态
    pub async fn is_enabled(&self, plugin_id: &str) -> bool {
        let key = format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, plugin_id);
        match self.settings.get(&key).await {
            Ok(Some(v)) => v == "true",
            _ => false,
        }
    }

    /// 设置插件启用状态并持久化
    pub async fn set_enabled(&self, plugin_id: &str, enabled: bool) -> Result<()> {
        let key = format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, plugin_id);
        self.settings
            .set(key, enabled.to_string())
            .await?;
        tracing::info!(plugin_id = %plugin_id, enabled = enabled, "Plugin enabled state persisted");
        Ok(())
    }

    /// 标记插件错误状态
    pub async fn mark_error(&self, plugin_id: &str, error: String) {
        let mut plugins = self.plugins.write().await;
        if let Some(plugin) = plugins.get_mut(plugin_id) {
            plugin.state = PluginState::Error { error };
        }
    }

    /// 获取存储管理器引用
    pub fn storage(&self) -> &PluginStorage {
        &self.storage
    }
}
