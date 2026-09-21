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

/// 插件响应附加头提取（票 03）：`headers` 字段为 `{ "Header-Name": "value" }`
/// 字符串映射（如 file-tree-children 的 `Cache-Control`）；缺失/非法条目忽略。
/// 值只接受字符串——宿主不替插件解释或转换头值。
pub(crate) fn plugin_http_headers(response: &serde_json::Value) -> Vec<(String, String)> {
    let Some(headers) = response.get("headers").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    headers
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|value| (k.clone(), value.to_string())))
        .collect()
}

// ==================== 路由判定（纯函数，供测试固化） ====================

/// 声明式路径匹配（票 16 固化票据 03 的两条判据）
///
/// - `declared` 为空 = 插件未声明 `contributes.httpEndpoints` → **前缀内放行**，
///   未知道路返回 404 与否由插件自己判（过渡策略：既有插件零迁移）。
/// - `declared` 非空 = 已声明 → 按完整路径**精确匹配**，未命中即 404（请求不会
///   到达插件，避免未声明路径被枚举）。
pub(crate) fn plugin_http_path_allowed(declared: &[String], full_path: &str) -> bool {
    declared.is_empty() || declared.iter().any(|p| p == full_path)
}

/// 旧插件 id → 接管方 id（票 16 D1「桌面侧可选兜底」的落点）
///
/// 语义是「旧前缀的 HTTP 面改由新插件应答」，不是两个插件共享请求：只在旧插件
/// **未激活**且接管方**已激活**时生效（见 [`resolve_http_owner`]）。合并插件退役
/// 旧 auto-task 后端（票 17）后，移动端与已部署在项目里的旧 hook 脚本仍能按原
/// 路径打到桌面端；切断判定与代价评估记在票 16 Comments。
const LEGACY_HTTP_PLUGIN_ALIASES: &[(&str, &str)] = &[("com.bedcode.auto-task", "com.bedcode.terminal-session")];

/// 查旧前缀的接管方 id（无声明即 None）
pub(crate) fn legacy_http_alias(requested: &str) -> Option<&'static str> {
    LEGACY_HTTP_PLUGIN_ALIASES
        .iter()
        .find(|(from, _)| *from == requested)
        .map(|(_, to)| *to)
}

/// 该由哪个插件应答本次请求（纯决策，供测试）
///
/// 顺序即优先级：**请求前缀自身已激活时绝不抢占**——双轨并存期旧 auto-task 仍在
/// 写它自己的私有库，若把它的请求转给新插件，同一份数据会出现两个写者且回包来自
/// 空库（读到的队列永远是空的）。只有旧插件确实不在位（停用/未激活/error）时，
/// 兜底才把请求交给接管方；接管方也不在位则 None（保持原「未激活」错误响应）。
pub(crate) fn resolve_http_owner<'a>(
    requested: &'a str,
    requested_activated: bool,
    alias: Option<&'a str>,
    alias_activated: bool,
) -> Option<&'a str> {
    if requested_activated {
        return Some(requested);
    }
    match alias {
        Some(target) if alias_activated => Some(target),
        _ => None,
    }
}

// ==================== 转发内核（插件前缀路由与 HTTP 协议网关共用） ====================

/// 一次业务 HTTP 请求转发到插件 `_http_endpoint` 所需的全部入参
///
/// `pub(crate)`：`server/gateway.rs` 的业务 URL 别名走的是同一个内核——「新增传输机制」
/// 是票 01 明确禁止的形态，两条路径必须共用请求构造与响应映射（含 headers 白名单、
/// status/contentType 解析口径），否则同一请求经网关与经 `/api/plugin/*` 会答出两版。
pub(crate) struct PluginHttpRequest<'a> {
    /// 去掉 `/api/plugin/{plugin_id}/` 前缀后的端点段（网关路径取别名目标的端点段）
    pub endpoint_path: &'a str,
    pub method: &'a str,
    /// 已按 [`PLUGIN_HEADER_WHITELIST`] 过滤的请求头
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub body: serde_json::Value,
    pub query: serde_json::Value,
    /// 宿主验签后的可信设备上下文（仅网关路径注入；`/api/plugin/*` 无 JWT 时为 None）
    ///
    /// 只给 claims 派生的标识字段，**JWT 本体与指纹一律不透传**（AGENTS.md §8 凭据红线）。
    pub device: Option<serde_json::Value>,
}

