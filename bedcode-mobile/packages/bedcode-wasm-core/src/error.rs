//! crate 级错误类型（移动 `AppError` 形状）
//!
//! 形状真源 = 移动宿主 `src-tauri/src/system/error.rs`（票 17 批次 1 fork 对齐）。
//! 桌面 fork 面原经 `bedcode_server_base::error::AppError`——移动宿主不依赖
//! `bedcode-server-base`（桌面基础层），故 crate 内自持同形状类型；
//! 批次 2 宿主切换时经垫片统一（宿主 re-export 或反向对齐，见票 17 §2）。

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

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Egress error: {0}")]
    Egress(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Internal error: {0}")]
    Internal(String),
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

/// 允许在 crate::Result 函数中使用 anyhow::Context
///
/// 使用方式：在 Result<crate::AppError> 上调用 .context() / .with_context()
/// 后，通过 ? 运算符自动转换为 AppError::Internal（保留完整错误链）
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
