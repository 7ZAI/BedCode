//! File Controller
//!
//! Routes:
//! - POST /api/file-tree

use actix_web::{web, HttpResponse};
use crate::desktop::app_context::AppContext;
use crate::desktop::server::dtos::common::ApiResponse;
use crate::desktop::server::dtos::config_dto::*;
use std::path::PathBuf;

const MAX_DEPTH: usize = 20;

/// POST /api/file-tree
pub async fn get_file_tree(body: web::Json<FileTreeRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    // 根据 session_id 查找 working_dir
    let working_dir = match ctx
        .config_manager()
        .get_config_by_session_id(&body.session_id, ctx.session_manager())
        .await
    {
        Ok(config) => config.working_dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let root = PathBuf::from(&working_dir);
    if !root.is_dir() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(400, &format!("Working dir is not a directory: {}", working_dir)));
    }

    let filters = build_exclude_filters(&body.exclude_dirs);
    let root_clone = root.clone();
    let filters_clone = filters.clone();

    let tree_result = tokio::task::spawn_blocking(move || {
        scan_dir(&root_clone, &root_clone, &filters_clone, 0)
    }).await;

    match tree_result {
        Ok(Ok(tree)) => {
            let data = FileTreeResponseData { tree };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
        Err(e) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &format!("File tree scan failed: {}", e)))
        }
    }
}

#[derive(Clone)]
enum ExcludeFilter {
    Name(String),
    Path { parent: String, name: String },
}

fn build_exclude_filters(exclude_dirs: &[String]) -> Vec<ExcludeFilter> {
    exclude_dirs.iter().map(|pattern| {
        if let Some(slash_pos) = pattern.rfind('/') {
            ExcludeFilter::Path {
                parent: pattern[..slash_pos].to_string(),
                name: pattern[slash_pos + 1..].to_string(),
            }
        } else {
            ExcludeFilter::Name(pattern.clone())
        }
    }).collect()
}

fn should_exclude(relative_path: &str, dir_name: &str, filters: &[ExcludeFilter]) -> bool {
    for f in filters {
        match f {
            ExcludeFilter::Name(name) => { if dir_name == name { return true; } }
            ExcludeFilter::Path { parent, name } => {
                if dir_name == name && relative_path == parent { return true; }
            }
        }
    }
    false
}

fn scan_dir(root: &PathBuf, dir: &PathBuf, filters: &[ExcludeFilter], depth: usize) -> crate::Result<Vec<FileTreeNode>> {
    if depth > MAX_DEPTH { return Ok(Vec::new()); }
    let mut folders: Vec<FileTreeNode> = Vec::new();
    let mut files: Vec<FileTreeNode> = Vec::new();

    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        crate::AppError::Internal(format!("Failed to read dir {}: {}", dir.display(), e))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| crate::AppError::Internal(format!("Failed to read entry: {}", e)))?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') { continue; }
        let file_type = entry.file_type().map_err(|e| crate::AppError::Internal(format!("Failed to get file type: {}", e)))?;

        if file_type.is_dir() {
            let relative = dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().to_string();
            if should_exclude(&relative, &file_name, filters) { continue; }
            let child_dir = dir.join(&file_name);
            let children = scan_dir(root, &child_dir, filters, depth + 1)?;
            folders.push(FileTreeNode {
                name: file_name,
                node_type: "folder".to_string(),
                children: Some(children),
            });
        } else if file_type.is_file() {
            files.push(FileTreeNode {
                name: file_name,
                node_type: "file".to_string(),
                children: None,
            });
        }
    }

    folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut entries = folders;
    entries.extend(files);
    Ok(entries)
}
