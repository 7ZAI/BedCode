//! 文件浏览编排（票 03）：纯逻辑层——containment / exclude 过滤 / 树扫描 /
//! 内容读取 / git diff 解析，全部可 native 单测；宿主 file_controller 语义逐字复刻。
//!
//! **字节级契约锚点**（双轨对照测试比对目标）：
//! - 树节点形状：`{name, nodeType:"folder"|"file", path, children?}`——文件夹
//!   `children` 恒有（可为空数组），文件 `children` 省略（宿主
//!   `skip_serializing_if = "Option::is_none"`）
//! - 排序：目录树 = 文件夹在前、文件在后，各自 `name.to_lowercase()` 升序；
//!   git diff 树 = `BTreeMap` 字节序 + 文件夹在前（宿主 build_tree_from_paths）
//! - 错误文案（确定性部分逐字一致）：`Working dir is not a directory: {dir}` /
//!   `Not a git repository` / `dir_path must be relative to working directory` /
//!   `Directory not found: {path}` / `Access denied: directory is outside working
//!   directory` / `Access denied: file is outside working directory` /
//!   `File not found: {path}` / `Path is not a file` /
//!   `File too large ({size} bytes, max {max} bytes)` / `Not found: Session/Config
//!   not found: {id}`。嵌入 OS io::Error 细节的 500/415 文案允许尾部差异
//!   （宿主侧同样是运行时 io 错误字符串，双轨测试只锁 code 与确定性文案）。

use super::source::{FsPort, GitPort};

/// 最大树深（与宿主 `FILE_TREE_MAX_DEPTH` 逐字一致）
pub const MAX_DEPTH: usize = 20;
/// 文件内容大小上限（与宿主 `FILE_CONTENT_MAX_SIZE_BYTES` 逐字一致）
pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;
/// 文件树 children 缓存时长（与宿主 `FILE_TREE_CHILDREN_CACHE_MAX_AGE_SECS`）
pub const CHILDREN_CACHE_MAX_AGE_SECS: u32 = 30;

// ==================== 路径与 containment（安全红线） ====================

/// 组件级 `starts_with`（宿主 `Path::starts_with` 语义：按路径段比较，
/// 不做字符串前缀——`/a/bc` 不匹配 `/a/b`）。入参为 canonicalize 后的绝对路径
/// （symlink 已解析），`/` 与 `\` 都按分隔符处理（Windows canonical 输出反斜杠）。
pub fn path_starts_with(canonical_target: &str, canonical_root: &str) -> bool {
    fn parts(s: &str) -> Vec<&str> {
        s.split(['/', '\\']).filter(|p| !p.is_empty()).collect()
    }
    let root_parts = parts(canonical_root);
    let target_parts = parts(canonical_target);
    target_parts.len() >= root_parts.len()
        && root_parts
            .iter()
            .zip(target_parts.iter())
            .all(|(a, b)| a == b)
}

/// 目录穿越判定（宿主 `is_within_root` 同语义）：root / target 都 canonicalize
/// （不存在 → false），再组件级比较。拒绝 `../` 穿越与 symlink 逃逸。
pub fn is_within_root(fs: &impl FsPort, root: &str, target: &str) -> bool {
    let Ok(Some(canonical_root)) = fs.canonicalize(root) else {
        return false;
    };
    let Ok(Some(canonical_target)) = fs.canonicalize(target) else {
        return false;
    };
    path_starts_with(&canonical_target, &canonical_root)
}

/// 绝对路径判定（宿主 `Path::is_absolute` 的字符串近似：POSIX `/` 前缀、
/// Windows 盘符或 UNC 前缀——working_dir 场景两种平台都覆盖）
pub fn is_absolute_path(path: &str) -> bool {
    let b = path.as_bytes();
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    // 盘符：`C:\...` 或 `C:/...`
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/// 拼接相对路径到工作目录（宿主 `PathBuf::join` 近似：避免双分隔符；空相对路径
/// 返回工作目录自身）
pub fn join_path(base: &str, relative: &str) -> String {
    let trimmed = relative.trim_start_matches(['/', '\\']);
    if trimmed.is_empty() {
        return base.trim_end_matches(['/', '\\']).to_string();
    }
    format!("{}/{}", base.trim_end_matches(['/', '\\']), trimmed)
}

// ==================== exclude 过滤（宿主 build_exclude_filters 语义） ====================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExcludeFilter {
    Name(String),
    Path { parent: String, name: String },
}

