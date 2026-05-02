//! Error types for Claude Code Remote

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[error("PTY error: {0}")]
    Pty(String),

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

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Notification error: {0}")]
    Notification(String),

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error("Not found: {0}")]
    NotFound(String),

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

// Implement From for other error types
#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<tokio_tungstenite::tungstenite::Error> for AppError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        AppError::WebSocket(e.to_string())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<mdns_sd::Error> for AppError {
    fn from(e: mdns_sd::Error) -> Self {
        AppError::Discovery(e.to_string())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl From<keyring::Error> for AppError {
    fn from(e: keyring::Error) -> Self {
        AppError::Keyring(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
