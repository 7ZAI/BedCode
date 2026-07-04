//! File Controller
//!
//! Routes:
//! - POST /api/file-tree
//! - POST /api/file-content
//! - POST /api/diff-tree

use actix_web::{web, HttpResponse};
use crate::system::app_context::AppContext;
use crate::server::dtos::ApiResponse;
use crate::server::dtos::file_dto::*;
use crate::process::create_command;
use std::path::PathBuf;
use std::collections::HashSet;

const MAX_DEPTH: usize = 20;
/// 文件内容读取上限 2MB，防止传输过大文件
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

/// 解析 working_dir：id 可以是 session_id 或 config_id
///
/// 优先按 session_id 查找（通过 SessionManager → ConfigManager），失败后按 config_id 查找
/// 这样即使会话未运行，也可以通过 config_id 直接浏览文件
pub async fn resolve_working_dir(id: &str, ctx: &AppContext) -> crate::Result<String> {
    // 优先按 session_id 查找
    match ctx
        .config_manager()
        .get_config_by_session_id(id, ctx.session_manager())
        .await
    {
        Ok(config) => Ok(config.working_dir),
        Err(_) => {
            // 回退：按 config_id 直接查找
            ctx.config_manager()
                .get_config(id)
                .await?
                .map(|config| config.working_dir)
                .ok_or_else(|| crate::AppError::NotFound(format!(
                    "Session/Config not found: {}", id
                )))
        }
    }
}

/// POST /api/file-tree
pub async fn get_file_tree(body: web::Json<FileTreeRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    // session_id 可以是会话 ID 或配置 ID，优先按会话 ID 查找
    let working_dir = match resolve_working_dir(&body.session_id, ctx).await {
        Ok(dir) => dir,
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
        let file_type = entry.file_type().map_err(|e| crate::AppError::Internal(format!("Failed to get file type: {}", e)))?;

        if file_type.is_dir() {
            let relative = dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().to_string();
            if should_exclude(&relative, &file_name, filters) { continue; }
            let child_dir = dir.join(&file_name);
            // 统一使用 / 作为路径分隔符，避免 Windows 上 to_string_lossy 产生 \ 导致混合分隔符
            let normalized_relative = relative.replace('\\', "/");
            let node_path = if normalized_relative.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", normalized_relative, file_name)
            };
            let children = scan_dir(root, &child_dir, filters, depth + 1)?;
            folders.push(FileTreeNode {
                name: file_name,
                node_type: "folder".to_string(),
                path: Some(node_path),
                children: Some(children),
            });
        } else if file_type.is_file() {
            let relative = dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().to_string();
            // 统一使用 / 作为路径分隔符，避免 Windows 上 to_string_lossy 产生 \ 导致混合分隔符
            let normalized_relative = relative.replace('\\', "/");
            let node_path = if normalized_relative.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", normalized_relative, file_name)
            };
            files.push(FileTreeNode {
                name: file_name,
                node_type: "file".to_string(),
                path: Some(node_path),
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

/// POST /api/file-content
///
/// 根据 session_id 定位工作目录，读取 file_path 指定的文件内容
/// file_path 可以是相对路径（相对于工作目录）或绝对路径
/// session_id 可以是会话 ID 或配置 ID
pub async fn get_file_content(body: web::Json<FileContentRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&body.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    // 构建文件绝对路径：相对路径基于 working_dir 解析
    let file_path = PathBuf::from(&body.file_path);
    let abs_path = if file_path.is_absolute() {
        file_path
    } else {
        PathBuf::from(&working_dir).join(&file_path)
    };

    // 安全检查：路径必须在 working_dir 下，防止目录遍历
    let canonical_working = match PathBuf::from(&working_dir).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("Failed to resolve working dir: {}", e),
            ));
        }
    };

    // 文件不存在时 canonicalize 会失败，先检查
    if !abs_path.exists() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            404,
            &format!("File not found: {}", body.file_path),
        ));
    }

    let canonical_path = match std::path::Path::canonicalize(&abs_path) {
        Ok(p) => p,
        Err(e) => {
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("Failed to resolve file path: {}", e),
            ));
        }
    };

    if !canonical_path.starts_with(&canonical_working) {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            403,
            "Access denied: file is outside working directory",
        ));
    }

    if !canonical_path.is_file() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            400,
            "Path is not a file",
        ));
    }

    // 检查文件大小
    let file_size = match std::fs::metadata(&canonical_path) {
        Ok(m) => m.len(),
        Err(e) => {
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("Failed to read file metadata: {}", e),
            ));
        }
    };

    if file_size > MAX_FILE_SIZE {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            413,
            &format!("File too large ({} bytes, max {} bytes)", file_size, MAX_FILE_SIZE),
        ));
    }

    // 读取文件内容
    let path_for_read = canonical_path.clone();
    let read_result = tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(&path_for_read)
    }).await;

    match read_result {
        Ok(Ok(content)) => {
            let file_name = canonical_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let data = FileContentResponseData { content, file_name };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => {
            // 二进制文件等无法用 UTF-8 解码的情况
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                415,
                &format!("Failed to read file (possibly binary): {}", e),
            ))
        }
        Err(e) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("File read task failed: {}", e),
            ))
        }
    }
}

