//! Plugin Controller
//!
//! Routes:
//! - POST /api/plugin/task-status — 接收 Claude Code 插件推送的任务状态（plugin token 认证）
//! - POST /api/plugin/session-mode — 设置会话自动授权模式（plugin token 或 JWT 认证）
//! - GET /api/plugin/session-mode — 查询会话自动授权模式（plugin token 认证）

use actix_web::{web, HttpRequest, HttpResponse};

use crate::system::app_context::AppContext;
use crate::utils::auth::jwt::JwtService;
use crate::server::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::server::dtos::plugin_dto::{SessionModeRequest, TaskStatusRequest};
use crate::enums::TaskStatus;
use crate::system::config::AppConfig;

/// POST /api/plugin/task-status
///
/// 接收 Claude Code 插件推送的任务状态变更
/// 仅 plugin token 认证
pub async fn update_task_status(body: web::Json<TaskStatusRequest>) -> HttpResponse {
    tracing::debug!(
        "POST /api/plugin/task-status: session_id={}, status={}",
        body.session_id,
        body.status
    );

    // 验证 plugin token
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || body.token != config.plugin.token {
        tracing::warn!(
            "Plugin task-status auth failed: session_id={}, token_empty={}",
            body.session_id,
            config.plugin.token.is_empty()
        );
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    // 反序列化 status 字符串为 TaskStatus 枚举
    let task_status: TaskStatus = match serde_json::from_value(serde_json::Value::String(body.status.clone())) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!(
                "Invalid task status value: session_id={}, status={}",
                body.session_id,
                body.status
            );
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Invalid task status: {}. Must be one of: idle, in_progress, asking, completed, interrupted", body.status),
            ));
        }
    };

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    // 如果携带 bedcode_session_id，注册 Claude Code session → BedCode PTY session 映射
    if let Some(ref bedcode_sid) = body.bedcode_session_id {
        plugin_manager
            .register_session_mapping(&body.session_id, bedcode_sid)
            .await;
    }

    // 优先使用 bedcode_session_id 作为存储 key，
    // 这样前端可以通过 BedCode PTY session ID 查询到对应的任务状态
    let storage_key = body.bedcode_session_id.as_deref().unwrap_or(&body.session_id);

    // 更新任务状态并广播
    plugin_manager
        .update_task_status(storage_key, task_status, body.reason.clone(), body.questions.clone())
        .await
        .ok();

    tracing::info!(
        "Plugin task status updated: claude_sid={} bedcode_sid={} status={}",
        body.session_id,
        body.bedcode_session_id.as_deref().unwrap_or("N/A"),
        body.status
    );
    HttpResponse::Ok().json(ApiResponse::ok())
}

/// POST /api/plugin/session-mode
///
/// 设置会话自动授权模式（移动端切换自动/手动模式时调用）
/// 双认证：plugin token 或 JWT Authorization header 均可
pub async fn set_session_mode(req: HttpRequest, body: web::Json<SessionModeRequest>) -> HttpResponse {
    tracing::debug!(
        "POST /api/plugin/session-mode: session_id={}, auto_approve={}",
        body.session_id,
        body.auto_approve
    );

    // 双认证：plugin token 或 JWT
    let config = AppConfig::global();
    let plugin_token_valid = !config.plugin.token.is_empty() && body.token == config.plugin.token;

    let jwt_valid = validate_jwt_from_request(&req);

    if !plugin_token_valid && !jwt_valid {
        tracing::warn!(
            "Plugin session-mode auth failed: session_id={}, plugin_token_valid={}, jwt_valid={}",
            body.session_id,
            plugin_token_valid,
            jwt_valid
        );
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token or JWT authentication",
        ));
    }

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    plugin_manager.set_auto_mode(&body.session_id, body.auto_approve).await;

    tracing::info!(
        "Plugin session mode set: session_id={}, auto_approve={}, auth_by={}",
        body.session_id,
        body.auto_approve,
        if jwt_valid { "jwt" } else { "plugin_token" }
    );
    HttpResponse::Ok().json(ApiResponse::ok())
}

/// GET /api/plugin/session-mode?session_id=xxx
///
/// 查询会话自动授权模式（Python PreToolUse hook 调用）
/// 仅 plugin token 认证
pub async fn get_session_mode(query: web::Query<SessionModeQuery>) -> HttpResponse {
    tracing::debug!(
        "GET /api/plugin/session-mode: session_id={}",
        query.session_id
    );

    // 验证 plugin token
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || query.token != config.plugin.token {
        tracing::warn!(
            "Plugin session-mode query auth failed: session_id={}, token_empty={}",
            query.session_id,
            config.plugin.token.is_empty()
        );
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    // Python hook 传入的是 Claude Code session_id，需解析为 BedCode PTY session_id
    let resolved_id = plugin_manager.resolve_session_id(&query.session_id).await;
    let auto_approve = plugin_manager.get_auto_mode(&resolved_id).await;

    tracing::debug!(
        "Plugin session mode queried: claude_sid={} resolved_sid={} auto_approve={}",
        query.session_id,
        resolved_id,
        auto_approve
    );

    HttpResponse::Ok().json(ApiResponse::ok_with_data(serde_json::json!({
        "session_id": query.session_id,
        "auto_approve": auto_approve,
    })))
}

/// 从 HTTP 请求中验证 JWT Authorization header
fn validate_jwt_from_request(req: &HttpRequest) -> bool {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) => {
            let token = header.strip_prefix("Bearer ");
            match token {
                Some(t) => {
                    let jwt_service = JwtService::new();
                    jwt_service.verify_token_with_expiry(t).is_ok()
                }
                None => false,
            }
        }
        None => false,
    }
}

/// GET /api/plugin/session-mode 查询参数
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionModeQuery {
    /// Claude Code 会话 ID
    pub session_id: String,
    /// 认证 token
    pub token: String,
}
