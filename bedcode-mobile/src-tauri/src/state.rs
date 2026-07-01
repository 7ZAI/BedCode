//! Global State Module
//!
//! 全局单例管理器和 Token 存储

use std::sync::Arc;
use std::sync::OnceLock;

use crate::connection::manager::ConnectionManager;
use crate::auth::AuthManager;
use crate::session::SessionManager;

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
