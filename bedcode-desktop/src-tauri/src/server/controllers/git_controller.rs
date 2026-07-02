//! Git Controller
//!
//! Routes:
//! - GET /api/git/branches?session_id=X
//! - GET /api/git/status?session_id=X
//! - POST /api/git/checkout

use actix_web::{web, HttpResponse};
use crate::system::app_context::AppContext;
use crate::server::dtos::ApiResponse;
use crate::process::create_command;
use super::file_controller::resolve_working_dir;
use crate::server::dtos::git_dto::*;

/// GET /api/git/branches?session_id=X
///
/// 获取 git 分支列表。非 git 仓库返回 is_git_repo: false
pub async fn get_branches(query: web::Query<GitBranchesQuery>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&query.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    // 检查是否为 git 仓库
    let git_dir = std::path::Path::new(&working_dir).join(".git");
    if !git_dir.exists() {
        let data = GitBranchesResponseData {
            current_branch: None,
            branches: vec![],
            is_git_repo: false,
        };
        return HttpResponse::Ok().json(ApiResponse::ok_with_data(data));
    }

    let working_dir_clone = working_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        fetch_branches(&working_dir_clone)
    }).await;

    match result {
        Ok(Ok(data)) => HttpResponse::Ok().json(ApiResponse::ok_with_data(data)),
        Ok(Err(e)) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string())),
        Err(e) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &format!("Git branches task failed: {}", e))),
    }
}

/// POST /api/git/checkout
///
/// 切换到指定分支
pub async fn checkout(body: web::Json<GitCheckoutRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&body.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let branch = body.branch.clone();
    let working_dir_clone = working_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        run_git_checkout(&working_dir_clone, &branch)
    }).await;

    match result {
        Ok(Ok(new_branch)) => {
            let data = GitCheckoutResponseData { branch: new_branch };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Ok(Err(e)) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string())),
        Err(e) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &format!("Git checkout task failed: {}", e))),
    }
}

/// GET /api/git/status?session_id=X
///
/// 检查工作区是否有未提交的更改
pub async fn get_status(query: web::Query<GitBranchesQuery>) -> HttpResponse {
    let ctx = AppContext::global();

    let working_dir = match resolve_working_dir(&query.session_id, ctx).await {
        Ok(dir) => dir,
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) { 404 } else { 500 };
            return HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()));
        }
    };

    let working_dir_clone = working_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        check_git_status(&working_dir_clone)
    }).await;

    match result {
        Ok(Ok(data)) => HttpResponse::Ok().json(ApiResponse::ok_with_data(data)),
        Ok(Err(e)) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string())),
        Err(e) => HttpResponse::Ok().json(ApiResponse::<()>::error(500, &format!("Git status task failed: {}", e))),
    }
}

// ==================== Git Helpers ====================

/// Query params for GET /api/git/branches
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GitBranchesQuery {
    pub session_id: String,
}

/// 获取分支列表和当前分支
fn fetch_branches(working_dir: &str) -> crate::Result<GitBranchesResponseData> {
    // 获取当前分支名
    let current = run_git_command(working_dir, &["branch", "--show-current"])?;

    let current_branch = current.into_iter().next();

    // 获取所有本地分支（去掉 * 前缀和空格）
    let branches_raw = run_git_command(working_dir, &["branch", "--list"])?;
    let branches: Vec<String> = branches_raw
        .iter()
        .map(|line| line.trim_start_matches('*').trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(GitBranchesResponseData {
        current_branch,
        branches,
        is_git_repo: true,
    })
}

/// 执行 git checkout
fn run_git_checkout(working_dir: &str, branch: &str) -> crate::Result<String> {
    // 校验分支名，防止命令注入（只允许字母、数字、-、_、/、.）
    if !branch.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.') {
        return Err(crate::AppError::InvalidInput(format!("Invalid branch name: {}", branch)));
    }

    let output = create_command("git")
        .args(["checkout", branch])
        .current_dir(working_dir)
        .output()
        .map_err(|e| crate::AppError::Internal(format!("Failed to execute git checkout: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::AppError::Internal(format!("git checkout failed: {}", stderr)));
    }

    Ok(branch.to_string())
}

/// 执行 git 命令并解析输出为行列表
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
    let lines: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(lines)
}

/// 检查工作区是否有未提交的更改（git status --porcelain）
fn check_git_status(working_dir: &str) -> crate::Result<GitStatusResponseData> {
    let lines = run_git_command(working_dir, &["status", "--porcelain"])?;
    let changed_count = lines.len();
    Ok(GitStatusResponseData {
        has_changes: changed_count > 0,
        changed_count,
    })
}
