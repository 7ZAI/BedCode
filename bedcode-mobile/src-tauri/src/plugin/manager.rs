//! Mobile Plugin Manager
//!
//! 插件生命周期管理 — WASM 动态加载、激活、停用、状态持久化

use crate::plugin::approval::{
    compute_dir_hash, effective_permissions, verify_approval, ApprovalStatus, PluginApprovalStore,
};
use crate::plugin::loader::PluginLoader;
use crate::plugin::registry::builtin_manifests;
use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use crate::plugin::wasm_runtime::{LoadedComponentPlugin, WasmHostContext, WasmRuntime};
use crate::system::constants::plugin::PLUGIN_DATA_DIR;
use crate::system::constants::plugin::PLUGIN_ENABLED_KEY_PREFIX;
use crate::system::settings::SettingsManager;
use crate::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex as TokioMutex, RwLock};

/// 预授权路径提供者签名:返回插件「启用前需要授权的路径列表」。
///
/// 移动端镜像 desktop host.rs 同名抽象;由插件自身在 on_startup
/// 时通过 host function 注册,优先级高于默认 storage 读取。
pub type PreauthProvider = Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// file-transfer 插件 ID:启用前要求 shared_roots 非空,否则直接拒绝激活。
pub const FILE_TRANSFER_PLUGIN_ID: &str = "com.bedcode.file-transfer";

/// 预授权 storage key(file-transfer mount-local 时追加写入)。
pub const PREAUTH_PATHS_STORAGE_KEY: &str = "preauth_paths";

/// 预授权提供者注册表 — 跨 PluginManager 实例共享。
static PREAUTH_PROVIDERS: OnceLock<RwLock<HashMap<String, PreauthProvider>>> = OnceLock::new();

