//! Desktop-specific Error Types
//!
//! 桌面端专用错误类型和 From 实现

impl From<tokio_tungstenite::tungstenite::Error> for crate::AppError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        crate::AppError::WebSocket(e.to_string())
    }
}

impl From<keyring::Error> for crate::AppError {
    fn from(e: keyring::Error) -> Self {
        crate::AppError::Keyring(e.to_string())
    }
}