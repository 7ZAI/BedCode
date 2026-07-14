//! Mobile Plugin Manager
//!
//! 插件生命周期管理 — WASM 动态加载、激活、停用、状态持久化

use crate::plugin::loader::PluginLoader;
use crate::plugin::registry::builtin_manifests;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use crate::plugin::wasm_runtime::{LoadedWasmPlugin, WasmHostContext, WasmRuntime};
use crate::system::constants::plugin::PLUGIN_ENABLED_KEY_PREFIX;
use crate::system::constants::plugin::PLUGIN_DATA_DIR;
use crate::system::settings::SettingsManager;
use crate::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 插件生命周期管理器
pub struct PluginManager {
    /// 已加载的插件清单
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// WASM 运行时
    wasm_runtime: Arc<WasmRuntime>,
    /// 已加载的 WASM 插件实例
    wasm_plugins: Arc<RwLock<HashMap<String, LoadedWasmPlugin>>>,
    /// WASM 宿主上下文
    wasm_host_ctx: Arc<WasmHostContext>,
    /// 插件键值存储
    storage: Arc<PluginStorage>,
    /// 设置管理器
    settings: Arc<SettingsManager>,
    /// 插件数据目录
    plugins_dir: PathBuf,
}

impl PluginManager {
    /// 创建插件管理器
    pub fn new(
        app_data_dir: &PathBuf,
        settings: Arc<SettingsManager>,
        wasm_runtime: Arc<WasmRuntime>,
        wasm_host_ctx: Arc<WasmHostContext>,
    ) -> Self {
        let storage = Arc::new(PluginStorage::new(app_data_dir));
        let plugins_dir = app_data_dir.join(PLUGIN_DATA_DIR);

        // builtin_manifests() 当前返回空 Vec，内置插件走 APK assets 加载
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
                    source: PluginSource::FrontendOnly,
                    extension_path: String::new(),
                },
            );
        }

        Self {
            plugins: Arc::new(RwLock::new(plugins)),
            wasm_runtime,
            wasm_plugins: Arc::new(RwLock::new(HashMap::new())),
            wasm_host_ctx,
            storage,
            settings,
            plugins_dir,
        }
    }

    /// 扫描并加载所有插件
    ///
    /// 在 APK assets 解压后调用，扫描 plugins_dir 下的所有 plugin.json
    pub async fn scan_and_load(&self) {
        let (plugins, wasm_plugins) = PluginLoader::load_all(
            &self.plugins_dir,
            &self.wasm_runtime,
            &self.wasm_host_ctx,
        );

        let mut current_plugins = self.plugins.write().await;
        for (id, plugin) in plugins {
            current_plugins.insert(id, plugin);
        }
        drop(current_plugins);

        let mut current_wasm = self.wasm_plugins.write().await;
        for (id, wasm_plugin) in wasm_plugins {
            current_wasm.insert(id, wasm_plugin);
        }
    }

    /// 应用启动时：读取持久化启用状态，自动激活
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
    pub async fn activate(&self, plugin_id: &str, _app_handle: &tauri::AppHandle) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        if plugin.state == PluginState::Activated {
            return Ok(());
        }

        // WASM 插件：调用 activate 导出函数
        if plugin.manifest.plugin_type == PluginType::Wasm {
            let mut wasm_plugins = self.wasm_plugins.write().await;
            if let Some(wasm_plugin) = wasm_plugins.get_mut(plugin_id) {
                match wasm_plugin.activate() {
                    Ok(0) => {
                        plugin.state = PluginState::Activated;
                        tracing::info!(plugin_id = %plugin_id, "WASM plugin activated");
                    }
                    Ok(code) => {
                        let error = format!("WASM activate() returned error code: {}", code);
                        plugin.state = PluginState::Error { error: error.clone() };
                        return Err(crate::AppError::Plugin(error));
                    }
                    Err(e) => {
                        plugin.state = PluginState::Error { error: e.to_string() };
                        return Err(e);
                    }
                }
            } else {
                // WASM 实例不存在，仅标记前端激活
                plugin.state = PluginState::Activated;
                tracing::info!(plugin_id = %plugin_id, "Plugin activated (frontend only, no WASM instance)");
            }
        } else {
            // TS-only 插件：仅标记状态
            plugin.state = PluginState::Activated;
            tracing::info!(plugin_id = %plugin_id, "Plugin activated (ts-only)");
        }

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

        // WASM 插件：调用 deactivate 导出函数
        if plugin.manifest.plugin_type == PluginType::Wasm {
            let mut wasm_plugins = self.wasm_plugins.write().await;
            if let Some(wasm_plugin) = wasm_plugins.get_mut(plugin_id) {
                if let Err(e) = wasm_plugin.deactivate() {
                    tracing::warn!(plugin_id = %plugin_id, error = %e, "WASM plugin deactivate failed");
                }
            }
        }

        plugin.state = PluginState::Deactivated;
        tracing::info!(plugin_id = %plugin_id, "Plugin deactivated");
        Ok(())
    }

    /// 调用 WASM 插件命令
    pub async fn invoke_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args_json: &str,
    ) -> Result<String> {
        let mut wasm_plugins = self.wasm_plugins.write().await;
        let wasm_plugin = wasm_plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("WASM plugin not found: {}", plugin_id)))?;

        wasm_plugin.invoke_command(command_name, args_json)
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

    /// 获取插件数据目录
    pub fn plugins_dir(&self) -> &PathBuf {
        &self.plugins_dir
    }

    /// 获取 WASM 运行时引用
    pub fn wasm_runtime(&self) -> &Arc<WasmRuntime> {
        &self.wasm_runtime
    }

    /// 获取 WASM 宿主上下文引用
    pub fn wasm_host_ctx(&self) -> &Arc<WasmHostContext> {
        &self.wasm_host_ctx
    }
}
