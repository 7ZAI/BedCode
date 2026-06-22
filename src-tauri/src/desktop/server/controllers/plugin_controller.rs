//! Plugin Controller
//!
//! Routes:
//! - POST /api/plugin/task-status
//! - POST /api/plugin/session-mode
//! - GET /api/plugin/session-mode?session_id=xxx

use actix_web::{web, HttpResponse};

use crate::desktop::app_context::AppContext;
use crate::shared::model::api_dto::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::desktop::server::dtos::plugin_dto::{SessionModeRequest, TaskStatusRequest};
use crate::shared::enums::TaskStatus;
use crate::shared::system::config::AppConfig;

/// POST /api/plugin/task-status
///
/// 接收 Claude Code 插件推送的任务状态变更
pub async fn update_task_status(body: web::Json<TaskStatusRequest>) -> HttpResponse {
    // 验证 token
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || body.token != config.plugin.token {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    // 反序列化 status 字符串为 TaskStatus 枚举
    let task_status: TaskStatus = match serde_json::from_value(serde_json::Value::String(body.status.clone())) {
        Ok(s) => s,
        Err(_) => {
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Invalid task status: {}. Must be one of: idle, in_progress, asking, completed, interrupted", body.status),
            ));
        }
    };

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    // 更新任务状态并广播
    plugin_manager
        .update_task_status(&body.session_id, task_status, body.reason.clone(), body.questions.clone())
        .await
        .ok(); // update_task_status 目前不会失败，但保留 .ok() 防御性处理

    tracing::info!(
        "Plugin task status updated: session_id={}, status={}",
        body.session_id,
        body.status
    );
    HttpResponse::Ok().json(ApiResponse::ok())
}

/// POST /api/plugin/session-mode
///
/// 设置会话自动授权模式（移动端切换自动/手动模式时调用）
pub async fn set_session_mode(body: web::Json<SessionModeRequest>) -> HttpResponse {
    // 验证 token
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || body.token != config.plugin.token {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    plugin_manager.set_auto_mode(&body.session_id, body.auto_approve).await;

    tracing::info!(
        "Plugin session mode set: session_id={}, auto_approve={}",
        body.session_id,
        body.auto_approve
    );
    HttpResponse::Ok().json(ApiResponse::ok())
}

/// GET /api/plugin/session-mode?session_id=xxx
///
/// 查询会话自动授权模式（Python PreToolUse hook 调用）
pub async fn get_session_mode(query: web::Query<SessionModeQuery>) -> HttpResponse {
    // 验证 token
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || query.token != config.plugin.token {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    let ctx = AppContext::global();
    let plugin_manager = ctx.plugin_manager();

    let auto_approve = plugin_manager.get_auto_mode(&query.session_id).await;

    HttpResponse::Ok().json(ApiResponse::ok_with_data(serde_json::json!({
        "session_id": query.session_id,
        "auto_approve": auto_approve,
    })))
}

/// GET /api/plugin/session-mode 查询参数
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionModeQuery {
    /// Claude Code 会话 ID
    pub session_id: String,
    /// 认证 token
    pub token: String,
}