pub fn build_exclude_filters(exclude_dirs: &[String]) -> Vec<ExcludeFilter> {
    exclude_dirs
        .iter()
        .map(|pattern| {
            if let Some(slash_pos) = pattern.rfind('/') {
                ExcludeFilter::Path {
                    parent: pattern[..slash_pos].to_string(),
                    name: pattern[slash_pos + 1..].to_string(),
                }
            } else {
                ExcludeFilter::Name(pattern.clone())
            }
        })
        .collect()
}

pub fn should_exclude(relative_path: &str, dir_name: &str, filters: &[ExcludeFilter]) -> bool {
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

// ==================== 目录树扫描（宿主 scan_dir / scan_dir_single_level 语义） ====================

/// 相对路径（去掉根前缀；宿主 `strip_prefix` + `to_string_lossy`）
fn relative_path<'a>(dir: &'a str, root: &str) -> &'a str {
    if dir == root {
        ""
    } else if let Some(rest) = dir.strip_prefix(root) {
        rest.trim_start_matches(['/', '\\'])
    } else {
        dir
    }
}

/// 构建节点路径（相对路径 + 名称，`/` 分隔，与宿主逐字一致）
fn node_path(relative: &str, file_name: &str) -> String {
    let normalized = relative.replace('\\', "/");
    if normalized.is_empty() {
        file_name.to_string()
    } else {
        format!("{}/{}", normalized, file_name)
    }
}

/// 目录节点（文件夹：children 恒有；文件：children 省略——与宿主 serde 形状一致）
fn folder_node(name: &str, node_path: &str, children: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "name": name, "nodeType": "folder", "path": node_path, "children": children
    })
}

fn file_node(name: &str, node_path: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name, "nodeType": "file", "path": node_path
    })
}

/// 递归目录树（宿主 scan_dir 逐字语义：depth > MAX_DEPTH 返回空；跳过
/// symlink 等非目录非文件条目——`nodeType == "other"` 不进树；文件夹在前
/// 文件在后，各自 name.to_lowercase() 升序）
pub fn scan_dir(
    fs: &impl FsPort,
    root: &str,
    dir: &str,
    filters: &[ExcludeFilter],
    depth: usize,
) -> Result<Vec<serde_json::Value>, String> {
    if depth > MAX_DEPTH {
        return Ok(Vec::new());
    }
    let entries = fs
        .read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {}", dir, e))?;

    let mut folders: Vec<serde_json::Value> = Vec::new();
    let mut files: Vec<serde_json::Value> = Vec::new();

    for entry in entries {
        match entry.node_type {
            bedcode_plugin_api::host::FsNodeType::Folder => {
                let relative = relative_path(dir, root);
                if should_exclude(&relative, &entry.name, filters) {
                    continue;
                }
                let child_dir = join_path(dir, &entry.name);
                let path = node_path(relative, &entry.name);
                let children = scan_dir(fs, root, &child_dir, filters, depth + 1)?;
                folders.push(folder_node(&entry.name, &path, children));
            }
            bedcode_plugin_api::host::FsNodeType::File => {
                let relative = relative_path(dir, root);
                let path = node_path(relative, &entry.name);
                files.push(file_node(&entry.name, &path));
            }
            bedcode_plugin_api::host::FsNodeType::Other => {
                // symlink / 特殊条目：宿主 scan_dir 跳过
            }
        }
    }

    folders.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
    });
    files.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
    });
    folders.extend(files);
    Ok(folders)
}

