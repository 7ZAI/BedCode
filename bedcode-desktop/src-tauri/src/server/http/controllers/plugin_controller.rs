//! Plugin Controller
//!
//! Routes:
//! - ANY /api/plugin/{plugin_id}/{path:.*} — 插件动态 HTTP 端点代理

use actix_web::{web, HttpRequest, HttpResponse};
use std::collections::HashMap;

use bedcode_plugin_api::EndpointAuth;

use crate::server::http::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::server::http::middleware::jwt_auth::get_claims_from_request;
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

/// 端点认证判定（纯函数，票 08 / ABI v29）：登记档位 + 本次请求是否已过宿主验签 → 是否放行
///
/// `Jwt` 档要求 `verified`（宿主 JWT 中间件已注入 claims）；`None` 档免凭证，但仍
/// 携带 [`HttpCaller`] 身份，让插件自己按身份收紧（裁决 3）。
///
/// 路径声明判定（「只认登记、精确匹配」）已随 ABI v29 移交动态注册表
/// （`server/http/registry::find_by_internal`）——插件经 `host-http.register-endpoint`
/// 登记的内部路径即唯一可达面，未登记路径 404。
pub(crate) fn plugin_http_auth_allowed(auth: EndpointAuth, verified: bool) -> bool {
    match auth {
        EndpointAuth::Jwt => verified,
        EndpointAuth::None => true,
    }
}

/// 宿主判定的调用方身份（票 08 裁决 3）——转发给插件的 `caller` 字段取值
///
/// 三档互斥，按「已验签 > 环回 > 匿名」优先判定：
/// - [`HttpCaller::Device`]：JWT 验签通过的可信设备，`device` 字段带 claims 派生标识
/// - [`HttpCaller::Localhost`]：环回调用方（Claude Code / codex hook 脚本、本机工具），
///   无凭证可给，插件按「本机」这一固定标识区分
/// - [`HttpCaller::Anonymous`]：局域网内的匿名调用方（既未验签也非环回）
///
/// 凭据红线：任何一档都不带 JWT 本体与设备指纹（AGENTS.md §8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpCaller {
    Device,
    Localhost,
    Anonymous,
}

impl HttpCaller {
    /// 转发字段取值（线协议字符串，插件侧按它分派）
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Localhost => "localhost",
            Self::Anonymous => "anonymous",
        }
    }

    /// 身份判定（纯函数，供测试）：`verified` = 宿主已验签注入 claims，
    /// `loopback` = TCP 对端是环回地址
    pub(crate) fn classify(verified: bool, loopback: bool) -> Self {
        match (verified, loopback) {
            (true, _) => Self::Device,
            (false, true) => Self::Localhost,
            (false, false) => Self::Anonymous,
        }
    }
}

/// 本次请求的调用方身份与可信设备上下文（`/api/plugin/*` 与业务网关共用一条判据）
///
/// claims 只在验签通过后存在（`jwt_gateway` 注入），因此 `device` 与
/// [`HttpCaller::Device`] 同源同步；设备上下文只取 claims 派生标识，
/// **JWT 本体与指纹不出宿主**（AGENTS.md §8 凭据红线）。
pub(crate) fn caller_identity(req: &actix_web::HttpRequest) -> (HttpCaller, Option<serde_json::Value>) {
    let claims = get_claims_from_request(req);
    let loopback = req.peer_addr().is_some_and(|addr| addr.ip().is_loopback());
    let caller = HttpCaller::classify(claims.is_some(), loopback);
    let device = claims.map(|claims| {
        let mut device = serde_json::Map::new();
        device.insert("deviceId".to_string(), serde_json::Value::String(claims.sub));
        if let Some(name) = claims.device_name {
            device.insert("deviceName".to_string(), serde_json::Value::String(name));
        }
        serde_json::Value::Object(device)
    });
    (caller, device)
}

