//! 文件浏览域（票 03）/ 工作区 git 域（票 04）：本地文件树 / 文件内容 / diff 树 /
//! 文件 diff / git 分支 / git 状态 / git checkout 七个查询端点的插件侧实现——经
//! host-fs（v19 追加 read-dir / canonicalize / stat）+ host-process（v19 追加
//! run-sync）与 fs_auth 三层校验接管路径越界防护与大小/深度限制。
//!
//! 职责边界（spec 决策 4）：
//! - **在插件**：working_dir 解析（会话/配置 → 配置真源）、containment 判定
//!   （canonicalize + 组件级 starts_with）、exclude 过滤、递归/单层树扫描、
//!   内容读取（2MB 上限）、git diff 执行与 unified diff 解析、分支/状态/checkout
//!   编排与分支名白名单
//! - **留宿主**：fs_auth 授权管线（session 插件在插件白名单自动放行）、
//!   网关别名路由（Forward / PluginRequired 判定）、HTTP 验签
//!
//! HTTP 端点（网关别名表 `server/gateway.rs` BUSINESS_ROUTES 的插件目标）：
//! - `POST file-tree`（网关 /api/file-tree）
//! - `GET file-tree-children`（网关 /api/file-tree-children）
//! - `POST file-content`（网关 /api/file-content）
//! - `POST diff-tree`（网关 /api/diff-tree）
//! - `POST file-diff`（网关 /api/file-diff）
//! - `GET git/branches`（网关 /api/git/branches）
//! - `GET git/status`（网关 /api/git/status）
//! - `POST git/checkout`（网关 /api/git/checkout）
//!
//! 字节级契约：宿主 `file_controller` / `git_controller` 的响应形状与确定性错误
//! 文案逐字复刻（见 [`ops`] 模块文档）；动态 io 错误文案（500/415 尾部）允许差异。

pub mod ops;
pub mod source;

#[cfg(target_arch = "wasm32")]
use crate::file_browse::source::FsPort;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostSession;
use bedcode_plugin_api::http_response;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 本域 HTTP 端点清单（plugin.json `contributes.httpEndpoints` 的单一事实源，
/// 与网关别名表逐字一致；契约用例锁死）。票 03 五条 + 票 04 三条。
pub const HTTP_ENDPOINTS: &[&str] = &[
    "file-tree",
    "file-tree-children",
    "file-content",
    "diff-tree",
    "file-diff",
    "git/branches",
    "git/status",
    "git/checkout",
];

// ==================== HTTP 端点分派（wasm 运行时有实现；native 显性失败） ====================

