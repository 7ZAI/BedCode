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

use crate::server::http::dtos::session_dto::*;
use crate::server::http::dtos::ApiResponse;
use crate::server::http::middleware::jwt_auth::get_claims_from_request;
use crate::protocol::RendererSource;
use crate::system::app_context::AppContext;
use actix_web::{web, HttpRequest, HttpResponse};
use tauri::Emitter;

/// GET /api/sessions
pub async fn list_sessions(_req: HttpRequest) -> HttpResponse {
    let ctx = AppContext::global();

    // P1-b：会话真源在插件登记域（宿主无会话登记）；任务字段随视图透传
    let sessions: Vec<SessionItem> =
        match crate::utils::session_gateway::list_views(ctx.plugin_host().wasm_host_ctx()).await {
            Ok(views) => views
                .into_iter()
                .map(|s| SessionItem {
                    id: s.info.id,
                    name: s.info.name,
                    status: serde_json::to_value(&s.info.status)
                        .and_then(|v| serde_json::from_value::<String>(v))
                        .unwrap_or_else(|_| format!("{:?}", s.info.status)),
                    created_at: s.info.created_at.to_rfc3339(),
                    started_at: s.info.started_at.map(|t| t.to_rfc3339()),
                    session_type: Some("pty".to_string()),
                    config_id: Some(s.info.config_id),
                    task_status: s.task_status,
                    task_reason: s.task_reason,
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "GET /api/sessions: plugin surface unavailable");
                return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
            }
        };

    let data = SessionListResponseData { sessions };
    HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
}

