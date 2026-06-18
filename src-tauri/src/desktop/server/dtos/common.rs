//! Common DTOs for HTTP API responses

use serde::{Deserialize, Serialize};

/// HTTP API 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub code: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl ApiResponse<()> {
    pub fn ok() -> Self {
        Self { code: 0, message: "ok".to_string(), data: None }
    }

    pub fn error(code: u16, message: &str) -> Self {
        Self { code, message: message.to_string(), data: None }
    }
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok_with_data(data: T) -> Self {
        ApiResponse { code: 0, message: "ok".to_string(), data: Some(data) }
    }
}

// HTTP API 错误代码
pub const CODE_OK: u16 = 0;
pub const CODE_AUTH_FAILED: u16 = 1001;
pub const CODE_SESSION_NOT_FOUND: u16 = 1002;
pub const CODE_INVALID_REQUEST: u16 = 1003;
pub const CODE_TIMEOUT: u16 = 1004;
pub const CODE_PAIRING_FAILED: u16 = 1005;
pub const CODE_QR_FAILED: u16 = 1006;