/// 本域 HTTP 分派入口（lib.rs 业务分派落点；路径全等匹配，未知路径由任务域 404）
#[cfg(target_arch = "wasm32")]
pub fn handle_http_endpoint(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &serde_json::Value,
    query: &serde_json::Value,
) -> serde_json::Value {
    match path {
        "file-tree" => handle_file_tree(host, method, body),
        "file-tree-children" => handle_file_tree_children(host, method, query),
        "file-content" => handle_file_content(host, method, body),
        "diff-tree" => handle_diff_tree(host, method, body),
        "file-diff" => handle_file_diff(host, method, body),
        "git/branches" => handle_git_branches(host, method, query),
        "git/status" => handle_git_status(host, method, query),
        "git/checkout" => handle_git_checkout(host, method, body),
        _ => http_response::error(404, &format!("Not found: {} {}", method, path)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn handle_http_endpoint(
    _host: &WasmHost,
    _method: &str,
    _path: &str,
    _body: &serde_json::Value,
    _query: &serde_json::Value,
) -> serde_json::Value {
    http_response::error(500, "file browse unavailable outside wasm runtime")
}

// ==================== 公共前置：working_dir 解析 ====================

/// working_dir 解析（wasm 侧：会话列表经 host-session，配置真源经私有库）
#[cfg(target_arch = "wasm32")]
fn resolve_working_dir_via_host(id: &str) -> Result<String, String> {
    let sessions = match WasmHost.session_list() {
        Ok(Some(json)) => Some(json.to_string()),
        Ok(None) => None,
        Err(e) => return Err(format!("session list failed: {}", e.message)),
    };
    ops::resolve_working_dir(&WasmHost, sessions.as_deref(), id)
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_working_dir_via_host(_id: &str) -> Result<String, String> {
    Err("file browse unavailable outside wasm runtime".to_string())
}

/// working_dir 解析失败 → 与宿主同口径的 HTTP 错误响应（NotFound → 404，其余 500）
#[cfg(target_arch = "wasm32")]
fn working_dir_error_response(e: &str) -> serde_json::Value {
    let code = if e.starts_with("Not found:") {
        404
    } else {
        500
    };
    http_response::error(code, e)
}

// ==================== POST /api/file-tree ====================

#[cfg(target_arch = "wasm32")]
fn handle_file_tree(host: &WasmHost, method: &str, body: &serde_json::Value) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let exclude_dirs: Vec<String> = body
        .get("excludeDirs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    // 工作目录必须是目录（与宿主 400 文案逐字一致）
    let is_dir = WasmHost
        .stat(&working_dir)
        .map(|s| s.map(|st| st.is_dir).unwrap_or(false))
        .unwrap_or(false);
    if !is_dir {
        return http_response::error(
            400,
            &format!("Working dir is not a directory: {}", working_dir),
        );
    }

    let filters = ops::build_exclude_filters(&exclude_dirs);
    match ops::scan_dir(host, &working_dir, &working_dir, &filters, 0) {
        Ok(tree) => http_response::ok_with_data(serde_json::json!({ "tree": tree })),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== GET /api/file-tree-children ====================

#[cfg(target_arch = "wasm32")]
fn handle_file_tree_children(
    host: &WasmHost,
    method: &str,
    query: &serde_json::Value,
) -> serde_json::Value {
    if method != "GET" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    // 键名是 snake_case：宿主 `FileTreeChildrenQuery` 无 serde rename（GET query
    // 走字段原名），移动端 `useHttpApi::httpGetFileTreeChildren` 以
    // `session_id` / `dir_path` / `exclude_dirs` 构造 query——与 POST body 的
    // camelCase（DTO 带 rename_all）刻意不同，改错一边真机即 404
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let dir_path = query.get("dir_path").and_then(|v| v.as_str()).unwrap_or("");
    let exclude_dirs: Vec<String> = query
        .get("exclude_dirs")
        .and_then(|v| v.as_str())
        .map(|s| {
            s.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    // 解析 dir_path：空/None/"." 表示根目录；绝对路径拒绝
    let dir_path_str = dir_path.trim();
    let target_dir = if dir_path_str.is_empty() || dir_path_str == "." {
        working_dir.clone()
    } else {
        if ops::is_absolute_path(dir_path_str) {
            return http_response::error(400, "dir_path must be relative to working directory");
        }
        ops::join_path(&working_dir, dir_path_str)
    };

    // 目标必须是目录（404）
    let is_dir = WasmHost
        .stat(&target_dir)
        .map(|s| s.map(|st| st.is_dir).unwrap_or(false))
        .unwrap_or(false);
    if !is_dir {
        return http_response::error(404, &format!("Directory not found: {}", dir_path_str));
    }
    // containment（403）
    if !ops::is_within_root(host, &working_dir, &target_dir) {
        return http_response::error(403, "Access denied: directory is outside working directory");
    }

    let filters = ops::build_exclude_filters(&exclude_dirs);
    match ops::scan_dir_single_level(host, &working_dir, &target_dir, &filters) {
        Ok(children) => http_response::ok_with_data_headers(
            serde_json::json!({ "children": children }),
            serde_json::json!({
                "Cache-Control": format!("private, max-age={}", ops::CHILDREN_CACHE_MAX_AGE_SECS)
            }),
        ),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== POST /api/file-content ====================

#[cfg(target_arch = "wasm32")]
fn handle_file_content(
    host: &WasmHost,
    method: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = body.get("filePath").and_then(|v| v.as_str()).unwrap_or("");

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    match ops::read_file_content(host, &working_dir, file_path) {
        Ok(Ok((content, file_name))) => http_response::ok_with_data(serde_json::json!({
            "content": content,
            "fileName": file_name
        })),
        Ok(Err((code, message))) => http_response::error(code, &message),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== POST /api/diff-tree ====================

#[cfg(target_arch = "wasm32")]
fn handle_diff_tree(host: &WasmHost, method: &str, body: &serde_json::Value) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let exclude_dirs: Vec<String> = body
        .get("excludeDirs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    let is_dir = WasmHost
        .stat(&working_dir)
        .map(|s| s.map(|st| st.is_dir).unwrap_or(false))
        .unwrap_or(false);
    if !is_dir {
        return http_response::error(
            400,
            &format!("Working dir is not a directory: {}", working_dir),
        );
    }

    // 非 git 仓库判定（与宿主 400 文案逐字一致）
    let git_dir = ops::join_path(&working_dir, ".git");
    if !WasmHost.exists(&git_dir).unwrap_or(false) {
        return http_response::error(400, "Not a git repository");
    }

    let filters = ops::build_exclude_filters(&exclude_dirs);
    match ops::diff_file_tree(host, &working_dir, &filters) {
        Ok(tree) => http_response::ok_with_data(serde_json::json!({ "tree": tree })),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== POST /api/file-diff ====================

#[cfg(target_arch = "wasm32")]
fn handle_file_diff(host: &WasmHost, method: &str, body: &serde_json::Value) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = body.get("filePath").and_then(|v| v.as_str()).unwrap_or("");

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    let is_dir = WasmHost
        .stat(&working_dir)
        .map(|s| s.map(|st| st.is_dir).unwrap_or(false))
        .unwrap_or(false);
    if !is_dir {
        return http_response::error(
            400,
            &format!("Working dir is not a directory: {}", working_dir),
        );
    }

    let git_dir = ops::join_path(&working_dir, ".git");
    if !WasmHost.exists(&git_dir).unwrap_or(false) {
        return http_response::error(400, "Not a git repository");
    }

    match ops::file_diff(host, &working_dir, file_path) {
        Ok((file_name, lines)) => http_response::ok_with_data(serde_json::json!({
            "fileName": file_name,
            "lines": lines
        })),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== GET /api/git/branches（票 04） ====================

#[cfg(target_arch = "wasm32")]
fn handle_git_branches(
    host: &WasmHost,
    method: &str,
    query: &serde_json::Value,
) -> serde_json::Value {
    if method != "GET" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    // 宿主 `GitBranchesQuery` 无 serde rename：query 键名 snake_case（与
    // file-tree-children 同理，移动端以 session_id 构造）
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    // 非 git 仓库不是错误：200 + isGitRepo:false（宿主同格）
    let git_dir = ops::join_path(&working_dir, ".git");
    if !WasmHost.exists(&git_dir).unwrap_or(false) {
        return http_response::ok_with_data(serde_json::json!({
            "currentBranch": null,
            "branches": [],
            "isGitRepo": false,
        }));
    }

    match ops::git_branches(host, &working_dir) {
        Ok(data) => http_response::ok_with_data(data),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== GET /api/git/status（票 04） ====================

#[cfg(target_arch = "wasm32")]
fn handle_git_status(
    host: &WasmHost,
    method: &str,
    query: &serde_json::Value,
) -> serde_json::Value {
    if method != "GET" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    let session_id = query
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    // 宿主不做 .git 预检：非仓库由 git 自身非零退出 → 500（同格）
    match ops::git_status(host, &working_dir) {
        Ok(data) => http_response::ok_with_data(data),
        Err(e) => http_response::error(500, &e),
    }
}

// ==================== POST /api/git/checkout（票 04） ====================

#[cfg(target_arch = "wasm32")]
fn handle_git_checkout(
    host: &WasmHost,
    method: &str,
    body: &serde_json::Value,
) -> serde_json::Value {
    if method != "POST" {
        return http_response::error(405, &format!("Method not allowed: {method}"));
    }
    // 宿主 `GitCheckoutRequest` 带 rename_all=camelCase：body 键名 camelCase
    let session_id = body.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let branch = body.get("branch").and_then(|v| v.as_str()).unwrap_or("");

    let working_dir = match resolve_working_dir_via_host(session_id) {
        Ok(dir) => dir,
        Err(e) => return working_dir_error_response(&e),
    };

    match ops::git_checkout(host, &working_dir, branch) {
        Ok(switched) => http_response::ok_with_data(serde_json::json!({ "branch": switched })),
        Err(e) => http_response::error(500, &e),
    }
}
