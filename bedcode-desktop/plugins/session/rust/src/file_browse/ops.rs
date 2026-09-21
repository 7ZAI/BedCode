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
        vec![ "ls-files".to_string(), "--others".to_string(), "--exclude-standard".to_string() ],
    ];
    let results = git.run_batch(working_dir, &batch);

    // **串行语义保持**：按入参顺序判错，第一条失败即返回其错误（与旧串行实现
    // 的「第 N 步失败 → 报第 N 个错误」逐字一致；fail-collect 只影响其余命令
    // 是否被发起，不影响错误排序）；全部成功 → 按序合并去重
    let mut all_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, result) in results.into_iter().enumerate() {
        let lines = git_result_to_lines(result, batch[i].iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_browse::source::tests::{MockFs, MockGit};
    use std::collections::HashMap;

    fn temp_workspace() -> (std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(&root).expect("create root");
        (root, dir)
    }

    fn temp_root() -> std::path::PathBuf {
        temp_workspace().0
    }

    // ==================== containment（安全红线，与宿主 is_within_root 对照） ====================

    #[test]
    fn traversal_via_parent_chain_rejected() {
        let (root, _dir) = temp_workspace();
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).expect("create sub");
        let outside = root.parent().unwrap().join("evil");
        std::fs::create_dir_all(&outside).expect("create outside");
        let fs = MockFs;

        assert!(
            !is_within_root(&fs, &root.to_string_lossy(), &outside.to_string_lossy()),
            "同级目录越界必须拒绝"
        );
        let traversal = sub.join("../../evil");
        assert!(
            !is_within_root(&fs, &root.to_string_lossy(), &traversal.to_string_lossy()),
            "../ 穿越必须拒绝"
        );
    }

    #[test]
    fn within_root_allowed() {
        let (root, _dir) = temp_workspace();
        let child = root.join("a").join("b.txt");
        std::fs::create_dir_all(child.parent().unwrap()).expect("create parent");
        std::fs::write(&child, b"x").expect("write file");
        let fs = MockFs;
        assert!(is_within_root(
            &fs,
            &root.to_string_lossy(),
            &child.to_string_lossy()
        ));
        assert!(is_within_root(
            &fs,
            &root.to_string_lossy(),
            &root.to_string_lossy()
        ));
    }

    #[test]
    fn nonexistent_path_rejected_without_panic() {
        let root = temp_root();
        let fs = MockFs;
        let ghost = root.join("nope").join("ghost.txt");
        assert!(!is_within_root(
            &fs,
            &root.to_string_lossy(),
            &ghost.to_string_lossy()
        ));
    }

    /// 组件级 starts_with（不匹配前缀相似目录）
    #[test]
    fn path_starts_with_is_component_aware() {
        assert!(path_starts_with("/srv/app/src", "/srv/app"));
        assert!(path_starts_with("/srv/app", "/srv/app"));
        assert!(!path_starts_with("/srv/app2", "/srv/app"));
        assert!(!path_starts_with("/srv/app2/x", "/srv/app"));
        // Windows 反斜杠 canonical 输出
        assert!(path_starts_with(r"C:\work\src", r"C:\work"));
        assert!(!path_starts_with(r"C:\work2", r"C:\work"));
    }

    /// symlink 逃逸：canonicalize 解析后越界 → 拒绝
    #[cfg(unix)]
    #[test]
    fn symlink_escape_rejected() {
        let (root, _dir) = temp_workspace();
        let outside = root.parent().unwrap().join("secret.txt");
        std::fs::write(&outside, b"secret").expect("write outside");
        std::os::unix::fs::symlink(&outside, root.join("link.txt")).expect("symlink");
        let fs = MockFs;
        assert!(
            !is_within_root(
                &fs,
                &root.to_string_lossy(),
                &root.join("link.txt").to_string_lossy()
            ),
            "symlink 指向 root 外 → 拒绝"
        );
    }

    // ==================== exclude 过滤 ====================

    #[test]
    fn exclude_filters_match_host_semantics() {
        let filters =
            build_exclude_filters(&["node_modules".to_string(), "src/generated".to_string()]);
        assert_eq!(filters.len(), 2);
        // Name 匹配任意层级同名目录
        assert!(should_exclude("", "node_modules", &filters));
        assert!(should_exclude("a/b", "node_modules", &filters));
        // Path 匹配 parent + name
        assert!(should_exclude("src", "generated", &filters));
        assert!(!should_exclude("lib", "generated", &filters));
        assert!(!should_exclude("", "generated", &filters));
    }

    // ==================== 目录树扫描 ====================

    /// 文件夹在前、文件在后；各自 name 大小写不敏感排序（与宿主一致）
    #[test]
    fn scan_dir_orders_folders_first_case_insensitive() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("zeta")).unwrap();
        std::fs::create_dir_all(root.join("Alpha")).unwrap();
        std::fs::write(root.join("beta.txt"), "b").unwrap();
        std::fs::write(root.join("Gamma.txt"), "g").unwrap();
        let fs = MockFs;
        let tree = scan_dir(
            &fs,
            &root.to_string_lossy(),
            &root.to_string_lossy(),
            &[],
            0,
        )
        .unwrap();
        let names: Vec<&str> = tree.iter().map(|n| n["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec!["Alpha", "zeta", "beta.txt", "Gamma.txt"],
            "文件夹在前 + 大小写不敏感（beta < Gamma）"
        );
    }

    /// 排除目录不进树；symlink 跳过；文件夹 children 恒有、文件 children 省略
    #[test]
    fn scan_dir_applies_excludes_and_node_shapes() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src").join("main.rs"), "fn main(){}").unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("node_modules").join("x.js"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/nonexistent", root.join("deadlink")).unwrap();

        let fs = MockFs;
        let filters = build_exclude_filters(&["node_modules".to_string()]);
        let tree = scan_dir(
            &fs,
            &root.to_string_lossy(),
            &root.to_string_lossy(),
            &filters,
            0,
        )
        .unwrap();
        assert_eq!(tree.len(), 1, "node_modules 排除 + symlink 跳过");
        let src = &tree[0];
        assert_eq!(src["nodeType"], "folder");
        assert_eq!(src["path"], "src");
        assert!(src.get("children").is_some(), "文件夹 children 恒有");
        assert_eq!(src["children"][0]["name"], "main.rs");
        assert!(
            src["children"][0].get("children").is_none(),
            "文件 children 省略"
        );
    }

    /// 深度上限（宿主 MAX_DEPTH=20 语义）：超出层返回空
    #[test]
    fn scan_dir_stops_at_max_depth() {
        let root = temp_root();
        let mut cur = root.clone();
        for i in 0..25 {
            cur = cur.join(format!("d{i}"));
            std::fs::create_dir_all(&cur).unwrap();
        }
        let fs = MockFs;
        let tree = scan_dir(
            &fs,
            &root.to_string_lossy(),
            &root.to_string_lossy(),
            &[],
            0,
        )
        .unwrap();
        assert_eq!(tree.len(), 1);
        // 第 21 层起返回空（depth=20 可扫，depth=21 空）
        assert!(scan_dir(
            &fs,
            &root.to_string_lossy(),
            &root.to_string_lossy(),
            &[],
            21
        )
        .unwrap()
        .is_empty());
    }

    /// 单层扫描：不递归、文件夹 children 省略（与 file-tree-children 形状一致）
    #[test]
    fn scan_dir_single_level_is_non_recursive() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("a").join("deep")).unwrap();
        std::fs::write(root.join("a").join("f.txt"), "x").unwrap();
        let fs = MockFs;
        let children = scan_dir_single_level(
            &fs,
            &root.to_string_lossy(),
            &root.join("a").to_string_lossy(),
            &[],
        )
        .unwrap();
        assert_eq!(children.len(), 2, "a 下直系两项：deep 文件夹 + f.txt 文件");
        assert_eq!(children[0]["name"], "deep");
        assert_eq!(children[0]["nodeType"], "folder");
        assert!(
            children[0].get("children").is_none(),
            "单层扫描文件夹 children 省略（未加载）"
        );
        assert_eq!(children[1]["name"], "f.txt");
        assert_eq!(children[1]["path"], "a/f.txt");
    }

    // ==================== 文件内容 ====================

    #[test]
    fn read_file_content_success_and_errors() {
        let (root, _dir) = temp_workspace();
        let fs = MockFs;
        let ok_file = root.join("ok.txt");
        std::fs::write(&ok_file, "hello").unwrap();
        let (content, name) = read_file_content(&fs, &root.to_string_lossy(), "ok.txt")
            .unwrap()
            .unwrap();
        assert_eq!(content, "hello");
        assert_eq!(name, "ok.txt");

        // 不存在 → 404
        let err = read_file_content(&fs, &root.to_string_lossy(), "ghost.txt")
            .unwrap()
            .unwrap_err();
        assert_eq!(err.0, 404);
        // 越界 → 403（先创建外部文件：宿主对「不存在路径」先答 404，越过 root 的
        // 存在文件才落到 is_within_root 判定）
        let outside = root.parent().unwrap().join("evil.txt");
        std::fs::write(&outside, "evil").unwrap();
        let err = read_file_content(&fs, &root.to_string_lossy(), "../evil.txt")
            .unwrap()
            .unwrap_err();
        assert_eq!(err.0, 403);
        // 目录 → 400
        std::fs::create_dir_all(root.join("adir")).unwrap();
        let err = read_file_content(&fs, &root.to_string_lossy(), "adir")
            .unwrap()
            .unwrap_err();
        assert_eq!(err.0, 400);
        assert_eq!(err.1, "Path is not a file");
    }

    // ==================== git diff 树 ====================

    #[test]
    fn diff_file_tree_merges_and_builds_tree() {
        let root = temp_root().to_string_lossy().to_string();
        let mut outputs = HashMap::new();
        outputs.insert(
            (root.clone(), "diff --name-only".to_string()),
            "src/main.rs\nCargo.toml".to_string(),
        );
        outputs.insert(
            (root.clone(), "diff --cached --name-only".to_string()),
            "src/main.rs".to_string(),
        );
        outputs.insert(
            (
                root.clone(),
                "ls-files --others --exclude-standard".to_string(),
            ),
            "src/generated/code.rs\nnew.txt".to_string(),
        );
        let git = MockGit::new(outputs);
        let filters = build_exclude_filters(&["generated".to_string()]);
        let tree = diff_file_tree(&git, &root, &filters).unwrap();

        let names: Vec<&str> = tree.iter().map(|n| n["name"].as_str().unwrap()).collect();
        // 去重 src/main.rs；generated 目录被排除；BTreeMap 字节序 + 文件夹在前
        assert_eq!(names, vec!["src", "Cargo.toml", "new.txt"]);
        let src = &tree[0];
        assert_eq!(src["nodeType"], "folder");
        assert_eq!(src["children"][0]["name"], "main.rs");
    }

    // ==================== 单文件 diff 解析（与宿主 parse_unified_diff 对照） ====================

    #[test]
    fn parse_unified_diff_matches_host_semantics() {
        let diff = "\
diff --git a/main.rs b/main.rs
index 123..456 100644
--- a/main.rs
+++ b/main.rs
@@ -1,5 +1,6 @@
 use std::io;
+fn new_fn() {}
-fn old_fn() {}
 fn unchanged() {}
\\ No newline at end of file
";
        let lines = parse_unified_diff(diff);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["type"], "context");
        assert_eq!(lines[0]["content"], "use std::io;");
        assert_eq!(lines[0]["oldLineNo"], 1);
        assert_eq!(lines[0]["newLineNo"], 1);
        assert_eq!(lines[1]["type"], "added");
        assert_eq!(lines[1]["content"], "fn new_fn() {}");
        assert_eq!(lines[1]["oldLineNo"], serde_json::Value::Null);
        assert_eq!(lines[1]["newLineNo"], 2);
        assert_eq!(lines[2]["type"], "removed");
        assert_eq!(lines[2]["oldLineNo"], 2);
        assert_eq!(lines[2]["newLineNo"], serde_json::Value::Null);
        assert_eq!(lines[3]["type"], "context");
        assert_eq!(lines[3]["oldLineNo"], 3);
        assert_eq!(lines[3]["newLineNo"], 3);
    }

    #[test]
    fn parse_hunk_header_variants() {
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_header("@@ -1,5 +1,6 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_header("@@ -12 +34,2 @@"), Some((12, 34)));
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    // ==================== working_dir 解析 ====================

    #[test]
    fn resolve_working_dir_prefers_session_then_config() {
        let store = crate::config::store::tests::MockConfigStore::new(vec![
            crate::config::model::SessionConfig {
                id: "c1".into(),
                name: "工作台".into(),
                environment: "linux".into(),
                wsl_distro: None,
                working_dir: "/srv/app".into(),
                command: "bash".into(),
                auto_start: false,
                created_at: "2026-09-20T00:00:00Z".into(),
                updated_at: "2026-09-20T00:00:00Z".into(),
            },
        ]);
        let sessions = r#"[{"id":"s1","configId":"c1","name":"dev","status":"Running"}]"#;
        // 会话路径
        assert_eq!(
            resolve_working_dir(&store, Some(sessions), "s1").unwrap(),
            "/srv/app"
        );
        // 会话不存在 → 回退 config_id 路径
        assert_eq!(
            resolve_working_dir(&store, Some(sessions), "c1").unwrap(),
            "/srv/app"
        );
        // 两者都无 → NotFound 文案与宿主逐字一致
        let err = resolve_working_dir(&store, Some(sessions), "ghost").unwrap_err();
        assert_eq!(err, "Not found: Session/Config not found: ghost");
    }

    // ==================== 辅助 ====================

    #[test]
    fn join_path_and_file_name() {
        assert_eq!(join_path("/srv/app", "src/main.rs"), "/srv/app/src/main.rs");
        assert_eq!(join_path("/srv/app/", "src"), "/srv/app/src");
        assert_eq!(join_path("/srv/app", ""), "/srv/app");
        assert_eq!(join_path("/srv/app", "."), "/srv/app/.");
        assert_eq!(file_name_of("a/b/c.txt"), "c.txt");
        assert_eq!(file_name_of("c.txt"), "c.txt");
        assert_eq!(file_name_of("a\\b\\d.txt"), "d.txt");
        assert_eq!(file_name_of(""), "");
        assert!(is_absolute_path("/abs"));
        assert!(is_absolute_path(r"C:\win"));
        assert!(is_absolute_path("\\\\server\\share"));
        assert!(!is_absolute_path("rel/path"));
    }

    /// 类型占位：FsStat 形状锁定（与宿主 stat 输出一致）
    #[test]
    fn stat_shape_matches_host() {
        let (root, _dir) = temp_workspace();
        let fs = MockFs;
        std::fs::write(root.join("f.txt"), "12345").unwrap();
        let stat: bedcode_plugin_api::host::FsStat = fs
            .stat(&root.join("f.txt").to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(stat.size, 5);
        assert!(stat.is_file);
        assert!(!stat.is_dir);
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
    }

    /// 分支名白名单（宿主 git_controller 用例逐条对齐）
    #[test]
    fn branch_name_whitelist_matches_host() {
        for name in [
            "main",
            "feature/login",
            "v2.0.1",
            "hotfix_1",
            "release/2026-09",
            "a",
        ] {
            assert!(is_valid_branch_name(name), "合法分支名 {name} 应通过白名单");
        }
        for name in [
            "main;rm -rf /",
            "main && echo pwned",
            "--upload-pack=touch /tmp/x",
            "$(id)",
            "main`id`",
            "a b",
            "feature\\login",
            "main|sh",
        ] {
            assert!(!is_valid_branch_name(name), "注入形态 {name} 必须被拒绝");
        }
        // 空串显式拒绝（all() 对空迭代器恒真——变异点：去掉 is_empty 即假绿）
        assert!(!is_valid_branch_name(""));
        // 控制字符不在白名单（Unicode 字母属 alphanumeric 白名单，argv 执行无 shell 风险）
        assert!(!is_valid_branch_name("main\u{0000}"));
        assert!(!is_valid_branch_name("main\u{001b}"));
    }

    /// 分支列表：show-current 首行 + branch --list 剥 * 前缀滤空行
    #[test]
    fn git_branches_parses_host_way() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let git = MockGit::new(HashMap::from([
            (
                (cwd.clone(), "branch --show-current".to_string()),
                "dev\n".to_string(),
            ),
            (
                (cwd.clone(), "branch --list".to_string()),
                "* main\ndev\n\n  feature/x  \n".to_string(),
            ),
        ]));
        let v = git_branches(&git, &cwd).expect("branches");
        assert_eq!(
            v,
            serde_json::json!({
                "currentBranch": "dev",
                "branches": ["main", "dev", "feature/x"],
                "isGitRepo": true,
            }),
            "* 前缀剥离 + 空行滤除 + trim"
        );
        // 调用顺序：先 show-current 后 --list（宿主同序）
        let calls = git.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, vec!["branch", "--show-current"]);
        assert_eq!(calls[1].1, vec!["branch", "--list"]);
    }

    /// 空 show-current（unborn HEAD / detached）→ currentBranch null（宿主同格）
    #[test]
    fn git_branches_empty_current_is_null() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let git = MockGit::new(HashMap::from([
            (
                (cwd.clone(), "branch --show-current".to_string()),
                String::new(),
            ),
            ((cwd.clone(), "branch --list".to_string()), String::new()),
        ]));
        let v = git_branches(&git, &cwd).expect("branches");
        assert_eq!(v["currentBranch"], serde_json::Value::Null);
        assert_eq!(v["branches"], serde_json::json!([]));
        assert_eq!(v["isGitRepo"], true);
    }

    /// status 计数：porcelain 非空行数 → hasChanges/changedCount
    #[test]
    fn git_status_counts_porcelain_lines() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let dirty = MockGit::new(HashMap::from([(
            (cwd.clone(), "status --porcelain".to_string()),
            " M a.rs\n?? b.txt\n".to_string(),
        )]));
        let v = git_status(&dirty, &cwd).expect("status");
        assert_eq!(
            v,
            serde_json::json!({ "hasChanges": true, "changedCount": 2 })
        );

        let clean = MockGit::new(HashMap::new());
        let v = git_status(&clean, &cwd).expect("status");
        assert_eq!(
            v,
            serde_json::json!({ "hasChanges": false, "changedCount": 0 })
        );
    }

    /// checkout：白名单前置拒绝（不经 git，文案逐字）→ 成功返回目标分支
    #[test]
    fn git_checkout_validates_then_reports_branch() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let git = MockGit::new(HashMap::new());

        let err = git_checkout(&git, &cwd, "main;rm -rf /").expect_err("白名单拒绝");
        assert_eq!(err, "Invalid input: Invalid branch name: main;rm -rf /");
        assert!(
            git.calls.lock().unwrap().is_empty(),
            "白名单拒绝不得启动 git 进程"
        );

        let branch = git_checkout(&git, &cwd, "feature/x").expect("checkout");
        assert_eq!(branch, "feature/x");
        let calls = git.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, vec!["checkout", "feature/x"]);
    }

    /// 非零退出 → 500 文案与宿主 AppError::Internal Display 逐字一致
    #[test]
    fn git_checkout_maps_nonzero_exit_to_host_message() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let git = FailingGit {
            stderr: "error: pathspec 'nope' did not match any file(s) known to git\n".to_string(),
        };
        let err = git_checkout(&git, &cwd, "nope").expect_err("非零退出");
        assert_eq!(
            err,
            "Internal error: git checkout failed: error: pathspec 'nope' did not match any file(s) known to git\n",
            "stderr 原样拼接（含尾部换行，宿主同形）"
        );

        let empty = FailingGit {
            stderr: String::new(),
        };
        let err = git_checkout(&empty, &cwd, "nope").expect_err("空 stderr");
        assert_eq!(err, "Internal error: git checkout failed");
    }

    /// diff 树命令失败 → 文案带宿主前缀（run_git_lines 映射）
    #[test]
    fn git_failure_messages_carry_host_internal_prefix() {
        let (root, _dir) = temp_workspace();
        let cwd = root.to_string_lossy().to_string();
        let git = FailingGit {
            stderr: "fatal: not a git repository\n".to_string(),
        };
        let err = run_git_lines(&git, &cwd, &["status", "--porcelain"]).expect_err("失败");
        assert_eq!(
            err,
            "Internal error: git command failed: fatal: not a git repository\n"
        );

        let broken = BrokenGit;
        let err = run_git_lines(&broken, &cwd, &["status"]).expect_err("spawn 失败");
        assert!(
            err.starts_with("Internal error: Failed to execute git: "),
            "got: {err}"
        );

        let err = file_diff(&broken, &cwd, "a.rs").expect_err("diff spawn 失败");
        assert!(
            err.starts_with("Internal error: Failed to execute git diff: "),
            "got: {err}"
        );
    }
}
