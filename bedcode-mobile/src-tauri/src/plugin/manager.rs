//! Mobile Plugin Manager
//!
//! 插件生命周期管理 — WASM 动态加载、激活、停用、状态持久化

use async_trait::async_trait;
use crate::plugin::approval::{
    compute_dir_hash, effective_permissions, verify_approval, ApprovalStatus, PluginApprovalStore,
};
use crate::plugin::loader::PluginLoader;
use crate::plugin::registry::builtin_manifests;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use crate::plugin::wasm_runtime::{LoadedComponentPlugin, WasmHostContext, WasmRuntime};
use crate::system::constants::plugin::PLUGIN_ENABLED_KEY_PREFIX;
use crate::system::constants::plugin::PLUGIN_DATA_DIR;
use crate::system::settings::SettingsManager;
use crate::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex as TokioMutex, RwLock};

/// 插件生命周期管理器
pub struct PluginManager {
    /// 已加载的插件清单
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    /// WASM 运行时（延迟初始化，必须在 Tokio 上下文中创建）
    wasm_runtime: OnceLock<Arc<WasmRuntime>>,
    /// 已加载的 WASM 插件实例
    ///
    /// 每插件独立 Mutex：map 守卫只短持有（查找/增删），同步执行 WASM 期间
    /// 仅持有单插件实例锁，避免持 map 守卫执行 WASM 导致 host function 重入死锁
    wasm_plugins: Arc<RwLock<HashMap<String, Arc<TokioMutex<LoadedComponentPlugin>>>>>,
    /// WASM 宿主上下文（延迟初始化）
    wasm_host_ctx: OnceLock<Arc<WasmHostContext>>,
    /// 插件键值存储
    storage: Arc<PluginStorage>,
    /// 权限审批存储（批准记录 + 内容哈希钉扎，见 approval.rs）
    approvals: Arc<PluginApprovalStore>,
    /// 设置管理器
    settings: Arc<SettingsManager>,
    /// 插件数据目录
    plugins_dir: PathBuf,
    /// 插件数据库连接（WASM Host Function 使用；std Mutex，见 lib.rs 创建处注释）
    plugin_db: Arc<std::sync::Mutex<rusqlite::Connection>>,
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
        plugin_db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        app_handle: Arc<tauri::AppHandle>,
    ) -> Self {
        let storage = Arc::new(PluginStorage::new(app_data_dir));
        let approvals = Arc::new(PluginApprovalStore::new(storage.clone()));
        let plugins_dir = app_data_dir.join(PLUGIN_DATA_DIR);

        let fs_auth = Arc::new(crate::plugin::fs_auth::FsAuthChecker::new(
            storage.clone(),
            Some(app_handle.clone()),
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
            approvals,
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
    /// 必须在 Tokio 运行时上下文中调用（Engine 创建需要 Handle）。
    /// async：内部需 await 注入 dispatcher，禁止在运行时内使用 block_on（会 panic）
    pub async fn init_wasm_runtime(&self) -> crate::Result<()> {
        // AOT 缓存目录：宿主 cache 目录（非插件目录，防反序列化产物被投毒）
        let aot_cache_dir = self
            .app_handle
            .path()
            .app_cache_dir()
            .ok()
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
        let runtime = Arc::new(WasmRuntime::new(aot_cache_dir)?);

        // 插件状态上报回调：置 Error + 持久化未启用 + 前端通知
        let plugins = self.plugins.clone();
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> =
            Arc::new(move |plugin_id, error| {
                let plugins = plugins.clone();
                let settings = settings.clone();
                let app_handle = app_handle.clone();
                let pid = plugin_id.to_string();
                let err = error.to_string();
                // async block 需要独占所有权，外层克隆供 emit/日志使用
                let pid_clone = pid.clone();
                let err_clone = err.clone();

                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async move {
                        // 置 Error 状态
                        let mut map = plugins.write().await;
                        if let Some(p) = map.get_mut(&pid_clone) {
                            p.state = PluginState::Error { error: err_clone.clone() };
                        }
                        drop(map);

                        // 持久化未启用（下次启动不再自动激活）
                        if let Err(e) = settings.set(format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, &pid_clone), "false".to_string()).await {
                            tracing::warn!(plugin_id = %pid_clone, error = %e, "Failed to persist disabled state after plugin error");
                        }
                    });
                });

                // 通知前端
                if let Err(e) = app_handle.emit("plugin:error", serde_json::json!({
                    "pluginId": pid,
                    "error": err,
                })) {
                    tracing::error!(plugin_id = %pid, error = %e, "Failed to emit plugin:error event");
                }

                tracing::info!(plugin_id = %pid, error = %err, "Plugin reported error, marked Error and disabled");
            });

        let host_ctx = Arc::new(WasmHostContext::new(
            self.plugin_db.clone(),
            self.storage.clone(),
            Some(self.app_handle.clone()),
            self.fs_auth.clone(),
            self.message_bus.clone(),
            status_reporter,
        ));

        // 组件路径无启动期签名表校验：契约由 WIT 编译期保证，
        // 插件侧 `abi.version()` 协商在 instantiate_component 内逐实例校验

        let _ = self.wasm_runtime.set(runtime);
        let _ = self.wasm_host_ctx.set(host_ctx);

        // 注入 dispatcher（PluginManagerDispatcher 实现 MessageDispatcher）
        let dispatcher: Arc<dyn crate::plugin::message_bus::MessageDispatcher> = Arc::new(PluginManagerDispatcher {
            plugins: self.plugins.clone(),
            wasm_plugins: self.wasm_plugins.clone(),
        });
        self.message_bus.set_dispatcher(dispatcher).await;

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
            current_wasm.insert(id, Arc::new(TokioMutex::new(wasm_plugin)));
        }
    }

    /// 存量兼容：迁移后首启，对「已启用且无审批记录」的非内置插件自动批准一次
    ///
    /// 记录当前请求权限 + 目录哈希钉扎，避免升级后用户插件全部失活。
    /// 一次性语义由持久化 migration_done 标记保证：HashMismatch 撤销批准后
    /// 不会重新武装（篡改文件 → 重启不得静默重新批准），必须人工审批。
    /// 哈希计算走 spawn_blocking（插件目录读取不进 async 事件循环）。
    async fn auto_approve_legacy(&self, plugin_ids: &[String]) {
        let done = match self.approvals.migration_done().await {
            Ok(done) => done,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to read approval migration flag, skipping legacy auto-approval"
                );
                return;
            }
        };
        if done {
            return;
        }

        for id in plugin_ids {
            if self.is_trusted_source(id).await {
                continue;
            }
            let enabled = match self
                .settings
                .get(&format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, id))
                .await
            {
                Ok(Some(value)) => value == "true",
                Ok(None) => false,
                Err(e) => {
                    tracing::warn!(plugin_id = %id, error = %e, "Failed to read enabled state");
                    continue;
                }
            };
            if !enabled {
                continue;
            }
            match self.approvals.get(id).await {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(plugin_id = %id, error = %e, "Failed to read approval");
                    continue;
                }
            }

            let (extension_path, version, requested) = {
                let plugins = self.plugins.read().await;
                match plugins.get(id) {
                    Some(p) => (
                        p.extension_path.clone(),
                        p.manifest.version.clone(),
                        p.manifest.permissions.clone(),
                    ),
                    None => continue,
                }
            };
            let hash = {
                let ext = extension_path.clone();
                tokio::task::spawn_blocking(move || {
                    compute_dir_hash(std::path::Path::new(&ext))
                })
                .await
                .map_err(|e| {
                    crate::AppError::Plugin(format!("Approval hash task failed: {}", e))
                })
                .and_then(|r| r)
            };
            match hash {
                Ok(hash) => {
                    if let Err(e) = self.approvals.approve(id, &requested, &hash, &version).await {
                        tracing::warn!(
                            plugin_id = %id,
                            error = %e,
                            "Failed to auto-approve legacy enabled plugin"
                        );
                    } else {
                        tracing::info!(
                            plugin_id = %id,
                            "Legacy enabled plugin auto-approved (migration one-time)"
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    plugin_id = %id,
                    error = %e,
                    "Failed to hash plugin dir for legacy auto-approval"
                ),
            }
        }

        // 无论是否有插件被批准，一次性标记置位
        if let Err(e) = self.approvals.set_migration_done().await {
            tracing::warn!(error = %e, "Failed to persist approval migration flag");
        }
    }

    /// 应用启动时：读取持久化启用状态，自动激活
    pub async fn load_all(&self, app_handle: &tauri::AppHandle) {
        let plugins = self.plugins.read().await;
        let plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        drop(plugins);

        // 存量兼容：迁移后首启自动批准（一次性，见 auto_approve_legacy）
        self.auto_approve_legacy(&plugin_ids).await;

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

    /// 判断插件来源是否属于内置信任域（无需审批）
    ///
    /// ApkAsset（APK assets 随包，含无标记历史产物）与 FrontendOnly
    /// （内置注册）为应用构建产物，直接全量授权；FileInstall /
    /// RemoteDownload（用户安装）必须经过人工审批。
    pub async fn is_trusted_source(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins
            .get(plugin_id)
            .map(|p| {
                p.source == PluginSource::ApkAsset || p.source == PluginSource::FrontendOnly
            })
            .unwrap_or(false)
    }

    /// 批准插件权限（人工审批入口）
    ///
    /// 记录用户同意的权限全集（manifest 请求）+ 目录内容哈希钉扎。
    /// 仅用户安装插件需要审批；内置插件（ApkAsset/FrontendOnly）返回错误。
    /// 批准成功后插件状态 NeedsApproval → Loaded，由前端继续启用激活。
    pub async fn approve(&self, plugin_id: &str) -> Result<()> {
        if self.is_trusted_source(plugin_id).await {
            return Err(crate::AppError::Plugin(format!(
                "Builtin plugin '{}' does not require approval",
                plugin_id
            )));
        }
        let (extension_path, version, requested) = {
            let plugins = self.plugins.read().await;
            let plugin = plugins.get(plugin_id).ok_or_else(|| {
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;
            (
                plugin.extension_path.clone(),
                plugin.manifest.version.clone(),
                plugin.manifest.permissions.clone(),
            )
        };

        let content_hash = {
            let ext = extension_path.clone();
            tokio::task::spawn_blocking(move || {
                compute_dir_hash(std::path::Path::new(&ext))
            })
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Approval hash task failed: {}", e)))?
            .map_err(|e| crate::AppError::Plugin(format!("Failed to hash plugin dir: {}", e)))?
        };
        self.approvals
            .approve(plugin_id, &requested, &content_hash, &version)
            .await?;

        // NeedsApproval → Loaded（前端随后可启用激活）
        let mut plugins = self.plugins.write().await;
        if let Some(p) = plugins.get_mut(plugin_id) {
            if p.state == PluginState::NeedsApproval {
                p.state = PluginState::Loaded;
            }
        }
        tracing::info!(
            plugin_id = %plugin_id,
            permissions = ?requested,
            "Plugin permissions approved and content pinned"
        );
        Ok(())
    }

    /// 激活插件
    ///
    /// 锁约定：执行 WASM 导出函数期间不持有 plugins / wasm_plugins map 守卫
    /// （仅持单插件实例锁），避免 WASM 回调 host function 重入取 map 锁死锁。
    pub async fn activate(&self, plugin_id: &str, _app_handle: &tauri::AppHandle) -> Result<()> {
        // 0. 审批门禁（防冒名顶替获取权限）
        //
        // 内置插件（ApkAsset/FrontendOnly）属于应用构建信任域，直接放行；
        // 用户安装的插件必须已获人工批准且内容哈希未变，否则拒绝激活：
        // - 无批准 → NeedsApproval（权限清单未经用户确认，不得生效）
        // - 批准后文件被替换（哈希不匹配）→ 撤销批准 + NeedsApproval，
        //   防止「批准 A 插件后换入 B 插件代码」的在位冒名攻击
        let gate = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .map(|p| (p.source.clone(), p.extension_path.clone()))
        };
        let Some((source, extension_path)) = gate else {
            return Err(crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)));
        };
        if source != PluginSource::ApkAsset && source != PluginSource::FrontendOnly {
            let approval = self.approvals.get(plugin_id).await?;
            // 目录哈希不进 async 事件循环（Android 主线程阻塞风险）
            let (status, _current_hash) = {
                let approval_for_hash = approval.clone();
                let ext_for_hash = extension_path.clone();
                tokio::task::spawn_blocking(move || {
                    verify_approval(
                        approval_for_hash.as_ref(),
                        std::path::Path::new(&ext_for_hash),
                    )
                })
                .await
                .map_err(|e| {
                    crate::AppError::Plugin(format!("Approval verify task failed: {}", e))
                })?
                .map_err(|e| {
                    crate::AppError::Plugin(format!("Failed to verify plugin approval: {}", e))
                })?
            };
            match status {
                ApprovalStatus::Approved => {
                    // 生效权限 = 用户批准 ∩ manifest 请求（storage 恒授予），
                    // 收紧 LoadedPlugin.granted_permissions（宿主侧权限裁决点）。
                    // 注：WASM 实例内嵌权限集为实例化时的全量（上限），
                    // 宿主侧检查（manager.has_permission 等）以本集合为准。
                    let requested = {
                        let plugins = self.plugins.read().await;
                        plugins
                            .get(plugin_id)
                            .map(|p| p.manifest.permissions.clone())
                            .unwrap_or_default()
                    };
                    let effective = effective_permissions(&requested, approval.as_ref(), false);
                    let mut plugins = self.plugins.write().await;
                    if let Some(p) = plugins.get_mut(plugin_id) {
                        p.granted_permissions = effective;
                    }
                }
                _ => {
                    // Pending / HashMismatch 一律置 NeedsApproval 并拒绝激活
                    let mut plugins = self.plugins.write().await;
                    if let Some(p) = plugins.get_mut(plugin_id) {
                        p.state = PluginState::NeedsApproval;
                    }
                    if status == ApprovalStatus::HashMismatch {
                        tracing::warn!(
                            plugin_id = %plugin_id,
                            "Plugin content changed since approval, revoking approval"
                        );
                        // 撤销失败不阻断本次拒绝（插件仍无法激活），但必须留痕
                        if let Err(e) = self.approvals.revoke(plugin_id).await {
                            tracing::error!(
                                plugin_id = %plugin_id,
                                error = %e,
                                "Failed to revoke approval after content mismatch"
                            );
                        }
                    }
                    return Err(crate::AppError::Plugin(format!(
                        "Plugin '{}' requires user approval before activation (or its files changed since approval)",
                        plugin_id
                    )));
                }
            }
        }

        // 1. 检查状态与插件类型（短锁）
        let plugin_type = {
            let mut plugins = self.plugins.write().await;
            let plugin = plugins
                .get_mut(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

            if plugin.state == PluginState::Activated {
                return Ok(());
            }
            plugin.manifest.plugin_type.clone()
        };

        if plugin_type != PluginType::Wasm {
            // TS-only 插件：仅标记状态
            let mut plugins = self.plugins.write().await;
            if let Some(plugin) = plugins.get_mut(plugin_id) {
                plugin.state = PluginState::Activated;
            }
            tracing::info!(plugin_id = %plugin_id, "Plugin activated (ts-only)");
            return Ok(());
        }

        // 2. WASM 插件：取实例句柄（短锁），执行 activate 导出（不持 map 守卫）
        let wasm_plugin = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        };

        let Some(wasm_plugin) = wasm_plugin else {
            // WASM 实例不存在，仅标记前端激活
            let mut plugins = self.plugins.write().await;
            if let Some(plugin) = plugins.get_mut(plugin_id) {
                plugin.state = PluginState::Activated;
            }
            tracing::info!(plugin_id = %plugin_id, "Plugin activated (frontend only, no WASM instance)");
            return Ok(());
        };

        let result = {
            let mut loaded = wasm_plugin.lock().await;
            loaded.activate()
        };

        // 3. 根据执行结果更新状态（短锁）
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        match result {
            Ok(0) => {
                plugin.state = PluginState::Activated;
                tracing::info!(plugin_id = %plugin_id, "WASM plugin activated");
                Ok(())
            }
            Ok(code) => {
                let error = format!("WASM activate() returned error code: {}", code);
                plugin.state = PluginState::Error { error: error.clone() };
                Err(crate::AppError::Plugin(error))
            }
            Err(e) => {
                plugin.state = PluginState::Error { error: e.to_string() };
                Err(e)
            }
        }
    }

    /// 停用插件
    ///
    /// 锁约定同 activate：执行 WASM deactivate 导出期间不持 map 守卫
    pub async fn deactivate(&self, plugin_id: &str) -> Result<()> {
        // 1. 检查状态与插件类型（短锁）
        let plugin_type = {
            let mut plugins = self.plugins.write().await;
            let plugin = plugins
                .get_mut(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

            if plugin.state != PluginState::Activated {
                return Ok(());
            }
            plugin.manifest.plugin_type.clone()
        };

        // 2. WASM 插件：取实例句柄（短锁），执行 deactivate 导出（不持 map 守卫）
        if plugin_type == PluginType::Wasm {
            let wasm_plugin = {
                let wasm_plugins = self.wasm_plugins.read().await;
                wasm_plugins.get(plugin_id).cloned()
            };
            if let Some(wasm_plugin) = wasm_plugin {
                let result = {
                    let mut loaded = wasm_plugin.lock().await;
                    loaded.deactivate()
                };
                if let Err(e) = result {
                    tracing::warn!(plugin_id = %plugin_id, error = %e, "WASM plugin deactivate failed");
                }
            }
        }

        // 3. 清理消息总线订阅（async 上下文直接 await，禁止 block_on）
        self.message_bus.remove_all_subscriptions(plugin_id).await;

        // 3.5 摘除文件服务挂载（规格：“停用插件 = 服务消失”；卸载同经 deactivate 触发）
        // 末个挂载摘除时停服务 + Withdraw，否则重新公告
        {
            let fs = crate::state::get_file_service();
            fs.registry.unmount_plugin(plugin_id).await;
            fs.after_unmount().await;
        }

        // 4. 更新状态（短锁）
        let mut plugins = self.plugins.write().await;
        if let Some(plugin) = plugins.get_mut(plugin_id) {
            plugin.state = PluginState::Deactivated;
        }
        tracing::info!(plugin_id = %plugin_id, "Plugin deactivated");
        Ok(())
    }

    /// 调用 WASM 插件命令
    ///
    /// 取实例句柄后 drop map 守卫，命令执行期间仅持单插件实例锁
    pub async fn invoke_command(
        &self,
        plugin_id: &str,
        command_name: &str,
        args_json: &str,
    ) -> Result<String> {
        let wasm_plugin = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        };
        let Some(wasm_plugin) = wasm_plugin else {
            return Err(crate::AppError::Plugin(format!(
                "WASM plugin not found: {}",
                plugin_id
            )));
        };

        let mut loaded = wasm_plugin.lock().await;
        loaded.invoke_command(command_name, args_json)
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

    /// 插件是否处于 Activated 状态（Tauri command 身份校验用）
    pub async fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins
            .get(plugin_id)
            .map(|p| p.state == PluginState::Activated)
            .unwrap_or(false)
    }

    /// 插件是否持有指定权限（加载时从 manifest 解析并过滤为合法权限集）
    pub async fn has_permission(&self, plugin_id: &str, permission: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins
            .get(plugin_id)
            .map(|p| p.granted_permissions.contains(permission))
            .unwrap_or(false)
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

    /// 插件显式上报启动成功
    ///
    /// Error → Activated 自愈（插件修复配置后重新上报）；
    /// Loaded → Activated（前端未走标准 activate 流程时兜底）。
    /// 已激活状态保持不动。
    pub async fn report_ready(&self, plugin_id: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        if plugin.state != PluginState::Activated {
            plugin.state = PluginState::Activated;
            tracing::info!(plugin_id = %plugin_id, "Plugin reported ready, state set to Activated");
        }
        Ok(())
    }

    /// 卸载插件（仅用户安装的插件；内置插件拒绝）
    ///
    /// 停用 → 移除运行时实例 → 清理启用偏好与插件存储 → 删除插件目录
    pub async fn uninstall(&self, plugin_id: &str) -> Result<()> {
        {
            let plugins = self.plugins.read().await;
            let plugin = plugins.get(plugin_id).ok_or_else(|| {
                crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id))
            })?;
            if plugin.source == PluginSource::ApkAsset {
                return Err(crate::AppError::Plugin(format!(
                    "Builtin plugin cannot be uninstalled: {}",
                    plugin_id
                )));
            }
        }

        // 停用（若激活）并清理消息总线订阅
        self.deactivate(plugin_id).await?;

        // 移除 WASM 实例与插件记录
        self.wasm_plugins.write().await.remove(plugin_id);
        self.plugins.write().await.remove(plugin_id);

        // 清理启用偏好、审批记录与插件存储
        let enabled_key = format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, plugin_id);
        if let Err(e) = self.settings.remove(&enabled_key).await {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "Failed to remove enabled setting on uninstall");
        }
        if let Err(e) = self.approvals.revoke(plugin_id).await {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "Failed to revoke approval on uninstall");
        }
        if let Err(e) = self.storage().clear_plugin(plugin_id).await {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "Failed to clear plugin storage on uninstall");
        }

        // 删除插件目录
        let plugin_dir = self.plugins_dir.join(plugin_id);
        if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir).map_err(|e| {
                crate::AppError::Plugin(format!("Failed to remove plugin dir: {}", e))
            })?;
        }

        tracing::info!(plugin_id = %plugin_id, "Plugin uninstalled");
        Ok(())
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

    /// 调用 WASM 插件的上传策略钩子（`on_upload_request` 导出）
    ///
    /// 返回插件写入的决定 JSON；插件未加载/导出缺失/调用失败返回 None
    /// （调用方 registry 据此 fail-closed 拒绝上传）。
    /// 锁约定同 activate：执行导出期间不持 wasm_plugins map 守卫
    pub async fn call_upload_hook(&self, plugin_id: &str, meta_json: &str) -> Option<String> {
        let wasm_plugin = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        }?;
        let mut loaded = wasm_plugin.lock().await;
        match loaded.call_upload_hook(meta_json) {
            Ok(json) => Some(json),
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "upload hook export call failed"
                );
                None
            }
        }
    }

    /// 调用 WASM 插件的批量传输请求钩子（`on_transfer_request` 导出，v2）
    ///
    /// 返回插件写入的决定 JSON；插件未加载/导出缺失/调用失败返回 None
    /// （调用方 registry 据此 fail-closed 拒绝批请求）。
    /// 锁约定同 call_upload_hook：执行导出期间不持 wasm_plugins map 守卫
    pub async fn call_transfer_hook(&self, plugin_id: &str, meta_json: &str) -> Option<String> {
        let wasm_plugin = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        }?;
        let mut loaded = wasm_plugin.lock().await;
        match loaded.call_transfer_request(meta_json) {
            Ok(json) => Some(json),
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "transfer hook export call failed"
                );
                None
            }
        }
    }

    /// 分发生命周期事件到所有已激活插件
    ///
    /// 1. 快照目标插件（短锁）→ drop map 守卫 → 逐个锁单插件实例调用导出函数
    /// 2. 通过 app_handle.emit() 发射 Tauri 事件给前端 TS 插件
    pub async fn dispatch_lifecycle_event(&self, event: PluginLifecycleEvent) {
        let event_name = event.name();

        // 快照：声明了该事件的已激活 WASM 插件 id + 实例句柄（短锁）
        let targets: Vec<(String, Option<Arc<TokioMutex<LoadedComponentPlugin>>>)> = {
            let ids: Vec<String> = {
                let plugins = self.plugins.read().await;
                plugins
                    .values()
                    .filter(|p| {
                        p.state == PluginState::Activated
                            && p.manifest.plugin_type == PluginType::Wasm
                            && p.manifest.contributes.lifecycle.as_ref()
                                .map(|l| l.is_declared(event_name))
                                .unwrap_or(false)
                    })
                    .map(|p| p.manifest.id.clone())
                    .collect()
            };

            if ids.is_empty() {
                Vec::new()
            } else {
                let wasm_plugins = self.wasm_plugins.read().await;
                ids.into_iter()
                    .map(|id| {
                        let handle = wasm_plugins.get(&id).cloned();
                        (id, handle)
                    })
                    .collect()
            }
        };

        // WASM 插件回调（不持 map 守卫）
        for (id, wasm_plugin) in targets {
            let Some(wasm_plugin) = wasm_plugin else {
                continue;
            };
            let mut loaded = wasm_plugin.lock().await;
            if let Err(e) = loaded.call_lifecycle_event(&event) {
                tracing::warn!(
                    plugin_id = %id,
                    event = %event_name,
                    error = %e,
                    "WASM lifecycle callback failed"
                );
            }
        }

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
    wasm_plugins: Arc<RwLock<HashMap<String, Arc<TokioMutex<LoadedComponentPlugin>>>>>,
}

#[async_trait]
impl crate::plugin::message_bus::MessageDispatcher for PluginManagerDispatcher {
    /// 投递总线消息给 WASM 插件
    ///
    /// 由投递 worker 任务调用（async 上下文）：短读 map 取实例句柄 → drop map 守卫 →
    /// 持单插件锁执行 on_bus_message。投递串行进行，慢插件会推迟后续投递
    /// （换取全局顺序与无死锁）。
    async fn dispatch_to_wasm(&self, plugin_id: &str, msg: &bedcode_plugin_api_mobile::BusMessage) -> anyhow::Result<()> {
        let wasm_plugin = {
            let map = self.wasm_plugins.read().await;
            map.get(plugin_id).cloned()
        };
        let Some(wasm_plugin) = wasm_plugin else {
            tracing::warn!("PluginManagerDispatcher: WASM plugin '{}' not loaded, message dropped", plugin_id);
            return Ok(());
        };

        let mut loaded = wasm_plugin.lock().await;
        Ok(loaded.on_bus_message(msg)?)
    }

    async fn is_activated(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins
            .get(plugin_id)
            .map(|p| p.state == PluginState::Activated)
            .unwrap_or(false)
    }
}
