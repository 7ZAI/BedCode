//! Global State Module
//!
//! 移动端全局状态管理

use std::sync::RwLock;

/// 全局 Token 存储（移动端）
///
/// 前端启动时从 localStorage 读取并设置，发送消息时自动注入
static GLOBAL_TOKEN: RwLock<String> = RwLock::new(String::new());

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