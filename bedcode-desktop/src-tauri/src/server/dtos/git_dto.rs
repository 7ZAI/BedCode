//! Git API DTOs

use serde::{Deserialize, Serialize};

// ==================== Git Branches ====================

/// GET /api/git/branches response data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchesResponseData {
    /// 当前分支名，非 git 仓库时为 None
    pub current_branch: Option<String>,
    /// 所有本地分支列表
    pub branches: Vec<String>,
    /// 是否为 git 仓库
    pub is_git_repo: bool,
}

// ==================== Git Checkout ====================

/// POST /api/git/checkout request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCheckoutRequest {
    /// 会话 ID（优先使用）或配置 ID
    pub session_id: String,
    /// 目标分支名
    pub branch: String,
}

/// POST /api/git/checkout response data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCheckoutResponseData {
    pub branch: String,
}