/// 插件 `_http_endpoint` 入参构造（纯函数，供双轨契约测试逐字段锁定）
///
/// `headers` 字段为票据 04 增量追加、`device` 为票 01 网关追加——都是字段级追加，
/// 老插件忽略未知字段（协议增量原则）。
pub(crate) fn build_plugin_http_args(req: &PluginHttpRequest) -> serde_json::Value {
    let mut args = serde_json::json!({
        "method": req.method,
        "path": req.endpoint_path,
        "headers": serde_json::Value::Object(req.headers.clone()),
        "body": req.body,
        "query": req.query,
    });
    if let Some(device) = &req.device {
        args["device"] = device.clone();
    }
    args
}

/// 转发内核：调插件 `_http_endpoint` 并把 `{ status, body, contentType }` 映射为 HTTP 响应
///
/// 调用方必须先完成属主解析与端点声明判定（`plugin_http_path_allowed`）——本函数
/// 只做「送进去 + 把回包翻译成 HTTP」，不掺任何路由策略。
pub(crate) async fn forward_to_plugin(owner: &str, req: &PluginHttpRequest<'_>) -> HttpResponse {
    let request_args = build_plugin_http_args(req);
    let plugin_host = AppContext::global().plugin_host();
    let result = plugin_host
        .invoke_rust_command(owner, "_http_endpoint", request_args)
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
            // headers 可选（票 03）：插件透传附加响应头（如 Cache-Control）
            for (name, value) in plugin_http_headers(&response) {
                match actix_web::http::header::HeaderName::try_from(name.clone()) {
                    Ok(header_name) => {
                        builder.insert_header((header_name, value));
                    }
                    Err(_) => {
                        tracing::warn!(header = %name, "plugin http endpoint: invalid response header name, skipped");
                    }
                }
            }
            builder.json(response_body)
        }
        Err(e) => {
            tracing::error!(
                "Plugin HTTP endpoint error: plugin_id={}, path={}, error={}",
                owner,
                req.endpoint_path,
                e
            );
            HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_INVALID_REQUEST,
                &format!("Plugin endpoint error: {}", e),
            ))
        }
    }
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

    // 检查插件是否已激活；未激活时旧前缀可兜底转给接管方（票 16，见 resolve_http_owner）
    let ctx = AppContext::global();
    let plugin_host = ctx.plugin_host();
    let requested_activated = plugin_host.is_activated(&plugin_id).await;
    let alias = legacy_http_alias(&plugin_id);
    let alias_activated = match alias {
        Some(target) if !requested_activated => plugin_host.is_activated(target).await,
        _ => false,
    };
    let owner = match resolve_http_owner(&plugin_id, requested_activated, alias, alias_activated) {
        Some(owner) => owner,
        None => {
            return HttpResponse::Ok().json(ApiResponse::<()>::error(
                CODE_PLUGIN_AUTH_FAILED,
                &format!("Plugin {} is not activated", plugin_id),
            ))
        }
    };

    // 端点注册治理（票据 03 判据，票 16 固化）：插件已声明（manifest
    // contributes.httpEndpoints / toolProviders）时做精确路径匹配，未注册路径 404；
    // 未声明插件保持旧前缀 ANY 行为（auto-task 等既有插件零迁移）。
    // 声明清单按**属主插件**查（旧前缀兜底时按接管方的清单判定），full_path 同样
    // 按属主拼——插件侧收到的 `path` 字段保持请求原样，双轨期两实现共用同一分派表。
    let declared = plugin_host.registry().list_http_endpoint_paths(owner).await;
    let full_path = format!("/api/plugin/{}/{}", owner, endpoint_path);
    if !plugin_http_path_allowed(&declared, &full_path) {
        tracing::warn!(
            plugin_id = %owner,
            requested_plugin_id = %plugin_id,
            path = %full_path,
            "plugin http endpoint not registered (exact match required)"
        );
        return HttpResponse::NotFound().json(ApiResponse::<()>::error(
            CODE_INVALID_REQUEST,
            &format!("Plugin endpoint '{}' is not registered", full_path),
        ));
    }

    if owner != plugin_id {
        tracing::debug!(
            requested_plugin_id = %plugin_id,
            plugin_id = %owner,
            "legacy plugin http prefix served by successor plugin (D1 desktop-side fallback)"
        );
    }

    // 构造请求参数：method、path、白名单 headers、body、query
    // headers 字段为票据 04 增量追加——老插件忽略未知字段（字段演进增量原则）
    let method = req.method().as_str();
    let request_headers = filter_plugin_request_headers(req.headers());
    let request = PluginHttpRequest {
        endpoint_path: &endpoint_path,
        method,
        headers: request_headers,
        body: body.map(|b| b.into_inner()).unwrap_or(serde_json::Value::Null),
        query: serde_json::Value::Object(
            query
                .into_inner()
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect(),
        ),
        // 本路由不要求 JWT（hook 脚本无法持有 token），无验签结果可透传
        device: None,
    };
    forward_to_plugin(owner, &request).await
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

    /// 票 16：声明式匹配的两条判据固化（此前逻辑内联在 handler 里，无测试覆盖）
    ///
    /// 「未声明 → 前缀内放行」是既有插件（auto-task 从未声明）零迁移的前提；
    /// 「已声明 → 精确命中」是新插件的审计面。两者都必须在这一处判定，不允许
    /// 出现第三种（如前缀匹配声明路径——那会让 `/task-status` 声明放行
    /// `/task-status/../../admin`）。
    #[test]
    fn declared_paths_match_exactly_and_undeclared_pass_through() {
        let declared: Vec<String> = vec![
            "/api/plugin/com.bedcode.terminal-session/task-status".to_string(),
            "/api/plugin/com.bedcode.terminal-session/task-queue/add".to_string(),
        ];
        // 已声明：精确命中放行
        assert!(plugin_http_path_allowed(
            &declared,
            "/api/plugin/com.bedcode.terminal-session/task-status"
        ));
        assert!(plugin_http_path_allowed(
            &declared,
            "/api/plugin/com.bedcode.terminal-session/task-queue/add"
        ));
        // 已声明：未命中一律拒绝（含前缀相近、大小写不同、缺段、多段）
        assert!(!plugin_http_path_allowed(
            &declared,
            "/api/plugin/com.bedcode.terminal-session/task-history"
        ));
        assert!(
            !plugin_http_path_allowed(&declared, "/api/plugin/com.bedcode.terminal-session/task-status/extra"),
            "声明路径不得前缀匹配"
        );
        assert!(!plugin_http_path_allowed(
            &declared,
            "/api/plugin/com.bedcode.terminal-session/TASK-STATUS"
        ));
        // 未声明（空清单）：前缀内 ANY 放行，404 由插件自判
        assert!(plugin_http_path_allowed(
            &[],
            "/api/plugin/com.bedcode.auto-task/anything"
        ));
    }

    /// 票 16：旧前缀别名表——只登记已退役/在退役的那一条，且必须指向合并插件
    #[test]
    fn legacy_http_alias_maps_only_retired_plugin_prefix() {
        assert_eq!(legacy_http_alias("com.bedcode.auto-task"), Some("com.bedcode.terminal-session"));
        // 新 id 自身、其它在位插件、未知 id 都没有接管方
        assert_eq!(legacy_http_alias("com.bedcode.terminal-session"), None);
        assert_eq!(legacy_http_alias("com.bedcode.file-transfer"), None);
        assert_eq!(legacy_http_alias(""), None);
    }

    /// 票 16：属主解析的优先级——旧插件在位时绝不抢占（双写者/空库回包红线）
    #[test]
    fn http_owner_never_preempts_an_activated_plugin() {
        let old = "com.bedcode.auto-task";
        let new = "com.bedcode.terminal-session";
        // 旧插件仍激活：请求留在旧插件（合并插件不得改写成另一份私有库的数据）
        assert_eq!(
            resolve_http_owner(old, true, Some(new), true),
            Some(old),
            "双轨并存期不得抢占"
        );
        // 旧插件不在位 + 接管方已激活 → 兜底转给接管方
        assert_eq!(resolve_http_owner(old, false, Some(new), true), Some(new));
        // 两者都不在位 → None（保持原「未激活」错误响应，不凭空造插件）
        assert_eq!(resolve_http_owner(old, false, Some(new), false), None);
        // 未在别名表里的插件（alias = None）没有兜底可走：未激活就是未激活
        assert_eq!(
            resolve_http_owner("com.bedcode.file-transfer", false, None, true),
            None,
            "接管方在位也不得把无别名插件的请求转出去"
        );
        // 新前缀自身没有别名条目 → 不被别名回环（别名表是唯一 gate）
        assert_eq!(legacy_http_alias(new), None, "接管方自己不得有别名");
        assert_eq!(
            resolve_http_owner(new, false, legacy_http_alias(new), true),
            None,
            "新插件未激活时不得借旧插件兜底"
        );
    }
}
