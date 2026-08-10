//! Global State Module
//!
//! 全局单例管理器和 Token 存储

use std::sync::Arc;
use std::sync::OnceLock;

use crate::connection::manager::ConnectionManager;
use crate::auth::AuthManager;
use crate::session::SessionManager;
use crate::plugin::manager::PluginManager;
use crate::file_service::FileService;
use crate::system::info::SystemInfo;

// ==================== Global Token ====================

/// 全局 Token 存储（移动端）
///
/// 前端启动时从 localStorage 读取并设置，发送消息时自动注入
static GLOBAL_TOKEN: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

/// 设置全局 Token
pub fn set_global_token(token: &str) {
    let mut guard = GLOBAL_TOKEN.write().unwrap();
    *guard = token.to_string();
    tracing::info!("[GlobalToken] Token updated, length={}", token.len());
}

/// 获取全局 Token
pub fn get_global_token() -> String {
    let guard = GLOBAL_TOKEN.read().unwrap();
    guard.clone()
}

/// 清除全局 Token
pub fn clear_global_token() {
    let mut guard = GLOBAL_TOKEN.write().unwrap();
    *guard = String::new();
    tracing::info!("[GlobalToken] Token cleared");
}

// ==================== Manager Singletons ====================

/// 全局连接管理器单例
static CONNECTION_MANAGER: OnceLock<Arc<ConnectionManager>> = OnceLock::new();

/// 全局认证管理器单例
static AUTH_MANAGER: OnceLock<Arc<AuthManager>> = OnceLock::new();

/// 全局会话管理器单例
static SESSION_MANAGER: OnceLock<Arc<SessionManager>> = OnceLock::new();

/// 获取连接管理器
pub fn get_connection_manager() -> Arc<ConnectionManager> {
    CONNECTION_MANAGER.get_or_init(|| ConnectionManager::new()).clone()
}

/// 获取认证管理器
pub fn get_auth_manager() -> Arc<AuthManager> {
    AUTH_MANAGER.get_or_init(|| {
        let conn = get_connection_manager();
        AuthManager::new(conn)
    }).clone()
}

/// 获取会话管理器
pub fn get_session_manager() -> Arc<SessionManager> {
    SESSION_MANAGER.get_or_init(|| {
        let conn = get_connection_manager();
        SessionManager::new(conn)
    }).clone()
}

// ==================== System Info ====================

/// 全局系统信息单例
static SYSTEM_INFO: OnceLock<Arc<SystemInfo>> = OnceLock::new();

/// 初始化系统信息（在 lib.rs 启动流程中调用一次）
pub fn init_system_info(info: SystemInfo) -> Arc<SystemInfo> {
    let arc = Arc::new(info);
    let _ = SYSTEM_INFO.set(arc.clone());
    arc
}

/// 获取系统信息
///
/// # Panics
/// 如果 init_system_info 未调用则 panic
pub fn get_system_info() -> Arc<SystemInfo> {
    SYSTEM_INFO.get().expect("SystemInfo not initialized").clone()
}

/// 尝试获取系统信息（未初始化返回 None，供 best-effort 路径使用）
pub fn try_get_system_info() -> Option<Arc<SystemInfo>> {
    SYSTEM_INFO.get().cloned()
}

// ==================== Plugin Manager ====================

/// 全局插件管理器单例
static PLUGIN_MANAGER: OnceLock<Arc<PluginManager>> = OnceLock::new();

/// 初始化插件管理器（在 lib.rs setup 中调用）
pub fn init_plugin_manager(manager: Arc<PluginManager>) -> Arc<PluginManager> {
    let _ = PLUGIN_MANAGER.set(manager.clone());
    manager
}

/// 获取插件管理器
///
/// # Panics
/// 如果 init_plugin_manager 未调用则 panic
pub fn get_plugin_manager() -> Arc<PluginManager> {
    PLUGIN_MANAGER.get().expect("PluginManager not initialized").clone()
}

/// 尝试获取插件管理器（未初始化返回 None，供 best-effort 路径使用）
pub fn try_get_plugin_manager() -> Option<Arc<PluginManager>> {
    PLUGIN_MANAGER.get().cloned()
}

// ==================== File Service ====================

/// 获取文件服务单例（内网文件传输插件规格阶段 2）
///
/// OnceLock 惰性初始化（实现在 `file_service::get_file_service`）；
/// 首次调用必须在 tokio runtime 上下文内（启动上传会话 sweeper）
pub fn get_file_service() -> Arc<FileService> {
    crate::file_service::get_file_service()
}
