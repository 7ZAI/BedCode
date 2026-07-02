//! Config DTOs

use serde::Serialize;

// Re-export file tree types
pub use crate::server::dtos::file_dto::{FileTreeRequest, FileTreeNode, FileTreeResponseData};

/// GET /api/configs response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigListResponseData {
    pub configs: Vec<ConfigItem>,
}

/// Single config item
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigItem {
    pub id: String,
    pub name: String,
    pub environment: String,
    pub wsl_distro: Option<String>,
    pub working_dir: String,
    pub command: String,
}

/// GET /api/quick-actions response data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickActionListResponseData {
    pub actions: Vec<QuickActionItem>,
}

/// Quick action item
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickActionItem {
    pub id: String,
    pub name: String,
    pub content: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}
