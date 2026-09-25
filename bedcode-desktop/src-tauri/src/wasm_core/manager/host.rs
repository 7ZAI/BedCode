//! Plugin Host
//!
//! 插件宿主 — 生命周期管理（加载/激活/停用）
//! 协调 loader、permission、registry、storage、wasm_runtime 五个子系统
//! 支持静态注册（Rust 插件 via inventory）、文件扫描（TS-only 插件）和 WASM 模块（Rust+TS 插件）

use crate::db::Database;
use crate::wasm_core::manager::loader::PluginLoader;
use crate::wasm_core::manager::registry::PluginRegistry;
use crate::wasm_core::storage::PluginStorage;
use crate::wasm_core::manager::types::{DesktopPluginInfo, LoadedPlugin, PluginSource};
use crate::wasm_core::manager::runtime::{LoadedWasmPlugin, WasmHostContext, WasmRuntime};
use crate::wasm_core::permission::PermissionManager;
use crate::system::constants::{
    LIFECYCLE_SHUTDOWN, LIFECYCLE_STARTUP, PLUGIN_CALLBACK_TIMEOUT_SECS, PLUGIN_MANIFEST_FILE,
};
use bedcode_plugin_api::{PluginKind, PluginState, WasiPreopenDir, WsEndpointContribution};
use chrono::Utc;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

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
    message_bus: Arc<crate::wasm_core::bus::MessageBus>,
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
    frontend_channel: Arc<crate::wasm_core::security::frontend_channel::FrontendChannelRegistry>,
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
        let message_bus = Arc::new(crate::wasm_core::bus::MessageBus::new());

        let wasm_host_ctx = Arc::new(WasmHostContext::new(
            db.clone(),
            Arc::new(Mutex::new(HashMap::new())),
            storage.clone(),
            app_handle,
            permission.clone(),
            wasm_runtime.fs_auth().clone(),
            message_bus.clone(),
            Arc::new(crate::wasm_core::manager::capability::CapabilityRegistry::new()),
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

        // 2. 扫描文件系统的 TS-only 和 WASM 插件：随包内置目录 + 用户插件目录
        // （zip 安装，可卸载），共用同一套加载与 WASM 实例化逻辑。**去重状态跨
        // 两次扫描共享**（内置先到先得）：同 id 的用户副本被拒绝，不得顶替内置
        // 条目——分开两次 load_all 各持一份 seen_ids，用户副本会静默覆盖内置记录，
        // 随包插件随即被降级为 UserInstalled 而卡在审批门禁（拒绝激活）
        let (file_plugins, user_plugins) =
            PluginLoader::load_builtin_and_user(plugins_dir, user_plugins_dir, &permission);
        tracing::info!("[PluginHost] Found {} file-based plugin(s)", file_plugins.len());
        tracing::info!("[PluginHost] Found {} user-installed plugin(s)", user_plugins.len());

        // 3. 合并所有插件
        let mut all_plugins: HashMap<String, LoadedPlugin> = HashMap::new();

        // 添加静态注册的 Rust 插件
        for entry in static_plugins {
            let manifest = (entry.create_manifest)();
            let plugin_id = manifest.id.clone();

            // 授权结果只落在 PermissionManager（唯一真源）；LoadedPlugin 不再镜像
            // 一份 granted 列表（票 11 第 4 项：镜像字段只写不读）
            permission.grant_permissions(&plugin_id, &manifest.permissions);

            // 内置常驻语义：随二进制分发、无独立启停，注册即激活。
            // 直接置 Activated 使 notify_startup 的 on_startup 回调与
            // invoke_rust_command 的身份门禁对其真实生效（此前停在 Loaded 态、
            // 永不激活，与 "Static plugin loaded" 日志自相矛盾）
            let loaded = LoadedPlugin {
                manifest,
                state: PluginState::Activated,
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
            // WASM 实例化收为一条路径（票 11 第 2 项）：声明了 `rust_library` 的插件
            // 按同一函数实例化，未声明的纯前端插件走它的空分支（无实例、原记录入表）；
            // 文件缺失 / 加载失败 → Error 态入表（manifest 仍注册，列表可见可诊断）
            let (entry, wasm_instance) = Self::instantiate_wasm_plugin(&wasm_runtime, &wasm_host_ctx, &loaded);
            if let Some(instance) = wasm_instance {
                wasm_plugins_map.insert(id.clone(), instance);
            }
            all_plugins.insert(id, entry);
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
            frontend_channel: Arc::new(crate::wasm_core::security::frontend_channel::FrontendChannelRegistry::new()),
        };

        // 两阶段初始化：将 PluginHost（作为 PluginServices 实现）注入 WasmHostContext
        // 必须在 auto_activate 之前完成，否则 host_session_lifecycle_register 无法获取宿主服务
        host.wasm_host_ctx().set_services(Arc::new(host.clone())).await;

        // 两阶段注入：host-task 执行引擎（core-task）+ 单元执行器注册表（C3/C4）
        // host_api/task.rs 经 TaskEngine 接口调用 core-task；execute_unit 经 UnitExecutor
        // 注册表分发——manager::task 不再直调 host_api 域函数，host_api 不再依赖 manager
        host.wasm_host_ctx()
            .set_task_engine(Arc::new(crate::wasm_core::manager::task::CoreTaskEngine))
            .await;
        crate::wasm_core::manager::task::register_unit_executor(Arc::new(
            crate::wasm_core::host_api::fs::FsUnitExecutor,
        ));
        crate::wasm_core::manager::task::register_unit_executor(Arc::new(
            crate::wasm_core::host_api::process::ProcessUnitExecutor,
        ));
        crate::wasm_core::manager::task::register_unit_executor(Arc::new(
            crate::wasm_core::host_api::http::HttpUnitExecutor,
        ));

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

    /// 扫描导出 `auth-policy` 能力的激活插件（认证中心角色发现，HTTP 路由代码注册
    /// 下沉专项阶段 3）：返回运行中（Activated / Degraded）且实例化时探测到
    /// `auth-policy` 能力导出的插件 id 清单（按 id 升序，确定性）。
    ///
    /// 取代认证中心角色的硬编码插件 id：任何导出该能力的激活插件都可能是认证中心；
    /// 空清单 = 无认证中心（宿主策略回退）。
    pub async fn auth_center_candidates(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        let wasm_plugins = self.wasm_plugins.read().await;
        let mut out = Vec::new();
        for (id, instance) in wasm_plugins.iter() {
            // 仅运行中的实例可作为认证中心（停用/未激活的实例不参与策略裁决）
            let running = plugins
                .get(id)
                .is_some_and(|p| matches!(p.state, PluginState::Activated | PluginState::Degraded(_)));
            if !running {
                continue;
            }
            let exported = {
                let guard = instance.lock().await;
                guard.exported_capabilities().to_vec()
            };
            if exported.iter().any(|c| c == crate::wasm_core::manager::capability::CAP_AUTH_POLICY) {
                out.push(id.clone());
            }
        }
        out.sort();
        tracing::debug!(candidates = ?out, "auth-policy capability candidates");
        out
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
    pub fn frontend_channel(&self) -> &Arc<crate::wasm_core::security::frontend_channel::FrontendChannelRegistry> {
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
    pub fn message_bus(&self) -> &Arc<crate::wasm_core::bus::MessageBus> {
        &self.message_bus
    }

    /// 初始化消息总线 dispatcher（必须在 new() 之后调用）
    pub async fn init_message_bus(&self) {
        let dispatcher: Arc<dyn crate::wasm_core::bus::MessageDispatcher> = Arc::new(self.clone());
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
pub mod api_bridge;
mod app_cli;
mod boot;
mod commands;
mod errors;
mod install;
mod preauth;
mod register;
mod services;
mod wasm;
// 保持原导出路径（crate::wasm_core::manager::host::PluginLifecycleListener 等）
// 票 03：插件侧会话生命周期 / 输入行监听器实现（原 `listeners` 模块）已删除——
// 宿主不再派发这两类回调，注册面与派发点同批退役。
// preauth 域（P2 拆分后 re-export 保持 host:: 路径兼容）
pub use preauth::register_preauth_provider;
#[allow(unused_imports)] // 兼容 host:: 路径（测试经 super:: 引用；preauth.rs 内部自用）
pub(crate) use preauth::{collect_preauth_paths, preauth_providers, PreauthProvider, PREAUTH_PATHS_STORAGE_KEY};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::bus::{MessageBus, MessageDispatcher};
    use crate::wasm_core::manager::runtime::PluginServices;
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

    // 用例与脚手架按域拆分（票 11 第 7 项）：内联块只留 imports / 测试常量 / 子模块声明。
    // 模块树 `host::tests::<文件>` 与内联形态等价；各域文件经 `use super::*;` 拿到宿主项
    // 与测试常量，跨文件复用的脚手架另按需显式引入（顶层项已标 `pub(super)`）。
    mod approval_test;
    mod commands_test;
    mod contributions_test;
    mod host_api_test;
    mod lifecycle_test;
    mod runtime_preauth_test;
    mod scaffold;
    mod scan_dedup_test;
    mod system_component_test;
    mod wasm_flow_test;
}
