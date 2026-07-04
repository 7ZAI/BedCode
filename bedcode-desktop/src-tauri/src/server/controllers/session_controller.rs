//! Session Controller
//!
//! HTTP REST API endpoints for session management
//! Routes:
//! - GET    /api/sessions
//! - POST   /api/sessions/start
//! - POST   /api/sessions/{id}/stop
//! - POST   /api/sessions/{id}/resize
//! - POST   /api/sessions/{id}/input
//! - DELETE /api/sessions/{id}/remove

use actix_web::{web, HttpRequest, HttpResponse};
use tauri::Emitter;
use crate::system::app_context::AppContext;
use crate::server::dtos::ApiResponse;
use crate::server::dtos::session_dto::*;
use crate::server::middleware::jwt_auth::get_claims_from_request;

/// GET /api/sessions
pub async fn list_sessions(_req: HttpRequest) -> HttpResponse {
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let sessions: Vec<SessionItem> = session_manager
        .list_sessions()
        .await
        .into_iter()
        .map(|s| SessionItem {
            id: s.id,
            name: s.name,
            status: serde_json::to_value(&s.status)
                .and_then(|v| serde_json::from_value::<String>(v))
                .unwrap_or_else(|_| format!("{:?}", s.status)),
            created_at: s.created_at.to_rfc3339(),
            started_at: s.started_at.map(|t| t.to_rfc3339()),
            session_type: Some("pty".to_string()),
            config_id: Some(s.config_id),
            task_status: s.task_status.map(|ts| {
                serde_json::to_string(&ts)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string()
            }),
            task_reason: s.task_reason,
        })
        .collect();

    let data = SessionListResponseData { sessions };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/sessions/start
pub async fn start_session(
    req: HttpRequest,
    body: web::Json<StartSessionRequest>,
) -> HttpResponse {
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);

    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    match session_manager.create_session_with_source(&body.config_id, device_name).await {
        Ok(session_id) => {
            let app_handle = ctx.app_handle();
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));

            let data = StartSessionResponseData {
                session_id,
                status: "running".to_string(),
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!(error = %e, config_id = %body.config_id, "Failed to start session");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/stop
pub async fn stop_session(
    req: HttpRequest,
    path: web::Path<String>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);
    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    match session_manager.kill_session_with_source(&session_id, device_name).await {
        Ok(()) => {
            let app_handle = ctx.app_handle();
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "Failed to stop session");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/resize
pub async fn resize_session(
    path: web::Path<String>,
    body: web::Json<ResizeSessionRequest>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    if let Err(e) = session_manager.resize_session(&session_id, body.cols, body.rows).await {
        tracing::warn!(error = %e, session_id = %session_id, "Failed to resize session");
        return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
    }

    HttpResponse::Ok().json(ApiResponse::ok())
}

/// DELETE /api/sessions/{id}/remove
pub async fn remove_session(
    req: HttpRequest,
    path: web::Path<String>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let device_name = get_claims_from_request(&req)
        .and_then(|c| c.device_name);
    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    match session_manager.remove_session_with_source(&session_id, device_name).await {
        Ok(()) => {
            let app_handle = ctx.app_handle();
            let _ = app_handle.emit("sessions-refresh", serde_json::json!({
                "refreshType": "sessions",
                "source": source,
            }));
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "Failed to remove session");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/input
///
/// 通过 HTTP 直接写入终端输入，绕过 WebSocket 的 send_and_wait 阻塞
/// 适用于移动端长文本输入场景，避免 WebSocket 通道因等待 ack 导致超时
pub async fn send_session_input(
    path: web::Path<String>,
    body: web::Json<SessionInputRequest>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();
    let session_manager = ctx.session_manager();

    let data = body.data.clone();
    let special_key = body.special_key.clone();

    // 处理普通数据输入
    if !data.is_empty() {
        if let Err(e) = session_manager.write_input(&session_id, &data).await {
            tracing::error!(error = %e, session_id = %session_id, "Failed to write input to session");
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
        }
    }

    // 处理特殊键输入
    if let Some(ref key) = special_key {
        if let Err(e) = session_manager.send_special_key(&session_id, key).await {
            tracing::error!(error = %e, session_id = %session_id, "Failed to send special key to session");
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
        }
    }

    HttpResponse::Ok().json(ApiResponse::ok())
}
