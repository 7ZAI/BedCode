//! Plugin Controller
//!
//! Routes:
//! - ANY /api/plugin/{plugin_id}/{path:.*} — 插件动态 HTTP 端点代理

use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

use crate::server::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::system::app_context::AppContext;

// ==================== 插件动态 HTTP 端点代理 ====================

/// 插件 HTTP 端点响应状态提取（纯函数，供测试）：`status` 字段缺失/非数字/越界
/// → 默认 200；仅接受 actix 合法区间 100..=999（原实现 `as u16` 截断在 >65535
/// 时会误放行截断后的合法码，此处用 try_from 拒绝截断）
pub(crate) fn plugin_http_status(response: &serde_json::Value) -> u16 {
    response
        .get("status")
        .and_then(|v| v.as_u64())
        .and_then(|s| u16::try_from(s).ok())
        .filter(|s| (100..=999).contains(s))
        .unwrap_or(200)
}

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
            let status = plugin_http_status(&response);
            let response_body = response.get("body").cloned().unwrap_or(serde_json::Value::Null);

            HttpResponse::build(
                actix_web::http::StatusCode::from_u16(status).unwrap_or(actix_web::http::StatusCode::OK),
            )
            .json(response_body)
        }
        Err(e) => {
            tracing::error!(
                "Plugin HTTP endpoint error: plugin_id={}, path={}, error={}",
                plugin_id,
                endpoint_path,
                e
            );
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Plugin endpoint error: {}", e),
            ))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_http_status_accepts_valid_codes() {
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 200})), 200);
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 201})), 201);
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 599})), 599);
    }

    #[test]
    fn plugin_http_status_defaults_when_missing_or_invalid() {
        // 缺失 status / 非对象 → 默认 200
        assert_eq!(plugin_http_status(&serde_json::json!({})), 200);
        assert_eq!(plugin_http_status(&serde_json::json!({"body": 1})), 200);
        // 非数字 → 200
        assert_eq!(plugin_http_status(&serde_json::json!({"status": "abc"})), 200);
        // 小数 / 负数 → 200
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 200.5})), 200);
        assert_eq!(plugin_http_status(&serde_json::json!({"status": -1})), 200);
        // 低于 actix 合法区间（from_u16 失败回退）→ 200
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 50})), 200);
        // 超 u16 上限：原实现 `as u16` 截断可能误放行，try_from 拒绝 → 200
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 65536})), 200);
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 999999})), 200);
        // 超 actix 区间上限 → 200
        assert_eq!(plugin_http_status(&serde_json::json!({"status": 1000})), 200);
    }
}
