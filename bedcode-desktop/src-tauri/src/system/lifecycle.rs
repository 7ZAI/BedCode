//! Application Lifecycle
//!
//! 应用生命周期钩子系统 — 核心模块和插件可注册回调到启动/关闭/窗口关闭等生命周期节点
//! 通过 LifecycleRegistry 全局单例注册，由 Tauri RunEvent 循环驱动执行

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::system::constants::lifecycle::HOOK_TIMEOUT_SECS;

/// 生命周期阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecyclePhase {
    /// 应用启动完成 — 所有服务初始化后触发
    Startup,
    /// 应用即将关闭 — 执行清理逻辑
    Shutdown,
    /// 窗口关闭请求 — 可阻止关闭
    WindowCloseRequested,
}

/// 异步钩子回调类型
type AsyncHookFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// 窗口关闭请求钩子回调类型
/// 返回 true 允许关闭，返回 false 阻止关闭
type WindowCloseHookFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// 已注册的钩子条目
struct HookEntry {
    /// 注册者标识（模块名或插件 ID）
    owner: String,
    /// 执行优先级（数值越小越先执行）
    priority: i32,
    /// 钩子回调
    hook: AsyncHookFn,
}

/// 窗口关闭请求钩子条目
struct WindowCloseHookEntry {
    owner: String,
    priority: i32,
    hook: WindowCloseHookFn,
}

/// 生命周期钩子注册表
///
/// 核心模块和插件通过全局单例注册 Startup/Shutdown/WindowCloseRequested 钩子，
/// 由 Tauri RunEvent 循环在对应阶段驱动执行。
///
/// 内部使用 std::sync::RwLock — 注册只在启动时发生，执行时短暂持锁克隆列表后释放，
/// 不阻塞异步运行时。
///
/// 优先级约定：
/// - 核心服务：0-99（SessionManager=10, PluginHost=20, ServerSupervisor=30）
/// - 插件：100-999
/// - 日志/辅助：9999+
pub struct LifecycleRegistry {
    startup_hooks: std::sync::RwLock<Vec<HookEntry>>,
    shutdown_hooks: std::sync::RwLock<Vec<HookEntry>>,
    window_close_hooks: std::sync::RwLock<Vec<WindowCloseHookEntry>>,
}

/// 单个 Shutdown 钩子的超时时间
const SHUTDOWN_HOOK_TIMEOUT_SECS: u64 = HOOK_TIMEOUT_SECS;

impl LifecycleRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            startup_hooks: std::sync::RwLock::new(Vec::new()),
            shutdown_hooks: std::sync::RwLock::new(Vec::new()),
            window_close_hooks: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// 注册 Startup 钩子
    ///
    /// # Arguments
    /// * `owner` - 注册者标识，用于日志
    /// * `priority` - 执行优先级，数值越小越先执行
    /// * `hook` - 异步回调
    pub fn on_startup<F, Fut>(&self, owner: &str, priority: i32, hook: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut hooks = self.startup_hooks.write().unwrap();
        hooks.push(HookEntry {
            owner: owner.to_string(),
            priority,
            hook: Arc::new(move || Box::pin(hook())),
        });
        hooks.sort_by_key(|e| e.priority);
    }

    /// 注册 Shutdown 钩子
    ///
    /// # Arguments
    /// * `owner` - 注册者标识，用于日志
    /// * `priority` - 执行优先级，数值越小越先执行
    /// * `hook` - 异步回调
    pub fn on_shutdown<F, Fut>(&self, owner: &str, priority: i32, hook: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut hooks = self.shutdown_hooks.write().unwrap();
        hooks.push(HookEntry {
            owner: owner.to_string(),
            priority,
            hook: Arc::new(move || Box::pin(hook())),
        });
        hooks.sort_by_key(|e| e.priority);
    }

    /// 注册 WindowCloseRequested 钩子
    ///
    /// 返回 false 可阻止窗口关闭。
    /// 任一钩子返回 false 则整体阻止关闭。
    ///
    /// # Arguments
    /// * `owner` - 注册者标识，用于日志
    /// * `priority` - 执行优先级，数值越小越先执行
    /// * `hook` - 异步回调，返回 true 允许关闭，false 阻止关闭
    pub fn on_window_close_requested<F, Fut>(&self, owner: &str, priority: i32, hook: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        let mut hooks = self.window_close_hooks.write().unwrap();
        hooks.push(WindowCloseHookEntry {
            owner: owner.to_string(),
            priority,
            hook: Arc::new(move || Box::pin(hook())),
        });
        hooks.sort_by_key(|e| e.priority);
    }

    /// 执行所有 Startup 钩子（按 priority 升序）
    pub async fn run_startup_hooks(&self) {
        // 克隆列表后立即释放锁，避免在异步执行期间持锁
        let hooks: Vec<_> = {
            let hooks = self.startup_hooks.read().unwrap();
            hooks.iter().map(|e| (e.owner.clone(), e.priority, e.hook.clone())).collect()
        };

        tracing::info!("Running {} startup hook(s)", hooks.len());

        for (owner, priority, hook) in hooks {
            tracing::debug!("Startup hook: {} (priority={})", owner, priority);
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(SHUTDOWN_HOOK_TIMEOUT_SECS),
                hook(),
            )
            .await;

            if result.is_err() {
                tracing::error!(
                    owner = %owner,
                    "Startup hook timed out after {}s",
                    SHUTDOWN_HOOK_TIMEOUT_SECS
                );
            }
        }

        tracing::info!("All startup hooks completed");
    }

    /// 执行所有 Shutdown 钩子（按 priority 升序）
    ///
    /// 每个钩子有独立超时保护，超时后继续执行后续钩子。
    pub async fn run_shutdown_hooks(&self) {
        let hooks: Vec<_> = {
            let hooks = self.shutdown_hooks.read().unwrap();
            hooks.iter().map(|e| (e.owner.clone(), e.priority, e.hook.clone())).collect()
        };

        tracing::info!("Running {} shutdown hook(s)", hooks.len());

        for (owner, priority, hook) in hooks {
            tracing::info!("Shutdown hook: {} (priority={})", owner, priority);
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(SHUTDOWN_HOOK_TIMEOUT_SECS),
                hook(),
            )
            .await;

            if result.is_err() {
                tracing::error!(
                    owner = %owner,
                    "Shutdown hook timed out after {}s, continuing",
                    SHUTDOWN_HOOK_TIMEOUT_SECS
                );
            }
        }

        tracing::info!("All shutdown hooks completed");
    }

    /// 执行所有 WindowCloseRequested 钩子
    ///
    /// 任一钩子返回 false 则阻止关闭。
    /// 返回 true 表示允许关闭，false 表示阻止关闭。
    pub async fn run_window_close_hooks(&self) -> bool {
        let hooks: Vec<_> = {
            let hooks = self.window_close_hooks.read().unwrap();
            hooks.iter().map(|e| (e.owner.clone(), e.priority, e.hook.clone())).collect()
        };

        for (owner, priority, hook) in hooks {
            let result = hook().await;
            if !result {
                tracing::info!(
                    "Window close prevented by hook: {} (priority={})",
                    owner,
                    priority
                );
                return false;
            }
        }

        true
    }
}

