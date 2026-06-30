//! Mobile Manager Singletons
//!
//! 全局管理器单例，供 commands 和其他模块使用

use std::sync::Arc;
use std::sync::OnceLock;

use crate::mobile::remote::ConnectionManager;
use crate::mobile::auth::AuthManager;
use crate::mobile::session::SessionManager;

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
