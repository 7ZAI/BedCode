//! Plugin Controller
//!
//! Routes:
//! - POST /api/plugin/task-status

use actix_web::{web, HttpResponse};

use crate::desktop::app_context::AppContext;
use crate::shared::model::api_dto::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED, CODE_SESSION_NOT_FOUND};
use crate::desktop::server::dtos::plugin_dto::TaskStatusRequest;
use crate::shared::enums::TaskStatus;
use crate::shared::system::config::AppConfig;

/// POST /api/plugin/task-status
///
/// 接收 Claude Code 插件推送的任务状态变更
pub async fn update_task_status(body: web::Json<TaskStatusRequest>) -> HttpResponse {
    // 验证 token（token 为空时拒绝所有请求，避免无认证开放 API）
    let config = AppConfig::global();
    if config.plugin.token.is_empty() || body.token != config.plugin.token {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token",
        ));
    }

    // 使用 serde 反序列化 status 字符串，与 TaskStatus 枚举定义保持同步
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

    // 通过 session_id 查找 Plugin 会话并更新状态
    match plugin_manager.update_task_status(&body.session_id, task_status, body.reason.clone()).await {
        Ok(()) => {
            tracing::info!(
                "Plugin task status updated: session_id={}, status={}",
                body.session_id,
                body.status
            );
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            let code = if matches!(e, crate::AppError::NotFound(_)) {
                CODE_SESSION_NOT_FOUND
            } else {
                500
            };
            HttpResponse::Ok().json(ApiResponse::<()>::error(code, &e.to_string()))
        }
    }
}