/// 单层扫描（宿主 scan_dir_single_level 语义）：不递归，文件夹 children 为 None
/// （省略）。exclude 判定用**归一化**相对路径（宿主此处 `.replace('\\', "/")`）。
pub fn scan_dir_single_level(
    fs: &impl FsPort,
    root: &str,
    dir: &str,
    filters: &[ExcludeFilter],
) -> Result<Vec<serde_json::Value>, String> {
    let entries = fs
        .read_dir(dir)
        .map_err(|e| format!("Failed to read dir {}: {}", dir, e))?;

    let mut folders: Vec<serde_json::Value> = Vec::new();
    let mut files: Vec<serde_json::Value> = Vec::new();

    for entry in entries {
        match entry.node_type {
            bedcode_plugin_api::host::FsNodeType::Folder => {
                let relative = relative_path(dir, root);
                if should_exclude(&relative.replace('\\', "/"), &entry.name, filters) {
                    continue;
                }
                let path = node_path(relative, &entry.name);
                folders.push(serde_json::json!({
                    "name": entry.name, "nodeType": "folder", "path": path
                }));
            }
            bedcode_plugin_api::host::FsNodeType::File => {
                let relative = relative_path(dir, root);
                let path = node_path(relative, &entry.name);
                files.push(file_node(&entry.name, &path));
            }
            bedcode_plugin_api::host::FsNodeType::Other => {}
        }
    }

    folders.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
    });
    files.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
    });
    folders.extend(files);
    Ok(folders)
}

// ==================== 文件内容（宿主 get_file_content 语义） ====================

/// 文件内容读取（纯判定 + 读取；错误文案确定性部分与宿主逐字一致）
///
/// 返回 `Ok(Ok((content, file_name)))` / `Ok(Err((code, message)))`——错误 code
/// 与宿主 HttpStatus 对应（404/403/400/413/415）。
pub fn read_file_content(
    fs: &impl FsPort,
    working_dir: &str,
    file_path: &str,
) -> Result<Result<(String, String), (u16, String)>, String> {
    let abs_path = if is_absolute_path(file_path) {
        file_path.to_string()
    } else {
        join_path(working_dir, file_path)
    };

    if !fs.exists(&abs_path)? {
        return Ok(Err((404, format!("File not found: {}", file_path))));
    }
    if !is_within_root(fs, working_dir, &abs_path) {
        return Ok(Err((
            403,
            "Access denied: file is outside working directory".to_string(),
        )));
    }
    let Some(stat) = fs.stat(&abs_path)? else {
        return Ok(Err((404, format!("File not found: {}", file_path))));
    };
    if !stat.is_file {
        return Ok(Err((400, "Path is not a file".to_string())));
    }
    if stat.size > MAX_FILE_SIZE {
        return Ok(Err((
            413,
            format!(
                "File too large ({} bytes, max {} bytes)",
                stat.size, MAX_FILE_SIZE
            ),
        )));
    }
    let content = match fs.read(&abs_path)? {
        Some(content) => content,
        None => return Ok(Err((404, format!("File not found: {}", file_path)))),
    };
    let file_name = file_name_of(file_path);
    Ok(Ok((content, file_name)))
}

/// 取路径末段文件名（宿主 `Path::file_name` + `to_string_lossy` 近似：`/` 与 `\`
/// 都算分隔符；空/以分隔符结尾返回空串）
pub fn file_name_of(path: &str) -> String {
    path.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("")
        .to_string()
}

// ==================== git diff 树（宿主 get_diff_file_tree 语义） ====================

