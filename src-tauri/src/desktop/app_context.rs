//! Application Context
//!
//! 全局单实例容器，集中管理桌面端所有全局服务的引用
//! 在 lib.rs 的 run() 中一次性创建，后续通过 AppContext::global() 获取

use crate::desktop::plugin::PluginManager;
use crate::desktop::server::services::PairingService;
use crate::desktop::session::{SessionConfigManager, SessionManager};
use crate::desktop::auth::QrTokenManager;
use crate::shared::db::Database;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{broadcast, Mutex};

/// 桌面端全局服务容器
///
/// 所有全局单实例统一注册在此，避免在 lib.rs 中散落大量 clone 变量
/// 通过 AppContext::global() 获取，各模块按需取用
pub struct AppContext {
    /// 数据库实例
    db: Arc<Mutex<Database>>,
    /// 会话管理器
    session_manager: Arc<SessionManager>,
    /// 会话配置管理器
    config_manager: Arc<SessionConfigManager>,
    /// 插件管理器
    plugin_manager: Arc<PluginManager>,
    /// 配对服务
    pairing_service: Arc<PairingService>,
    /// QR Token 管理器
    qr_manager: Arc<QrTokenManager>,
    /// Tauri AppHandle
    app_handle: Arc<AppHandle>,
    /// 同步事件发送器
    sync_tx: broadcast::Sender<crate::desktop::events::DesktopSyncEvent>,
    /// 资源目录路径（用于项目级 hooks 脚本复制）
    resource_dir: Arc<PathBuf>,
}

/// 全局单实例存储 — init() 和 global() 必须引用同一个 static
static APP_CONTEXT: std::sync::OnceLock<AppContext> = std::sync::OnceLock::new();

impl AppContext {
    /// 获取全局单例引用
    pub fn global() -> &'static Self {
        APP_CONTEXT.get().expect("AppContext not initialized, call AppContext::init() first")
    }

    /// 初始化全局容器（仅在 lib.rs run() 中调用一次）
    ///
    /// 返回 &'static Self，后续通过 global() 获取同一实例
    pub fn init(ctx: AppContext) -> &'static Self {
        APP_CONTEXT.get_or_init(|| ctx)
    }

    // ==================== Accessors ====================

    pub fn db(&self) -> &Arc<Mutex<Database>> {
        &self.db
    }

    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
    }

    pub fn config_manager(&self) -> &Arc<SessionConfigManager> {
        &self.config_manager
    }

    pub fn plugin_manager(&self) -> &Arc<PluginManager> {
        &self.plugin_manager
    }

    pub fn pairing_service(&self) -> &Arc<PairingService> {
        &self.pairing_service
    }

    pub fn qr_manager(&self) -> &Arc<QrTokenManager> {
        &self.qr_manager
    }

    pub fn app_handle(&self) -> &Arc<AppHandle> {
        &self.app_handle
    }

    pub fn sync_tx(&self) -> &broadcast::Sender<crate::desktop::events::DesktopSyncEvent> {
        &self.sync_tx
    }

    pub fn resource_dir(&self) -> &Arc<PathBuf> {
        &self.resource_dir
    }
}

/// 构建器，用于分步组装 AppContext
pub struct AppContextBuilder {
    db: Option<Arc<Mutex<Database>>>,
    session_manager: Option<Arc<SessionManager>>,
    config_manager: Option<Arc<SessionConfigManager>>,
    plugin_manager: Option<Arc<PluginManager>>,
    pairing_service: Option<Arc<PairingService>>,
    qr_manager: Option<Arc<QrTokenManager>>,
    app_handle: Option<Arc<AppHandle>>,
    sync_tx: Option<broadcast::Sender<crate::desktop::events::DesktopSyncEvent>>,
    resource_dir: Option<Arc<PathBuf>>,
}

impl AppContextBuilder {
    pub fn new() -> Self {
        Self {
            db: None,
            session_manager: None,
            config_manager: None,
            plugin_manager: None,
            pairing_service: None,
            qr_manager: None,
            app_handle: None,
            sync_tx: None,
            resource_dir: None,
        }
    }

    pub fn db(mut self, db: Arc<Mutex<Database>>) -> Self {
        self.db = Some(db);
        self
    }

    pub fn session_manager(mut self, sm: Arc<SessionManager>) -> Self {
        self.session_manager = Some(sm);
        self
    }

    pub fn config_manager(mut self, cm: Arc<SessionConfigManager>) -> Self {
        self.config_manager = Some(cm);
        self
    }

    pub fn plugin_manager(mut self, pm: Arc<PluginManager>) -> Self {
        self.plugin_manager = Some(pm);
        self
    }

    pub fn pairing_service(mut self, ps: Arc<PairingService>) -> Self {
        self.pairing_service = Some(ps);
        self
    }

    pub fn qr_manager(mut self, qm: Arc<QrTokenManager>) -> Self {
        self.qr_manager = Some(qm);
        self
    }

    pub fn app_handle(mut self, ah: Arc<AppHandle>) -> Self {
        self.app_handle = Some(ah);
        self
    }

    pub fn sync_tx(mut self, tx: broadcast::Sender<crate::desktop::events::DesktopSyncEvent>) -> Self {
        self.sync_tx = Some(tx);
        self
    }

    pub fn resource_dir(mut self, rd: Arc<PathBuf>) -> Self {
        self.resource_dir = Some(rd);
        self
    }

    /// 构建并初始化全局 AppContext
    pub fn build_and_init(self) -> &'static AppContext {
        let ctx = AppContext {
            db: self.db.expect("AppContext: db is required"),
            session_manager: self.session_manager.expect("AppContext: session_manager is required"),
            config_manager: self.config_manager.expect("AppContext: config_manager is required"),
            plugin_manager: self.plugin_manager.expect("AppContext: plugin_manager is required"),
            pairing_service: self.pairing_service.expect("AppContext: pairing_service is required"),
            qr_manager: self.qr_manager.expect("AppContext: qr_manager is required"),
            app_handle: self.app_handle.expect("AppContext: app_handle is required"),
            sync_tx: self.sync_tx.expect("AppContext: sync_tx is required"),
            resource_dir: self.resource_dir.expect("AppContext: resource_dir is required"),
        };
        AppContext::init(ctx)
    }
}

impl Default for AppContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