/// POST /api/diff-tree
///
/// 获取 git 改动文件构成的文件树
/// session_id 可以是会话 ID 或配置 ID
pub async fn get_diff_tree(body: web::Json<DiffTreeRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&body.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let root = PathBuf::from(&working_dir);
    if !root.is_dir() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            400,
            &format!("Working dir is not a directory: {}", working_dir),
        ));
    }

    // 检查是否为 git 仓库
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            400,
            "Not a git repository",
        ));
    }

    let filters = build_exclude_filters(&body.exclude_dirs);
    let working_dir_clone = working_dir.clone();
    let filters_clone = filters.clone();

    let result = tokio::task::spawn_blocking(move || {
        get_diff_file_tree(&working_dir_clone, &filters_clone)
    }).await;

    match result {
        Ok(Ok(tree)) => {
            let data = FileTreeResponseData { tree };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
        Err(e) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("Diff tree task failed: {}", e),
            ))
        }
    }
}

/// 执行 git diff 获取改动文件列表，过滤后构建树
fn get_diff_file_tree(working_dir: &str, filters: &[ExcludeFilter]) -> crate::Result<Vec<FileTreeNode>> {
    // 获取未暂存的改动（工作区 vs 暂存区）
    let unstaged = run_git_command(working_dir, &["diff", "--name-only"])?;
    // 获取已暂存但未提交的改动（暂存区 vs HEAD）
    let staged = run_git_command(working_dir, &["diff", "--cached", "--name-only"])?;
    // 获取未跟踪的文件
    let untracked = run_git_command(working_dir, &["ls-files", "--others", "--exclude-standard"])?;

    // 合并去重
    let mut all_paths: HashSet<String> = HashSet::new();
    for path in unstaged.iter().chain(staged.iter()).chain(untracked.iter()) {
        all_paths.insert(path.clone());
    }

    // 过滤掉被排除规则匹配的路径中的目录组件
    let filtered_paths: Vec<String> = all_paths
        .into_iter()
        .filter(|path| {
            // 检查路径中的每个目录组件是否被排除
            let parts: Vec<&str> = path.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                let parent = parts[..i].join("/");
                if should_exclude(&parent, part, filters) {
                    return false;
                }
            }
            true
        })
        .collect();

    if filtered_paths.is_empty() {
        return Ok(Vec::new());
    }

    // 将扁平路径列表构建为嵌套树结构
    Ok(build_tree_from_paths(&filtered_paths))
}

/// 执行 git 命令并解析输出为路径列表
fn run_git_command(working_dir: &str, args: &[&str]) -> crate::Result<Vec<String>> {
    let output = create_command("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .map_err(|e| crate::AppError::Internal(format!("Failed to execute git: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::AppError::Internal(format!("git command failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(paths)
}

/// 将扁平路径列表构建为嵌套树结构
///
/// 输入: ["src/main.rs", "src/lib/mod.rs", "Cargo.toml"]
/// 输出:
///   folder "src"
///     file "main.rs"
///     folder "lib"
///       file "mod.rs"
///   file "Cargo.toml"
fn build_tree_from_paths(paths: &[String]) -> Vec<FileTreeNode> {
    // 用嵌套 HashMap 收集路径，再转换为 Vec<FileTreeNode>
    use std::collections::BTreeMap;

    enum Entry {
        File,
        Dir(BTreeMap<String, Entry>),
    }

    let mut root: BTreeMap<String, Entry> = BTreeMap::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            if is_last {
                // 文件节点
                current.insert(part.to_string(), Entry::File);
            } else {
                // 目录节点
                let entry = current
                    .entry(part.to_string())
                    .or_insert_with(|| Entry::Dir(BTreeMap::new()));
                match entry {
                    Entry::Dir(children) => {
                        current = children;
                    }
                    Entry::File => {
                        // 路径冲突（同一路径既是文件又是目录），忽略
                        break;
                    }
                }
            }
        }
    }

    fn map_to_tree(map: &BTreeMap<String, Entry>, parent_path: &str) -> Vec<FileTreeNode> {
        let mut nodes = Vec::new();
        for (name, entry) in map {
            let node_path = if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", parent_path, name)
            };
            match entry {
                Entry::File => {
                    nodes.push(FileTreeNode {
                        name: name.clone(),
                        node_type: "file".to_string(),
                        path: Some(node_path),
                        children: None,
                    });
                }
                Entry::Dir(children) => {
                    let child_nodes = map_to_tree(children, &node_path);
                    nodes.push(FileTreeNode {
                        name: name.clone(),
                        node_type: "folder".to_string(),
                        path: Some(node_path),
                        children: Some(child_nodes),
                    });
                }
            }
        }
        // 文件夹在前，文件在后
        let mut folders: Vec<FileTreeNode> = nodes.iter()
            .filter(|n| n.node_type == "folder")
            .cloned()
            .collect();
        let files: Vec<FileTreeNode> = nodes.iter()
            .filter(|n| n.node_type == "file")
            .cloned()
            .collect();
        folders.extend(files);
        folders
    }

    map_to_tree(&root, "")
}