/// 三个只读 git 命令 → **并行**（v20 host-task execute-batch，池线程真并发）→
/// 去重合并 → exclude 过滤 → 嵌套树（宿主逐字语义；票 03/04 串行 run-sync 先例
/// 的升级——三命令互相独立、只读，无 git 仓库锁冲突）
pub fn diff_file_tree(
    git: &impl GitPort,
    working_dir: &str,
    filters: &[ExcludeFilter],
) -> Result<Vec<serde_json::Value>, String> {
    let batch = [
        vec!["diff".to_string(), "--name-only".to_string()],
        vec![
            "diff".to_string(),
            "--cached".to_string(),
            "--name-only".to_string(),
        ],
        vec![
            "ls-files".to_string(),
            "--others".to_string(),
            "--exclude-standard".to_string(),
        ],
    ];
    let results = git.run_batch(working_dir, &batch);

    // **串行语义保持**：按入参顺序判错，第一条失败即返回其错误（与旧串行实现
    // 的「第 N 步失败 → 报第 N 个错误」逐字一致；fail-collect 只影响其余命令
    // 是否被发起，不影响错误排序）；全部成功 → 按序合并去重
    let mut all_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, result) in results.into_iter().enumerate() {
        let lines = git_result_to_lines(
            result,
            batch[i]
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .as_slice(),
        )?;
        for line in lines {
            all_paths.insert(line);
        }
    }

    // 过滤被排除规则命中的路径中的目录组件（宿主语义：逐组件检查 parent+name）
    let filtered: Vec<String> = all_paths
        .into_iter()
        .filter(|path| {
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

    if filtered.is_empty() {
        return Ok(Vec::new());
    }
    Ok(build_tree_from_paths(&filtered))
}

/// 执行 git 并解析输出为非空行列表（宿主 run_git_command 语义；非零退出码报错）
///
/// 错误文案与宿主 `AppError` Display 逐字一致（`Internal error: ` 前缀 +
/// stderr 原样，不 trim——宿主 lossy 转换后直接拼进文案，尾部换行保留）
fn run_git_lines(
    git: &impl GitPort,
    working_dir: &str,
    args: &[&str],
) -> Result<Vec<String>, String> {
    git_result_to_lines(git.run(working_dir, args), args)
}

/// `ProcessSyncResult` → 非空行列表（transport Err / 超时 / 非零退出码的错误
/// 文案与宿主 AppError Display 逐字一致）。`run` 与 `run_batch`（并行单元）
/// 共用——保证串行/并行两条路径错误语义相同。
fn git_result_to_lines(
    result: Result<bedcode_plugin_api::host::ProcessSyncResult, String>,
    args: &[&str],
) -> Result<Vec<String>, String> {
    let result = result.map_err(|e| format!("Internal error: Failed to execute git: {e}"))?;
    if result.timed_out {
        return Err("Internal error: git command timed out".to_string());
    }
    if result.exit_code != Some(0) {
        return Err(if result.stderr.is_empty() {
            format!("Internal error: git command failed: {}", args.join(" "))
        } else {
            format!("Internal error: git command failed: {}", result.stderr)
        });
    }
    Ok(result
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// 扁平路径列表 → 嵌套树（宿主 build_tree_from_paths 语义：BTreeMap 字节序 +
/// 文件夹在前文件在后；路径冲突忽略）
fn build_tree_from_paths(paths: &[String]) -> Vec<serde_json::Value> {
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
                current.insert(part.to_string(), Entry::File);
            } else {
                let entry = current
                    .entry(part.to_string())
                    .or_insert_with(|| Entry::Dir(BTreeMap::new()));
                match entry {
                    Entry::Dir(children) => current = children,
                    Entry::File => break,
                }
            }
        }
    }

    fn map_to_tree(map: &BTreeMap<String, Entry>, parent_path: &str) -> Vec<serde_json::Value> {
        let mut nodes: Vec<serde_json::Value> = Vec::new();
        for (name, entry) in map {
            let node_path = if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", parent_path, name)
            };
            match entry {
                Entry::File => nodes.push(file_node(name, &node_path)),
                Entry::Dir(children) => {
                    let child_nodes = map_to_tree(children, &node_path);
                    nodes.push(folder_node(name, &node_path, child_nodes));
                }
            }
        }
        // 文件夹在前、文件在后（宿主 map_to_tree 末尾同语义）
        let mut folders: Vec<serde_json::Value> = nodes
            .iter()
            .filter(|n| n["nodeType"] == "folder")
            .cloned()
            .collect();
        let files: Vec<serde_json::Value> = nodes
            .iter()
            .filter(|n| n["nodeType"] == "file")
            .cloned()
            .collect();
        folders.extend(files);
        folders
    }

    map_to_tree(&root, "")
}

// ==================== 单文件 git diff（宿主 parse_git_diff 语义） ====================

/// 单文件 diff：git diff -- <file> → 解析 unified diff 为结构化行
pub fn file_diff(
    git: &impl GitPort,
    working_dir: &str,
    file_path: &str,
) -> Result<(String, Vec<serde_json::Value>), String> {
    let result = git
        .run(working_dir, &["diff", "--", file_path])
        .map_err(|e| format!("Internal error: Failed to execute git diff: {e}"))?;
    if result.exit_code != Some(0) {
        return Err(if result.stderr.is_empty() {
            "Internal error: git diff failed".to_string()
        } else {
            format!("Internal error: git diff failed: {}", result.stderr)
        });
    }
    let file_name = file_name_of(file_path);
    let stdout = result.stdout;
    if stdout.trim().is_empty() {
        return Ok((file_name, Vec::new()));
    }
    Ok((file_name, parse_unified_diff(&stdout)))
}