/// POST /api/sessions/start
pub async fn start_session(req: HttpRequest, body: web::Json<StartSessionRequest>) -> HttpResponse {
    let ctx = AppContext::global();

    let device_name = get_claims_from_request(&req).and_then(|c| c.device_name);

    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    // 启动端终端组件默认网格：两者齐备且 >0 才生效，作为 PTY 初始尺寸
    let initial_size = match (body.cols, body.rows) {
        (Some(cols), Some(rows)) if cols > 0 && rows > 0 => Some((cols, rows)),
        _ => None,
    };

    // host-business-decarriage 收尾：移动端 HTTP 启动线同样走插件编排
    // （插件必需，无宿主降级）；响应形状与错误码 1002 保持不变。
    match crate::utils::session_gateway::start(
        ctx.plugin_host().wasm_host_ctx(),
        &body.config_id,
        initial_size.map(|(c, _)| c),
        initial_size.map(|(_, r)| r),
        true,
        device_name.as_deref(),
    )
    .await
    {
        Ok(session_id) => {
            // 无头/测试上下文无 AppHandle：跳过前端刷新通知
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(
                    "sessions-refresh",
                    serde_json::json!({
                        "refreshType": "sessions",
                        "source": source,
                    }),
                );
            }

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
pub async fn stop_session(req: HttpRequest, path: web::Path<String>) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();

    let device_name = get_claims_from_request(&req).and_then(|c| c.device_name);
    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    match crate::utils::session_gateway::stop(
        ctx.plugin_host().wasm_host_ctx(),
        &session_id,
        device_name,
    )
    .await
    {
        Ok(()) => {
            // 无头/测试上下文无 AppHandle：跳过前端刷新通知
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(
                    "sessions-refresh",
                    serde_json::json!({
                        "refreshType": "sessions",
                        "source": source,
                    }),
                );
            }
            HttpResponse::Ok().json(ApiResponse::ok())
        }
        Err(e) => {
            tracing::error!(error = %e, session_id = %session_id, "Failed to stop session");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// POST /api/sessions/{id}/resize
///
/// 正统渲染端裁决：来源身份取自 JWT claims 的 device_name（移动端）；
/// 无 claims（未认证/桌面回退）视为 Desktop。裁决不通过时返回
/// ResizeOutcome::NeedsConfirmation（未应用），客户端弹窗确认后带 force 重发。
pub async fn resize_session(
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ResizeSessionRequest>,
) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();

    // 来源身份：移动端 JWT 携带 device_name；缺失时回退 Desktop（不会静默覆盖，
    // 仍受 NeedConfirmation 门控）
    let device_name = get_claims_from_request(&req).and_then(|c| c.device_name);
    let source = match device_name {
        Some(name) => RendererSource::Mobile { device_name: name },
        None => {
            tracing::warn!(
                session_id = %session_id,
                "resize request without device_name claims, treating as Desktop source"
            );
            RendererSource::Desktop
        }
    };

    match crate::utils::session_gateway::resize(
        ctx.plugin_host().wasm_host_ctx(),
        &session_id,
        body.cols,
        body.rows,
        source,
        body.force,
    )
    .await
    {
        Ok(outcome) => HttpResponse::Ok().json(ApiResponse::ok_with_data(outcome)),
        Err(e) => {
            tracing::warn!(error = %e, session_id = %session_id, "Failed to resize session");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}

/// DELETE /api/sessions/{id}/remove
pub async fn remove_session(req: HttpRequest, path: web::Path<String>) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();

    let device_name = get_claims_from_request(&req).and_then(|c| c.device_name);
    let source = device_name.clone().unwrap_or_else(|| "mobile".to_string());

    match crate::utils::session_gateway::remove(
        ctx.plugin_host().wasm_host_ctx(),
        &session_id,
        device_name,
    )
    .await
    {
        Ok(()) => {
            // 无头/测试上下文无 AppHandle：跳过前端刷新通知
            if let Some(handle) = ctx.app_handle() {
                let _ = handle.emit(
                    "sessions-refresh",
                    serde_json::json!({
                        "refreshType": "sessions",
                        "source": source,
                    }),
                );
            }
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
pub async fn send_session_input(path: web::Path<String>, body: web::Json<SessionInputRequest>) -> HttpResponse {
    let session_id = path.into_inner();
    let ctx = AppContext::global();

    let data = body.data.clone();
    let special_key = body.special_key.clone();

    // 处理普通数据输入
    if !data.is_empty() {
        if let Err(e) = crate::utils::session_gateway::input(ctx.plugin_host().wasm_host_ctx(), &session_id, &data).await {
            tracing::error!(error = %e, session_id = %session_id, "Failed to write input to session");
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
        }
    }

    // 处理特殊键输入
    if let Some(ref key) = special_key {
        if let Err(e) = crate::utils::session_gateway::special_key(
            ctx.plugin_host().wasm_host_ctx(),
            &session_id,
            key,
        )
        .await
        {
            tracing::error!(error = %e, session_id = %session_id, "Failed to send special key to session");
            return HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()));
        }
    }

    HttpResponse::Ok().json(ApiResponse::ok())
}

/// GET /api/sessions/{id}/history
///
/// 一次性历史拉取（用户需求 3：历史不再走 WS 重播，按快照字节锚点一次性取回）。
/// 从 `from`（缺省 0）起截取 `[from, snapshot_offset)` 字节（chunk 级跳过 +
/// 半块 slice），携带字节三件套元数据供消费端做历史拼接/截断判定。
/// 会话不存在 → 404；from 旧于 min_offset → 收敛到 min_offset 返回。
pub async fn get_session_history(path: web::Path<String>, query: web::Query<SessionHistoryQuery>) -> HttpResponse {
    let session_id = path.into_inner();
    let from = query.from.unwrap_or(0);
    let ctx = AppContext::global();
    // websocket 业务下沉票 08：历史快照经插件 `session-history` 互调（宿主不再
    // 直读会话输出环）；插件未激活 / 会话不存在显性报错，不静默当「无数据」
    match crate::utils::session_gateway::history_snapshot(ctx.plugin_host().wasm_host_ctx(), &session_id, from).await {
        Ok((data, min_offset, snapshot_offset, history_bytes)) => {
            // 链路调试（终端字节对账）：移动端缓存头被淘汰时经此接口增量补历史，
            // 字节三件套与移动端 terminal_get_history 日志对照
            tracing::debug!(
                session_id = %session_id,
                from_offset = from,
                min_offset,
                snapshot_offset,
                history_bytes,
                payload_bytes = data.len(),
                "session history served via http"
            );
            let response = SessionHistoryData {
                min_offset,
                snapshot_offset,
                history_bytes,
                data_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data),
            };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(response))
        }
        Err(e) => {
            tracing::debug!(session_id = %session_id, error = %e, "history fetch failed (plugin surface)");
            HttpResponse::Ok().json(ApiResponse::<()>::error(1002, &e.to_string()))
        }
    }
}
