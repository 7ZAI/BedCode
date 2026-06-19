//! Shared API DTOs
//!
//! 桌面端和移动端共用的 HTTP API 数据传输对象

use serde::{Deserialize, Serialize};

// ==================== Common API Response ====================

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
pub const CODE_PLUGIN_AUTH_FAILED: u16 = 1007;

// ==================== File Tree DTOs ====================

/// POST /api/file-tree request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeRequest {
    pub session_id: String,
    pub exclude_dirs: Vec<String>,
}

/// POST /api/file-tree response data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeResponseData {
    pub tree: Vec<FileTreeNode>,
}

/// File tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: String,
    /// 相对于工作目录的路径（如 "src/main.rs"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}

// ==================== File Content DTOs ====================

/// POST /api/file-content request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentRequest {
    pub session_id: String,
    pub file_path: String,
}

/// POST /api/file-content response data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentResponseData {
    pub content: String,
    pub file_name: String,
}

// ==================== Diff Tree DTOs ====================

/// POST /api/diff-tree request
///
/// 与 FileTreeRequest 相同参数，返回仅包含 git 改动文件的树
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffTreeRequest {
    pub session_id: String,
    pub exclude_dirs: Vec<String>,
}