fn preauth_providers() -> &'static RwLock<HashMap<String, PreauthProvider>> {
    PREAUTH_PROVIDERS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 注册预授权路径提供者(供 plugin 内部 host function 调用,
/// 优先级高于默认 storage 读取)。同名 plugin_id 覆盖。
pub async fn register_preauth_provider(plugin_id: &str, provider: PreauthProvider) {
    let mut map = preauth_providers().write().await;
    map.insert(plugin_id.to_string(), provider);
}

/// 收集插件的预授权路径:注册的 provider 优先,否则返回空(由
/// PluginManager::preauthorize_plugin 从 storage 补足)。
async fn collect_preauth_paths(plugin_id: &str) -> Vec<String> {
    if let Some(provider) = preauth_providers().read().await.get(plugin_id).cloned() {
        return provider(plugin_id);
    }
    Vec::new()
}

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
    ///
    /// Option 化以允许 `#[cfg(test)]` 模块构造无头 PluginManager（不走
    /// `app_handle.path()` / `app_handle.emit()` 的路径）。生产调用方
    /// `lib.rs` 仍传 `Some(...)`；`None` 分支只走状态机与 WASM 生命周期，
    /// 不依赖任何 Tauri 能力（详见 `init_wasm_runtime` 与 `emit_frontend_event`
    /// 的 None 降级路径）
    app_handle: Option<Arc<tauri::AppHandle>>,
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
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> Self {
        let storage = Arc::new(PluginStorage::new(app_data_dir));
        let approvals = Arc::new(PluginApprovalStore::new(storage.clone()));
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
            let permissions: std::collections::HashSet<String> = manifest.permissions.iter().cloned().collect();
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
            .as_ref()
            .ok_or_else(|| crate::AppError::Plugin("app_handle missing for AOT cache dir".to_string()))?
            .path()
            .app_cache_dir()
            .ok()
            .map(|d| d.join("wasm-aot"));
        self.init_wasm_runtime_with(aot_cache_dir).await
    }

    /// 抽离的初始化内核：把 aot_cache_dir 作为入参注入，
    /// 允许 `#[cfg(test)]` 模块在 `app_handle = None` 场景下用 `None` 调用，
    /// 绕开 `app_handle.path().app_cache_dir()` 读
    async fn init_wasm_runtime_with(
        &self,
        aot_cache_dir: Option<PathBuf>,
    ) -> crate::Result<()> {
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
        let app_handle = self.app_handle.as_ref().cloned();
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(move |plugin_id, error| {
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
            if let Some(handle) = app_handle.as_ref() {
                if let Err(e) = handle.emit(
                    "plugin:error",
                    serde_json::json!({
                        "pluginId": pid,
                        "error": err,
                    }),
                ) {
                    tracing::error!(plugin_id = %pid, error = %e, "Failed to emit plugin:error event");
                }
            }

            tracing::info!(plugin_id = %pid, error = %err, "Plugin reported error, marked Error and disabled");
        });

        let host_ctx = Arc::new(WasmHostContext::new(
            self.plugin_db.clone(),
            self.storage.clone(),
            self.app_handle.as_ref().cloned(),
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

        let (plugins, wasm_plugins) = PluginLoader::load_all(&self.plugins_dir, wasm_runtime, wasm_host_ctx);

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
            let enabled = match self.settings.get(&format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, id)).await {
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
                tokio::task::spawn_blocking(move || compute_dir_hash(std::path::Path::new(&ext)))
                    .await
                    .map_err(|e| crate::AppError::Plugin(format!("Approval hash task failed: {}", e)))
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
            if let Ok(Some(value)) = self
                .settings
                .get(&format!("{}{}", PLUGIN_ENABLED_KEY_PREFIX, &id))
                .await
            {
                if value == "true" {
                    if let Err(e) = self.activate(&id).await {
                        tracing::warn!(plugin_id = %id, error = %e, "Failed to auto-activate plugin on startup");
                    }
                }
            }
        }

        // 通知所有插件应用启动完成
        self.dispatch_lifecycle_event(PluginLifecycleEvent::AppStartup).await;

        // 仅在生产路径使用 app_handle：显式忽略未用变量警告
        let _ = app_handle;
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
            .map(|p| p.source == PluginSource::ApkAsset || p.source == PluginSource::FrontendOnly)
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
            let plugin = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;
            (
                plugin.extension_path.clone(),
                plugin.manifest.version.clone(),
                plugin.manifest.permissions.clone(),
            )
        };

        let content_hash = {
            let ext = extension_path.clone();
            tokio::task::spawn_blocking(move || compute_dir_hash(std::path::Path::new(&ext)))
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

    /// 预授权(启用前置):收集插件需授权路径 → 调 `fs_auth::check_batch`
    /// 单次合并弹窗。失败直接返回 `AppError::Plugin`,**不**改 state。
    /// **必须在 `activate` 步骤0(审批门禁)之后、状态写之前调用,持有
    /// plugins 锁时禁止调用**(check_batch 会发事件、可能回调宿主)。
    ///
    /// 路径来源:已注册的 `PreauthProvider` 优先;否则从 `PluginStorage`
    /// `preauth_paths` 数组读(file-transfer mount-local 同步写入)。
    /// file-transfer 共享目录未配置 → 立即返回错误,提示去设置页配置。
    pub async fn preauthorize_plugin(&self, plugin_id: &str) -> Result<()> {
        // 1. 收集路径(注册 provider 优先,否则 storage 数组)
        let mut paths = collect_preauth_paths(plugin_id).await;
        if paths.is_empty() {
            if let Ok(Some(value)) = self.storage.get(plugin_id, PREAUTH_PATHS_STORAGE_KEY).await {
                if let Value::Array(arr) = value {
                    paths = arr
                        .into_iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }
        }

        // 2. file-transfer 共享目录未配置:直接拒绝,避免启用空功能插件
        if plugin_id == FILE_TRANSFER_PLUGIN_ID && paths.is_empty() {
            tracing::warn!(
                plugin_id = %plugin_id,
                "preauthorize: file-transfer requires shared_roots to be configured first"
            );
            return Err(crate::AppError::Plugin(
                "Please configure shared directories in plugin settings first".to_string(),
            ));
        }

        if paths.is_empty() {
            return Ok(());
        }

        // 3. 合并未授权路径为单次弹窗(check_batch 内部已实现事件 emit + 30s 超时)
        let allowed = self
            .fs_auth
            .check_batch(plugin_id, &paths, crate::plugin::fs_auth::FsOp::Read)
            .await;
        if !allowed {
            return Err(crate::AppError::Plugin(
                "Plugin enable denied: file access authorization rejected".to_string(),
            ));
        }
        Ok(())
    }

    /// 激活插件
    ///
    /// 锁约定：执行 WASM 导出函数期间不持有 plugins / wasm_plugins map 守卫
    /// （仅持单插件实例锁），避免 WASM 回调 host function 重入取 map 锁死锁。
    ///
    /// `_app_handle` 移除：原签名保留 `&tauri::AppHandle` 是为给 `app_handle: Some(...)`
    /// 场景做显式依赖；现状体内未使用且 Tauri 命令侧 `app.state()` 已可取
    /// `Arc<PluginManager>` 内的 `app_handle` 字段，删去以允许 `#[cfg(test)]`
    /// 构造无头 PluginManager 走真 activate 路径
    pub async fn activate(&self, plugin_id: &str) -> Result<()> {
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
                    verify_approval(approval_for_hash.as_ref(), std::path::Path::new(&ext_for_hash))
                })
                .await
                .map_err(|e| crate::AppError::Plugin(format!("Approval verify task failed: {}", e)))?
                .map_err(|e| crate::AppError::Plugin(format!("Failed to verify plugin approval: {}", e)))?
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

        // 0.5 预授权 — 审批门禁之后、状态写之前(无锁);失败直接返回,
        // 前端 catch 后回退 toggle。loading 遮罩由前端 toggle 推迟到此
        // 调用之后才显示,确保授权弹窗与 loading 不会同时出现
        self.preauthorize_plugin(plugin_id).await?;

        // 1. 检查状态与插件类型（短锁）
        let plugin_type = {
            let mut plugins = self.plugins.write().await;
            let plugin = plugins
                .get_mut(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

            if matches!(plugin.state, PluginState::Activated | PluginState::Degraded { .. }) {
                // Activated / Degraded 均为终态：Degraded 可重试激活但重入无副作用，
                // 保持当前状态返回成功（幂等）
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

        // 2. WASM 插件：置中间态 Activating（列表可见激活进行中）
        {
            let mut plugins = self.plugins.write().await;
            if let Some(plugin) = plugins.get_mut(plugin_id) {
                plugin.state = PluginState::Activating;
            }
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

        let activation_result: std::result::Result<PluginState, String> = {
            let mut loaded = wasm_plugin.lock().await;
            // phase 1: WASM activate() 导出
            match loaded.activate() {
                Ok(0) => {
                    // phase 1b: 激活成功后立即调用 on_startup 导出，结果驱动终态
                    match loaded.on_startup() {
                        Ok(()) => Ok(PluginState::Activated),
                        Err(e) => {
                            tracing::error!(
                                plugin_id = %plugin_id,
                                error = %e,
                                "WASM plugin on_startup failed, state degraded"
                            );
                            Ok(PluginState::Degraded { error: e.to_string() })
                        }
                    }
                }
                Ok(code) => Err(format!("WASM activate() returned error code: {}", code)),
                Err(e) => Err(e.to_string()),
            }
        };

        // 3. 根据执行结果更新状态（短锁）
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

        match activation_result {
            Ok(state) => {
                plugin.state = state.clone();
                match state {
                    PluginState::Activated => {
                        tracing::info!(plugin_id = %plugin_id, "WASM plugin activated");
                    }
                    PluginState::Degraded { error } => {
                        tracing::error!(
                            plugin_id = %plugin_id,
                            error = %error,
                            "WASM plugin activated but degraded (startup init incomplete)"
                        );
                    }
                    _ => {}
                }
                Ok(())
            }
            Err(e) => {
                plugin.state = PluginState::Error { error: e.clone() };
                Err(crate::AppError::Plugin(e))
            }
        }
    }

    /// 停用插件
    ///
    /// 锁约定同 activate：执行 WASM deactivate 导出期间不持 map 守卫
    pub async fn deactivate(&self, plugin_id: &str) -> Result<()> {
        // ADR 0022 v2：插件停用即回收其全部 mDNS 浏览句柄（host-mdns 生命周期随属主）
        crate::plugin::wasm_runtime::host_impl::purge_browsers_for_plugin(plugin_id);

        // 1. 检查状态与插件类型（短锁）
        let plugin_type = {
            let mut plugins = self.plugins.write().await;
            let plugin = plugins
                .get_mut(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;

            if !matches!(plugin.state, PluginState::Activated | PluginState::Degraded { .. }) {
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
    pub async fn invoke_command(&self, plugin_id: &str, command_name: &str, args_json: &str) -> Result<String> {
        let wasm_plugin = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        };
        let Some(wasm_plugin) = wasm_plugin else {
            return Err(crate::AppError::Plugin(format!("WASM plugin not found: {}", plugin_id)));
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
        self.settings.set(key, enabled.to_string()).await?;
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
            let plugin = plugins
                .get(plugin_id)
                .ok_or_else(|| crate::AppError::Plugin(format!("Plugin not found: {}", plugin_id)))?;
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
            std::fs::remove_dir_all(&plugin_dir)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to remove plugin dir: {}", e)))?;
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

    /// 分发生命周期事件到所有已激活插件
    ///
    /// 1. 快照目标插件（短锁）→ drop map 守卫 → 逐个锁单插件实例调用导出函数
    /// 2. 通过 app_handle.emit() 发射 Tauri 事件给前端 TS 插件
    pub async fn dispatch_lifecycle_event(&self, event: PluginLifecycleEvent) {
        let event_name = event.name();

        // AppStartup 的 WASM on_startup 已前置到 activate()（phase 1b），
        // 此处不再经 dispatch 二次分发（防 on_startup 被执行两次）；仅保留前端事件。
        // 其余事件（AppShutdown/auth/disconnect/session/terminal）照常对
        // Activated + Degraded 插件投递 —— Degraded 实例是活的，运行期回调仍应收到。
        if !matches!(event, PluginLifecycleEvent::AppStartup) {
            // 快照：声明了该事件的已激活（或降级）WASM 插件 id + 实例句柄（短锁）
            let targets: Vec<(String, Option<Arc<TokioMutex<LoadedComponentPlugin>>>)> = {
                let ids: Vec<String> = {
                    let plugins = self.plugins.read().await;
                    plugins
                        .values()
                        .filter(|p| {
                            matches!(
                                p.state,
                                PluginState::Activated | PluginState::Degraded { .. }
                            ) && p.manifest.plugin_type == PluginType::Wasm
                                && p.manifest
                                    .contributes
                                    .lifecycle
                                    .as_ref()
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
        }

        // 前端 Tauri 事件发射
        self.emit_frontend_event(&event);
    }

    /// 发射前端 Tauri 生命周期事件
    fn emit_frontend_event(&self, event: &PluginLifecycleEvent) {
        let tauri_event = format!("plugin:lifecycle:{}", event.tauri_event_name());
        let payload = event.to_payload();
        if let Some(handle) = &self.app_handle {
            if let Err(e) = handle.emit(&tauri_event, payload) {
                tracing::error!(
                    event = %tauri_event,
                    error = %e,
                    "Failed to emit frontend lifecycle event"
                );
            }
        } else {
            // 无头上下文（测试场景）：跳过 emit，不报错
            tracing::debug!(
                event = %tauri_event,
                "app_handle not set, skipping frontend lifecycle emit"
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
    async fn dispatch_to_wasm(
        &self,
        plugin_id: &str,
        msg: &bedcode_plugin_api_mobile::BusMessage,
    ) -> anyhow::Result<()> {
        let wasm_plugin = {
            let map = self.wasm_plugins.read().await;
            map.get(plugin_id).cloned()
        };
        let Some(wasm_plugin) = wasm_plugin else {
            tracing::warn!(
                "PluginManagerDispatcher: WASM plugin '{}' not loaded, message dropped",
                plugin_id
            );
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

// ==================== 状态机单元测试（spec §3.3 / issue 01 验证清单） ====================
//
// 设计依据：
// - 与桌面端 host.rs:test_activate_degraded_on_startup_failure_then_retry_recovers
//   同模式（真组件 + storage 开关）
// - 移动端 plugin-component-test 已有 `on-startup-fail` feature（L107-109），
//   `wasm_runtime/component.rs:745 build_test_component(features)` 编译产物
// - 行为覆盖：activate 失败→Degraded / Degraded 可重试→Activated /
//   Degraded 停用→Deactivated / dispatch_lifecycle 跳过 AppStartup 二次调用
//
// 锁约束：manager 字段全 pub(crate)；mod tests 在同文件内访问合法

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::component::tests::{build_host_ctx, build_test_component};
    use crate::plugin::wasm_runtime::LoadedComponentPlugin;
    use std::collections::HashSet;
    use std::sync::Arc as StdArc;
    use tempfile::TempDir;
    use wasmtime::component::Component;

    const TEST_PID_DEGRADED: &str = "com.bedcode.test.mgr-degraded";
    const TEST_PID_RETRY: &str = "com.bedcode.test.mgr-retry";
    const TEST_PID_DEACTIVATE: &str = "com.bedcode.test.mgr-deactivate";
    const TEST_PID_DISPATCH: &str = "com.bedcode.test.mgr-dispatch";

    /// 构造最小可激活的 PluginManager
    ///
    /// - app_handle = None（绕开 `app_handle.path()` 读 AOT cache dir 路径）
    /// - 注入 WasmRuntime (no aot cache) + WasmHostContext（无 fs_auth app_handle 依赖）
    /// - 注入 MessageDispatcher 桥
    async fn setup_manager() -> (PluginManager, TempDir) {
        let tmp = TempDir::new().expect("tempdir");
        let app_data_dir = tmp.path().to_path_buf();
        let settings = StdArc::new(SettingsManager::new(&app_data_dir).expect("settings"));
        let plugin_db = StdArc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("in-memory sqlite"),
        ));

        let manager = PluginManager::new(
            &app_data_dir,
            settings,
            plugin_db,
            None, // app_handle = None：测试不走 emit / path
        );

        // init_wasm_runtime_with(None) 走无 AOT cache 路径；
        // 内部 status_reporter 是 no-op 闭包，app_handle 缺失由
        // `self.app_handle.as_ref().cloned()` 产生 None host_ctx，与
        // build_host_ctx(tmp) 同形态
        manager.init_wasm_runtime_with(None).await.expect("init wasm runtime");

        // init_wasm_runtime 内部不 set_dispatcher，需手动补
        let dispatcher: StdArc<dyn crate::plugin::message_bus::MessageDispatcher> =
            StdArc::new(PluginManagerDispatcher {
                plugins: manager.plugins.clone(),
                wasm_plugins: manager.wasm_plugins.clone(),
            });
        manager.message_bus.set_dispatcher(dispatcher).await;

        (manager, tmp)
    }

    /// 在 manager.plugins map 内插入一条 LoadedPlugin 记录（state 由调用方定）
    async fn seed_plugin(
        manager: &PluginManager,
        pid: &str,
        state: PluginState,
    ) {
        let manifest = bedcode_plugin_api_mobile::types::PluginManifest {
            id: pid.to_string(),
            name: pid.to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            author: String::new(),
            main: String::new(),
            plugin_type: bedcode_plugin_api_mobile::types::PluginType::Wasm,
            permissions: vec![],
            contributes: bedcode_plugin_api_mobile::types::PluginContributes {
                lifecycle: Some(bedcode_plugin_api_mobile::types::LifecycleContribution {
                    on_auth_success: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            icon: None,
            wasm_hash: String::new(),
            rust_library: String::new(),
        };
        let mut plugins = manager.plugins.write().await;
        plugins.insert(
            pid.to_string(),
            LoadedPlugin {
                manifest,
                state,
                granted_permissions: HashSet::new(),
                source: PluginSource::ApkAsset, // 信任域放行审批门禁（manager.rs:514）
                extension_path: "/tmp/test".to_string(),
            },
        );
    }

    /// 用真组件实例化一个 LoadedComponentPlugin 并塞入 manager.wasm_plugins
    async fn attach_wasm(
        manager: &PluginManager,
        tmp: &TempDir,
        pid: &str,
        features: &[&str],
    ) {
        let host_ctx = build_host_ctx(tmp);
        let runtime = manager.wasm_runtime.get().expect("wasm runtime initialized");
        let component = Component::from_binary(runtime.engine(), &build_test_component(features))
            .expect("compile test component");
        let loaded: LoadedComponentPlugin = runtime
            .instantiate_component(&component, pid, host_ctx, HashSet::new())
            .expect("instantiate component");
        let mut wasm_plugins = manager.wasm_plugins.write().await;
        wasm_plugins.insert(
            pid.to_string(),
            StdArc::new(tokio::sync::Mutex::new(loaded)),
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_on_startup_failure_enters_degraded() {
        let (manager, tmp) = setup_manager().await;
        seed_plugin(&manager, TEST_PID_DEGRADED, PluginState::Loaded).await;
        attach_wasm(&manager, &tmp, TEST_PID_DEGRADED, &["on-startup-fail"]).await;

        // phase 1: activate() 自身 Ok → 继续 phase 1b on_startup
        // phase 1b: on_startup 返回 Err("startup init failed (test)")
        // → 终态 Degraded { error 含 "startup init failed" }
        let result = manager.activate(TEST_PID_DEGRADED).await;
        assert!(result.is_ok(), "activate 调用本身应成功（phase 1a Ok），实际: {:?}", result);

        let info = manager.get_info(TEST_PID_DEGRADED).await.expect("plugin info");
        match info.state {
            PluginState::Degraded { error } => {
                assert!(
                    error.contains("startup init failed"),
                    "Degraded 错误信息应含 guest 自报原因，实际: {}",
                    error
                );
            }
            other => panic!("期望 Degraded 终态，实际: {:?}", other),
        }

        // is_activated() 严格 Activated 语义保持（manager.rs:761-767）
        assert!(
            !manager.is_activated(TEST_PID_DEGRADED).await,
            "Degraded 不应通过 is_activated() 严格门禁"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_degraded_retry_to_activated() {
        let (manager, tmp) = setup_manager().await;
        seed_plugin(&manager, TEST_PID_RETRY, PluginState::Loaded).await;
        attach_wasm(&manager, &tmp, TEST_PID_RETRY, &["on-startup-fail"]).await;

        // 首次激活：on_startup 失败 → Degraded
        manager.activate(TEST_PID_RETRY).await.expect("first activate");
        let info = manager.get_info(TEST_PID_RETRY).await.expect("info");
        assert!(matches!(info.state, PluginState::Degraded { .. }), "首次激活应入 Degraded");

        // manager.rs:586 守卫：Activated | Degraded 重入早返回 Ok，**不重试 on_startup**
        // — spec §3.3 「Degraded 可重试激活但重入无副作用」明确走幂等路径
        // 要重试必须先 deactivate 干净回落，再换 Ok 形态组件 activate
        manager.deactivate(TEST_PID_RETRY).await.expect("deactivate to reset");
        // 替换 wasm 实例为 Ok 形态（不破坏 Deactivated 终态约束，仅换实例）
        attach_wasm(&manager, &tmp, TEST_PID_RETRY, &[]).await;

        // 把状态显式置 Loaded（deactivate 已落 Deactivated，activate 仍会从
        // Deactivated 走完整 phase 1b——manager.rs:586 仅守卫 Activated|Degraded）
        {
            let mut plugins = manager.plugins.write().await;
            if let Some(p) = plugins.get_mut(TEST_PID_RETRY) {
                p.state = PluginState::Loaded;
            }
        }
        manager.activate(TEST_PID_RETRY).await.expect("retry activate");
        let info = manager.get_info(TEST_PID_RETRY).await.expect("info after retry");
        assert_eq!(
            info.state,
            PluginState::Activated,
            "替换 Ok 形态组件后重试激活应入 Activated"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_degraded_falls_back_to_deactivated() {
        let (manager, tmp) = setup_manager().await;
        seed_plugin(&manager, TEST_PID_DEACTIVATE, PluginState::Loaded).await;
        attach_wasm(&manager, &tmp, TEST_PID_DEACTIVATE, &["on-startup-fail"]).await;

        manager.activate(TEST_PID_DEACTIVATE).await.expect("activate to degraded");
        let info = manager.get_info(TEST_PID_DEACTIVATE).await.expect("info");
        assert!(matches!(info.state, PluginState::Degraded { .. }), "应先入 Degraded");

        // manager.rs:696 守卫：Activated | Degraded 都允许 deactivate
        manager.deactivate(TEST_PID_DEACTIVATE).await.expect("deactivate from degraded");
        let info = manager.get_info(TEST_PID_DEACTIVATE).await.expect("info after deactivate");
        assert_eq!(
            info.state,
            PluginState::Deactivated,
            "Degraded 停用应干净回落 Deactivated"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dispatch_lifecycle_app_startup_skips_wasm_side() {
        let (manager, tmp) = setup_manager().await;
        seed_plugin(&manager, TEST_PID_DISPATCH, PluginState::Loaded).await;
        attach_wasm(&manager, &tmp, TEST_PID_DISPATCH, &[]).await;

        manager.activate(TEST_PID_DISPATCH).await.expect("activate");
        let info = manager.get_info(TEST_PID_DISPATCH).await.expect("info");
        assert_eq!(info.state, PluginState::Activated, "前置：先入 Activated");

        // AppStartup 事件不调 WASM on_startup（manager.rs:911-915 守卫）；
        // 行为回归断言 = 不 panic + state 不变
        manager
            .dispatch_lifecycle_event(PluginLifecycleEvent::AppStartup)
            .await;
        let info = manager.get_info(TEST_PID_DISPATCH).await.expect("info after app startup");
        assert_eq!(
            info.state,
            PluginState::Activated,
            "AppStartup 不应改变已激活插件状态"
        );

        // AuthSuccess 事件对声明 onAuthSuccess 的插件正常分发（不 panic 即代表
        // dispatch 路径在 Degraded 守卫（manager.rs:922-925 扩为 Activated|Degraded）
        // 之外仍正确路由到 WASM call_lifecycle_event）
        manager
            .dispatch_lifecycle_event(PluginLifecycleEvent::AuthSuccess)
            .await;
        let info = manager.get_info(TEST_PID_DISPATCH).await.expect("info after auth success");
        assert_eq!(
            info.state,
            PluginState::Activated,
            "AuthSuccess 不应改变已激活插件状态（仅回调）"
        );
    }
}