/// 解析 unified diff 文本（宿主 parse_unified_diff 逐字语义）
pub fn parse_unified_diff(diff_text: &str) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    let mut old_line: u32 = 0;
    let mut new_line: u32 = 0;
    let mut in_hunk = false;

    for line in diff_text.lines() {
        if line.starts_with("diff --git") || line.starts_with("index ") {
            continue;
        }
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if line.starts_with("@@") {
            if let Some((o, n)) = parse_hunk_header(line) {
                old_line = o;
                new_line = n;
                in_hunk = true;
            }
            continue;
        }
        if !in_hunk {
            continue;
        }
        if let Some(content) = line.strip_prefix('-') {
            result.push(serde_json::json!({
                "type": "removed", "content": content,
                "oldLineNo": old_line, "newLineNo": serde_json::Value::Null,
            }));
            old_line += 1;
        } else if let Some(content) = line.strip_prefix('+') {
            result.push(serde_json::json!({
                "type": "added", "content": content,
                "oldLineNo": serde_json::Value::Null, "newLineNo": new_line,
            }));
            new_line += 1;
        } else if let Some(content) = line.strip_prefix(' ') {
            result.push(serde_json::json!({
                "type": "context", "content": content,
                "oldLineNo": old_line, "newLineNo": new_line,
            }));
            old_line += 1;
            new_line += 1;
        } else if line.starts_with('\\') {
            continue;
        }
    }
    result
}

/// 解析 hunk header `@@ -a,b +c,d @@`（宿主 parse_hunk_header 语义）
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
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

// ==================== 工作区 git 查询域（票 04；宿主 git_controller 语义逐字复刻） ====================

/// 分支名白名单校验（宿主 `is_valid_branch_name` 逐字复刻）：只允许字母/数字/
/// `-`/`_`/`/`/`.`，拒绝一切 shell 元字符与路径穿越形态——命令注入安全红线
/// （AGENTS.md §8）。空串拒绝（all() 对空迭代器恒真，须显式排除）；run-sync
/// argv 数组执行不经 shell，白名单是纵深防御不是唯一防线。
pub fn is_valid_branch_name(branch: &str) -> bool {
    !branch.is_empty()
        && branch
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
}

/// 分支列表与当前分支（宿主 `fetch_branches` 逐字语义：`branch --show-current`
/// 首行为当前分支；`branch --list` 剥 `*` 前缀、trim、滤空行）
pub fn git_branches(git: &impl GitPort, working_dir: &str) -> Result<serde_json::Value, String> {
    let current = run_git_lines(git, working_dir, &["branch", "--show-current"])?;
    let current_branch = current.into_iter().next();

    let branches_raw = run_git_lines(git, working_dir, &["branch", "--list"])?;
    let branches: Vec<String> = branches_raw
        .iter()
        .map(|line| line.trim_start_matches('*').trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(serde_json::json!({
        "currentBranch": current_branch,
        "branches": branches,
        "isGitRepo": true,
    }))
}

/// 工作区改动计数（宿主 `check_git_status` 语义：porcelain 非空行数）
pub fn git_status(git: &impl GitPort, working_dir: &str) -> Result<serde_json::Value, String> {
    let lines = run_git_lines(git, working_dir, &["status", "--porcelain"])?;
    let changed_count = lines.len();
    Ok(serde_json::json!({
        "hasChanges": changed_count > 0,
        "changedCount": changed_count,
    }))
}

/// 切换分支（宿主 `run_git_checkout` 逐字语义）：白名单前置，错误文案逐字一致；
/// 成功返回目标分支名（宿主回执同形）
pub fn git_checkout(git: &impl GitPort, working_dir: &str, branch: &str) -> Result<String, String> {
    if !is_valid_branch_name(branch) {
        return Err(format!("Invalid input: Invalid branch name: {branch}"));
    }
    let result = git
        .run(working_dir, &["checkout", branch])
        .map_err(|e| format!("Internal error: Failed to execute git checkout: {e}"))?;
    if result.timed_out {
        return Err("Internal error: git checkout timed out".to_string());
    }
    if result.exit_code != Some(0) {
        return Err(if result.stderr.is_empty() {
            "Internal error: git checkout failed".to_string()
        } else {
            format!("Internal error: git checkout failed: {}", result.stderr)
        });
    }
    Ok(branch.to_string())
}

// ==================== working_dir 解析（宿主 resolve_working_dir 语义） ====================

/// 解析 working_dir：id 可为 session_id 或 config_id
///
/// 与宿主 `SessionConfigManager::get_config_by_session_id` → 回退 `get_config`
/// 同语义：优先在会话列表（host-session list）按 id 找会话取 configId → 配置真源
/// （插件私有库）取 working_dir；失败回退把 id 当 config_id 直接查配置真源。
/// 两路都失败 → `Not found: Session/Config not found: {id}`（宿主 NotFound 文案）。
pub fn resolve_working_dir(
    configs: &impl crate::config::store::ConfigStore,
    sessions_json: Option<&str>,
    id: &str,
) -> Result<String, String> {
    // 会话路径：list → 找 id 匹配 → configId → 配置真源
    if let Some(json) = sessions_json {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
            if let Some(arr) = value.as_array() {
                if let Some(cfg_id) = arr.iter().find_map(|s| {
                    let sid = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if sid == id {
                        s.get("configId")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    } else {
                        None
                    }
                }) {
                    if let Some(config) = crate::config::ops::get(configs, &cfg_id)? {
                        return Ok(config.working_dir);
                    }
                }
            }
        }
    }
    // 回退：id 本身是 config_id
    if let Some(config) = crate::config::ops::get(configs, id)? {
        return Ok(config.working_dir);
    }
    Err(format!("Not found: Session/Config not found: {}", id))
}

