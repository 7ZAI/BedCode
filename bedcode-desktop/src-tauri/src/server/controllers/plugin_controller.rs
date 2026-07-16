//! Plugin Controller
//!
//! Routes:
//! - ANY /api/plugin/{plugin_id}/{path:.*} — 插件动态 HTTP 端点代理

use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

use crate::system::app_context::AppContext;
use crate::utils::auth::jwt::JwtService;
use crate::server::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::system::config::AppConfig;

// ==================== 插件动态 HTTP 端点代理 ====================

/// ANY /api/plugin/{plugin_id}/{path:.*}
///
/// 插件动态 HTTP 端点 — 请求到达后通过 PluginHost.invoke_rust_command 路由到插件 handler。
/// 仅支持已激活的 Rust / WASM 插件，TS-only 插件的 HTTP 端点通过前端 Tauri event 桥接。
///
/// 认证方式：plugin token 或 JWT
pub async fn plugin_http_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: Option<web::Json<serde_json::Value>>,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let (plugin_id, endpoint_path) = path.into_inner();

    // 认证：从 query 中提取 plugin token，或从 Authorization header 中提取 JWT
    let config = AppConfig::global();
    let token = query.get("token").cloned().unwrap_or_default();
    let plugin_token_valid = !config.plugin.token.is_empty() && token == config.plugin.token;
    let jwt_valid = validate_jwt_from_request(&req);

    if !plugin_token_valid && !jwt_valid {
        tracing::warn!(
            "Plugin HTTP endpoint auth failed: plugin_id={}, path={}",
            plugin_id, endpoint_path
        );
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            "Invalid plugin token or JWT authentication",
        ));
    }

    // 检查插件是否已激活
    let ctx = AppContext::global();
    let plugin_host = ctx.plugin_host();
    if !plugin_host.is_activated(&plugin_id).await {
        return HttpResponse::Ok().json(ApiResponse::<()>::error(
            CODE_PLUGIN_AUTH_FAILED,
            &format!("Plugin {} is not activated", plugin_id),
        ));
    }

    // 构造请求参数：包含 method、path、body、query
    let method = req.method().as_str();
    let request_args = serde_json::json!({
        "method": method,
        "path": endpoint_path,
        "body": body.map(|b| b.into_inner()).unwrap_or(serde_json::Value::Null),
        "query": query.into_inner(),
    });

    // 通过 plugin_invoke 路由到插件的 _http_endpoint command
    let result = plugin_host
        .invoke_rust_command(&plugin_id, "_http_endpoint", request_args)
        .await;

    match result {
        Ok(response) => {
            // 插件返回格式：{ status: number, body: any }
            let status = response.get("status")
                .and_then(|v| v.as_u64())
                .unwrap_or(200) as u16;
            let response_body = response.get("body")
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap_or(actix_web::http::StatusCode::OK))
                .json(response_body)
        }
        Err(e) => {
            tracing::error!(
                "Plugin HTTP endpoint error: plugin_id={}, path={}, error={}",
                plugin_id, endpoint_path, e
            );
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Plugin endpoint error: {}", e),
            ))
        }
    }
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
