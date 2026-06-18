//! Mobile HTTP Commands
//!
//! 移动端通过 HTTP 调用桌面端 API 的 Tauri 命令

use crate::Result;
use crate::mobile::remote::http_client::FileTreeApi;
use crate::mobile::managers::get_connection_manager;

/// 获取文件树
///
/// 通过 HTTP POST /api/file-tree 调用桌面端
#[tauri::command]
pub async fn http_get_file_tree(session_id: String, exclude_dirs: Vec<String>) -> Result<Vec<serde_json::Value>> {
    tracing::info!("[http_get_file_tree] session_id={}, exclude_dirs={:?}", session_id, exclude_dirs);

    let conn = get_connection_manager();
    let http = conn.http_client();

    let tree: Vec<crate::shared::model::api_dto::FileTreeNode> = FileTreeApi::get_file_tree(http, &session_id, exclude_dirs).await?;

    // FileTreeNode → serde_json::Value 供前端使用
    let values = serde_json::to_value(&tree)
        .map_err(|e| crate::AppError::Internal(format!("Failed to serialize file tree: {}", e)))?
        .as_array()
        .cloned()
        .unwrap_or_default();

    tracing::info!("[http_get_file_tree] Response OK, {} nodes", values.len());
    Ok(values)
}
