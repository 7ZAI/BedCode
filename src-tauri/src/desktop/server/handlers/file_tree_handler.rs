//! File Tree Handler
//!
//! POST /api/file-tree — 根据会话配置扫描目录，生成文件树结构

use crate::desktop::app_context::AppContext;
use crate::shared::websocket::server::http_router::{
    ApiResponse, FileTreeNode, FileTreeRequest, FileType, HttpRouteHandler, HttpRequestContext,
};
use crate::Result;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct FileTreeHandler;

#[async_trait]
impl HttpRouteHandler for FileTreeHandler {
    async fn handle(&self, ctx: &HttpRequestContext) -> Result<String> {
        let req: FileTreeRequest = serde_json::from_str(&ctx.body).map_err(|e| {
            crate::shared::system::error::AppError::Internal(format!(
                "Invalid request body: {}",
                e
            ))
        })?;

        // 通过 AppContext 获取服务，根据 session_id 查找配置的 working_dir
        let ctx_ref = AppContext::global();
        let working_dir = match ctx_ref
            .config_manager()
            .get_config_by_session_id(&req.session_id, ctx_ref.session_manager())
            .await
        {
            Ok(config) => config.working_dir,
            Err(e) => {
                let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
                let resp = ApiResponse::<()>::error(code, &e.to_string());
                return Ok(serde_json::to_string(&resp)?);
            }
        };

        let root = PathBuf::from(&working_dir);
        if !root.is_dir() {
            let resp = ApiResponse::<()>::error(400, &format!("Working dir is not a directory: {}", working_dir));
            return Ok(serde_json::to_string(&resp)?);
        }

        // 构建过滤规则
        let filters = build_exclude_filters(&req.exclude_dirs);

        // 递归扫描目录（在 spawn_blocking 中执行，避免阻塞 Tokio runtime）
        let root = root.clone();
        let filters = filters.clone();
        let tree = tokio::task::spawn_blocking(move || scan_dir(&root, &root, &filters, 0))
            .await
            .map_err(|e| crate::shared::system::error::AppError::Internal(format!(
                "File tree scan task failed: {}", e
            )))??;

        let resp = ApiResponse::ok_with_data(tree);
        Ok(serde_json::to_string(&resp)?)
    }
}

// ==================== Filter Logic ====================

/// 最大递归深度，防止符号链接环或超深目录导致栈溢出
const MAX_DEPTH: usize = 20;

/// 过滤规则：纯名称匹配任意层级，带路径的仅匹配指定父级下
#[derive(Clone)]
enum ExcludeFilter {
    /// 纯目录名，如 "node_modules"，匹配所有层级
    Name(String),
    /// 带父级路径，如 "src/node_modules"，仅在指定路径下匹配
    Path { parent: String, name: String },
}

fn build_exclude_filters(exclude_dirs: &[String]) -> Vec<ExcludeFilter> {
    exclude_dirs
        .iter()
        .map(|pattern| {
            if let Some(slash_pos) = pattern.rfind('/') {
                // "src/node_modules" → parent="src", name="node_modules"
                ExcludeFilter::Path {
                    parent: pattern[..slash_pos].to_string(),
                    name: pattern[slash_pos + 1..].to_string(),
                }
            } else {
                // 纯名称
                ExcludeFilter::Name(pattern.clone())
            }
        })
        .collect()
}

/// 检查目录是否应被排除
///
/// `relative_path` 是相对于 root 的路径（如 "src/components"）
/// `dir_name` 是当前目录名（如 "components"）
fn should_exclude(relative_path: &str, dir_name: &str, filters: &[ExcludeFilter]) -> bool {
    for f in filters {
        match f {
            ExcludeFilter::Name(name) => {
                if dir_name == name {
                    return true;
                }
            }
            ExcludeFilter::Path { parent, name } => {
                if dir_name == name && relative_path == parent {
                    return true;
                }
            }
        }
    }
    false
}

// ==================== Directory Scanner ====================

/// 递归扫描目录，生成文件树
///
/// `root` — 项目根目录（用于计算相对路径）
/// `dir` — 当前正在扫描的目录
/// `depth` — 当前递归深度，超过 MAX_DEPTH 时停止
fn scan_dir(root: &PathBuf, dir: &PathBuf, filters: &[ExcludeFilter], depth: usize) -> Result<Vec<FileTreeNode>> {
    if depth > MAX_DEPTH {
        return Ok(Vec::new());
    }

    let mut entries: Vec<FileTreeNode> = Vec::new();

    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        crate::shared::system::error::AppError::Internal(format!(
            "Failed to read dir {}: {}",
            dir.display(),
            e
        ))
    })?;

    // 收集并排序：文件夹在前，文件在后，各自按名称排序
    let mut folders: Vec<FileTreeNode> = Vec::new();
    let mut files: Vec<FileTreeNode> = Vec::new();

    for entry in read_dir {
        let entry = entry.map_err(|e| {
            crate::shared::system::error::AppError::Internal(format!(
                "Failed to read entry: {}",
                e
            ))
        })?;

        let file_name = entry.file_name().to_string_lossy().to_string();

        // 跳过隐藏文件/目录（以 . 开头）
        if file_name.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type().map_err(|e| {
            crate::shared::system::error::AppError::Internal(format!(
                "Failed to get file type: {}",
                e
            ))
        })?;

        if file_type.is_dir() {
            // 计算相对路径，用于过滤匹配
            let relative = dir.strip_prefix(root)
                .unwrap_or(dir)
                .to_string_lossy()
                .to_string();

            // 命中过滤规则则跳过
            if should_exclude(&relative, &file_name, filters) {
                continue;
            }

            let child_dir = dir.join(&file_name);
            let children = scan_dir(root, &child_dir, filters, depth + 1)?;

            folders.push(FileTreeNode {
                name: file_name,
                node_type: FileType::Folder,
                children: Some(children),
                expanded: None,
            });
        } else if file_type.is_file() {
            files.push(FileTreeNode {
                name: file_name,
                node_type: FileType::File,
                children: None,
                expanded: None,
            });
        }
        // 忽略符号链接等其他类型
    }

    // 文件夹在前，文件在后，各自按名称排序
    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    entries.extend(folders);
    entries.extend(files);

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_exclude_name_only() {
        let filters = build_exclude_filters(&["node_modules".to_string(), ".git".to_string()]);

        assert!(should_exclude("src", "node_modules", &filters));
        assert!(should_exclude("", "node_modules", &filters));
        assert!(should_exclude("packages/app", "node_modules", &filters));
        assert!(!should_exclude("src", "components", &filters));
    }

    #[test]
    fn test_should_exclude_with_path() {
        let filters = build_exclude_filters(&["src/node_modules".to_string()]);

        // 仅在 src/ 下匹配 node_modules
        assert!(should_exclude("src", "node_modules", &filters));
        // 其他路径下的 node_modules 不匹配
        assert!(!should_exclude("packages/app", "node_modules", &filters));
        assert!(!should_exclude("", "node_modules", &filters));
    }

    #[test]
    fn test_should_exclude_mixed() {
        let filters = build_exclude_filters(&[
            "node_modules".to_string(),
            "src/test_fixtures".to_string(),
        ]);

        // 纯名称：全局匹配
        assert!(should_exclude("", "node_modules", &filters));
        assert!(should_exclude("packages/app", "node_modules", &filters));

        // 带路径：仅匹配指定父级
        assert!(should_exclude("src", "test_fixtures", &filters));
        assert!(!should_exclude("tests", "test_fixtures", &filters));
    }
}