/// 旧插件 id → 接管方 id（票 16 D1「桌面侧可选兜底」的落点；票 07 B2 追加 id 改名项）
///
/// 语义是「旧前缀的 HTTP 面改由新插件应答」，不是两个插件共享请求：只在旧插件
/// **未激活**且接管方**已激活**时生效（见 [`resolve_http_owner`]）。合并插件退役
/// 旧 auto-task 后端（票 17）后，移动端与已部署在项目里的旧 hook 脚本仍能按原
/// 路径打到桌面端；票 06 插件 id 改名后，旧 id 的 HTTP 前缀
/// `/api/plugin/com.bedcode.session/*` 在双投窗口内同样兜底到新插件（票 07 验收：
/// 旧前缀窗口内可用）；切断判定与代价评估记在票 16 Comments。
const LEGACY_HTTP_PLUGIN_ALIASES: &[(&str, &str)] = &[
    ("com.bedcode.auto-task", "com.bedcode.terminal-session"),
    ("com.bedcode.session", "com.bedcode.terminal-session"),
];

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
    /// 模板段捕获参数（ABI v29 动态路由：host 别名 `{id}` 模板的捕获值；
    /// 精确命中 / 无模板时为空）
    pub params: serde_json::Map<String, serde_json::Value>,
    /// 宿主判定的调用方身份（票 08 裁决 3：`device` / `localhost` / `anonymous`）
    pub caller: HttpCaller,
    /// 宿主验签后的可信设备上下文（仅 [`HttpCaller::Device`] 时有值）
    ///
    /// 只给 claims 派生的标识字段，**JWT 本体与指纹一律不透传**（AGENTS.md §8 凭据红线）。
    pub device: Option<serde_json::Value>,
}

/// 插件 `_http_endpoint` 入参构造（纯函数，供双轨契约测试逐字段锁定）
///
/// `headers` 字段为票据 04 增量追加、`device` 为票 01 网关追加、`caller` 为票 08 追加
/// ——都是字段级追加，老插件忽略未知字段（协议增量原则）。
pub(crate) fn build_plugin_http_args(req: &PluginHttpRequest) -> serde_json::Value {
    let mut args = serde_json::json!({
        "method": req.method,
        "path": req.endpoint_path,
        "headers": serde_json::Value::Object(req.headers.clone()),
        "body": req.body,
        "query": req.query,
        // 免凭证调用方不再「没有身份」：三档固定取值让插件能区分本机 hook 与局域网匿名
        "caller": req.caller.as_str(),
    });
    // 模板捕获参数（ABI v29 动态路由）：命中 `{id}` 模板的 host 别名请求才携带；
    // 无模板的请求给空对象（老插件忽略未知字段，字段演进增量原则）
    args["params"] = serde_json::Value::Object(req.params.clone());
    if let Some(device) = &req.device {
        args["device"] = device.clone();
    }
    args
}

/// 转发内核：调插件 `_http_endpoint` 并把 `{ status, body, contentType }` 映射为 HTTP 响应
///
/// 调用方必须先完成属主解析与端点登记判定（`server/http/registry::find_by_internal`）——
/// 本函数只做「送进去 + 把回包翻译成 HTTP」，不掺任何路由策略。
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

/// 「调用方未验签」响应（票 08）：HTTP 401 + 业务码 1007，与 JWT 中间件的 401 同一码
///
/// 文案同时点名两条出路（带合法 JWT / 在 manifest 逐条声明 `auth: "none"`）——
/// 收紧后打不通的调用方必须能从错误里读出该改哪一侧，而不是只看到「authentication failed」。
pub(crate) fn plugin_http_unauthenticated_response(full_path: &str) -> HttpResponse {
    HttpResponse::Unauthorized().json(ApiResponse::<()>::error(
        CODE_PLUGIN_AUTH_FAILED,
        &format!(
            "Plugin endpoint '{}' requires auth \"jwt\"; present a valid JWT or declare auth: \"none\" in contributes.httpEndpoints",
            full_path
        ),
    ))
}

