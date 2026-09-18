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

/// 调用方请求 headers 白名单（票据 04）：只透传业务相关头，
/// 避免透传全部请求头带来的凭据泄露与枚举面。
/// 凭据类头（authorization / cookie / proxy-authorization 等）一律不透传。
const PLUGIN_HEADER_WHITELIST: &[&str] = &["content-type", "accept", "x-request-id"];

/// 按白名单过滤调用方请求 headers（纯函数，供测试）
///
/// 返回插件可读的 headers 对象；白名单外（含全部凭据头）不进入结果。
pub(crate) fn filter_plugin_request_headers(
    headers: &actix_web::http::header::HeaderMap,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (key, value) in headers.iter() {
        let name = key.as_str().to_ascii_lowercase();
        if PLUGIN_HEADER_WHITELIST.contains(&name.as_str()) {
            if let Ok(v) = value.to_str() {
                out.insert(name, serde_json::Value::String(v.to_string()));
            }
        }
    }
    out
}

/// 插件响应 content-type 提取（票据 04）：`contentType` 字段指定响应类型；
/// 缺失/非法时返回 None（调用方保持默认 application/json）。
pub(crate) fn plugin_http_content_type(response: &serde_json::Value) -> Option<String> {
    response
        .get("contentType")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
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

    // 端点注册治理（票据 03）：插件已声明（manifest contributes.toolProviders）时
    // 做精确路径匹配，未注册路径 404；未声明插件保持旧前缀 ANY 行为
    // （auto-task 等既有插件零迁移的过渡策略）。
    let declared = plugin_host.registry().list_http_endpoint_paths(&plugin_id).await;
    if !declared.is_empty() {
        let full_path = format!("/api/plugin/{}/{}", plugin_id, endpoint_path);
        let matched = declared.iter().any(|p| *p == full_path);
        if !matched {
            tracing::warn!(
                plugin_id = %plugin_id,
                path = %full_path,
                "plugin http endpoint not registered (exact match required)"
            );
            return HttpResponse::NotFound().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Plugin endpoint '{}' is not registered", full_path),
            ));
        }
    }

    // 构造请求参数：method、path、白名单 headers、body、query
    // headers 字段为票据 04 增量追加——老插件忽略未知字段（字段演进增量原则）
    let method = req.method().as_str();
    let request_headers = filter_plugin_request_headers(req.headers());
    let request_args = serde_json::json!({
        "method": method,
        "path": endpoint_path,
        "headers": request_headers,
        "body": body.map(|b| b.into_inner()).unwrap_or(serde_json::Value::Null),
        "query": query.into_inner(),
    });

    // 通过 plugin_invoke 路由到插件的 _http_endpoint command
    let result = plugin_host
        .invoke_rust_command(&plugin_id, "_http_endpoint", request_args)
        .await;

    match result {
        Ok(response) => {
            // 插件返回格式：{ status: number, body: any, contentType?: string }
            let status = plugin_http_status(&response);
            let response_body = response.get("body").cloned().unwrap_or(serde_json::Value::Null);

            // contentType 可选：插件可指定（如 text/plain / image/png），默认 application/json
            let mut builder = HttpResponse::build(
                actix_web::http::StatusCode::from_u16(status).unwrap_or(actix_web::http::StatusCode::OK),
            );
            if let Some(content_type) = plugin_http_content_type(&response) {
                builder.insert_header((actix_web::http::header::CONTENT_TYPE, content_type));
            }
            builder.json(response_body)
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

    /// 请求头白名单（票据 04）：白名单内透传（小写），凭据/白名单外丢弃
    #[test]
    fn filter_plugin_request_headers_whitelist_only() {
        use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
        let mut headers = HeaderMap::new();
        // HeaderName 仅接受小写（http crate 规范）
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(HeaderName::from_static("accept"), HeaderValue::from_static("*/*"));
        headers.insert(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_static("req-1"),
        );
        // 凭据与无关头：一律不透传
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer secret"),
        );
        headers.insert(
            HeaderName::from_static("cookie"),
            HeaderValue::from_static("session=abc"),
        );
        headers.insert(HeaderName::from_static("x-custom"), HeaderValue::from_static("no"));

        let filtered = filter_plugin_request_headers(&headers);
        assert_eq!(
            filtered.get("content-type").and_then(|v| v.as_str()),
            Some("application/json")
        );
        assert_eq!(filtered.get("accept").and_then(|v| v.as_str()), Some("*/*"));
        assert_eq!(filtered.get("x-request-id").and_then(|v| v.as_str()), Some("req-1"));
        assert!(filtered.get("authorization").is_none(), "凭据头不得透传");
        assert!(filtered.get("cookie").is_none(), "Cookie 不得透传");
        assert!(filtered.get("x-custom").is_none(), "白名单外头不得透传");
    }

    /// 插件响应 contentType：缺失返回 None（默认 application/json），指定则原样返回
    #[test]
    fn plugin_http_content_type_extracted_or_default() {
        assert_eq!(plugin_http_content_type(&serde_json::json!({})), None);
        assert_eq!(plugin_http_content_type(&serde_json::json!({"body": 1})), None);
        assert_eq!(
            plugin_http_content_type(&serde_json::json!({"contentType": "text/plain"})),
            Some("text/plain".to_string())
        );
        // 非字符串 contentType → None（保持默认）
        assert_eq!(plugin_http_content_type(&serde_json::json!({"contentType": 123})), None);
    }
}
