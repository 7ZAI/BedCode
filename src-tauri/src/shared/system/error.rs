//! Error types for Claude Code Remote
//!
//! 共享错误类型 - 桌面端和移动端都可用

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Session error: {0}")]
    Session(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Notification error: {0}")]
    Notification(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[error("PTY error: {0}")]
    Pty(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

// Implement Serialize for Tauri IPC compatibility
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl From<notify::Error> for AppError {
    fn from(e: notify::Error) -> Self {
        AppError::Internal(format!("File watcher error: {}", e))
    }
}

/// 允许在 crate::Result 函数中使用 anyhow::Context
///
/// 使用方式：在 Result<crate::AppError> 上调用 .context() / .with_context()
/// 后，通过 ? 运算符自动转换为 AppError::Internal（保留完整错误链）
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}