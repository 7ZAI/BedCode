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
use std::sync::OnceLock;
use tauri::Emitter;
use tokio::sync::RwLock;

/// 插件生命周期管理器
pub struct PluginManager {
    /// 已加载的插件清单
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// WASM 运行时（延迟初始化，必须在 Tokio 上下文中创建）
    wasm_runtime: OnceLock<Arc<WasmRuntime>>,
    /// 已加载的 WASM 插件实例
    wasm_plugins: Arc<RwLock<HashMap<String, LoadedWasmPlugin>>>,
    /// WASM 宿主上下文（延迟初始化）
    wasm_host_ctx: OnceLock<Arc<WasmHostContext>>,
    /// 插件键值存储
    storage: Arc<PluginStorage>,
    /// 设置管理器
    settings: Arc<SettingsManager>,
    /// 插件数据目录
    plugins_dir: PathBuf,
    /// 插件数据库连接（WASM Host Function 使用）
    plugin_db: Arc<tokio::sync::Mutex<rusqlite::Connection>>,
    /// Tauri AppHandle
    app_handle: Arc<tauri::AppHandle>,
    /// 文件系统访问校验器
    fs_auth: Arc<crate::plugin::fs_auth::FsAuthChecker>,
    /// 消息总线
    message_bus: Arc<crate::plugin::message_bus::MessageBus>,
}