// ==================== Tests ====================

// ==================== Tests ====================

// 用例按功能拆至 `ops/tests/`（本内联模块的子模块路径由 rustc
// 自动解析到该目录；模块树 `file_browse::ops::tests::<文件>` 与内联形态等价，私有项可见性不受影响）。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_browse::source::tests::{MockFs, MockGit};
    use std::collections::HashMap;
    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）

    fn temp_workspace() -> (std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(&root).expect("create root");
        (root, dir)
    }
    fn temp_root() -> std::path::PathBuf {
        temp_workspace().0
    }
    // ==================== 工作区 git 查询域（票 04） ====================
    /// 非零退出的 git 双（宿主 run_git_command 失败路径：500 + Internal error 前缀）
    struct FailingGit {
        stderr: String,
    }
    impl GitPort for FailingGit {
        fn run(
            &self,
            _cwd: &str,
            _args: &[&str],
        ) -> Result<bedcode_plugin_api::host::ProcessSyncResult, String> {
            Ok(bedcode_plugin_api::host::ProcessSyncResult {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: self.stderr.clone(),
                timed_out: false,
            })
        }

        fn run_batch(
            &self,
            cwd: &str,
            batch: &[Vec<String>],
        ) -> Vec<Result<bedcode_plugin_api::host::ProcessSyncResult, String>> {
            // 与 MockGit 同语义的显式串行（确定性注入）
            batch
                .iter()
                .map(|args| self.run(cwd, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
                .collect()
        }
    }
    /// 启动失败的 git 双（宿主 spawn 失败路径）
    struct BrokenGit;
    impl GitPort for BrokenGit {
        fn run(
            &self,
            _cwd: &str,
            _args: &[&str],
        ) -> Result<bedcode_plugin_api::host::ProcessSyncResult, String> {
            Err("process error: spawn 'git' failed: No such file or directory".to_string())
        }

        fn run_batch(
            &self,
            cwd: &str,
            batch: &[Vec<String>],
        ) -> Vec<Result<bedcode_plugin_api::host::ProcessSyncResult, String>> {
            batch
                .iter()
                .map(|args| self.run(cwd, &args.iter().map(|s| s.as_str()).collect::<Vec<_>>()))
                .collect()
        }
    }
    mod containment_is_within_root;
    mod diff_parse_unified_diff;
    mod exclude;
    mod git_04;
    mod git_04_2;
    mod git_diff;
    mod join_path_and_file_name;
    mod read_file_content_success;
    mod scan_dir_orders_folders;
    mod working_dir;
}
