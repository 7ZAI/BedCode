//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage、wasm_runtime 五个子系统
//! 支持静态注册（Rust 插件 via inventory）、文件扫描（TS-only 插件）和 WASM 模块（Rust+TS 插件）

use crate::db::Database;
use crate::plugin::manager::loader::PluginLoader;
use crate::plugin::manager::registry::PluginRegistry;
use crate::plugin::manager::storage::PluginStorage;
use crate::plugin::manager::types::{DesktopPluginInfo, LoadedPlugin, PluginSource};
use crate::plugin::manager::wasm_runtime::{LoadedWasmPlugin, WasmHostContext, WasmRuntime};
use crate::plugin::permission::PermissionManager;
use crate::session::{SessionConfigManager, SessionManager};
use crate::system::constants::event;
use crate::system::constants::plugin::{PLUGIN_CALLBACK_TIMEOUT_SECS, PLUGIN_MANIFEST_FILE};
use bedcode_plugin_api::{PluginKind, PluginState};
use chrono::Utc;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

/// file-transfer 插件 ID(启用先行门禁测试用;移动端无独立镜像常量)。
pub const FILE_TRANSFER_PLUGIN_ID: &str = "com.bedcode.file-transfer";

/// WASM 插件 trap 自动重载最小间隔（秒）
///
/// wasmtime 同步引擎下任何一次 trap 都会污染整个 Store（`set_trapped`），
/// 之后该实例所有调用持续报 `CannotEnterComponent`，唯一恢复途径是整体重载。
/// 自动重载用最小间隔限频，防「重载后立刻再 trap」时无限重载风暴
/// （持久性 bug 时最多每间隔重试一次，期间插件保持 Error 态）。
const PLUGIN_AUTO_RELOAD_MIN_INTERVAL_SECS: u64 = 30;

/// 插件运行时异常前端提示最小间隔（秒）
///
/// 统一异常通道（`PLUGIN_RUNTIME_ERROR`）按插件合并提示：重载循环等
/// 连发异常场景下只弹一次 toast，日志始终记录全量错误。
const PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS: u64 = 15;

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
    message_bus: Arc<crate::plugin::bus::MessageBus>,
    /// 插件定时器（plugin_id → tokio 任务句柄，v6 ADR 0003）
    ///
    /// 重复注册替换旧句柄；插件停用/应用关闭时中止。
    /// 用 std Mutex：仅短时间的 map 操作，不跨 await 持锁
    plugin_timers: Arc<std::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// WASM 插件 trap 自动重载限频表（plugin_id → 最近一次自动重载时刻）
    ///
    /// std Mutex：仅短时 map 操作，不跨 await 持锁
    wasm_reload_throttle: Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    /// 插件运行时异常前端提示限频表（plugin_id → 最近一次 toast 时刻）
    ///
    /// 见 [`PLUGIN_RUNTIME_ERROR_NOTIFY_INTERVAL_SECS`]；std Mutex：短时 map 操作
    runtime_error_notify_throttle: Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    /// 应用关闭标志：deactivate_all（应用退出）置位
    ///
    /// 插件 deactivate 内的卸载动作（如 CLI 安装清理）据此跳过：
    /// 应用正常退出 ≠ 用户停用插件，随包 CLI 应保留（下次启动 activate 幂等重装）
    shutting_down: Arc<std::sync::atomic::AtomicBool>,
    /// 用户插件目录（zip 安装目标，dev 合入）：卸载与 zip 安装均以此目录为落点
    user_plugins_dir: PathBuf,
    /// 前端插件通道身份（审计票 06 / P0-5）：loader 会话密钥 + 插件令牌 →
    /// 身份解析，堵住「前端 `plugin_*` 命令自报 plugin_id」这条通道
    frontend_channel: Arc<crate::plugin::security::frontend_channel::FrontendChannelRegistry>,
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
        // 用户插件目录（app_data_dir/plugins，zip 安装目标，可卸载；dev 合入）
        user_plugins_dir: &Path,
        session_manager: Arc<SessionManager>,
        config_manager: Arc<SessionConfigManager>,
        // Option 化：无头/测试上下文无 AppHandle（与 WasmRuntime/WasmHostContext 同策略），
        // 依赖前端事件的宿主能力在调用处降级
        app_handle: Option<Arc<tauri::AppHandle>>,
    ) -> Self {
        tracing::info!("[PluginHost] Initializing with plugins_dir: {:?}", plugins_dir);

        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let storage = Arc::new(PluginStorage::new(db.clone()));

        // 构建 WASM 运行时和宿主上下文
        let wasm_runtime =
            Arc::new(WasmRuntime::new(storage.clone(), app_handle.clone()).expect("Failed to initialize WASM runtime"));

        // 创建消息总线（dispatcher 延迟注入，在 init_message_bus 中设置）
        let message_bus = Arc::new(crate::plugin::bus::MessageBus::new());

        let wasm_host_ctx = Arc::new(WasmHostContext::new(
            db.clone(),
            Arc::new(Mutex::new(HashMap::new())),
            storage.clone(),
            session_manager,
            config_manager,
            app_handle,
            permission.clone(),
            wasm_runtime.fs_auth().clone(),
            message_bus.clone(),
        ));

        // core-security × core-monitor：决策计数埋点两阶段注入
        // （monitor 生于 WasmRuntime，晚于宿主上下文构建）
        wasm_host_ctx.security().set_monitor(wasm_runtime.monitor());

        // 1. 收集静态注册的 Rust 插件
        let static_plugins: Vec<&'static bedcode_plugin_api::BedcodePluginEntry> =
            inventory::iter::<bedcode_plugin_api::BedcodePluginEntry>
                .into_iter()
                .collect();
        tracing::info!(
            "[PluginHost] Found {} static plugin(s) from inventory",
            static_plugins.len()
        );

        // 2. 扫描文件系统中的 TS-only 和 WASM 插件
        let file_plugins = PluginLoader::load_all(plugins_dir, &permission, None);
        tracing::info!("[PluginHost] Found {} file-based plugin(s)", file_plugins.len());

        // 2b. 扫描用户插件目录（zip 安装，可卸载）：与内置目录共用同一套加载与
        // WASM 实例化逻辑，先内置后用户（重复 id 由 load_all 的 seen_ids 拒绝）
        let user_plugins = PluginLoader::load_all(user_plugins_dir, &permission, Some(PluginSource::UserInstalled));
        tracing::info!("[PluginHost] Found {} user-installed plugin(s)", user_plugins.len());

        // 3. 合并所有插件
        let mut all_plugins: HashMap<String, LoadedPlugin> = HashMap::new();

        // 添加静态注册的 Rust 插件
        for entry in static_plugins {
            let manifest = (entry.create_manifest)();
            let plugin_id = manifest.id.clone();

            let granted = permission.grant_permissions(&plugin_id, &manifest.permissions);

            // 内置常驻语义：随二进制分发、无独立启停，注册即激活。
            // 直接置 Activated 使 notify_startup 的 on_startup 回调与
            // invoke_rust_command 的身份门禁对其真实生效（此前停在 Loaded 态、
            // 永不激活，与 "Static plugin loaded" 日志自相矛盾）
            let loaded = LoadedPlugin {
                manifest,
                state: PluginState::Activated,
                granted_permissions: granted,
                extension_path: String::new(),
                activated_at: Some(Utc::now()),
                source: PluginSource::StaticRegistry,
            };

            tracing::info!(
                "Static plugin activated (builtin): {} v{}",
                loaded.manifest.id,
                loaded.manifest.version
            );
            all_plugins.insert(plugin_id, loaded);
        }

        // 添加文件扫描的插件（包含 TS-only 和 WASM 来源判定）
        let mut wasm_plugins_map: HashMap<String, Arc<Mutex<LoadedWasmPlugin>>> = HashMap::new();

        for (id, loaded) in file_plugins.into_iter().chain(user_plugins) {
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
                            state: PluginState::Error(format!("WASM module not found: {}", wasm_path.display())),
                            ..loaded
                        },
                    );
                    continue;
                }

                // 阶段 A 共存入口：按产物格式自动选择 core module / component
                match wasm_runtime.load_plugin_from_file(
                    &wasm_path,
                    &id,
                    wasm_host_ctx.clone(),
                    &loaded.manifest.wasi_preopen_dirs,
                    loaded.manifest.resource_overrides.as_ref(),
                ) {
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
            plugin_timers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            wasm_reload_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_error_notify_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            user_plugins_dir: user_plugins_dir.to_path_buf(),
            frontend_channel: Arc::new(
                crate::plugin::security::frontend_channel::FrontendChannelRegistry::new(),
            ),
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

        // 4. 系统组件优先激活（core-plugin-manager）：内置、默认启用、先于
        // 应用插件——其能力注册表装配必须先于应用插件激活时的依赖检查
        host.activate_system_components().await;

        // 5. 根据持久化状态自动激活之前已激活的插件
        tracing::info!("[PluginHost] Starting auto-activation from persisted state...");
        host.auto_activate_from_persisted_state().await;

        let count = host.plugins.read().await.len();
        let wasm_count = host.wasm_plugins.read().await.len();
        // 汇总日志按真实状态分计数：degraded/error 不再隐没在 activated 里
        let mut activated_count = 0usize;
        let mut degraded_count = 0usize;
        let mut error_count = 0usize;
        for p in host.plugins.read().await.values() {
            match &p.state {
                PluginState::Activated => activated_count += 1,
                PluginState::Degraded(_) => degraded_count += 1,
                PluginState::Error(_) => error_count += 1,
                _ => {}
            }
        }
        tracing::info!(
            "[PluginHost] Initialization complete: {} plugin(s) total, {} wasm, {} activated, {} degraded, {} error",
            count,
            wasm_count,
            activated_count,
            degraded_count,
            error_count
        );
        host
    }

    /// 将所有已加载插件的 manifest contributes 注册到 registry

    /// 注册 Rust 插件的 command handlers 到运行时注册表（inventory 静态注册）

    /// 注册 Rust 插件的 terminal handlers 到运行时注册表（inventory 静态注册）

    // ==================== Accessors ====================

    /// 获取 WASM 宿主上下文引用
    pub fn wasm_host_ctx(&self) -> &Arc<WasmHostContext> {
        &self.wasm_host_ctx
    }

    /// 调用指定插件实例的能力导出（票 12 C3：宿主 server 中间件取认证中心策略）
    ///
    /// 直接按插件 ID 直查实例并调用（不经能力注册表路由——`auth-policy` 仅探测
    /// 不路由，消费方是宿主中间件而非插件 import）。前置校验：实例已加载（未
    /// 加载/未激活 → 无实例）且实例化时探测到该能力导出；任一项缺失 → 外层
    /// Err（调用方降级）。
    ///
    /// 外层 Err = 实例缺失/能力缺失/传输错误（trap 等）；内层 `Results` 元组
    /// 含 WIT `result<T, string>` 本体（guest 自报错误），两层语义分离。
    pub async fn call_plugin_capability_export<Params, Results>(
        &self,
        plugin_id: &str,
        capability: &str,
        export_name: &str,
        params: Params,
    ) -> crate::Result<Results>
    where
        Params: wasmtime::component::ComponentNamedList + wasmtime::component::Lower + Send,
        Results: wasmtime::component::ComponentNamedList + wasmtime::component::Lift + Send + 'static,
    {
        let instance = {
            let wasm_plugins = self.wasm_plugins.read().await;
            wasm_plugins.get(plugin_id).cloned()
        };
        let Some(instance) = instance else {
            return Err(crate::AppError::Plugin(format!(
                "plugin '{}' not loaded (no wasm instance)",
                plugin_id
            )));
        };
        let mut guard = instance.lock().await;
        if !guard.exported_capabilities().iter().any(|c| c == capability) {
            return Err(crate::AppError::Plugin(format!(
                "plugin '{}' does not export capability '{}'",
                plugin_id, capability
            )));
        }
        guard.call_capability_export::<Params, Results>(export_name, params)
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

    /// 前端插件通道身份注册表（loader 会话密钥 / 插件令牌）
    pub fn frontend_channel(
        &self,
    ) -> &Arc<crate::plugin::security::frontend_channel::FrontendChannelRegistry> {
        &self.frontend_channel
    }

    /// 重置前端通道会话（新的一次页面加载）：旧 loader 密钥与全部插件令牌失效
    ///
    /// 由 Tauri `on_page_load` 钩子调用（dev 下页面刷新需能重新取得宿主面凭证），
    /// 也可在测试中显式调用以模拟前端重启。
    pub fn reset_frontend_loader_session(&self, reason: &str) -> usize {
        let revoked = self.frontend_channel.reset();
        tracing::info!(
            reason = %reason,
            revoked_tokens = revoked,
            "[PluginChannel] 前端通道会话已重置"
        );
        revoked
    }

    /// 获取 WASM 运行时引用
    pub fn wasm_runtime(&self) -> &Arc<WasmRuntime> {
        &self.wasm_runtime
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::plugin::bus::MessageBus> {
        &self.message_bus
    }

    /// 初始化消息总线 dispatcher（必须在 new() 之后调用）
    pub async fn init_message_bus(&self) {
        let dispatcher: Arc<dyn crate::plugin::bus::MessageDispatcher> = Arc::new(self.clone());
        self.message_bus.set_dispatcher(dispatcher).await;
        // v11：注入 core-monitor 注册表（订阅者队列满丢弃 / 格式不匹配拒绝计数）。
        // 必须在首次订阅（插件激活）之前完成——激活流程在 PluginHost::new 之后
        self.message_bus.set_monitor(self.wasm_runtime.monitor()).await;
        tracing::info!("[PluginHost] MessageBus dispatcher initialized");
    }

    // ==================== Lifecycle ====================

    /// 获取所有已加载插件的信息列表
    pub async fn list_plugins(&self) -> Vec<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        let list: Vec<DesktopPluginInfo> = plugins.values().map(DesktopPluginInfo::from).collect();
        tracing::debug!("[PluginHost] list_plugins() returning {} plugin(s)", list.len());
        for info in &list {
            tracing::debug!(
                "[PluginHost]   - {} (state={:?}, type={:?})",
                info.id,
                info.state,
                info.plugin_type
            );
        }
        list
    }

    /// 获取单个插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> Option<DesktopPluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(DesktopPluginInfo::from)
    }
}