/// POST /api/file-diff
///
/// 获取指定文件的 git diff 内容，解析为结构化行数据
/// session_id 可以是会话 ID 或配置 ID
pub async fn get_file_diff(body: web::Json<FileDiffRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&body.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let root = PathBuf::from(&working_dir);
    if !root.is_dir() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            400,
            &format!("Working dir is not a directory: {}", working_dir),
        ));
    }

    // 检查是否为 git 仓库
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            400,
            "Not a git repository",
        ));
    }

    let file_path = body.file_path.clone();
    let working_dir_clone = working_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        parse_git_diff(&working_dir_clone, &file_path)
    }).await;

    match result {
        Ok(Ok((file_name, lines))) => {
            let data = FileDiffResponseData { file_name, lines };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
        Err(e) => {
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                500,
                &format!("File diff task failed: {}", e),
            ))
        }
    }
}

/// 执行 git diff 并解析 unified diff 输出为结构化行数据
fn parse_git_diff(working_dir: &str, file_path: &str) -> crate::Result<(String, Vec<FileDiffLine>)> {
    let output = create_command("git")
        .args(["diff", "--", file_path])
        .current_dir(working_dir)
        .output()
        .map_err(|e| crate::AppError::Internal(format!("Failed to execute git diff: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::AppError::Internal(format!("git diff failed: {}", stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // 无改动时返回空
    if stdout.is_empty() {
        let file_name = PathBuf::from(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        return Ok((file_name, Vec::new()));
    }

    let file_name = PathBuf::from(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let lines = parse_unified_diff(&stdout);
    Ok((file_name, lines))
}

/// 解析 unified diff 文本为 FileDiffLine 列表
fn parse_unified_diff(diff_text: &str) -> Vec<FileDiffLine> {
    let mut result = Vec::new();
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;
    let mut in_hunk = false;

    for line in diff_text.lines() {
        // 跳过 diff header 行
        if line.starts_with("diff --git") || line.starts_with("index ") {
            continue;
        }
        // 跳过 --- a/ 和 +++ b/ 行
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }

        // 解析 hunk header: @@ -old_start[,old_count] +new_start[,new_count] @@
        if line.starts_with("@@") {
            if let Some(hunk_info) = parse_hunk_header(line) {
                old_line = hunk_info.0;
                new_line = hunk_info.1;
                in_hunk = true;
            }
            continue;
        }

        if !in_hunk {
            continue;
        }

        // 解析 diff 行
        if let Some(content) = line.strip_prefix('-') {
            result.push(FileDiffLine {
                line_type: "removed".to_string(),
                content: content.to_string(),
                old_line_no: Some(old_line),
                new_line_no: None,
            });
            old_line += 1;
        } else if let Some(content) = line.strip_prefix('+') {
            result.push(FileDiffLine {
                line_type: "added".to_string(),
                content: content.to_string(),
                old_line_no: None,
                new_line_no: Some(new_line),
            });
            new_line += 1;
        } else if let Some(content) = line.strip_prefix(' ') {
            result.push(FileDiffLine {
                line_type: "context".to_string(),
                content: content.to_string(),
                old_line_no: Some(old_line),
                new_line_no: Some(new_line),
            });
            old_line += 1;
            new_line += 1;
        } else if line.starts_with('\\') {
            // "\ No newline at end of file" — 忽略
            continue;
        }
    }

    result
}

/// 解析 hunk header `@@ -a,b +c,d @@` 提取起始行号
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    // 格式: @@ -old_start[,old_count] +new_start[,new_count] @@
    let text = line.trim_start_matches('@').trim_start();
    let text = text.split('@').next()?;

    let parts: Vec<&str> = text.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let old_start: u32 = parts[0]
        .trim_start_matches('-')
        .split(',')
        .next()?
        .parse()
        .ok()?;

    let new_start: u32 = parts[1]
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse()
        .ok()?;

    Some((old_start, new_start))
}
