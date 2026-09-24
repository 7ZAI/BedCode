//! 宿主上下文（host-api 消费方家园，spec 票 04）
//!
//! `WasmHostContext`（宿主子系统引用聚合）、`PluginServices`（插件宿主服务抽象，
//! 两阶段注入解 PluginHost 互引）、`ProcessRegistry` / `kill_process_group`
//! （host-process 进程注册/终止，v8）原定义于 `crate::wasm_core::manager::runtime`，
//! 迁入本模块使 host_api 消费方不再反向 import manager（见
//! `.scratch/2026-09-24-wasm-core-decouple/spec.md` C1/C5）。
//!
//! **capability 端口（票 04）**：宿主上下文对 manager 的唯一剩余类型依赖（能力
//! 注册表具体类型）经 [`CapabilityProvider`] trait 消除——WasmHostContext 持有
//! `Arc<dyn CapabilityProvider>`，host_api 只经 trait 消费能力路由；具体实现
//! （manager::capability 的注册表）在 manager 侧直连（方向合法）。唯一保留的
//! manager 引用：trait 签名涉 `LoadedWasmPlugin`（wasmtime 装配域类型，按票 04
//! 规则「方法签名涉 Runtime 类型时保持对该类型的引入」）。
//!
//! `manager/runtime.rs` 保留 `pub use` 再导出（manager→host_api 合法方向），
//! 历史路径编译绿直至迭代清理。

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::db::Database;
use crate::wasm_core::manager::runtime::LoadedWasmPlugin;
use crate::wasm_core::permission::PermissionManager;
use crate::wasm_core::security::fs_auth::FsAuthChecker;
use crate::wasm_core::storage::PluginStorage;

// tauri::Manager：`AppHandle::path()` 扩展（get_or_create_plugin_db 数据目录派生）
use tauri::Manager;