impl PluginManager {
    /// 创建插件管理器（不初始化 WASM 运行时）
    ///
    /// WASM 运行时通过 init_wasm_runtime() 延迟初始化，
    /// 因为 Engine 创建需要 Tokio 运行时上下文
    pub fn new(
        app_data_dir: &PathBuf,
        settings: Arc<SettingsManager>,
        plugin_db: Arc<tokio::sync::Mutex<rusqlite::Connection>>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        let storage = Arc::new(PluginStorage::new(app_data_dir));
        let plugins_dir = app_data_dir.join(PLUGIN_DATA_DIR);

        let fs_auth = Arc::new(crate::plugin::fs_auth::FsAuthChecker::new(
            storage.clone(),
            app_handle.clone(),
        ));
        let message_bus = Arc::new(crate::plugin::message_bus::MessageBus::new());

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
            wasm_runtime: OnceLock::new(),
            wasm_plugins: Arc::new(RwLock::new(HashMap::new())),
            wasm_host_ctx: OnceLock::new(),
            storage,
            settings,
            plugins_dir,
            plugin_db,
            app_handle,
            fs_auth,
            message_bus,
        }
    }

    /// 延迟初始化 WASM 运行时
    ///
    /// 必须在 Tokio 运行时上下文中调用（Engine 创建需要 Handle）
    pub fn init_wasm_runtime(&self) -> crate::Result<()> {
        let runtime = Arc::new(WasmRuntime::new(
            self.plugin_db.clone(),
            self.storage.clone(),
            self.app_handle.clone(),
        )?);

        let host_ctx = Arc::new(WasmHostContext::new(
            self.plugin_db.clone(),
            self.storage.clone(),
            self.app_handle.clone(),
            self.fs_auth.clone(),
            self.message_bus.clone(),
        ));

        let _ = self.wasm_runtime.set(runtime);
        let _ = self.wasm_host_ctx.set(host_ctx);

        // 注入 dispatcher（PluginManagerDispatcher 实现 MessageDispatcher）
        let dispatcher: Arc<dyn crate::plugin::message_bus::MessageDispatcher> = Arc::new(PluginManagerDispatcher {
            plugins: self.plugins.clone(),
            wasm_plugins: self.wasm_plugins.clone(),
        });
        let bus = self.message_bus.clone();
        let handle = tokio::runtime::Handle::current();
        handle.block_on(bus.set_dispatcher(dispatcher));

        Ok(())
    }

    /// 扫描并加载所有插件
    ///
    /// 在 APK assets 解压后调用，扫描 plugins_dir 下的所有 plugin.json
    /// 需要 WASM 运行时已初始化（调用 init_wasm_runtime 后）
    pub async fn scan_and_load(&self) {
        let Some(wasm_runtime) = self.wasm_runtime.get() else {
            tracing::warn!("[PluginManager] WASM runtime not initialized, skipping scan");
            return;
        };
        let Some(wasm_host_ctx) = self.wasm_host_ctx.get() else {
            tracing::warn!("[PluginManager] WASM host context not initialized, skipping scan");
            return;
        };

        let (plugins, wasm_plugins) = PluginLoader::load_all(
            &self.plugins_dir,
            wasm_runtime,
            wasm_host_ctx,
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

        // 通知所有插件应用启动完成
        self.dispatch_lifecycle_event(PluginLifecycleEvent::AppStartup).await;
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

        // 清理消息总线订阅
        let bus = self.message_bus.clone();
        let pid = plugin_id.to_string();
        let handle = tokio::runtime::Handle::current();
        handle.block_on(bus.remove_all_subscriptions(&pid));

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

    /// 获取文件系统访问校验器引用
    pub fn fs_auth(&self) -> &Arc<crate::plugin::fs_auth::FsAuthChecker> {
        &self.fs_auth
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::plugin::message_bus::MessageBus> {
        &self.message_bus
    }

    /// 获取插件数据目录
    pub fn plugins_dir(&self) -> &PathBuf {
        &self.plugins_dir
    }

    /// 获取 WASM 运行时引用
    ///
    /// # Panics
    /// 如果 init_wasm_runtime 未调用则 panic
    pub fn wasm_runtime(&self) -> &Arc<WasmRuntime> {
        self.wasm_runtime.get().expect("WasmRuntime not initialized")
    }

    /// 获取 WASM 宿主上下文引用
    ///
    /// # Panics
    /// 如果 init_wasm_runtime 未调用则 panic
    pub fn wasm_host_ctx(&self) -> &Arc<WasmHostContext> {
        self.wasm_host_ctx.get().expect("WasmHostContext not initialized")
    }

    /// 分发生命周期事件到所有已激活插件
    ///
    /// 1. 遍历所有已激活的 WASM 插件，检查声明后调用导出函数
    /// 2. 通过 app_handle.emit() 发射 Tauri 事件给前端 TS 插件
    pub async fn dispatch_lifecycle_event(&self, event: PluginLifecycleEvent) {
        let event_name = event.name();

        // 快速检查：是否有任何 WASM 插件声明了该事件
        let plugins = self.plugins.read().await;
        let any_wasm_declared = plugins.values().any(|p| {
            p.state == PluginState::Activated
                && p.manifest.plugin_type == PluginType::Wasm
                && p.manifest.contributes.lifecycle.as_ref()
                    .map(|l| l.is_declared(event_name))
                    .unwrap_or(false)
        });

        if any_wasm_declared {
            // WASM 插件回调
            for (id, plugin) in plugins.iter() {
                if plugin.state != PluginState::Activated {
                    continue;
                }
                if plugin.manifest.plugin_type != PluginType::Wasm {
                    continue;
                }
                if !plugin.manifest.contributes.lifecycle.as_ref()
                    .map(|l| l.is_declared(event_name))
                    .unwrap_or(false)
                {
                    continue;
                }

                let mut wasm_plugins = self.wasm_plugins.write().await;
                if let Some(wasm_plugin) = wasm_plugins.get_mut(id) {
                    if let Err(e) = wasm_plugin.call_lifecycle_event(&event) {
                        tracing::warn!(
                            plugin_id = %id,
                            event = %event_name,
                            error = %e,
                            "WASM lifecycle callback failed"
                        );
                    }
                }
            }
        }

        drop(plugins);

        // 前端 Tauri 事件发射
        self.emit_frontend_event(&event);
    }

    /// 发射前端 Tauri 生命周期事件
    fn emit_frontend_event(&self, event: &PluginLifecycleEvent) {
        let tauri_event = format!("plugin:lifecycle:{}", event.tauri_event_name());
        let payload = event.to_payload();
        if let Err(e) = self.app_handle.emit(&tauri_event, payload) {
            tracing::error!(
                event = %tauri_event,
                error = %e,
                "Failed to emit frontend lifecycle event"
            );
        }
    }
}

/// PluginManager 的 MessageDispatcher 代理
///
/// 独立结构体避免 PluginManager 直接实现 trait 导致的生命周期问题
struct PluginManagerDispatcher {
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    wasm_plugins: Arc<RwLock<HashMap<String, LoadedWasmPlugin>>>,
}

impl crate::plugin::message_bus::MessageDispatcher for PluginManagerDispatcher {
    fn dispatch_to_wasm(&self, plugin_id: &str, msg: &bedcode_plugin_api_mobile::BusMessage) -> anyhow::Result<()> {
        let mut wasm_plugins = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.wasm_plugins.write())
        });
        if let Some(wasm_plugin) = wasm_plugins.get_mut(plugin_id) {
            wasm_plugin.on_bus_message(msg)?;
        }
        Ok(())
    }

    fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.plugins.read())
        });
        plugins.get(plugin_id)
            .map(|p| p.state == PluginState::Activated)
            .unwrap_or(false)
    }
}