// ==================== 子模块（自本文件拆分） ====================
// 职责面拆分（P2）：激活/停用、启动通知、注册、安装卸载、预授权、wasm 实例、错误上报
mod activation;
mod app_cli;
mod boot;
mod commands;
mod errors;
mod install;
mod listeners;
mod preauth;
mod register;
mod services;
mod wasm;
// 保持原导出路径（crate::plugin::manager::host::PluginLifecycleListener 等）
pub use listeners::{PluginInputListener, PluginLifecycleListener};
// preauth 域（P2 拆分后 re-export 保持 host:: 路径兼容）
pub use preauth::register_preauth_provider;
#[allow(unused_imports)] // 兼容 host:: 路径（测试经 super:: 引用；preauth.rs 内部自用）
pub(crate) use preauth::{collect_preauth_paths, preauth_providers, PreauthProvider, PREAUTH_PATHS_STORAGE_KEY};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::bus::{MessageBus, MessageDispatcher};
    use crate::plugin::manager::wasm_runtime::PluginServices;
    use crate::session::{SessionInputListener, SessionLifecycleEvent, SessionLifecycleListener};
    use crate::system::config::AppConfig;
    use bedcode_plugin_api::{
        PluginCommand, PluginContributes, PluginManifest, PluginType, RustPluginContext, TerminalHandler,
    };
    use serde_json::json;
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// 测试用插件 ID（非 WASM 插件）
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";
    /// 测试用组件形态 WASM 插件 ID（与 plugin-component-test 的 manifest 一致）
    const TEST_WASM_PLUGIN_ID: &str = "com.bedcode.component-test";

    /// 本模块测试的不可测面说明：
    ///
    /// - `PluginHost::new`：依赖 inventory 静态注册表与真实插件目录；app_handle 已
    ///   Option 化（None = 无头测试上下文），但集成测试在 crate 外无法访问私有字段,
    ///   仍需通过结构体字面量直接构造（tests 位于 host.rs 内部，可访问私有字段），
    ///   覆盖 new() 之后的全部宿主行为。
    /// - `notify_startup` / `notify_shutdown` / `PluginServices::mark_plugin_error`：
    ///   无头测试上下文降级为纯日志跳过前端 emit（try_global None 分支），
    ///   静态插件回调链路由下方合成 inventory 条目测试覆盖。
    /// - `dispatch_*_to_plugin` 的错误分支（WASM 实例缺失/调用失败）：仅有日志
    ///   副作用，无返回值可断言；成功路径由 `test_dispatch_lifecycle_and_input_to_wasm_plugin`
    ///   以「分发后 store 未被污染」间接验证。
    /// - `invoke_wasm_command` 的非法 JSON 返回分支：需要构造返回坏 JSON 的恶意
    ///   WASM 插件，超出测试组件能力范围。

    /// 构造无头测试宿主（app_handle = None）
    ///
    /// 结构体字面量构造 PluginHost：字段私有但 tests 模块与 host.rs 同属一个
    /// 模块树，可访问。所有子系统均用真实实现 + 内存 SQLite，仅 Tauri 相关
    /// 能力降级（与 wasm_runtime.rs 测试同一策略）。
    async fn setup_host() -> PluginHost {
        // AppConfig 全局初始化（与 wasm_runtime 测试同策略；重复 init 幂等）
        static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
        CONFIG_INIT.call_once(|| {
            let mut config = AppConfig::default();
            config.network.port = 8765;
            AppConfig::init(config);
        });

        let db = Arc::new(Mutex::new(Database::new(&PathBuf::from(":memory:")).unwrap()));
        db.lock().await.init_schema().unwrap();
        let storage = Arc::new(PluginStorage::new(db.clone()));
        let session_manager = Arc::new(SessionManager::default());
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
            Database::new(&PathBuf::from(":memory:")).unwrap(),
        ))));

        let permission = Arc::new(PermissionManager::new());
        let registry = Arc::new(PluginRegistry::new());
        let message_bus = Arc::new(MessageBus::new());

        let wasm_runtime = Arc::new(WasmRuntime::new(storage.clone(), None).unwrap());

        let wasm_host_ctx = Arc::new(WasmHostContext::new(
            db,
            Arc::new(Mutex::new(HashMap::new())),
            storage.clone(),
            session_manager,
            config_manager,
            None,
            permission.clone(),
            wasm_runtime.fs_auth().clone(),
            message_bus.clone(),
        ));

        wasm_host_ctx.security().set_monitor(wasm_runtime.monitor());

        PluginHost {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            registry,
            permission,
            storage,
            rust_command_handlers: Arc::new(RwLock::new(HashMap::new())),
            rust_terminal_handlers: Arc::new(RwLock::new(Vec::new())),
            wasm_runtime,
            wasm_plugins: Arc::new(RwLock::new(HashMap::new())),
            wasm_host_ctx,
            message_bus,
            plugin_timers: Arc::new(std::sync::Mutex::new(HashMap::new())),
            wasm_reload_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            runtime_error_notify_throttle: Arc::new(std::sync::Mutex::new(HashMap::new())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            user_plugins_dir: std::env::temp_dir().join("bedcode-test-user-plugins"),
            frontend_channel: Arc::new(
                crate::plugin::security::frontend_channel::FrontendChannelRegistry::new(),
            ),
        }
    }

    /// 构造一个最小 LoadedPlugin（manifest 含 storage + terminal:input 权限）
    fn make_plugin(id: &str, source: PluginSource, state: PluginState) -> LoadedPlugin {
        LoadedPlugin {
            manifest: PluginManifest {
                id: id.to_string(),
                name: format!("Test {}", id),
                version: "1.0.0".to_string(),
                description: String::new(),
                author: String::new(),
                main: "index.ts".to_string(),
                permissions: vec!["storage".to_string(), "terminal:input".to_string()],
                api: vec![],
                contributes: PluginContributes::default(),
                plugin_type: PluginType::TsOnly,
                rust_library: String::new(),
                wasm_hash: String::new(),
                icon: None,
                wasi_preopen_dirs: vec![],
                kind: bedcode_plugin_api::PluginKind::Application,
                dependencies: vec![],
                resource_overrides: None,
            },
            state,
            granted_permissions: HashSet::new(),
            extension_path: String::new(),
            activated_at: None,
            source,
        }
    }

    // ==================== 静态注册插件（builtin 常驻语义） ====================

    /// 合成静态插件 ID（inventory 条目仅编译进测试二进制，不影响产物）
    const SYNTHETIC_STATIC_ID: &str = "com.bedcode.test-static";

    /// 记录 on_startup 是否被宿主回调（inventory 条目进程级唯一，标志跨测试共享）
    static SYNTHETIC_ON_STARTUP_CALLED: AtomicBool = AtomicBool::new(false);

    fn synthetic_manifest() -> PluginManifest {
        PluginManifest {
            id: SYNTHETIC_STATIC_ID.to_string(),
            name: "Synthetic Static".to_string(),
            version: "0.1.0".to_string(),
            description: String::new(),
            author: String::new(),
            main: String::new(),
            permissions: vec![],
            api: vec![],
            contributes: PluginContributes::default(),
            plugin_type: PluginType::Rust,
            rust_library: String::new(),
            wasm_hash: String::new(),
            icon: None,
            wasi_preopen_dirs: vec![],
            kind: bedcode_plugin_api::PluginKind::Application,
            dependencies: vec![],
            resource_overrides: None,
        }
    }

    fn synthetic_on_startup() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async {
            SYNTHETIC_ON_STARTUP_CALLED.store(true, Ordering::SeqCst);
            Ok(())
        })
    }

    fn synthetic_on_shutdown() -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn synthetic_noop_lifecycle(_ctx: RustPluginContext) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn synthetic_commands() -> Vec<PluginCommand> {
        vec![PluginCommand::new("ping", |_args| async move {
            Ok(json!({ "pong": true }))
        })]
    }

    fn synthetic_terminal_handlers() -> Vec<Box<dyn TerminalHandler>> {
        Vec::new()
    }

    // 合成静态注册条目：等价 submit_plugin! 的 inventory 注册链路
    inventory::submit! {
        bedcode_plugin_api::BedcodePluginEntry {
            id: SYNTHETIC_STATIC_ID,
            create_manifest: synthetic_manifest,
            activate: synthetic_noop_lifecycle,
            deactivate: synthetic_noop_lifecycle,
            register_commands: synthetic_commands,
            terminal_handlers: synthetic_terminal_handlers,
            on_startup: synthetic_on_startup,
            on_shutdown: synthetic_on_shutdown,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_static_builtin_activated_notifies_startup() {
        let host = setup_host().await;
        // 未激活（Loaded）时 notify_startup 不得回调 —— 旧行为矛盾点：
        // 日志称 loaded 却永不激活、on_startup 永不执行
        host.plugins.write().await.insert(
            SYNTHETIC_STATIC_ID.to_string(),
            make_plugin(SYNTHETIC_STATIC_ID, PluginSource::StaticRegistry, PluginState::Loaded),
        );
        SYNTHETIC_ON_STARTUP_CALLED.store(false, Ordering::SeqCst);
        host.notify_startup().await;
        assert!(
            !SYNTHETIC_ON_STARTUP_CALLED.load(Ordering::SeqCst),
            "未激活的静态插件不应收到 on_startup 回调"
        );

        // 置 Activated（模拟 new() 的 builtin 常驻初始化产物）：回调真实发生
        host.plugins.write().await.get_mut(SYNTHETIC_STATIC_ID).unwrap().state = PluginState::Activated;
        assert!(host.is_activated(SYNTHETIC_STATIC_ID).await);
        host.notify_startup().await;
        assert!(
            SYNTHETIC_ON_STARTUP_CALLED.load(Ordering::SeqCst),
            "notify_startup 应回调已激活静态插件的 on_startup"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_static_builtin_command_routed_after_activation() {
        let host = setup_host().await;
        // 经 register_rust_command_handlers 从合成 inventory 条目注册 handler
        host.register_rust_command_handlers().await;

        // 未激活：身份门禁拒绝
        host.plugins.write().await.insert(
            SYNTHETIC_STATIC_ID.to_string(),
            make_plugin(SYNTHETIC_STATIC_ID, PluginSource::StaticRegistry, PluginState::Loaded),
        );
        let err = host
            .invoke_rust_command(SYNTHETIC_STATIC_ID, "ping", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not activated"));

        // 置 Activated 后命令可达 handler
        host.plugins.write().await.get_mut(SYNTHETIC_STATIC_ID).unwrap().state = PluginState::Activated;
        let result = host
            .invoke_rust_command(SYNTHETIC_STATIC_ID, "ping", json!({}))
            .await
            .unwrap();
        assert_eq!(result, json!({ "pong": true }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_static_builtin_deactivate_and_reactivate() {
        let host = setup_host().await;
        host.register_rust_command_handlers().await;
        host.plugins.write().await.insert(
            SYNTHETIC_STATIC_ID.to_string(),
            make_plugin(
                SYNTHETIC_STATIC_ID,
                PluginSource::StaticRegistry,
                PluginState::Activated,
            ),
        );
        assert!(host
            .invoke_rust_command(SYNTHETIC_STATIC_ID, "ping", json!({}))
            .await
            .is_ok());

        // 停用：回落 Deactivated，命令门禁重新关闭
        host.deactivate_plugin(SYNTHETIC_STATIC_ID, false).await.unwrap();
        assert!(!host.is_activated(SYNTHETIC_STATIC_ID).await);
        assert_eq!(
            host.get_plugin(SYNTHETIC_STATIC_ID).await.unwrap().state,
            PluginState::Deactivated
        );
        assert!(host
            .invoke_rust_command(SYNTHETIC_STATIC_ID, "ping", json!({}))
            .await
            .is_err());

        // 重启：activate_plugin 对 StaticRegistry 跳过 WASM phase 直接置回 Activated，命令恢复路由
        host.activate_plugin(SYNTHETIC_STATIC_ID, false).await.unwrap();
        assert!(host.is_activated(SYNTHETIC_STATIC_ID).await);
        let result = host
            .invoke_rust_command(SYNTHETIC_STATIC_ID, "ping", json!({}))
            .await
            .unwrap();
        assert_eq!(result, json!({ "pong": true }));
    }

    // ==================== Accessors ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_accessors_return_shared_arcs() {
        let host = setup_host().await;
        // getter 返回的是与字段共享的同一 Arc（Clone 语义）
        assert!(Arc::ptr_eq(host.registry(), &host.registry));
        assert!(Arc::ptr_eq(host.permission(), &host.permission));
        assert!(Arc::ptr_eq(host.storage(), &host.storage));
        assert!(Arc::ptr_eq(host.wasm_runtime(), &host.wasm_runtime));
        assert!(Arc::ptr_eq(host.message_bus(), &host.message_bus));
        assert!(Arc::ptr_eq(host.wasm_host_ctx(), &host.wasm_host_ctx));
    }

    // ==================== Plugins Map 查询 ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_plugins_and_get_plugin() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        host.plugins.write().await.insert(
            "com.bedcode.static".to_string(),
            make_plugin("com.bedcode.static", PluginSource::StaticRegistry, PluginState::Loaded),
        );

        let list = host.list_plugins().await;
        assert_eq!(list.len(), 2);
        // 来源映射到前端友好字符串
        let scanned = list.iter().find(|p| p.id == TEST_PLUGIN_ID).unwrap();
        assert_eq!(scanned.source, "scanned");
        assert_eq!(scanned.state, PluginState::Activated);
        let builtin = list.iter().find(|p| p.id == "com.bedcode.static").unwrap();
        assert_eq!(builtin.source, "builtin");

        // get_plugin：命中与未命中
        assert!(host.get_plugin("com.missing").await.is_none());
        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.id, TEST_PLUGIN_ID);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_is_activated_by_state() {
        let host = setup_host().await;
        // 未注册插件 → false
        assert!(!host.is_activated("com.missing").await);

        for (state, expected) in [
            (PluginState::Loaded, false),
            (PluginState::Activated, true),
            (PluginState::Deactivated, false),
            (PluginState::Error("boom".into()), false),
        ] {
            host.plugins.write().await.insert(
                TEST_PLUGIN_ID.to_string(),
                make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, state),
            );
            assert_eq!(host.is_activated(TEST_PLUGIN_ID).await, expected);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_mark_error_updates_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );

        host.mark_error(TEST_PLUGIN_ID, "hooks install failed".to_string())
            .await;
        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Error("hooks install failed".to_string()));

        // 未注册插件：静默 no-op，不 panic
        host.mark_error("com.missing", "x".to_string()).await;
        assert!(host.get_plugin("com.missing").await.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_activated_state_excludes_static() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        host.plugins.write().await.insert(
            "com.bedcode.ts".to_string(),
            make_plugin("com.bedcode.ts", PluginSource::FileScan, PluginState::Deactivated),
        );
        // 静态注册插件即使激活也不应进入持久化映射（由应用进程生命周期托管）
        host.plugins.write().await.insert(
            "com.bedcode.static".to_string(),
            make_plugin(
                "com.bedcode.static",
                PluginSource::StaticRegistry,
                PluginState::Activated,
            ),
        );

        let map = host.get_activated_state().await;
        assert_eq!(map.len(), 2);
        assert_eq!(map.get(TEST_PLUGIN_ID), Some(&true));
        assert_eq!(map.get("com.bedcode.ts"), Some(&false));
        assert!(!map.contains_key("com.bedcode.static"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_should_lazy_activate_rules() {
        let host = setup_host().await;
        // 未注册 → false
        assert!(!host.should_lazy_activate("com.missing").await);

        // Loaded + 有命令贡献 → 需要按需激活
        let mut plugin = make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded);
        plugin.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.cmd".into(),
                title: "T".into(),
                icon: None,
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert(TEST_PLUGIN_ID.to_string(), plugin);
        assert!(host.should_lazy_activate(TEST_PLUGIN_ID).await);

        // 无任何扩展点贡献 → false（激活无意义）
        host.plugins.write().await.insert(
            "com.bedcode.empty".to_string(),
            make_plugin("com.bedcode.empty", PluginSource::FileScan, PluginState::Loaded),
        );
        assert!(!host.should_lazy_activate("com.bedcode.empty").await);

        // 已激活/已停用/错误态 → false（仅 Loaded 态参与按需激活）
        host.plugins.write().await.insert(
            "com.bedcode.act".to_string(),
            make_plugin("com.bedcode.act", PluginSource::FileScan, PluginState::Activated),
        );
        assert!(!host.should_lazy_activate("com.bedcode.act").await);

        // 静态注册插件 → false（生命周期由 inventory 注册表托管）
        let mut static_p = make_plugin("com.bedcode.s", PluginSource::StaticRegistry, PluginState::Loaded);
        static_p.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.cmd".into(),
                title: "T".into(),
                icon: None,
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert("com.bedcode.s".to_string(), static_p);
        assert!(!host.should_lazy_activate("com.bedcode.s").await);
    }

    // ==================== Manifest Contributions ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_register_manifest_contributions() {
        let host = setup_host().await;
        let mut plugin = make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded);
        plugin.manifest.contributes = PluginContributes {
            commands: vec![bedcode_plugin_api::CommandContribution {
                id: "test.hello".into(),
                title: "Hello".into(),
                icon: None,
            }],
            views: vec![bedcode_plugin_api::ViewContribution {
                id: "test.view".into(),
                view_type: "sidebar".into(),
                title: "V".into(),
                component: "View.vue".into(),
            }],
            ..Default::default()
        };
        host.plugins.write().await.insert(TEST_PLUGIN_ID.to_string(), plugin);

        host.register_manifest_contributions().await;

        let commands = host.registry().list_commands().await;
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].plugin_id, TEST_PLUGIN_ID);
        let views = host.registry().list_views().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].plugin_id, TEST_PLUGIN_ID);
        assert_eq!(views[0].view_type, "sidebar");
    }

    // ==================== 激活 / 停用（非 WASM 插件） ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_file_scan_flow() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        // 未注册插件 → Err
        let err = host.activate_plugin("com.missing", false).await.unwrap_err();
        assert!(err.to_string().contains("not found"));

        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Activated);
        // 激活时间被记录
        {
            let plugins_guard = host.plugins.read().await;
            let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
            assert!(loaded.activated_at.is_some());
        }
        // 重新授权：manifest 声明的合法权限已授予（storage 恒默认授予）
        let granted = host.permission().get_granted(TEST_PLUGIN_ID);
        assert!(granted.contains("storage"));
        assert!(granted.contains("terminal:input"));
        assert!(host.permission().check(TEST_PLUGIN_ID, "terminal:input"));

        // 重复激活幂等（已激活 → Ok）
        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert_eq!(
            host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
            PluginState::Activated
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_recovers_from_error_state() {
        let host = setup_host().await;
        // Error 态插件可重新激活（如 WASM 缺失被标记后修复文件再激活）
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(
                TEST_PLUGIN_ID,
                PluginSource::FileScan,
                PluginState::Error("wasm load failed".into()),
            ),
        );

        host.activate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert_eq!(
            host.get_plugin(TEST_PLUGIN_ID).await.unwrap().state,
            PluginState::Activated
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_plugin_persists_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();

        let persisted = host.storage().load_activated_plugins().await.unwrap();
        assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_plugin_flow() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        // 预注册一条命令贡献，验证停用时从 registry 摘除
        host.registry()
            .register_commands(
                TEST_PLUGIN_ID,
                &[bedcode_plugin_api::CommandContribution {
                    id: "test.cmd".into(),
                    title: "T".into(),
                    icon: None,
                }],
            )
            .await;
        assert_eq!(host.registry().list_commands().await.len(), 1);

        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();

        let info = host.get_plugin(TEST_PLUGIN_ID).await.unwrap();
        assert_eq!(info.state, PluginState::Deactivated);
        assert!(!host.is_activated(TEST_PLUGIN_ID).await);
        // 激活时间被清除
        {
            let plugins_guard = host.plugins.read().await;
            let loaded = plugins_guard.get(TEST_PLUGIN_ID).unwrap();
            assert!(loaded.activated_at.is_none());
        }
        // 权限被撤销（重新激活时重新授权）
        assert!(host.permission().get_granted(TEST_PLUGIN_ID).is_empty());
        // registry 贡献被摘除
        assert!(host.registry().list_commands().await.is_empty());

        // 未注册插件 → Err
        let err = host.deactivate_plugin("com.missing", false).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_all() {
        let host = setup_host().await;
        for id in ["com.bedcode.a", "com.bedcode.b"] {
            host.plugins.write().await.insert(
                id.to_string(),
                make_plugin(id, PluginSource::FileScan, PluginState::Activated),
            );
        }

        host.deactivate_all().await.unwrap();

        for id in ["com.bedcode.a", "com.bedcode.b"] {
            assert_eq!(host.get_plugin(id).await.unwrap().state, PluginState::Deactivated);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_auto_activate_from_persisted_state() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );

        // 持久化激活 → 手动停用（不持久化）→ 从持久化状态恢复激活
        host.activate_plugin(TEST_PLUGIN_ID, true).await.unwrap();
        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert!(!host.is_activated(TEST_PLUGIN_ID).await);

        host.auto_activate_from_persisted_state().await;
        assert!(host.is_activated(TEST_PLUGIN_ID).await);

        // 幽灵 ID 清理：持久化映射中不存在的插件被剔除（map 整体替换语义：
        // 重新写入时保留现有条目再插入幽灵 ID）
        let mut stale = HashMap::new();
        stale.insert("com.ghost".to_string(), true);
        stale.insert(TEST_PLUGIN_ID.to_string(), true);
        host.storage().save_activated_plugins(&stale).await.unwrap();
        host.auto_activate_from_persisted_state().await;
        let persisted = host.storage().load_activated_plugins().await.unwrap();
        assert!(!persisted.contains_key("com.ghost"));
        // 已存在的插件条目保留
        assert_eq!(persisted.get(TEST_PLUGIN_ID), Some(&true));
    }

    // ==================== Rust Command Dispatch ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_rust_command_gates() {
        let host = setup_host().await;
        // 未注册 / 未激活 → Err（调用者身份门禁）
        let err = host
            .invoke_rust_command("com.missing", "cmd", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not activated"));

        // TS-only 插件（FileScan）→ 拒绝 Rust command
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "cmd", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("TS-only"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_ok() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );
        // 运行时注册表直接注入 handler（等价于 register_rust_command_handlers 的产物）
        let cmd = PluginCommand::new("hello", |args| async move { Ok(serde_json::json!({ "echo": args })) });
        host.rust_command_handlers
            .write()
            .await
            .insert(format!("{}::hello", TEST_PLUGIN_ID), cmd);

        let result = host
            .invoke_rust_command(TEST_PLUGIN_ID, "hello", json!({"k": 1}))
            .await
            .unwrap();
        assert_eq!(result, json!({ "echo": { "k": 1 } }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_not_found() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );

        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "missing", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Command not found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invoke_static_command_handler_error() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::StaticRegistry, PluginState::Activated),
        );
        let cmd = PluginCommand::new("boom", |_args| async move { Err(anyhow::anyhow!("handler exploded")) });
        host.rust_command_handlers
            .write()
            .await
            .insert(format!("{}::boom", TEST_PLUGIN_ID), cmd);

        let err = host
            .invoke_rust_command(TEST_PLUGIN_ID, "boom", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Command execution error"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_rust_commands_parses_namespace() {
        let host = setup_host().await;
        for (pid, cmd_name, title) in [
            ("com.a", "cmd1", "One"),
            ("com.a", "cmd2", "Two"),
            ("com.b", "cmd3", "Three"),
        ] {
            let cmd =
                PluginCommand::new(cmd_name, |_args| async move { Ok(serde_json::json!(null)) }).with_title(title);
            host.rust_command_handlers
                .write()
                .await
                .insert(format!("{}::{}", pid, cmd_name), cmd);
        }

        let mut entries = host.list_rust_commands().await;
        // HashMap 迭代无序：按 (plugin_id, command_name) 排序后比较
        entries.sort_by(|a, b| {
            (a.plugin_id.clone(), a.command_name.clone()).cmp(&(b.plugin_id.clone(), b.command_name.clone()))
        });
        let pairs: Vec<(String, String)> = entries
            .iter()
            .map(|e| (e.plugin_id.clone(), e.command_name.clone()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("com.a".to_string(), "cmd1".to_string()),
                ("com.a".to_string(), "cmd2".to_string()),
                ("com.b".to_string(), "cmd3".to_string()),
            ]
        );
        // 全名 `plugin_id::command_name` 正确拆分
        assert_eq!(entries[0].title, "One");
    }

    // ==================== Terminal Handler Pipeline ====================

    /// 记录 on_input_submitted 观测并转换输入/输出的 mock 处理器
    struct MockTerminalHandler {
        submitted: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl TerminalHandler for MockTerminalHandler {
        fn on_input(&self, _session_id: &str, text: &str) -> Option<String> {
            Some(format!("[{}]", text))
        }

        fn on_output(&self, _session_id: &str, data: &str) -> Option<String> {
            Some(data.to_uppercase())
        }

        fn on_input_submitted(&self, _session_id: &str, text: &str) {
            self.submitted.lock().unwrap().push(text.to_string());
        }
    }

    /// 默认实现（全部透传）的处理器
    struct PassthroughHandler;

    impl TerminalHandler for PassthroughHandler {}

    #[tokio::test(flavor = "multi_thread")]
    async fn test_terminal_handler_pipeline() {
        let host = setup_host().await;
        // 无 handler：输入输出原样透传
        assert!(!host.has_terminal_handlers().await);
        assert_eq!(host.process_terminal_input("s1", "echo hi").await, "echo hi");
        assert_eq!(host.process_terminal_output("s1", "Hello").await, "Hello");

        let submitted = Arc::new(std::sync::Mutex::new(Vec::new()));
        host.rust_terminal_handlers
            .write()
            .await
            .push(Box::new(MockTerminalHandler {
                submitted: submitted.clone(),
            }));
        // 第二个 handler 不修改（验证 None 语义透传）
        host.rust_terminal_handlers
            .write()
            .await
            .push(Box::new(PassthroughHandler));

        assert!(host.has_terminal_handlers().await);
        assert_eq!(host.process_terminal_input("s1", "echo hi").await, "[echo hi]");
        assert_eq!(host.process_terminal_output("s1", "Hello").await, "HELLO");
        // 观察回调：提交行原样送达
        host.process_input_submitted("s1", "ls -la").await;
        host.process_input_submitted("s1", "pwd").await;
        assert_eq!(
            *submitted.lock().unwrap(),
            vec!["ls -la".to_string(), "pwd".to_string()]
        );
    }

    // ==================== MessageDispatcher ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_message_dispatcher_is_activated() {
        let host = setup_host().await;
        assert!(!MessageDispatcher::is_activated(&host, "com.missing"));

        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );
        assert!(MessageDispatcher::is_activated(&host, TEST_PLUGIN_ID));

        host.plugins.write().await.insert(
            "com.bedcode.d".to_string(),
            make_plugin("com.bedcode.d", PluginSource::FileScan, PluginState::Deactivated),
        );
        assert!(!MessageDispatcher::is_activated(&host, "com.bedcode.d"));
    }

    // ==================== WASM 插件（真实组件测试插件） ====================

    /// 将 wit-bindgen 产出的 core module 编码为组件
    /// （与 wasm_runtime.rs 测试同策略，等价于 `wasm-tools component new`）

    /// 构建测试用组件插件并编码为组件（packages/plugin-component-test）
    fn build_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-component-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_component_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/wit/bedcode.wit"),
            ];
            let module_modified = std::fs::metadata(&module_path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            let needs_rebuild = src_files.iter().any(|f| {
                std::fs::metadata(f)
                    .and_then(|m| m.modified())
                    .map(|t| t > module_modified)
                    .unwrap_or(true)
            });

            if !needs_rebuild {
                return std::fs::read(&module_path).expect("Failed to read test component module");
            }
        }

        let manifest_path = plugin_dir.join("Cargo.toml");
        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                manifest_path.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for test component");
        assert!(status.success(), "Test component WASM build failed");

        std::fs::read(&module_path).expect("Failed to read test component after build")
    }

    /// 将组件形态测试插件实例化并注入宿主（plugins + wasm_plugins 双表）
    ///
    /// 返回插件 ID；组件 invoke 内 host_storage 读回的 key 预写入
    /// `component-test-key`。extension_path 指向临时目录（invoke 的
    /// resource_dir 注入断言用）。
    async fn setup_wasm_plugin(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
        let component = host
            .wasm_runtime()
            .compile_component(&build_test_component())
            .expect("compile test component");
        let plugin = host
            .wasm_runtime()
            .instantiate_component(&component, TEST_WASM_PLUGIN_ID, host.wasm_host_ctx().clone(), &[], None)
            .expect("instantiate test component");

        host.storage()
            .set(TEST_WASM_PLUGIN_ID, "component-test-key", json!({"k": "v"}))
            .await
            .expect("preset storage key");

        let extension_path = tmp_dir.path().to_string_lossy().to_string();
        host.wasm_plugins
            .write()
            .await
            .insert(TEST_WASM_PLUGIN_ID.to_string(), Arc::new(Mutex::new(plugin)));

        let mut loaded = make_plugin(TEST_WASM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
        loaded.manifest.rust_library = "bedcode_plugin_component_test".to_string();
        loaded.extension_path = extension_path;
        host.plugins
            .write()
            .await
            .insert(TEST_WASM_PLUGIN_ID.to_string(), loaded);

        TEST_WASM_PLUGIN_ID.to_string()
    }

    /// v8 契约端到端（宿主侧）：on_startup 自报失败 → Degraded 终态；
    /// 移除故障开关后重试 → Activated；随后停用干净回落。
    /// 失败开关为组件测试插件的 storage key `component-test-fail-startup`
    #[tokio::test(flavor = "multi_thread")]
    async fn test_activate_degraded_on_startup_failure_then_retry_recovers() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        // 预置启动失败开关：激活调用本身成功，但终态必须如实落 Degraded
        host.storage()
            .set(&pid, "component-test-fail-startup", json!(true))
            .await
            .expect("preset fail-startup switch");
        host.activate_plugin(&pid, false)
            .await
            .expect("activation call must succeed even when on_startup reports failure");
        let info = host.get_plugin(&pid).await.unwrap();
        assert!(
            matches!(&info.state, PluginState::Degraded(reason) if reason.contains("simulated startup init failure")),
            "expected Degraded with guest-reported reason, got {:?}",
            info.state
        );

        // 持久化意图映射：Degraded 视为已启用（下次启动仍重试）
        let activated_state = host.get_activated_state().await;
        assert_eq!(
            activated_state.get(&pid),
            Some(&true),
            "degraded counts as enabled intent"
        );

        // 重试激活（移除开关）→ 回到 Activated
        host.storage()
            .delete(&pid, "component-test-fail-startup")
            .await
            .expect("clear fail-startup switch");
        host.activate_plugin(&pid, false)
            .await
            .expect("retry activation must succeed");
        let info = host.get_plugin(&pid).await.unwrap();
        assert_eq!(info.state, PluginState::Activated);

        // 激活态停用干净回落
        host.deactivate_plugin(&pid, false)
            .await
            .expect("deactivate after recovery ok");
        let info = host.get_plugin(&pid).await.unwrap();
        assert_eq!(info.state, PluginState::Deactivated);
    }

    /// 持久化写入路径：persist=true 时 Degraded 以 true 落库（用户意图语义）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_persisted_intent_keeps_degraded_enabled() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        host.storage()
            .set(&pid, "component-test-fail-startup", json!(true))
            .await
            .expect("preset fail-startup switch");
        host.activate_plugin(&pid, true)
            .await
            .expect("activation call must succeed");

        let map = host.storage().load_activated_plugins().await.unwrap();
        assert_eq!(
            map.get(&pid),
            Some(&true),
            "degraded plugin must persist as enabled intent"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_wasm_plugin_activate_invoke_deactivate() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        // 激活：调用组件 __bedcode_activate + on_startup，无错误码
        host.activate_plugin(&pid, false).await.unwrap();
        assert!(host.is_activated(&pid).await);

        // 命令调用：组件 echo + host_storage 读回 + resource_dir 自动注入
        let result = host
            .invoke_rust_command(&pid, "test.echo", json!({"hello": "host"}))
            .await
            .unwrap();
        assert_eq!(result["name"], "test.echo");
        assert_eq!(result["stored"], json!({"k": "v"}));
        let args: serde_json::Value = serde_json::from_str(result["args"].as_str().unwrap()).unwrap();
        assert_eq!(args["resource_dir"], json!(tmp_dir.path().to_string_lossy()));

        // 停用：调用组件 on_shutdown + __bedcode_deactivate
        host.deactivate_plugin(&pid, false).await.unwrap();
        assert!(!host.is_activated(&pid).await);
        assert_eq!(host.get_plugin(&pid).await.unwrap().state, PluginState::Deactivated);

        // 停用后调用被门禁拒绝
        let err = host
            .invoke_rust_command(&pid, "test.echo", json!({}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not activated"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_dispatch_lifecycle_and_input_to_wasm_plugin() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let pid = setup_wasm_plugin(&host, &tmp_dir).await;

        // 未激活：dispatch 被门禁静默丢弃（无 panic、不触碰 store）
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
        host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s1", "text": "hi"}));

        host.activate_plugin(&pid, false).await.unwrap();

        // 经真实 listener 走「事件 → payload 构造 → dispatch → wasm 回调」全链路
        let lifecycle_listener = PluginLifecycleListener::new(pid.clone(), host.clone());
        SessionLifecycleListener::on_session_lifecycle(
            &lifecycle_listener,
            &SessionLifecycleEvent::Created {
                session_id: "s1".to_string(),
                config_id: "c1".to_string(),
                name: "n".to_string(),
                working_dir: "/tmp".to_string(),
            },
        );
        let input_listener = PluginInputListener::new(pid.clone(), host.clone());
        SessionInputListener::on_input_submitted(&input_listener, "s1", "echo hi");

        // 直接分发不同 payload 形态
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "stopped", "sessionId": "s1"}));
        host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s2", "text": "ls"}));

        // store 未被污染：分发全部成功后 command 调用仍可用
        let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
        assert_eq!(result["name"], "test.echo");

        // 停用后 dispatch 门禁丢弃，invoke 拒绝
        host.deactivate_plugin(&pid, false).await.unwrap();
        host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
        assert!(host.invoke_rust_command(&pid, "test.echo", json!({})).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_reload_wasm_plugin_cycle() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        // 把组件字节写入临时插件目录（reload 从文件重新加载）
        let wasm_bytes = build_test_component();
        let wasm_path = tmp_dir.path().join("bedcode_plugin_component_test.wasm");
        std::fs::write(&wasm_path, &wasm_bytes).unwrap();

        let pid = setup_wasm_plugin(&host, &tmp_dir).await;
        host.activate_plugin(&pid, false).await.unwrap();

        // 完整卸载-重载-激活循环
        host.reload_wasm_plugin(&pid).await.unwrap();
        assert!(host.is_activated(&pid).await);

        // 重载后的新实例可用
        let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
        assert_eq!(result["name"], "test.echo");
    }

    // ==================== PluginServices（可测部分） ====================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_plugin_timer_register_replace_abort() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
        );

        // 注册定时器（3600s 间隔：测试期间不会触发 tick 回调）
        PluginServices::register_plugin_timer(&host, TEST_PLUGIN_ID.to_string(), 3600, "tick".to_string());
        assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

        // 重复注册替换旧句柄（v6 ADR 0003：同一插件仅保留一个定时器）
        PluginServices::register_plugin_timer(&host, TEST_PLUGIN_ID.to_string(), 3600, "tick".to_string());
        assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

        // 停用中止定时器（不再到点回调）
        host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
        assert!(host.plugin_timers.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_schedule_plugin_reload_throttle() {
        let host = setup_host().await;
        // 未激活插件：调度后后台任务直接退出，仅验证限频表行为
        host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
        {
            let throttle = host.wasm_reload_throttle.lock().unwrap();
            assert!(throttle.contains_key(TEST_PLUGIN_ID));
        }
        // 30 秒窗口内再次调度被限频跳过：不新增条目
        host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
        {
            let throttle = host.wasm_reload_throttle.lock().unwrap();
            assert_eq!(throttle.len(), 1);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_session_listener_registration_via_services() {
        let host = setup_host().await;
        let session_manager = host.wasm_host_ctx().session_manager_arc();

        // PluginServices 实现的注册路径（listener 构造 + block_on_async 注册）。
        // 注册结果在 SessionManager 内部（无公开查询接口），此处验证不 panic、
        // 且注册的 listener 可被停用流程按 plugin_id 摘除
        PluginServices::register_session_lifecycle_listener(&host, TEST_PLUGIN_ID.to_string(), session_manager.clone());
        PluginServices::register_session_input_listener(&host, TEST_PLUGIN_ID.to_string(), session_manager.clone());

        // listener 自身携带正确 plugin_id（停用摘除依赖此标识）
        let l1 = PluginLifecycleListener::new(TEST_PLUGIN_ID.to_string(), host.clone());
        assert_eq!(l1.plugin_id(), TEST_PLUGIN_ID);
        assert_eq!(SessionLifecycleListener::plugin_id(&l1), Some(TEST_PLUGIN_ID));
        let l2 = PluginInputListener::new(TEST_PLUGIN_ID.to_string(), host);
        assert_eq!(SessionInputListener::plugin_id(&l2), Some(TEST_PLUGIN_ID));
    }

    #[tokio::test]
    async fn notify_plugin_runtime_error_throttle_and_no_app_context() {
        // 统一异常通道（PLUGIN_RUNTIME_ERROR）：
        // 1. 无 AppContext（测试/无头）时降级为纯日志，不 panic
        // 2. 同一插件窗口内二次通知被节流（不重复提示），节流表只记录一次
        let host = setup_host().await;

        host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "panic", "boom").await;
        host.notify_plugin_runtime_error(TEST_PLUGIN_ID, "trap", "boom again")
            .await;

        let throttle = host.runtime_error_notify_throttle.lock().unwrap();
        assert!(
            throttle.contains_key(TEST_PLUGIN_ID),
            "first call must record throttle entry"
        );
        // 窗口内二次调用不新增/刷新条目（被节流）
        assert_eq!(throttle.len(), 1, "second call within window must be throttled");
    }

    // ==================== preauthorize_plugin 预授权钩子 ====================
    //
    // 验证「先授权再 loading」改造的契约:
    // 1. 无 provider + 无 storage:直接放行(空路径 = 无需预授权)
    // 2. file-transfer 无共享目录:同样放行(启用先行,配置由插件设置面板引导)
    // 3. 路径已在 storage:不需要 provider,直接放行(check_batch 空路径短路)

    /// 无 provider + 无 storage 预授权路径:空路径直接放行。
    /// 对应「普通插件(无 fs 权限)启用 → 不出现 fs 弹窗,loading 正常显示」场景。
    #[tokio::test]
    async fn preauthorize_empty_paths_passes() {
        let host = setup_host().await;
        let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
        assert!(result.is_ok(), "empty preauth paths must pass, got: {:?}", result.err());
    }

    /// file-transfer 共享目录未配置:放行(启用先行)。
    /// 硬拒绝会造成死锁——共享目录配置入口在插件 UI 内,而插件 UI 加载
    /// 依赖激活成功,「配置需激活 → 激活需先配置」互为前置,首次启用永远失败。
    #[tokio::test]
    async fn preauthorize_file_transfer_empty_shared_roots_passes() {
        let host = setup_host().await;
        let result = host.preauthorize_plugin(super::FILE_TRANSFER_PLUGIN_ID).await;
        assert!(
            result.is_ok(),
            "file-transfer with empty shared_roots must pass (enable-first), got: {:?}",
            result.err()
        );
    }

    /// manifest `wasiPreopenDirs` 声明目录并入预授权收集(如 ai-chatbox 数据
    /// 目录)。未授权 + 无头上下文(check_batch 保守拒绝)→ 返回「授权被拒」
    /// 错误——若声明目录未被收集,空路径会直接放行,本用例即失去意义
    #[tokio::test]
    async fn preauthorize_collects_manifest_preopen_dirs_ungranted_denied() {
        let host = setup_host().await;
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );
        host.plugins
            .write()
            .await
            .get_mut(TEST_PLUGIN_ID)
            .expect("test plugin in map")
            .manifest
            .wasi_preopen_dirs = vec!["${home}/.bedcode-preauth-probe".to_string()];

        let err = host
            .preauthorize_plugin(TEST_PLUGIN_ID)
            .await
            .expect_err("ungranted manifest dir must be collected and denied headless");
        assert!(
            err.to_string().contains("denied"),
            "must fail via check_batch deny (not storage/parse), got: {}",
            err
        );
    }

    /// 声明目录已授权(storage fs_granted_paths 前缀命中)→ check_batch 短路
    /// 通过,preauthorize 整体放行
    #[tokio::test]
    async fn preauthorize_manifest_preopen_dir_granted_passes() {
        let host = setup_host().await;
        let expanded = format!(
            "{}/.bedcode-preauth-probe",
            std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
        );
        host.plugins.write().await.insert(
            TEST_PLUGIN_ID.to_string(),
            make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Loaded),
        );
        host.plugins
            .write()
            .await
            .get_mut(TEST_PLUGIN_ID)
            .expect("test plugin in map")
            .manifest
            .wasi_preopen_dirs = vec!["${home}/.bedcode-preauth-probe".to_string()];
        host.storage
            .set(TEST_PLUGIN_ID, "fs_granted_paths", json!([expanded]))
            .await
            .unwrap();

        let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
        assert!(
            result.is_ok(),
            "granted manifest dir must pass, got: {:?}",
            result.err()
        );
    }

    /// storage 已写入 preauth_paths 数组:从 storage 读取路径,非 file-transfer
    /// 插件直接放行(check_batch 空 path 列表会短路返回 true,无头上下文
    /// 不发事件)。
    #[tokio::test]
    async fn preauthorize_reads_paths_from_storage() {
        let host = setup_host().await;
        // 写入 storage 数组 — 不影响 plugin_id 隔离(只有自身能读)
        host.storage
            .set(
                TEST_PLUGIN_ID,
                super::PREAUTH_PATHS_STORAGE_KEY,
                json!([std::env::temp_dir().to_string_lossy()]),
            )
            .await
            .unwrap();
        // check_batch 在无头 app_handle 上下文下对未授权路径保守拒绝,
        // 因此我们只验证「路径已被收集」并不期望一定通过;重要的是
        // preauthorize 不会因 storage 读取而 panic,且调用了 check_batch
        let result = host.preauthorize_plugin(TEST_PLUGIN_ID).await;
        // 接受 Ok 或 Err(无头上下文拒绝)— 但不能是 storage 解析错误
        if let Err(e) = &result {
            assert!(
                !e.to_string().contains("parse") && !e.to_string().contains("deserialize"),
                "storage parse error indicates collector bug: {}",
                e
            );
        }
    }

    // ==================== 系统组件与能力装配（core-plugin-manager，票据 06） ====================

    /// 系统组件 fixture 插件 ID（与 packages/plugin-system-test 的 manifest 一致）
    const TEST_SYSTEM_PLUGIN_ID: &str = "com.bedcode.system-test";

    /// 构建系统组件 fixture 并编码为组件（packages/plugin-system-test，
    /// 与 build_test_component 同策略：mtime 新鲜度检查 + cargo build）
    fn build_system_test_component() -> Vec<u8> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let packages_dir = manifest_dir.join("../packages");
        let plugin_dir = packages_dir.join("plugin-system-test");

        let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
        let module_path = output_dir.join("bedcode_plugin_system_test.wasm");

        if module_path.exists() {
            let src_files = [
                plugin_dir.join("src/lib.rs"),
                packages_dir.join("plugin-sdk-desktop/rust/wit/bedcode.wit"),
            ];
            let module_modified = std::fs::metadata(&module_path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let needs_rebuild = src_files.iter().any(|f| {
                std::fs::metadata(f)
                    .and_then(|m| m.modified())
                    .map(|t| t > module_modified)
                    .unwrap_or(true)
            });
            if !needs_rebuild {
                return std::fs::read(&module_path).expect("Failed to read system test module");
            }
        }

        let status = std::process::Command::new("cargo")
            .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
            .args([
                "build",
                "--target",
                "wasm32-wasip3",
                "--release",
                "--manifest-path",
                plugin_dir.join("Cargo.toml").to_str().unwrap(),
            ])
            .status()
            .expect("Failed to run cargo build for system test component");
        assert!(status.success(), "System test component WASM build failed");
        std::fs::read(&module_path).expect("Failed to read system test module after build")
    }

    /// 实例化系统组件 fixture 并注入宿主（kind=System，探测断言含 host-storage）
    async fn setup_system_component(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
        let component = host
            .wasm_runtime()
            .compile_component(&build_system_test_component())
            .expect("compile system test component");
        let plugin = host
            .wasm_runtime()
            .instantiate_component(
                &component,
                TEST_SYSTEM_PLUGIN_ID,
                host.wasm_host_ctx().clone(),
                &[],
                None,
            )
            .expect("instantiate system test component");
        // 实例化探测：plugin-system world 的 host-storage 导出应被识别为可路由能力
        assert_eq!(
            plugin.exported_capabilities(),
            &["host-storage".to_string()],
            "system component must export host-storage capability"
        );

        host.wasm_plugins
            .write()
            .await
            .insert(TEST_SYSTEM_PLUGIN_ID.to_string(), Arc::new(Mutex::new(plugin)));

        let mut loaded = make_plugin(TEST_SYSTEM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
        loaded.manifest.rust_library = "bedcode_plugin_system_test".to_string();
        loaded.manifest.kind = bedcode_plugin_api::PluginKind::System;
        loaded.extension_path = tmp_dir.path().to_string_lossy().to_string();
        host.plugins
            .write()
            .await
            .insert(TEST_SYSTEM_PLUGIN_ID.to_string(), loaded);

        TEST_SYSTEM_PLUGIN_ID.to_string()
    }

    /// 装配闭环：系统组件注册能力 + 应用插件经 Linker 路由消费，
    /// 读到的值来自系统组件实例私有 KV 而非宿主 SQLite（证明转发到达组件实例）
    #[tokio::test(flavor = "multi_thread")]
    async fn test_system_component_capability_routing_end_to_end() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let sys_id = setup_system_component(&host, &tmp_dir).await;
        let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

        // 应用插件声明能力依赖（激活时校验）
        host.plugins
            .write()
            .await
            .get_mut(&app_id)
            .unwrap()
            .manifest
            .dependencies = vec!["host-storage".to_string()];

        // 宿主 SQLite 预写对照值（若未路由，应用插件将读到它）
        host.storage()
            .set(&app_id, "component-test-key", json!({"k": "v"}))
            .await
            .expect("preset host storage key");

        // 系统组件先激活 → 能力注册表切换为系统组件提供者
        host.activate_plugin(&sys_id, false)
            .await
            .expect("activate system component");
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            format!("system:{}", sys_id),
            "host-storage must be provided by system component after activation"
        );

        // 应用插件后激活：依赖检查命中系统组件提供者
        host.activate_plugin(&app_id, false).await.expect("activate app plugin");

        // 预置系统组件实例的私有 KV（host-side 直接调用其能力导出）
        let sys_inst = host.get_wasm_plugin(&sys_id).await.unwrap();
        let set_result = sys_inst
            .lock()
            .await
            .call_capability_export::<(String, String), (Result<(), String>,)>(
                "bedcode:plugin/host-storage.set",
                ("component-test-key".to_string(), r#"{"sys":"routed"}"#.to_string()),
            )
            .expect("capability set transport");
        assert!(set_result.0.is_ok(), "capability set guest result: {:?}", set_result);

        // 应用插件消费：invoke 内 host_storage::get("component-test-key") 应经
        // Linker 路由转发到系统组件实例（读到系统组件私有值，而非宿主 SQLite）
        let result = host
            .invoke_rust_command(&app_id, "test.echo", json!({}))
            .await
            .expect("invoke app command");
        assert_eq!(
            result["stored"],
            json!({"sys": "routed"}),
            "routed read must return system component value, got: {}",
            result
        );

        // 显式按键读取同样路由
        let result = host
            .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "component-test-key"}))
            .await
            .expect("invoke storage-get");
        assert_eq!(result["value"], json!({"sys": "routed"}), "got: {}", result);
    }

    /// 票 05 闭环：server 认证策略（验签 → 会话中心 `auth-policy` 导出 → 放行/拒绝）
    ///
    /// 真实会话中心 wasip3 产物经宿主 async 运行时加载：验签由宿主 `JwtService`
    /// 执行（中间件路径，密码学引擎不移动），验签后经
    /// `auth_center::enforce_connection_policy` 取会话中心 `auth-policy` capability
    /// 导出做策略裁决（结构 / claims / 时效 + **信任撤销检查**）。覆盖：
    /// - 未激活（api 注册表无标记）→ 宿主策略回退（Ok，无单点）
    /// - 激活 + 内核 `pairings` 空 → 放行（无信任锚点只凭验签，搬迁前语义）
    /// - 内核存在活跃配对记录 → 放行
    /// - 经插件撤销（`session.trust.revoke` 软删内核真源）→ 拒绝（原因透出）
    /// - 未撤销的其他设备 token → 放行
    /// - 实例消失（停用）→ 能力调用失败 → 宿主策略回退（Ok）
    ///
    /// 票 05 落地：策略目标自旧认证中心插件改指会话中心；信任判据自插件私有镜像
    /// 改为内核 `pairings` 表（host-auth 记录面），测试直查内核真源断言软删。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_auth_policy_closed_loop() {
        use crate::utils::auth::auth_center as bridge;

        const PAIRING_ID: &str = "p-policy";
        let session_id = bridge::SESSION_PLUGIN_ID;
        let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
        if !wasm_path.exists() {
            eprintln!("[skip] session wasip3 artifact not built");
            return;
        }

        let host = setup_host().await;
        let component = host
            .wasm_runtime()
            .compile_component(&std::fs::read(&wasm_path).expect("read session artifact"))
            .expect("compile session artifact");
        let plugin = host
            .wasm_runtime()
            .instantiate_component(&component, session_id, host.wasm_host_ctx().clone(), &[], None)
            .expect("instantiate session");
        if !plugin.exported_capabilities().iter().any(|c| c == "auth-policy") {
            eprintln!("[skip] session artifact lacks auth-policy export (rebuild with current SDK)");
            return;
        }
        host.wasm_plugins
            .write()
            .await
            .insert(session_id.to_string(), Arc::new(Mutex::new(plugin)));
        let mut loaded = make_plugin(session_id, PluginSource::Wasm, PluginState::Loaded);
        loaded.manifest.rust_library = "bedcode_plugin_terminal_session".to_string();
        // 权限经 manifest 声明在 activate 时授予（生产装配路径；手工 grant 会被
        // activate 的 manifest 重新授权覆盖）
        loaded.manifest.permissions = vec!["auth".to_string(), "peer".to_string()];
        host.plugins.write().await.insert(session_id.to_string(), loaded);

        host.wasm_host_ctx().api_registry().register(session_id, &[]);
        host.activate_plugin(session_id, false).await.expect("activate session");

        // 验签在宿主执行（中间件路径）：无效 token 连策略都到不了
        let bad = "not-a-jwt";
        assert!(crate::utils::auth::JwtService::new()
            .verify_token_with_expiry(bad)
            .is_err());

        // ============ 未激活 → 宿主策略回退（无单点） ============
        assert!(
            !bridge::session_active(host.wasm_host_ctx()),
            "未注册标记 api → 视为未激活"
        );
        let valid_token = crate::utils::auth::JwtService::new()
            .generate_token(
                "device-1".to_string(),
                Some("Pixel 9".to_string()),
                Some("fp-abc".to_string()),
            )
            .expect("issue host token");
        crate::utils::auth::JwtService::new()
            .verify_token_with_expiry(&valid_token)
            .expect("host verifies signature");
        assert!(
            bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
            "会话中心未激活 → 宿主策略放行"
        );

        // ============ 激活 → 策略取会话中心（锚点 = trust-list） ============
        host.wasm_host_ctx()
            .api_registry()
            .register(session_id, &[bridge::SESSION_MARKER_API.to_string()]);
        assert!(bridge::session_active(host.wasm_host_ctx()));

        // 内核无配对记录 → 放行（无信任锚点，仅凭验签；搬迁前语义）
        assert!(
            bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
            "内核无记录 → 放行"
        );

        // 内核写入活跃配对记录（配对完成流的宿主写入路径）→ 放行
        {
            let db = host.wasm_host_ctx().database().lock().await;
            db.conn()
                .execute(
                    "INSERT INTO pairings (id, device_name, device_fingerprint, public_key, address, \
                     paired_at, connect_count, is_active) VALUES (?1, 'Pixel 9', 'fp-abc', 'pk', NULL, \
                     '2026-09-19T00:00:00Z', 1, 1)",
                    rusqlite::params![PAIRING_ID],
                )
                .expect("seed kernel pairing");
        }
        assert!(
            bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
            "已配对设备 → 放行"
        );

        // 经插件撤销（软删内核真源）→ 拒绝（拒绝原因必须可读）
        let revoked = host
            .invoke_rust_command(session_id, "session.trust.revoke", json!({"id": PAIRING_ID}))
            .await
            .expect("session.trust.revoke");
        assert_eq!(revoked["removed"], true);
        {
            let db = host.wasm_host_ctx().database().lock().await;
            let active: i32 = db
                .conn()
                .query_row(
                    "SELECT is_active FROM pairings WHERE id = ?1",
                    rusqlite::params![PAIRING_ID],
                    |row| row.get(0),
                )
                .expect("软删保留记录");
            assert_eq!(active, 0, "撤销由插件写内核真源（host-auth 记录面）");
        }
        let deny = bridge::enforce_connection_policy(&host, &valid_token).expect_err("撤销后必须拒绝");
        assert!(deny.contains("revoked"), "拒绝原因可读: {}", deny);

        // 未撤销记录的其他设备 token → 放行（内核未命中从宽，搬迁前语义）
        let other_token = crate::utils::auth::JwtService::new()
            .generate_token(
                "device-2".to_string(),
                Some("Phone 2".to_string()),
                Some("fp-xyz".to_string()),
            )
            .expect("issue other token");
        assert!(
            bridge::enforce_connection_policy(&host, &other_token).is_ok(),
            "未撤销设备 → 放行"
        );

        // ============ 实例消失 → 能力调用失败 → 宿主策略回退 ============
        host.wasm_plugins.write().await.remove(session_id);
        assert!(
            bridge::enforce_connection_policy(&host, &valid_token).is_ok(),
            "实例缺失 → 宿主策略回退（会话中心故障不误杀全部连接）"
        );
    }

    /// 依赖缺失：应用插件声明未知能力名 → 激活失败，错误信息指明能力名
    #[tokio::test(flavor = "multi_thread")]
    async fn test_activation_fails_with_missing_dependency_named() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

        host.plugins
            .write()
            .await
            .get_mut(&app_id)
            .unwrap()
            .manifest
            .dependencies = vec!["host-no-such-cap".to_string()];

        let err = host
            .activate_plugin(&app_id, false)
            .await
            .expect_err("activation must fail on missing capability");
        let msg = err.to_string();
        assert!(
            msg.contains("host-no-such-cap"),
            "error must name the missing capability, got: {}",
            msg
        );
        // 失败落 Error 终态（不留悬挂 Activating）
        let info = host.get_plugin(&app_id).await.unwrap();
        assert!(
            matches!(&info.state, PluginState::Error(e) if e.contains("host-no-such-cap")),
            "expected Error state naming missing capability, got {:?}",
            info.state
        );

        // 依赖宿主原语能力则放行（host-storage 恒由宿主原语/系统组件提供）
        host.plugins
            .write()
            .await
            .get_mut(&app_id)
            .unwrap()
            .manifest
            .dependencies = vec!["host-storage".to_string()];
        host.activate_plugin(&app_id, false)
            .await
            .expect("host primitive capability dependency must be satisfiable");
    }

    /// 系统组件 trap 隔离：转发调用中系统组件 panic，错误隔离为应用插件的
    /// Err 返回（应用插件实例不中毒、可继续调用），能力回落宿主原语
    #[tokio::test(flavor = "multi_thread")]
    async fn test_system_component_trap_isolated_and_reverts_to_host() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let sys_id = setup_system_component(&host, &tmp_dir).await;
        let app_id = setup_wasm_plugin(&host, &tmp_dir).await;

        host.storage()
            .set(&app_id, "component-test-key", json!({"k": "v"}))
            .await
            .expect("preset host storage key");
        host.activate_plugin(&sys_id, false)
            .await
            .expect("activate system component");
        host.activate_plugin(&app_id, false).await.expect("activate app plugin");
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            format!("system:{}", sys_id)
        );

        // 触发系统组件 trap（sys-test.panic key）：应用插件的 invoke 不 trap，
        // guest 收到 Err 并序列化进 storageError 字段（trap 不跨实例扩散）
        let result = host
            .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "sys-test.panic"}))
            .await
            .expect("app plugin invoke must survive system component trap");
        let storage_error = result["storageError"].as_str().unwrap_or("");
        assert!(
            storage_error.contains("system component capability call failed"),
            "forwarded trap must surface as guest-visible error, got: {}",
            result
        );

        // 能力自愈：trap 后回落宿主原语，后续调用读到宿主 SQLite 对照值
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            "host",
            "capability must revert to host primitive after system component trap"
        );
        let result = host
            .invoke_rust_command(&app_id, "test.storage-get", json!({"key": "component-test-key"}))
            .await
            .expect("app plugin must keep working after revert");
        assert_eq!(
            result["value"],
            json!({"k": "v"}),
            "host primitive fallback, got: {}",
            result
        );
    }

    /// 系统组件停用（只停不删）：能力回落宿主原语；重新激活后再装配
    #[tokio::test(flavor = "multi_thread")]
    async fn test_deactivate_system_component_reverts_capability() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host().await;
        let sys_id = setup_system_component(&host, &tmp_dir).await;

        host.activate_plugin(&sys_id, false)
            .await
            .expect("activate system component");
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            format!("system:{}", sys_id)
        );

        host.deactivate_plugin(&sys_id, false)
            .await
            .expect("deactivate system component");
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            "host",
            "capability must revert to host primitive on system component deactivation"
        );

        // 系统组件启停不持久化（默认启用语义：持久化真源是「内置」而非用户状态）
        let persisted = host.get_activated_state().await;
        assert!(
            !persisted.contains_key(&sys_id),
            "system component must be excluded from persisted activation state"
        );

        // 重新激活 → 能力再装配
        host.activate_plugin(&sys_id, false)
            .await
            .expect("re-activate system component");
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            format!("system:{}", sys_id)
        );
    }

    /// 启动加载顺序（集成测试）：PluginHost::new 全路径——系统组件先于应用
    /// 插件激活，应用插件（持久化启用 + 能力依赖）激活时注册表已含系统组件
    /// 提供者；两者终态均 Activated
    #[tokio::test(flavor = "multi_thread")]
    async fn test_boot_activates_system_components_before_app_plugins() {
        // AppConfig 初始化（与 setup_host 同策略，重复 init 幂等）
        static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
        CONFIG_INIT.call_once(|| {
            let mut config = AppConfig::default();
            config.network.port = 8765;
            AppConfig::init(config);
        });

        let tmp_dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = tmp_dir.path().join("plugins");
        let sys_id = "com.bedcode.system-test";
        let app_id = "com.bedcode.component-test";

        // 写两个插件包：系统组件（type=system）+ 应用插件（dependencies）
        for (id, rust_lib, manifest_extra) in [
            (sys_id, "bedcode_plugin_system_test", r#", "type": "system""#),
            (
                app_id,
                "bedcode_plugin_component_test",
                r#", "dependencies": ["host-storage"]"#,
            ),
        ] {
            let dir = plugins_dir.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            let manifest = format!(
                r#"{{"id": "{}", "name": "{}", "version": "0.1.0", "pluginType": "rust-ts", "rustLibrary": "{}", "permissions": ["storage"]{}}}"#,
                id, id, rust_lib, manifest_extra
            );
            std::fs::write(dir.join("plugin.json"), manifest).unwrap();
            let component = if id == sys_id {
                build_system_test_component()
            } else {
                build_test_component()
            };
            std::fs::write(dir.join(format!("{}.wasm", rust_lib)), component).unwrap();
        }

        // 预置持久化启用状态：仅应用插件（系统组件默认启用、无需持久化）
        let db = Arc::new(Mutex::new(
            Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
        ));
        db.lock().await.init_schema().unwrap();
        PluginStorage::new(db.clone())
            .save_activated_plugins(&HashMap::from([(app_id.to_string(), true)]))
            .await
            .expect("seed persisted activation state");

        let session_manager = Arc::new(SessionManager::default());
        let config_manager = Arc::new(SessionConfigManager::new(Arc::new(Mutex::new(
            Database::new(&std::path::PathBuf::from(":memory:")).unwrap(),
        ))));

        // 用户插件目录（dev 合入的第 3 参）：本用例无用户安装插件，指向空临时目录
        let user_plugins_dir = tmp_dir.path().join("user-plugins");
        let host = PluginHost::new(
            db,
            &plugins_dir,
            &user_plugins_dir,
            session_manager,
            config_manager,
            None,
        )
        .await;

        // 终态：两者均 Activated（应用插件的依赖检查在系统组件装配之后执行，
        // 激活成功即顺序成立的语义断言）
        let sys_info = host.get_plugin(sys_id).await.expect("system component present");
        let app_info = host.get_plugin(app_id).await.expect("app plugin present");
        assert_eq!(sys_info.state, PluginState::Activated, "system component state");
        assert_eq!(app_info.state, PluginState::Activated, "app plugin state");

        // 能力注册表：host-storage 由系统组件提供
        assert_eq!(
            host.wasm_host_ctx().capabilities().provider_kind("host-storage"),
            format!("system:{}", sys_id),
            "host-storage must be assembled to system component at boot"
        );

        // 激活时序：系统组件不晚于应用插件（语义断言之上的时序佐证）
        let (sys_at, app_at) = {
            let plugins = host.plugins.read().await;
            (
                plugins.get(sys_id).and_then(|p| p.activated_at),
                plugins.get(app_id).and_then(|p| p.activated_at),
            )
        };
        match (sys_at, app_at) {
            (Some(sys_at), Some(app_at)) => assert!(sys_at <= app_at, "system component must activate first"),
            _ => panic!("both plugins must record activated_at"),
        }
    }

    // ==================== 审批门禁（ADR 0020 / 审计票 03） ====================

    /// 注册一个「用户 zip 安装」的 TS-only 插件，安装目录指向 `dir`
    async fn register_user_installed(host: &PluginHost, id: &str, dir: &Path, permissions: &[&str]) {
        let mut plugin = make_plugin(id, PluginSource::UserInstalled, PluginState::Loaded);
        plugin.manifest.permissions = permissions.iter().map(|p| p.to_string()).collect();
        plugin.extension_path = dir.to_string_lossy().to_string();
        host.plugins.write().await.insert(id.to_string(), plugin);
    }

    /// 无批准记录 → 拒绝激活、落 NeedsApproval、一位权限都不授予
    #[tokio::test]
    async fn user_installed_plugin_needs_approval_before_activation() {
        let host = setup_host().await;
        let id = "com.bedcode.test-needs-approval";
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(PLUGIN_MANIFEST_FILE), r#"{"id":"x"}"#).unwrap();
        register_user_installed(&host, id, tmp.path(), &["storage", "terminal:input"]).await;

        let err = host.activate_plugin(id, false).await.unwrap_err();
        assert!(
            err.to_string().contains("requires user approval"),
            "必须显性报告「需人工批准」，实际: {err}"
        );
        assert!(!host.is_activated(id).await, "未批准的插件不得进入 Activated");
        assert_eq!(
            host.get_plugin(id).await.expect("plugin present").state,
            PluginState::NeedsApproval,
            "拒绝激活必须落 NeedsApproval（前端据此显示审批入口）"
        );
        assert!(
            host.permission().get_granted(id).is_empty(),
            "未批准路径不得授予任何权限（曾出现 storage 恒授予的旁路）"
        );
    }

    /// 批准后激活 → 生效权限 = 批准 ∩ 请求，且词汇表外的声明被丢弃
    #[tokio::test]
    async fn approve_then_activate_grants_only_effective_permissions() {
        let host = setup_host().await;
        let id = "com.bedcode.test-approve";
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(PLUGIN_MANIFEST_FILE), r#"{"id":"x"}"#).unwrap();
        register_user_installed(
            &host,
            id,
            tmp.path(),
            &["storage", "not:a:permission", "terminal:input"],
        )
        .await;

        // 先撞一次门禁：状态落到待授权（真实用户路径）
        assert!(host.activate_plugin(id, false).await.is_err());

        let approved = host.approve_plugin(id).await.expect("approve ok");
        assert_eq!(
            approved,
            vec!["storage".to_string(), "terminal:input".to_string()],
            "批准清单只含词汇表内的声明位（保持声明顺序、丢弃装饰词汇）"
        );
        assert_eq!(
            host.get_plugin(id).await.expect("plugin present").state,
            PluginState::Deactivated,
            "批准只解除闸门：状态从待授权复位为未启用"
        );

        host.activate_plugin(id, false).await.expect("激活成功");
        assert_eq!(
            host.get_plugin(id).await.expect("plugin present").state,
            PluginState::Activated
        );
        let granted = host.permission().get_granted(id);
        assert!(granted.contains("storage"));
        assert!(granted.contains("terminal:input"));
        assert!(
            !granted.contains("not:a:permission"),
            "词汇表外的声明不得进入生效集，实际: {granted:?}"
        );
    }

    /// 批准集严格小于请求集 → 生效集跟着收窄（交集语义必须落到权限管理器）
    #[tokio::test]
    async fn activation_grants_only_approved_subset_of_declared() {
        use crate::plugin::security::approval::{compute_dir_hash, PluginApprovalStore};

        let host = setup_host().await;
        let id = "com.bedcode.test-partial-approval";
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(PLUGIN_MANIFEST_FILE), r#"{"id":"x"}"#).unwrap();
        register_user_installed(&host, id, tmp.path(), &["storage", "terminal:input", "process:run"]).await;

        // 只批准 storage（按位批准的记录形态：批准集是请求集的真子集）
        let hash = compute_dir_hash(tmp.path()).expect("hash ok");
        PluginApprovalStore::new(host.storage().clone())
            .approve(id, &["storage".to_string()], &hash, "1.0.0")
            .await
            .expect("seed approval");

        host.activate_plugin(id, false).await.expect("已批准 → 可激活");
        assert_eq!(
            host.permission().get_granted(id),
            HashSet::from(["storage".to_string()]),
            "生效集必须是批准 ∩ 请求，未批准的声明位不得因 manifest 声明而生效"
        );
    }

    /// 批准后目录内容变化 → 拒绝激活、撤销批准、回 NeedsApproval
    #[tokio::test]
    async fn approval_revoked_when_plugin_content_changes() {
        let host = setup_host().await;
        let id = "com.bedcode.test-hash-pin";
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(PLUGIN_MANIFEST_FILE), r#"{"id":"x"}"#).unwrap();
        std::fs::write(tmp.path().join("index.js"), "console.log(1)").unwrap();
        register_user_installed(&host, id, tmp.path(), &["storage"]).await;

        host.approve_plugin(id).await.expect("approve ok");

        // 批准之后替换插件代码（在位冒名顶替）
        std::fs::write(tmp.path().join("index.js"), "console.log('evil')").unwrap();

        let err = host.activate_plugin(id, false).await.unwrap_err();
        assert!(err.to_string().contains("requires user approval"), "实际: {err}");
        assert_eq!(
            host.get_plugin(id).await.expect("plugin present").state,
            PluginState::NeedsApproval
        );
        let record = crate::plugin::security::approval::PluginApprovalStore::new(host.storage().clone())
            .get(id)
            .await
            .expect("read approvals");
        assert!(record.is_none(), "内容不匹配必须撤销批准记录");
        assert!(host.permission().get_granted(id).is_empty());
    }

    /// 私有库文件（plugin.db）运行期变化不得触发撤销：启用 → 停用 → 再启用仍可激活
    #[tokio::test]
    async fn runtime_private_db_does_not_invalidate_approval() {
        let host = setup_host().await;
        let id = "com.bedcode.test-private-db";
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(PLUGIN_MANIFEST_FILE), r#"{"id":"x"}"#).unwrap();
        register_user_installed(&host, id, tmp.path(), &["storage"]).await;
        host.approve_plugin(id).await.expect("approve ok");

        // 首次启用会创建私有库；停用后再启用必须仍通过审批门禁
        host.activate_plugin(id, false).await.expect("首次激活");
        std::fs::write(tmp.path().join("plugin.db"), b"SQLite format 3").unwrap();
        host.deactivate_plugin(id, false).await.expect("停用");
        host.activate_plugin(id, false).await.expect("再次激活不得被误判为内容被替换");
    }

    /// 信任分档：随包来源（内置 WASM / 文件扫描 / 静态注册）免审批
    #[tokio::test]
    async fn builtin_source_skips_approval_gate() {
        let host = setup_host().await;
        let id = "com.bedcode.test-builtin-scan";
        // 目录不存在也不触发哈希计算 —— 免审批来源根本不走审批门禁
        let mut plugin = make_plugin(id, PluginSource::FileScan, PluginState::Loaded);
        plugin.extension_path = "/nonexistent/plugins/com.bedcode.test-builtin-scan".to_string();
        host.plugins.write().await.insert(id.to_string(), plugin);

        host.activate_plugin(id, false).await.expect("内置来源免审批");
        assert!(host.is_activated(id).await);
    }

    /// 对免审批来源调用批准 → 显性报错（不做无意义写入）
    #[tokio::test]
    async fn approve_rejects_trusted_source() {
        let host = setup_host().await;
        let id = "com.bedcode.test-builtin-approve";
        host.plugins
            .write()
            .await
            .insert(id.to_string(), make_plugin(id, PluginSource::FileScan, PluginState::Loaded));

        let err = host.approve_plugin(id).await.unwrap_err();
        assert!(err.to_string().contains("approval is not required"), "实际: {err}");
    }
}