/// 插件宿主服务抽象 — 解耦 WasmHostContext 与 PluginHost 的循环依赖
///
/// WasmHostContext 需要回调插件宿主（注册会话生命周期监听器），
/// 而 PluginHost 持有 WasmHostContext —— 通过 trait 对象 + 两阶段注入打破类型互引：
/// 本模块只依赖此 trait，`PluginHost` 在 `wasm_core::manager::host` 模块中实现它
pub trait PluginServices: Send + Sync + 'static {
    // 票 03 删除的两条注册面（`register_session_lifecycle_listener` /
    // `register_session_input_listener`）：宿主侧观察注册表与派发点一并退役，
    // 见 `session/session_manager.rs` 的「观察面（票 03 已退役）」节。

    /// 标记插件为错误状态
    ///
    /// 仅通知前端弹窗提示，不改变插件状态（保持激活，会话照常运行）。
    /// 由 `host_mark_plugin_error` Host Function 转发，插件自身检测到
    /// 配置失败（如 hooks 脚本拷贝失败）时调用。
    fn mark_plugin_error(&self, plugin_id: String, error: String);

    /// 为指定插件注册宿主周期定时器（v6，ADR 0003）
    ///
    /// 宿主按 interval_secs 到点调用插件的 command（附当前时间参数），
    /// 幂等判断归插件。重复注册替换该插件已有定时器。
    fn register_plugin_timer(&self, plugin_id: String, interval_secs: u64, command: String);

    /// 分发进程执行完成事件到插件（host-process，v8）
    ///
    /// 由 host_impl/process.rs 在进程结束时调用：经插件 export
    /// `on_process_done` 投递 `{ run_id, exit_code, timed_out }`。
    /// 插件未激活/已卸载时调用失败，仅记日志（尽力而为）。
    fn dispatch_process_done(&self, plugin_id: String, event: serde_json::Value);

    /// 分发宿主并发任务事件到插件（host-task，v20）
    ///
    /// 由 core-task 消费派发任务调用（每插件单线程串行，实例锁天然要求）：
    /// 经可选导出 `events-task#on-task-event` 投递 `{ jobId, phase, ... }`；
    /// 未导出（旧 SDK 产物）时降级 `Ok(false)` → 事件丢弃 + 计数（宿主不缓存，
    /// `status` / `list-jobs` 自愈）。插件未激活/已卸载时调用失败仅记日志
    /// （尽力而为，同 dispatch_process_done）。
    fn dispatch_task_event(&self, plugin_id: String, event: serde_json::Value);

    /// 安装插件随包 CLI（host-app，v8）：复制到用户 bin 目录 + 注册 PATH（幂等）
    ///
    /// 源 = 插件包目录 `cli/<file-name>`（Windows 自动补 .exe）；
    /// `bin_dir` 为空用平台默认。返回安装后的 bin 目录绝对路径。
    /// 由 host_impl/app.rs 经 block_on_async 驱动（宿主侧注册表/PATH 实现）。
    /// 返回 Box<dyn Future> 保持 trait dyn 兼容（async fn 会破坏 Arc<dyn>）。
    fn install_cli(
                &self,
        plugin_id: String,
        file_name: String,
        bin_dir: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>>;

    /// 卸载插件随包 CLI（host-app，v8）：删文件 + 移除仅本插件的 PATH 条目（幂等）
    ///
    /// 应用关闭流程（deactivate_all 置位 shutting_down）中调用时自动跳过，
    /// CLI 随下次激活重新安装。
    fn uninstall_cli(
                &self,
        plugin_id: String,
        file_name: String,
        bin_dir: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>>;

    /// 插件自身资源目录（host-app，v25 函数级追加，**无权限门**）：返回插件安装
    /// 目录绝对路径（`extension_path` 剥离 verbatim 前缀，与生命周期事件 payload
    /// 的 `resource_dir` 同值）。由 host_impl/app.rs 经 block_on_async 驱动。
    /// 插件未加载 → `Err`（不静默返回空串，调用方据此显性失败）。
    fn plugin_resource_dir(
                &self,
        plugin_id: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + '_>>;
}

/// 能力查询/路由端口（票 04 ISP 化）
///
/// host_api 侧只经本 trait 消费能力路由（`manager::capability::forward_*` 与宿主
/// 上下文访问器），不接触注册表具体类型；具体注册表在 manager 侧实现本 trait。
/// 系统组件装配（register_system_component 等）仍以具体注册表在 manager
/// 侧直连（方向合法）。
///
/// 签名涉 `LoadedWasmPlugin`（wasmtime 装配域类型）——按票 04 规则「方法签名涉
/// Runtime 类型时保持对该类型的引入」保留此单一 manager 依赖，后续随装配域下沉
/// 一并处置。
pub trait CapabilityProvider: Send + Sync {
    /// 能力是否已有提供者（依赖检查用；未知能力名 = 无提供者）
    fn is_available(&self, name: &str) -> bool;
    /// 返回依赖清单中未装配的能力名（依赖缺失报错用）
    fn missing(&self, dependencies: &[String]) -> Vec<String>;
    /// 注册系统组件为能力提供者（替换宿主原语，二选一装配）
    fn register_system_component(
        &self,
        name: &str,
        plugin_id: &str,
        instance: Arc<Mutex<LoadedWasmPlugin>>,
    ) -> crate::Result<()>;
    /// 能力回落宿主原语（仅当前提供者确为该插件时生效）
    fn revert_to_host(&self, name: &str, plugin_id: &str);
    /// 撤销某系统组件的全部能力提供（停用/重建前清理）
    fn revert_all_from(&self, plugin_id: &str);
    /// 查询路由目标：系统组件提供且调用方非提供者自身时返回（提供者 ID, 实例句柄）
    fn system_component_instance(
        &self,
        name: &str,
        caller_plugin_id: &str,
    ) -> Option<(String, Arc<Mutex<LoadedWasmPlugin>>)>;
    /// 当前提供者类别（诊断/测试用）
    fn provider_kind(&self, name: &str) -> String;
}

/// 宿主上下文（注入到 WasmPluginState）
///
/// 持有宿主子系统引用，Host Functions 通过此上下文访问宿主能力
/// plugin_services 使用两阶段初始化：new() 时为 None，PluginHost 构造完成后通过 set_services() 注入
pub struct WasmHostContext {
    /// 主库连接（host_impl 域函数访问，host_api 迁移后 pub(crate)）
    pub(crate) db: Arc<Mutex<Database>>,
    /// 插件独立数据库池 — 每插件一个独立 .db 文件和连接
    pub(crate) plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
    pub(crate) storage: Arc<PluginStorage>,
    /// 密钥托管读缓存（v15 host-auth）：read-through（get 命中直接返回，
    /// set/delete 失效对应键）；真源为主库 plugin_secrets 表（重启后一致）
    pub(crate) secrets_cache: Arc<std::sync::RwLock<std::collections::HashMap<(String, String), String>>>,
    /// Tauri AppHandle（无头/测试上下文为 None，emit/路径类宿主能力降级）
    pub(crate) app_handle: Option<Arc<tauri::AppHandle>>,
    /// 插件私有库根目录覆盖（布局 `<root>/<plugin_id>/plugin.db`）
    ///
    /// 生产为 `None`：走 `app_handle` 的 `app_data_dir()/plugins/<plugin_id>`；
    /// 无头测试经 [`Self::set_plugin_db_root`] 注入（tao 事件循环不允许在测试
    /// 线程建 AppHandle，故测试只能经注入点拿到真实私有库——票 08 的 S1 闭环
    /// 需要它）。
    plugin_db_root: Arc<std::sync::RwLock<Option<PathBuf>>>,
    pub(crate) permission: Arc<PermissionManager>,
    pub(crate) fs_auth: Arc<FsAuthChecker>,
    pub(crate) message_bus: Arc<crate::wasm_core::bus::MessageBus>,
    /// 插件宿主服务（两阶段初始化，避免 PluginHost 与 WasmHostContext 类型互引）
    plugin_services: Arc<RwLock<Option<Arc<dyn PluginServices>>>>,
    /// 运行中进程注册表（host-process，v8）：run_id → 进程句柄
    ///
    /// host_impl/process.rs 注册/移除；kill（超时/取消）经此查找句柄。
    process_registry: Arc<ProcessRegistry>,
    /// 插件互调 api 注册表（ADR-0017）：激活登记 / 停用注销，
    /// `bus_publish` 对 `bedcode.api.*` 请求 topic 做目标校验
    api_registry: Arc<crate::wasm_core::security::api_registry::ApiRegistry>,
    /// 统一授权框架（core-security）：三段决策管线 + 仲裁器注册表
    security: crate::wasm_core::security::SecurityFramework,
    /// 能力注册表（core-plugin-manager 票据 06）：能力名 → 宿主原语/系统组件
    /// 实例（二选一装配）；host_impl 宿主函数内经此路由
    capabilities: Arc<dyn CapabilityProvider>,
}

/// 运行中的进程（记录 pid 供进程组 kill）
///
/// Child 句柄由执行任务（host_impl/process.rs）独占持有：`Child::wait`
/// 在整个进程生命周期内独占 `&mut self`，注册表若同时持句柄，kill 路径
/// 将阻塞到进程自然退出（死锁）；按 pid 杀进程组则与 wait 无冲突。
struct RunningProcess {
    /// 发起执行的插件 ID（完成事件分发目标）
    plugin_id: String,
    /// 子进程 pid（process_group(0)/CREATE_NEW_PROCESS_GROUP 后为进程组组长）
    pid: u32,
}

/// 进程注册表（run_id → 运行中进程）
///
/// 生命周期：`process_run` 注册 → 进程结束/kill 后移除。
/// 应用退出时进程由 OS 回收（孤儿进程随宿主进程终止）。
pub struct ProcessRegistry {
    runs: std::sync::RwLock<HashMap<String, RunningProcess>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        Self {
            runs: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 注册运行中进程（run_id 由调用方预生成，UUID）
    ///
    /// 同步锁：临界区仅 map 操作（无 await），wasm host 调用栈内直接可用
    pub fn register(&self, run_id: String, plugin_id: String, pid: u32) {
        let mut runs = self.runs.write().unwrap_or_else(|e| e.into_inner());
        runs.insert(run_id, RunningProcess { plugin_id, pid });
    }

    /// 移除并返回进程的发起插件 ID（进程结束/kill 后调用）
    pub fn remove(&self, run_id: &str) -> Option<String> {
        let mut runs = self.runs.write().unwrap_or_else(|e| e.into_inner());
        runs.remove(run_id).map(|p| p.plugin_id)
    }

    /// 终止进程组（尽力而为）：找到记录则按 pid 杀进程组，返回是否找到
    pub async fn kill(&self, run_id: &str) -> bool {
        let pid = {
            let runs = self.runs.read().unwrap_or_else(|e| e.into_inner());
            match runs.get(run_id) {
                Some(proc) => proc.pid,
                None => return false,
            }
        };
        kill_process_group(pid).await;
        true
    }

    /// 运行中进程数（并发限制/诊断用）
    pub fn running_count(&self) -> usize {
        let runs = self.runs.read().unwrap_or_else(|e| e.into_inner());
        runs.len()
    }
}

/// 终止进程组（尽力而为，超时 kill 与插件取消共用）
///
/// - unix：`kill -9 -<pgid>`（`process_group(0)` 保证 pgid == pid）
/// - Windows：`taskkill /F /T /PID`（/T 连带子进程树）
///
/// 返回是否成功发起（进程已退出 / pid 无效返回 false，属预期内竞态）。
pub(crate) async fn kill_process_group(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = tokio::process::Command::new("taskkill");
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        // CREATE_NO_WINDOW：taskkill 为控制台程序，避免超时杀进程时黑窗闪烁
        cmd.creation_flags(0x0800_0000);
        match cmd.output().await {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                tracing::warn!(
                    pid,
                    output = %String::from_utf8_lossy(&o.stderr),
                    "kill_process_group: taskkill reported failure"
                );
                false
            }
            Err(e) => {
                tracing::warn!(pid, error = %e, "kill_process_group: taskkill failed");
                false
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 直接系统调用杀进程组（负 pgid = 组；process_group(0) 后组 id == 进程 pid）。
        // 不使用外部 kill 命令：命令进程是第二个 tokio child，在
        // current_thread（sh 的 wait）与 ambient multi_thread（kill 命令）双 runtime
        // 共享全局 SIGCHLD handler 的场景下，kill 命令退出与目标被杀同时发生时，
        // 两个 reaper 竞争 waitpid(-1) 回收 zombie，可能吞掉 sh 的退出通知导致
        // wait() 永久挂起（CI flaky：process_kill_terminates_process_group）。
        // 同步系统调用不产生 child，从根上消除该竞争。
        let rc = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if rc == 0 {
            return true;
        }
        let err = std::io::Error::last_os_error();
        tracing::warn!(
            pid,
            error = %err,
            "kill_process_group: kill(-pid) failed"
        );
        false
    }
}

impl WasmHostContext {
    /// 创建宿主上下文
    ///
    /// `app_handle` 为 None 时（无头/测试上下文）emit、数据目录等能力不可用。
    /// 第 8 参 `capabilities`（票 04 DIP 注入）：宿主上下文持 trait 对象，不依赖
    /// manager 具体注册表；重构为 Builder 留后续票。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<Mutex<Database>>,
        plugin_dbs: Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>,
        storage: Arc<PluginStorage>,
        app_handle: Option<Arc<tauri::AppHandle>>,
        permission: Arc<PermissionManager>,
        fs_auth: Arc<FsAuthChecker>,
        message_bus: Arc<crate::wasm_core::bus::MessageBus>,
        capabilities: Arc<dyn CapabilityProvider>,
    ) -> Self {
        let api_registry = Arc::new(crate::wasm_core::security::api_registry::ApiRegistry::new());
        // 统一授权框架：注册既有资源的仲裁器（fs 三层校验 / api-call 互调门）
        let security = crate::wasm_core::security::SecurityFramework::new();
        security.register(Arc::new(crate::wasm_core::security::framework::FsAuthorizer::new(
            permission.clone(),
            fs_auth.clone(),
        )));
        security.register(Arc::new(crate::wasm_core::security::framework::ApiCallAuthorizer::new(
            api_registry.clone(),
        )));
        Self {
            db,
            plugin_dbs,
            storage,
            secrets_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            app_handle,
            permission,
            fs_auth,
            message_bus,
            plugin_services: Arc::new(RwLock::new(None)),
            process_registry: Arc::new(ProcessRegistry::new()),
            api_registry,
            security,
            capabilities,
            // 私有库根目录覆盖：生产 None（走 app_handle 的 app_data_dir），
            // 无头测试在构造后经 `set_plugin_db_root` 注入（见 setup_wasm_runtime）
            plugin_db_root: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// 插件私有库根目录覆盖：生产为 `None`（走 `app_handle` 的
    /// `app_data_dir()/plugins/<plugin_id>`）；无头/集成测试无 AppHandle
    /// （tao 事件循环不允许测试线程建 AppHandle），激活会话中心等插件前经此
    /// 注入临时根目录以获得真实私有库——认证记录下沉（v24）后配对/历史真源
    /// 在插件私有库，无私有库的无头上下文无法驱动认证链路。
    pub fn set_plugin_db_root(&self, root: Option<PathBuf>) {
        *self.plugin_db_root.write().unwrap_or_else(|e| e.into_inner()) = root;
    }

    /// 读取插件私有库根目录覆盖（None = 走 app_handle 派生）
    fn plugin_db_root_opt(&self) -> Option<PathBuf> {
        self.plugin_db_root
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// 获取进程注册表引用（host-process）
    /// 文件系统访问校验器（票 07：闭环用例预置「已记住」授权记录的唯一入口）
    pub(crate) fn fs_auth(&self) -> &Arc<FsAuthChecker> {
        &self.fs_auth
    }

    /// 权限管理器（core-task 单元预检读声明闸门用，见 `manager::task`）
    pub(crate) fn permission(&self) -> &Arc<crate::wasm_core::permission::PermissionManager> {
        &self.permission
    }

    pub fn process_registry(&self) -> &Arc<ProcessRegistry> {
        &self.process_registry
    }

    /// 获取主库句柄（宿主侧读写内核表的入口）
    ///
    /// 生产路径经各 host_impl 域函数访问（权限门 + 属主校验在域函数内）；
    /// 本访问器供宿主装配/测试直接读写内核真源（如 `pairings` 表播种与断言，
    /// 见 `host::tests::test_server_auth_policy_closed_loop`）。
    pub(crate) fn database(&self) -> &Arc<Mutex<Database>> {
        &self.db
    }

    /// 两阶段初始化：PluginHost 构造完成后注入宿主服务
    ///
    /// 必须在 PluginHost::new() 返回后、任何插件 activate 之前调用
    pub async fn set_services(&self, services: Arc<dyn PluginServices>) {
        *self.plugin_services.write().await = Some(services);
    }

    /// 获取宿主服务引用
    ///
    /// 在两阶段初始化完成前返回 None
    pub async fn services(&self) -> Option<Arc<dyn PluginServices>> {
        self.plugin_services.read().await.clone()
    }

    /// 获取消息总线引用
    pub fn message_bus(&self) -> &Arc<crate::wasm_core::bus::MessageBus> {
        &self.message_bus
    }

    /// 获取插件互调 api 注册表引用（ADR-0017 门禁）
    pub fn api_registry(&self) -> &Arc<crate::wasm_core::security::api_registry::ApiRegistry> {
        &self.api_registry
    }

    /// 获取统一授权框架引用（core-security 三段决策管线）
    pub fn security(&self) -> &crate::wasm_core::security::SecurityFramework {
        &self.security
    }

    /// 宿主侧互调调用（票 11 命令面桥接）：以宿主虚拟身份发布 JSON-RPC 请求到
    /// `bedcode.api.<api>` topic 并等待回复。
    ///
    /// 与插件 mutual 调用的区别：调用方身份为 [`host_impl::api::HOST_API_CALLER_ID`]
    /// （宿主不是插件，reply topic 路由 `bedcode.api.reply.<caller>.<id>` 用）；
    /// 互调门禁（ADR 0017 层 1）只校验目标 api 已声明、不校验调用方——未激活
    /// 插件在注册表无登记 → 门禁拒绝 → 调用方降级宿主实现。
    pub fn call_plugin_api_host(
        &self,
        request_topic: &str,
        payload_json: &str,
        timeout_ms: u64,
    ) -> Result<String, String> {
        crate::wasm_core::host_api::api::api_call(
            self,
            self,
            self,
            crate::wasm_core::host_api::api::HOST_API_CALLER_ID,
            request_topic,
            payload_json,
            timeout_ms,
        )
    }

    /// 获取能力注册表引用（core-plugin-manager：系统组件装配与能力路由）
    pub fn capabilities(&self) -> &Arc<dyn CapabilityProvider> {
        &self.capabilities
    }

    /// 丢弃插件独立数据库连接（卸载时调用，dev 合入的卸载完整性）
    ///
    /// 仅从连接池移除（不删库文件）：删除插件目录前须先释放文件句柄，
    /// 否则在部分平台（Windows）会因文件仍被占用而删不掉
    pub async fn drop_plugin_db(&self, plugin_id: &str) {
        let dropped = self.plugin_dbs.lock().await.remove(plugin_id).is_some();
        if dropped {
            tracing::debug!(plugin_id = %plugin_id, "Plugin database connection dropped");
        }
    }

    /// 应用句柄引用；无头 / 测试上下文为 None
    ///
    /// 存在的理由（审计票 12）：`app_handle` 字段是模块私有的，而 peer-net 节点的
    /// **属主清理钩子**在 `manager/host/activation.rs`——那里不是本模块的后代，读不到
    /// 字段。此前那条路走 `AppContext::try_global()`，但 boot 装配期全局尚未注册，
    /// 「插件在 activate 里起了节点、随后激活失败」这个窗口就拿不到句柄去收，
    /// 所以直接从上下文字段取（它在 `PluginHost::new` 之前就已就位）
    pub(crate) fn app_handle(&self) -> Option<&tauri::AppHandle> {
        self.app_handle.as_deref()
    }

    /// 获取或懒加载插件独立数据库
    ///
    /// 首次调用时创建目录 + 打开/创建 plugin.db + 缓存连接
    /// 后续调用直接返回缓存的连接
    pub async fn get_or_create_plugin_db(&self, plugin_id: &str) -> crate::Result<Arc<Mutex<Database>>> {
        // 快速路径：已缓存
        {
            let dbs = self.plugin_dbs.lock().await;
            if let Some(db) = dbs.get(plugin_id) {
                return Ok(db.clone());
            }
        }

        // 慢路径：创建数据库。数据目录来源二选一（`aot_cache_dir` 同模式）：
        // - 生产：`app_handle` 派生 `app_data_dir()/plugins/<plugin_id>`
        // - 无头测试：`plugin_db_root` 注入（tao 事件循环不允许在测试线程建
        //   AppHandle，故无头上下文必须显式给根目录才能测插件私有库）
        let plugin_dir = match (&self.app_handle, self.plugin_db_root_opt()) {
            (Some(app_handle), _) => {
                let app_data_dir = app_handle
                    .path()
                    .app_data_dir()
                    .map_err(|e| crate::AppError::Plugin(format!("Failed to get app data dir: {}", e)))?;
                app_data_dir.join("plugins").join(plugin_id)
            }
            (None, Some(root)) => root.join(plugin_id),
            (None, None) => {
                return Err(crate::AppError::Plugin(
                    "plugin database unavailable in headless context (no app_handle)".to_string(),
                ))
            }
        };

        // 创建插件数据目录
        if !plugin_dir.exists() {
            std::fs::create_dir_all(&plugin_dir).map_err(|e| {
                crate::AppError::Plugin(format!(
                    "Failed to create plugin data dir '{}': {}",
                    plugin_dir.display(),
                    e
                ))
            })?;
        }

        let db_path = plugin_dir.join("plugin.db");
        let db = Database::new(&db_path)?;

        // 缓存连接
        let db_arc = Arc::new(Mutex::new(db));
        {
            let mut dbs = self.plugin_dbs.lock().await;
            // 双重检查：另一个线程可能已插入
            if let Some(existing) = dbs.get(plugin_id) {
                return Ok(existing.clone());
            }
            dbs.insert(plugin_id.to_string(), db_arc.clone());
        }

        tracing::info!(plugin_id = %plugin_id, path = %db_path.display(), "Plugin database created/opened");
        Ok(db_arc)
    }
}

// ==================== 角色接口（ISP，票 05 接口隔离） ====================
// 域函数签名只收自己消费的窄角色视图（`&dyn XxxScope`），不再人手一个
// `&WasmHostContext` 上帝对象；`WasmHostContext` 实现全部角色接口，调用点
// `ctx.as_ref()`（&WasmHostContext）自动 unsize coerce 到任一 `&dyn` 角色。
// trait upcasting（1.86+）下域内互调满足被调者 scope 需求即可。

/// 主库 / 插件独立库访问视图
pub trait DbScope: Send + Sync {
    /// 主库句柄（宿主侧读写内核真源；域函数内访问经权限门 + 属主校验）
    fn database(&self) -> &Arc<Mutex<Database>>;
    /// 插件独立库连接池
    fn plugin_dbs(&self) -> &Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>>;
    /// 获取或懒加载插件独立数据库（无头上下文须先注入 plugin_db_root，否则报错）。
    /// async 方法经 Pin<Box<dyn Future>> 返回（async-fn-in-trait 在 1.98 仍非 dyn
    /// 兼容，返回 boxed future 保持 trait object 化，票 05）
    fn get_or_create_plugin_db<'a>(
        &'a self,
        plugin_id: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = crate::Result<Arc<Mutex<Database>>>> + Send + 'a>>;
}

/// 权限仲裁视图（`check_permission` 经此取 PermissionManager）
pub trait PermissionScope: Send + Sync {
    /// 权限管理器（SDK VALID_PERMISSIONS 授权面）
    fn permission(&self) -> &Arc<crate::wasm_core::permission::PermissionManager>;
}

/// 插件存储视图（StorageScope，票 03 中立层）
pub trait StorageScope: Send + Sync {
    /// 插件持久化存储（SQLite plugin_storage 表）
    fn storage(&self) -> &Arc<crate::wasm_core::storage::PluginStorage>;
}

/// 文件系统访问校验视图
pub trait FsAuthScope: Send + Sync {
    /// 三层 fs 校验器（已授权路径判据，绝不从池线程触发弹窗）
    fn fs_auth(&self) -> &Arc<crate::wasm_core::security::fs_auth::FsAuthChecker>;
}

/// 消息总线视图
pub trait BusScope: Send + Sync {
    /// 内核消息总线（发布/订阅/事件广播）
    fn message_bus(&self) -> &Arc<crate::wasm_core::bus::MessageBus>;
}

/// Tauri AppHandle 视图（无头 / 测试上下文为 None，emit/路径类能力降级）
pub trait AppHandleScope: Send + Sync {
    /// 应用句柄；无头上下文 None
    fn app_handle(&self) -> Option<&tauri::AppHandle>;
}

/// 插件宿主服务视图（两阶段注入的 PluginServices trait 对象）
pub trait ServicesScope: Send + Sync {
    /// 宿主服务引用（两阶段初始化完成前返回 None；async：内部 async RwLock；
    /// Pin<Box<dyn Future>> 返回保持 dyn 兼容，票 05）
    fn services<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn crate::wasm_core::host_api::context::PluginServices>>> + Send + 'a>>;
}

/// 进程注册表视图（host-process，v8）
pub trait ProcessScope: Send + Sync {
    /// 运行中进程注册表（run_id → 进程句柄）
    fn process_registry(&self) -> &Arc<crate::wasm_core::host_api::context::ProcessRegistry>;
}

/// 插件互调 api 注册表视图（ADR-0017 门禁）
pub trait ApiRegistryScope: Send + Sync {
    /// 激活登记 / 停用注销的 api 注册表
    fn api_registry(&self) -> &Arc<crate::wasm_core::security::api_registry::ApiRegistry>;
}

/// 统一授权框架视图（core-security 三段决策管线）
pub trait SecurityScope: Send + Sync {
    /// 授权框架（authorize 决策 + 仲裁器注册表 + monitor 埋点）
    fn security(&self) -> &crate::wasm_core::security::SecurityFramework;
}

/// 能力路由视图（票 04 trait 化端口，host_api 只经 &dyn 消费）
pub trait CapabilityScope: Send + Sync {
    /// 能力查询/路由端口
    fn capabilities(&self) -> &Arc<dyn CapabilityProvider>;
}

/// 密钥托管读缓存视图（v15 host-auth；set/delete 失效对应键）
pub trait SecretsScope: Send + Sync {
    /// read-through 缓存（真源为主库 plugin_secrets 表）
    fn secrets_cache(&self) -> &Arc<std::sync::RwLock<std::collections::HashMap<(String, String), String>>>;
}

// ==================== WasmHostContext 角色接口实现 ====================

impl DbScope for WasmHostContext {
    fn database(&self) -> &Arc<Mutex<Database>> {
        self.database()
    }
    fn plugin_dbs(&self) -> &Arc<Mutex<HashMap<String, Arc<Mutex<Database>>>>> {
        &self.plugin_dbs
    }
    fn get_or_create_plugin_db<'a>(
        &'a self,
        plugin_id: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = crate::Result<Arc<Mutex<Database>>>> + Send + 'a>> {
        Box::pin(self.get_or_create_plugin_db(plugin_id))
    }
}

impl PermissionScope for WasmHostContext {
    fn permission(&self) -> &Arc<crate::wasm_core::permission::PermissionManager> {
        self.permission()
    }
}

impl StorageScope for WasmHostContext {
    fn storage(&self) -> &Arc<crate::wasm_core::storage::PluginStorage> {
        &self.storage
    }
}

impl FsAuthScope for WasmHostContext {
    fn fs_auth(&self) -> &Arc<crate::wasm_core::security::fs_auth::FsAuthChecker> {
        self.fs_auth()
    }
}

impl BusScope for WasmHostContext {
    fn message_bus(&self) -> &Arc<crate::wasm_core::bus::MessageBus> {
        self.message_bus()
    }
}

impl AppHandleScope for WasmHostContext {
    fn app_handle(&self) -> Option<&tauri::AppHandle> {
        self.app_handle()
    }
}

impl ServicesScope for WasmHostContext {
    fn services<'a>(
        &'a self,
    ) -> Pin<Box<dyn std::future::Future<Output = Option<Arc<dyn crate::wasm_core::host_api::context::PluginServices>>> + Send + 'a>> {
        Box::pin(self.services())
    }
}

impl ProcessScope for WasmHostContext {
    fn process_registry(&self) -> &Arc<crate::wasm_core::host_api::context::ProcessRegistry> {
        self.process_registry()
    }
}

impl ApiRegistryScope for WasmHostContext {
    fn api_registry(&self) -> &Arc<crate::wasm_core::security::api_registry::ApiRegistry> {
        self.api_registry()
    }
}

impl SecurityScope for WasmHostContext {
    fn security(&self) -> &crate::wasm_core::security::SecurityFramework {
        self.security()
    }
}

impl CapabilityScope for WasmHostContext {
    fn capabilities(&self) -> &Arc<dyn CapabilityProvider> {
        self.capabilities()
    }
}

impl SecretsScope for WasmHostContext {
    fn secrets_cache(&self) -> &Arc<std::sync::RwLock<std::collections::HashMap<(String, String), String>>> {
        &self.secrets_cache
    }
}