/// ANY /api/plugin/{plugin_id}/{path:.*}
///
/// 插件动态 HTTP 端点 — 请求到达后通过 PluginHost.invoke_rust_command 路由到插件 handler。
/// 仅支持已激活的 Rust / WASM 插件，TS-only 插件的 HTTP 端点通过前端 Tauri event 桥接。
///
/// 认证：`jwt_gateway` 中间件先验签（有效 JWT 时注入 claims），本 handler 按**属主端点
/// 声明的档位**决定是否要求已验签（票 08）：manifest 未声明 `auth` 即最严档 `jwt`，
/// 免凭证必须逐条显式声明 `auth: "none"`。服务监听 0.0.0.0，收紧前「插件端点对局域网
/// 无凭证可达」正是本票处置的风险。
pub async fn plugin_http_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: Option<web::Json<serde_json::Value>>,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let (plugin_id, endpoint_path) = path.into_inner();

    // 中间件只做「有 JWT 就验签」；本路由无凭证的请求（环回 hook、局域网匿名）
    // 放行到这里，由下面的端点级档位判定决定是否真的到达插件。

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

    // 端点注册治理（ABI v29 动态路由）：只认插件经 host-http.register-endpoint 登记
    // 的内部路径（含 manifest 声明面退役后由插件激活期代码注册的全部端点），未注册
    // 路径 404——「未声明清单 → 前缀内 ANY 放行」的过渡策略已退役，未登记的插件
    // 没有 HTTP 面。声明按**属主插件**查（旧前缀兜底时按接管方的登记判定），
    // full_path 同样按属主拼——插件侧收到的 `path` 字段保持请求原样。
    let full_path = format!("/api/plugin/{}/{}", owner, endpoint_path);
    let Some(entry) = crate::server::http::registry::find_by_internal(&full_path) else {
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
    };

    // 认证档位（票 08 / ABI v29）：按属主登记档位判定，未声明 auth 即要求宿主已验签
    let (caller, device) = caller_identity(&req);
    if !plugin_http_auth_allowed(entry.auth, matches!(caller, HttpCaller::Device)) {
        tracing::warn!(
            plugin_id = %owner,
            path = %full_path,
            caller = %caller.as_str(),
            auth = %entry.auth.as_str(),
            "plugin http endpoint requires an authenticated caller"
        );
        return plugin_http_unauthenticated_response(&full_path);
    }

    if owner != plugin_id {
        tracing::debug!(
            requested_plugin_id = %plugin_id,
            plugin_id = %owner,
            "legacy plugin http prefix served by successor plugin (D1 desktop-side fallback)"
        );
    }

    // 构造请求参数：method、path、白名单 headers、body、query、调用方身份
    // headers 字段为票据 04 增量追加，caller / device 为票 01 / 票 08 追加——
    // 老插件忽略未知字段（字段演进增量原则）
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
        // 内部路径精确命中：无模板捕获（模板只存在于 host 别名面）
        params: serde_json::Map::new(),
        caller,
        device,
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

    /// 票 08：端点级认证判定只有「档位 × 是否已验签」两个输入，四格全覆盖
    #[test]
    fn auth_tier_judges_verification_state_only() {
        assert!(
            plugin_http_auth_allowed(EndpointAuth::Jwt, true),
            "已验签的可信设备可命中 jwt 档"
        );
        assert!(
            !plugin_http_auth_allowed(EndpointAuth::Jwt, false),
            "要验签的端点遇到未验签调用方（含环回 hook）必须拒——收紧的本体"
        );
        assert!(plugin_http_auth_allowed(EndpointAuth::None, true));
        assert!(
            plugin_http_auth_allowed(EndpointAuth::None, false),
            "显式声明 none 是免凭证可达的唯一形态"
        );
    }

    /// 票 08：身份判定的优先级与线协议取值
    ///
    /// 已验签压倒环回（本机带 token 的请求就是 device）；拿不到对端地址时按匿名处理
    /// （fail-closed：绝不因为「读不到地址」就升成本机身份）。
    #[test]
    fn caller_classification_priority_and_wire_values() {
        assert_eq!(HttpCaller::classify(true, true), HttpCaller::Device);
        assert_eq!(HttpCaller::classify(true, false), HttpCaller::Device);
        assert_eq!(HttpCaller::classify(false, true), HttpCaller::Localhost);
        assert_eq!(HttpCaller::classify(false, false), HttpCaller::Anonymous);
        // 线协议取值是插件侧的判据，改名即破坏契约
        assert_eq!(HttpCaller::Device.as_str(), "device");
        assert_eq!(HttpCaller::Localhost.as_str(), "localhost");
        assert_eq!(HttpCaller::Anonymous.as_str(), "anonymous");
    }

    /// 票 08 + 凭据红线：`caller_identity` 只给 claims 派生标识，JWT 本体与指纹不外泄
    ///
    /// claims 走真实签发/验签链路（不手搓结构体，字段增减不会让本用例假绿）。
    #[test]
    fn caller_identity_carries_only_host_derived_labels() {
        use crate::utils::auth::jwt::JwtService;
        use actix_web::HttpMessage;

        // 无 claims、无对端地址（TestRequest 默认）→ anonymous，且不带设备上下文
        let req = actix_web::test::TestRequest::get()
            .uri("/api/plugin/com.bedcode.terminal-session/task-status")
            .to_http_request();
        assert_eq!(
            caller_identity(&req),
            (HttpCaller::Anonymous, None),
            "未验签且取不到对端地址时按最弱身份处理"
        );

        // 无 claims 但对端是环回 → localhost（hook 脚本可被插件区分）
        let req = actix_web::test::TestRequest::get()
            .uri("/api/plugin/com.bedcode.terminal-session/task-status")
            .peer_addr("127.0.0.1:54321".parse().expect("loopback addr"))
            .to_http_request();
        assert_eq!(caller_identity(&req), (HttpCaller::Localhost, None));

        // 非环回对端同样落 anonymous（IPv6 环回则算本机）
        let req = actix_web::test::TestRequest::get()
            .uri("/api/plugin/com.bedcode.terminal-session/task-status")
            .peer_addr("192.168.1.7:54321".parse().expect("lan addr"))
            .to_http_request();
        assert_eq!(caller_identity(&req).0, HttpCaller::Anonymous);
        let req = actix_web::test::TestRequest::get()
            .uri("/api/plugin/com.bedcode.terminal-session/task-status")
            .peer_addr("[::1]:54321".parse().expect("ipv6 loopback addr"))
            .to_http_request();
        assert_eq!(caller_identity(&req).0, HttpCaller::Localhost);

        // 已验签 → device + claims 派生标识；指纹与 token 本体一律不进转发入参
        let jwt = JwtService::new();
        let token = jwt
            .generate_token(
                "device-1".to_string(),
                Some("Pixel 9".to_string()),
                Some("fp-secret".to_string()),
            )
            .expect("issue token");
        let claims = jwt.verify_token_with_expiry(&token).expect("verify token");
        assert_eq!(
            claims.fingerprint.as_deref(),
            Some("fp-secret"),
            "前置：claims 里确实带指纹，下面断言的「不外泄」才有意义"
        );
        let req = actix_web::test::TestRequest::get()
            .uri("/api/plugin/com.bedcode.terminal-session/task-status")
            .peer_addr("192.168.1.7:54321".parse().expect("lan addr"))
            .to_http_request();
        req.extensions_mut().insert(claims);
        let (caller, device) = caller_identity(&req);
        assert_eq!(caller, HttpCaller::Device, "已验签压倒对端地址判定");
        assert_eq!(
            device,
            Some(serde_json::json!({ "deviceId": "device-1", "deviceName": "Pixel 9" }))
        );
        let rendered = device.expect("device 上下文").to_string();
        assert!(!rendered.contains("fp-secret"), "指纹不得透传给插件");
        assert!(!rendered.contains(&token), "JWT 本体不得透传给插件");
    }

    /// 票 08：认证拒绝的响应形状是「401 + 1007 + 点名两条出路」，而不是 200 带业务码
    ///
    /// 走真实 actix body 读取（不是只看构造器），否则「忘了 `.Unauthorized()`」
    /// 这种最可能的写错方式不会被测到。
    #[actix_web::test]
    async fn unauthenticated_response_is_http_401_with_both_remedies_named() {
        let resp = plugin_http_unauthenticated_response("/api/plugin/com.bedcode.terminal-session/task-queue/add");
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNAUTHORIZED,
            "免凭证拒绝必须是 HTTP 401，不能沿用宿主业务面的 200 + 业务码"
        );
        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], CODE_PLUGIN_AUTH_FAILED as u64);
        let message = json["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("/api/plugin/com.bedcode.terminal-session/task-queue/add"),
            "文案须点名被拒端点: {message}"
        );
        assert!(
            message.contains("jwt") && message.contains("contributes.httpEndpoints"),
            "文案须给出两条出路: {message}"
        );
    }

    /// 票 16：旧前缀别名表——只登记已退役/在退役的那一条，且必须指向合并插件
    #[test]
    fn legacy_http_alias_maps_only_retired_plugin_prefix() {
        assert_eq!(
            legacy_http_alias("com.bedcode.auto-task"),
            Some("com.bedcode.terminal-session")
        );
        // 票 07 B2：改名前的旧 id 前缀同样兜底到新插件（双投窗口）
        assert_eq!(
            legacy_http_alias("com.bedcode.session"),
            Some("com.bedcode.terminal-session")
        );
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