impl Default for LifecycleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局单例
static LIFECYCLE_REGISTRY: std::sync::LazyLock<LifecycleRegistry> =
    std::sync::LazyLock::new(LifecycleRegistry::new);

/// 获取全局生命周期注册表
pub fn lifecycle_registry() -> &'static LifecycleRegistry {
    &LIFECYCLE_REGISTRY
}

// ==================== 核心模块钩子注册 ====================

/// 注册核心模块的 Shutdown 钩子
///
/// 在 lib.rs setup 闭包中调用，注册各核心服务的优雅关闭逻辑。
/// 执行顺序由优先级决定：SessionManager(10) → PluginHost(15/20) → Server(30) → mDNS(40) → Power(50)
pub fn register_core_lifecycle_hooks() {
    let registry = lifecycle_registry();

    // PluginHost 通知插件启动 — 优先级 10，最早执行
    registry.on_startup("plugin-host-startup", 10, || async {
        let ctx = crate::system::app_context::AppContext::global();
        ctx.plugin_host().notify_startup().await;
    });

    // SessionManager — 优先级 10，最先清理（停止所有 PTY 进程）
    registry.on_shutdown("session-manager", 10, || async {
        let ctx = crate::system::app_context::AppContext::global();
        ctx.session_manager().shutdown().await;
    });

    // PluginHost 通知插件关闭 — 优先级 15，在 deactivate 之前
    registry.on_shutdown("plugin-host-shutdown", 15, || async {
        let ctx = crate::system::app_context::AppContext::global();
        ctx.plugin_host().notify_shutdown().await;
    });

    // PluginHost 停用所有插件 — 优先级 20
    registry.on_shutdown("plugin-host-deactivate", 20, || async {
        let ctx = crate::system::app_context::AppContext::global();
        if let Err(e) = ctx.plugin_host().deactivate_all().await {
            tracing::error!("Failed to deactivate all plugins during shutdown: {}", e);
        }
    });

    // ServerSupervisor — 优先级 30，停止 HTTP/WS 服务器
    registry.on_shutdown("server-supervisor", 30, || async {
        let supervisor = crate::server::supervisor::ServerSupervisor::global();
        if supervisor.is_running().await {
            if let Err(e) = supervisor.stop().await {
                tracing::error!("Failed to stop server during shutdown: {}", e);
            }
        }
    });

    // mDNS 广播 — 优先级 40
    registry.on_shutdown("mdns-advertiser", 40, || async {
        let ctx = crate::system::app_context::AppContext::global();
        let advertiser = ctx.mdns_advertiser();
        let mut a = advertiser.write().await;
        if let Err(e) = a.stop().await {
            tracing::error!("Failed to stop mDNS during shutdown: {}", e);
        }
    });

    // PowerManager — 优先级 50，释放休眠阻止
    registry.on_shutdown("power-manager", 50, || async {
        crate::system::power::power_manager().disable();
    });

    // 最终日志 — 优先级 9999
    registry.on_shutdown("lifecycle-log", 9999, || async {
        tracing::info!("BedCode Desktop shutdown complete");
    });
}

/// 注册窗口关闭请求钩子
///
/// 检查是否有运行中的会话，阻止意外关闭。
pub fn register_window_close_hooks() {
    let registry = lifecycle_registry();

    registry.on_window_close_requested("session-guard", 10, || async {
        let ctx = crate::system::app_context::AppContext::global();
        let sm = ctx.session_manager();
        let sessions = sm.list_sessions().await;
        let has_running = sessions
            .iter()
            .any(|s| matches!(s.status, crate::enums::SessionStatus::Running | crate::enums::SessionStatus::Starting));

        if has_running {
            tracing::warn!("Window close prevented: running sessions exist");
        }
        !has_running
    });
}
