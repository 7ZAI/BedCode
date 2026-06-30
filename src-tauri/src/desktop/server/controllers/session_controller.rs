//! Session Controller
//!
//! HTTP REST API endpoints for session management
//! Routes:
//! - GET  /api/sessions
//! - POST /api/sessions/start
//! - POST /api/sessions/{id}/stop
//! - POST /api/sessions/{id}/resize
//! - DELETE /api/sessions/{id}/remove

use actix_web::{web, HttpRequest, HttpResponse};
use tauri::Emitter;
use crate::desktop::app_context::AppContext;
use crate::shared::model::api_dto::ApiResponse;
use crate::desktop::server::dtos::session_dto::*;
use crate::desktop::server::middleware::jwt_auth::get_claims_from_request;

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
            tracing::error!("Failed to start session: {}", e);
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
            tracing::error!("Failed to stop session: {}", e);
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
        tracing::warn!("Failed to resize session: {}", e);
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
            tracing::error!("Failed to remove session: {}", e);
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}
