//! Plugin Controller
//!
//! Routes:
//! - ANY /api/plugin/{plugin_id}/{path:.*} — 插件动态 HTTP 端点代理

use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

use crate::system::app_context::AppContext;
use crate::server::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};

// ==================== 插件动态 HTTP 端点代理 ====================

/// ANY /api/plugin/{plugin_id}/{path:.*}
///
/// 插件动态 HTTP 端点 — 请求到达后通过 PluginHost.invoke_rust_command 路由到插件 handler。
/// 仅支持已激活的 Rust / WASM 插件，TS-only 插件的 HTTP 端点通过前端 Tauri event 桥接。
///
/// 认证：JWT 由网关中间件统一校验；无 JWT 的本地调用方（如 hook 脚本）由中间件放行，
/// 此 handler 仅校验插件激活状态
pub async fn plugin_http_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: Option<web::Json<serde_json::Value>>,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let (plugin_id, endpoint_path) = path.into_inner();

    // 认证由网关中间件统一处理：
    // - JWT 请求：中间件校验通过后 claims 已注入 extensions
    // - 无 JWT 的请求（如 hook 脚本）：中间件对 /api/plugin/* 路径放行；
    //   本 handler 不校验任何凭证（历史 BEDCODE_TOKEN 凭证从未被宿主校验，已移除），
    //   仅校验插件激活状态。服务监听 0.0.0.0，插件端点对局域网可达

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
