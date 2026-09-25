//! HTTP 协议网关 — 平台基础服务（HTTP 路由代码注册下沉专项，ABI v29）
//!
//! ## 它是什么
//!
//! 宿主 HTTP 传输面的路由**登记权**已整体移交插件（用户裁定 ④）：插件经
//! `host-http.register-endpoint` 在运行时注册自身路由（含对外 URL 别名、方法、
//! 认证档位），宿主只保留四件事——通用注册表（`server/http/registry`）、通用判定
//! （本文件 [`decide`]）、通用转发（复用 `plugin_controller::forward_to_plugin`）、
//! 验签引擎（`middleware/jwt_auth`）。本模块**零业务路由常量**：不再持有任何
//! 业务 URL 别名表 / 业务域枚举 / 硬编码插件 id。
//!
//! ## 三条不变量
//!
//! 1. **形状不变**：对外 URL、方法、响应 JSON 与迁移前逐字节一致。URL 别名由
//!    插件注册声明（含 `{id}` 模板段），网关按注册表精确/模板匹配，模板捕获值
//!    经 `params` 字段传给插件；响应形状契约锁在本文件测试段。
//! 2. **验签不移动**：移动端 JWT 仍在宿主 `/api` scope 的中间件里统一校验
//!    （AGENTS.md §8 认证红线），网关只挂在它**之后**；转发只透传 claims 派生的
//!    设备标识与 `caller` 三档调用方身份，**JWT 本体与指纹不出宿主**。档位判定
//!    统一在网关（只有网关看得见注册表档位）：`jwt` 档要求宿主已验签，`none` 档
//!    免验签转发。
//! 3. **不发明传输机制**：转发复用 `http/controllers/plugin_controller` 的同一内核
//!    （[`forward_to_plugin`]）与同一请求构造（headers 白名单 / status/contentType
//!    解析口径）。
//!
//! ## 未命中与降级
//!
//! - 未命中注册别名 → 原样放行交路由表（404 / 宿主自持端点照旧）；
//! - 命中但属主插件未激活（注册在册与停用之间的竞态）→ 明确报「插件未激活」
//!   （HTTP 200 + 业务码 1007，与 `/api/plugin/*` 面同口径）；
//! - 命中但档位 `jwt` 而本次未验签 → 401 + 业务码 1007（报「要认证」而非
//!   「插件未激活」，方向不能指错）。
//!
//! 停用回收：插件停用 → 宿主清空其全部注册路由（`registry::purge_for_plugin`），
//! 未激活插件的别名不再可达（404，fail-visible，不静默占用对外 URL 空间）。

use actix_web::body::MessageBody;
use actix_web::dev::{Payload, ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{web, Error, FromRequest, HttpResponse};
use serde_json::Value;

use crate::server::http::controllers::plugin_controller::{
    caller_identity, filter_plugin_request_headers, forward_to_plugin, HttpCaller, PluginHttpRequest,
};
use crate::server::http::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::server::http::registry;
use crate::system::app_context::AppContext;
use bedcode_plugin_api::EndpointAuth;

// ==================== 网关判定 ====================

/// 网关对一次请求的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayDecision {
    /// 命中注册别名且判定通过 → 切插件路径
    Forward,
    /// 属主插件未激活（注册在册与停用之间的竞态）→ 明确报「插件未激活」
    PluginRequired,
    /// 端点档位要求已验签而本次请求没有 → 401
    ///
    /// 单独一档而不是回 [`GatewayDecision::PluginRequired`]：那会把「你没登录」报成
    /// 「插件未激活」，指错方向。
    AuthRequired,
    /// 未命中注册别名 / 插件面不可判定 → 原样放行交路由表
    PassThrough,
}

/// 降级判定（纯函数，转发条件的单一事实源）
///
/// - `entry_auth`：注册档位（`jwt` 要求宿主已验签；`none` 免验签转发）。
/// - `verified`：宿主 JWT 中间件是否已验签并注入 claims（[`caller_identity`] 的
///   device 档）。**需要验签时绝不转发**——这道守卫是网关自身的性质，不依赖
///   中间件注册顺序（顺序错乱时最多多拒一次，不会漏验签）。
/// - `activated`：属主插件是否激活。注册在册而属主停用（竞态）→ 明确报
///   「插件未激活」而非把请求交给不存在的插件。
pub fn decide(entry_auth: EndpointAuth, verified: bool, activated: bool) -> GatewayDecision {
    if !activated {
        return GatewayDecision::PluginRequired;
    }
    if entry_auth == EndpointAuth::Jwt && !verified {
        return GatewayDecision::AuthRequired;
    }
    GatewayDecision::Forward
}

/// 「插件未激活」响应：与 `/api/plugin/*` 路由的未激活响应同口径（HTTP 200 + 业务码 1007）
fn plugin_unavailable_response(entry: &registry::HttpRouteEntry) -> HttpResponse {
    tracing::warn!(
        plugin_id = %entry.owner,
        host_path = %entry.host_path.as_deref().unwrap_or("(internal)"),
        "业务端点不可用（属主插件未激活）"
    );
    HttpResponse::Ok().json(ApiResponse::<()>::error(
        CODE_PLUGIN_AUTH_FAILED,
        &format!("Plugin {} is not activated", entry.owner),
    ))
}

/// 转发失败时的显式错误响应（与宿主业务端点的错误口径一致：HTTP 200 + 业务码）
fn bad_request(message: &str) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::<()>::error(CODE_INVALID_REQUEST, message))
}

/// 「调用方未验签」响应：HTTP 401 + 业务码 1007，与 JWT 中间件的 401 同一码
///
/// 这条分支生产链路正常走不到（中间件在网关之外已把无 JWT 的 `jwt` 档请求 401），
/// 它是「中间件顺序被改错」时的兜底，所以回 401 而不是回「插件未激活」。
fn unauthorized_response(path: &str) -> HttpResponse {
    HttpResponse::Unauthorized().json(ApiResponse::<()>::error(
        CODE_PLUGIN_AUTH_FAILED,
        &format!("Authentication required for {}", path),
    ))
}

/// 查询串 → 转发用 JSON 对象
///
/// 复用 `web::Query<HashMap<String,String>>`——与 `/api/plugin/*` 路由同一个提取器，因此
/// 百分号转义、重复键（后者覆盖前者）、非 UTF-8 序列（lossy 替换）的口径与宿主逐字一致。
/// 该目标类型下提取实际不会失败，但错误仍走「显式拒绝」而非 `expect`：这条路对局域网
/// 可达（0.0.0.0），panic 即 DoS 面。
fn query_object(query_string: &str) -> Result<Value, String> {
    match web::Query::<std::collections::HashMap<String, String>>::from_query(query_string) {
        Ok(q) => Ok(Value::Object(
            q.into_inner().into_iter().map(|(k, v)| (k, Value::String(v))).collect(),
        )),
        Err(e) => {
            tracing::warn!(error = %e, query = %query_string, "业务端点查询串解析失败");
            Err(format!("Invalid query string: {e}"))
        }
    }
}

/// 请求体 → 转发用 JSON
///
/// 三种情形分开：
/// - 无载荷 → `Null`（与 `_http_endpoint` 的 `Option<Json>` 同口径，GET 端点即此分支）；
/// - 合法 JSON → 原样交给插件（不重新序列化宿主 DTO，避免未知字段被丢弃）；
/// - 有载荷但非 JSON → 显式失败（业务端点今天的宿主行为是拒绝，不许静默当空请求）。
async fn body_value(req: &actix_web::HttpRequest, payload: &mut Payload) -> Result<Value, String> {
    let bytes = match web::Bytes::from_request(req, payload).await {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!(error = %e, "业务端点请求体读取失败");
            return Err(format!("Failed to read request body: {e}"));
        }
    };
    if bytes.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice::<Value>(&bytes).map_err(|e| format!("Invalid JSON body: {e}"))
}

/// 网关中间件：注册别名（精确 + 模板）→ 插件，其余请求原样放行
///
/// 挂在 `/api` scope 的 JWT 中间件**之后**（`Scope::wrap` 后注册者先执行，故本中间件注册在
/// 验签之前；顺序语义见 [`scope_wrap_registration_puts_jwt_outermost`]）。即便接线顺序被改错，
/// [`decide`] 的 `verified` 前置也会挡住未验签请求进插件。
pub(crate) async fn business_gateway<B>(req: ServiceRequest, next: Next<B>) -> Result<ServiceResponse, Error>
where
    B: MessageBody + 'static,
{
    let path = req.path().to_string();
    let method = req.method().as_str().to_string();
    // 未命中注册别名（精确匹配 + `{id}` 模板匹配）→ 原样放行交路由表
    let Some(host_match) = registry::find_by_host(&path, &method) else {
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    };
    let entry = host_match.entry.clone();

    // 调用方身份（票 08）：device 档 = 宿主 JWT 中间件已验签并注入 claims。
    // 环回 / 匿名两档只在插件把该端点显式注册成 `auth: "none"` 时才可能走到转发。
    let (caller, device) = caller_identity(req.request());
    let verified = matches!(caller, HttpCaller::Device);
    // 无 AppContext（无头 / 库级测试 / 初始化中间态）= 插件面不可判定 → 交回链条，
    // 绝不凭注册表就把请求递给不存在的插件
    let decision = match AppContext::try_global() {
        None => GatewayDecision::PassThrough,
        Some(ctx) => {
            let activated = ctx.plugin_host().is_activated(&entry.owner).await;
            decide(entry.auth, verified, activated)
        }
    };
    tracing::debug!(
        path = %path,
        plugin_id = %entry.owner,
        endpoint = %entry.path,
        caller = %caller.as_str(),
        forward = matches!(decision, GatewayDecision::Forward),
        "HTTP 协议网关判定"
    );

    match decision {
        GatewayDecision::PassThrough => next.call(req).await.map(|res| res.map_into_boxed_body()),
        GatewayDecision::PluginRequired => {
            let (http_req, _payload) = req.into_parts();
            Ok(ServiceResponse::new(http_req, plugin_unavailable_response(&entry)))
        }
        GatewayDecision::AuthRequired => {
            tracing::warn!(
                path = %path,
                plugin_id = %entry.owner,
                caller = %caller.as_str(),
                "业务端点要求已验签调用方，本次请求未通过宿主 JWT 验签"
            );
            let (http_req, _payload) = req.into_parts();
            Ok(ServiceResponse::new(http_req, unauthorized_response(&path)))
        }
        GatewayDecision::Forward => {
            // 判定完成才开始消费载荷：query / headers / claims 只读，body 走 payload
            let headers = filter_plugin_request_headers(req.headers());
            let query = match query_object(req.query_string()) {
                Ok(v) => v,
                Err(msg) => {
                    let (http_req, _payload) = req.into_parts();
                    return Ok(ServiceResponse::new(http_req, bad_request(&msg)));
                }
            };
            let (http_req, mut payload) = req.into_parts();
            let body = match body_value(&http_req, &mut payload).await {
                Ok(v) => v,
                Err(msg) => return Ok(ServiceResponse::new(http_req, bad_request(&msg))),
            };
            // 模板捕获参数（`{id}` 段）随请求注入插件；精确命中时为空对象
            let params = host_match
                .params
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect();
            let request = PluginHttpRequest {
                endpoint_path: &entry.path,
                method: &method,
                headers,
                body,
                query,
                params,
                caller,
                device,
            };
            let resp = forward_to_plugin(&entry.owner, &request).await;
            Ok(ServiceResponse::new(http_req, resp))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::http::controllers::plugin_controller::build_plugin_http_args;
    use crate::server::http::middleware::jwt_auth::jwt_gateway;
    use crate::utils::auth::jwt::JwtService;

    /// 注册一条测试别名（全局注册表跨用例共享：路径带用例唯一段）
    fn register_test_alias(owner: &str, path: &str, host: &str, methods: &[&str], auth: EndpointAuth) {
        let methods: Vec<String> = methods.iter().map(|s| s.to_string()).collect();
        registry::register(owner, path, Some(host), &methods, auth).expect("register test alias");
    }

    fn purge(owner: &str) {
        registry::purge_for_plugin(owner);
    }

    // ==================== 判定 ====================

    /// 转发条件：已验签 × 属主激活 × 档位齐备
    #[test]
    fn decide_forwards_only_when_verified_activated_and_tier_ok() {
        assert_eq!(decide(EndpointAuth::Jwt, true, true), GatewayDecision::Forward);
        assert_eq!(
            decide(EndpointAuth::None, false, true),
            GatewayDecision::Forward,
            "none 档免验签转发"
        );
        // 未验签 + jwt 档 → 要认证（报「要认证」而非「插件未激活」）
        assert_eq!(decide(EndpointAuth::Jwt, false, true), GatewayDecision::AuthRequired);
        // 属主未激活（注册在册与停用竞态）→ 明确报「插件未激活」
        assert_eq!(decide(EndpointAuth::Jwt, true, false), GatewayDecision::PluginRequired);
        assert_eq!(
            decide(EndpointAuth::None, false, false),
            GatewayDecision::PluginRequired
        );
    }

    // ==================== 转发入参 ====================

    /// 转发入参形状：与 `/api/plugin/*` 共用同一构造器，故两条路径不可能各答一版。
    ///
    /// `params`（ABI v29 模板捕获）恒存在：精确命中时为 `{}`，模板命中时携带捕获值；
    /// `device` 只在已验签时出现。
    #[test]
    fn forwarded_request_shape_locks_device_headers_and_params() {
        let mut headers = serde_json::Map::new();
        headers.insert("content-type".to_string(), Value::String("application/json".into()));
        let args = build_plugin_http_args(&PluginHttpRequest {
            endpoint_path: "configs",
            method: "GET",
            headers,
            body: Value::Null,
            query: Value::Object(serde_json::Map::from_iter([(
                "session_id".to_string(),
                Value::String("s-1".to_string()),
            )])),
            params: serde_json::Map::new(),
            caller: HttpCaller::Device,
            device: Some(serde_json::json!({ "deviceId": "device-1" })),
        });
        assert_eq!(
            args,
            serde_json::json!({
                "method": "GET",
                "path": "configs",
                "headers": { "content-type": "application/json" },
                "body": null,
                "query": { "session_id": "s-1" },
                "params": {},
                "caller": "device",
                "device": { "deviceId": "device-1" },
            })
        );

        // 模板捕获参数：`{id}` 捕获值随 `params` 注入插件
        let with_params = build_plugin_http_args(&PluginHttpRequest {
            endpoint_path: "sessions/stop",
            method: "POST",
            headers: serde_json::Map::new(),
            body: Value::Null,
            query: Value::Object(serde_json::Map::new()),
            params: serde_json::Map::from_iter([("id".to_string(), Value::String("s-1".to_string()))]),
            caller: HttpCaller::Localhost,
            device: None,
        });
        assert_eq!(with_params["params"], serde_json::json!({ "id": "s-1" }));

        let no_device = build_plugin_http_args(&PluginHttpRequest {
            endpoint_path: "configs",
            method: "GET",
            headers: serde_json::Map::new(),
            body: Value::Null,
            query: Value::Object(serde_json::Map::new()),
            params: serde_json::Map::new(),
            caller: HttpCaller::Localhost,
            device: None,
        });
        assert!(no_device.get("device").is_none(), "无验签结果时不写 device 键");
        assert_eq!(
            no_device.get("caller").and_then(|v| v.as_str()),
            Some("localhost"),
            "免凭证调用方也必须带可区分的身份"
        );
    }

    /// 查询串解析：与 `web::Query<HashMap<String,String>>` 提取器同口径
    #[test]
    fn query_object_keeps_query_extractor_semantics() {
        assert_eq!(
            query_object("session_id=s-1&limit=3").unwrap(),
            serde_json::json!({ "session_id": "s-1", "limit": "3" })
        );
        assert_eq!(query_object("").unwrap(), serde_json::json!({}));
        // 百分号转义必须与宿主提取器同解码口径（否则插件看到的参数与宿主不同）
        assert_eq!(
            query_object("path=%2Fsrv%2Fapp").unwrap(),
            serde_json::json!({ "path": "/srv/app" })
        );
        // 提取器的既有语义一并锁住（不是网关的解释）：重复键后者覆盖、非 UTF-8 走 lossy
        assert_eq!(query_object("a=1&a=2").unwrap(), serde_json::json!({ "a": "2" }));
        assert_eq!(
            query_object("bad=%FF").unwrap(),
            serde_json::json!({ "bad": "\u{FFFD}" })
        );
    }

    /// 请求体提取：无载荷 → Null；合法 JSON → 原样；畸形 JSON → 显式失败
    #[actix_web::test]
    async fn body_extraction_distinguishes_empty_from_malformed() {
        let (http_req, mut payload) = actix_web::test::TestRequest::get()
            .uri("/api/configs")
            .to_srv_request()
            .into_parts();
        assert_eq!(body_value(&http_req, &mut payload).await.unwrap(), Value::Null);

        let (http_req, mut payload) = actix_web::test::TestRequest::post()
            .uri("/api/file-tree")
            .insert_header(("content-type", "application/json"))
            .set_json(serde_json::json!({ "session_id": "s-1" }))
            .to_srv_request()
            .into_parts();
        assert_eq!(
            body_value(&http_req, &mut payload).await.unwrap(),
            serde_json::json!({ "session_id": "s-1" })
        );

        let (http_req, mut payload) = actix_web::test::TestRequest::post()
            .uri("/api/file-tree")
            .insert_header(("content-type", "application/json"))
            .set_payload(b"{not json".to_vec())
            .to_srv_request()
            .into_parts();
        let err = body_value(&http_req, &mut payload).await.unwrap_err();
        assert!(err.contains("Invalid JSON body"), "畸形 body 必须显式失败, got: {err}");
    }

    // ==================== 中间件行为（真实 actix 栈） ====================

    /// 组装「JWT 中间件（外）→ 网关中间件（内）→ 哨兵宿主 handler」的最小 `/api` scope
    ///
    /// 两个中间件都取生产实现（`jwt_gateway` / `business_gateway`），所以这里验的是真实
    /// 链路顺序，不是测试自己搭的近似物。哨兵 handler 复刻宿主自持端点的回包形状。
    fn test_scope() -> impl actix_web::dev::HttpServiceFactory + 'static {
        use actix_web::middleware::from_fn;
        web::scope("/api")
            .wrap(from_fn(business_gateway))
            .wrap(from_fn(jwt_gateway))
            .route("/configs", web::get().to(host_sentinel))
            .route("/git/branches", web::get().to(host_sentinel))
    }

    /// `Scope::wrap` 的注册顺序语义（app.rs 挂载顺序的依据，第三方 API 的承重假设）
    #[actix_web::test]
    async fn scope_wrap_registration_puts_jwt_outermost() {
        use actix_web::body::MessageBody;
        use actix_web::middleware::{from_fn, Next};
        use std::sync::{Arc, Mutex};

        async fn probe<B>(
            name: &'static str,
            trace: Arc<Mutex<Vec<&'static str>>>,
            req: ServiceRequest,
            next: Next<B>,
        ) -> Result<ServiceResponse, Error>
        where
            B: MessageBody + 'static,
        {
            trace.lock().unwrap().push(name);
            next.call(req).await.map(|res| res.map_into_boxed_body())
        }

        let trace = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let t_gateway = trace.clone();
        let t_jwt = trace.clone();
        let app = actix_web::test::init_service(
            actix_web::App::new().service(
                web::scope("/api")
                    .wrap(from_fn(move |req, next| probe("gateway", t_gateway.clone(), req, next)))
                    .wrap(from_fn(move |req, next| probe("jwt", t_jwt.clone(), req, next)))
                    .route("/x", web::get().to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        actix_web::test::call_service(&app, actix_web::test::TestRequest::get().uri("/api/x").to_request()).await;
        assert_eq!(
            *trace.lock().unwrap(),
            vec!["jwt", "gateway"],
            "最后注册的中间件必须先执行，否则 app.rs 的「先验签后网关」失效"
        );
    }

    async fn host_sentinel() -> HttpResponse {
        HttpResponse::Ok().json(ApiResponse::ok_with_data(serde_json::json!({ "configs": [] })))
    }

    fn bearer_token() -> String {
        JwtService::new()
            .generate_token("device-1".to_string(), Some("Pixel 9".to_string()), None)
            .expect("issue token")
    }

    /// 未验签请求绝不进网关转发：`/api/configs` 无 token → 401（今天的形状）
    #[actix_web::test]
    async fn unverified_requests_never_reach_gateway() {
        let app = actix_web::test::init_service(actix_web::App::new().service(test_scope())).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get().uri("/api/configs").to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
        let body: Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["code"], CODE_PLUGIN_AUTH_FAILED as u64);
    }

    /// 未登记路径 / 方法不符：网关不介入，状态码与「同一 scope 不挂网关」完全一致
    ///
    /// 用对照组而不是写死 405：`Scope` 挂了中间件之后，actix 对「路径在、方法不在」的
    /// 应答并不是教科书上的 405（今天宿主面就是这一格行为）。对照组把标准钉在「与不挂
    /// 网关时相同」，比钉一个记错的数字更诚实。
    #[actix_web::test]
    async fn unregistered_and_wrong_method_requests_are_not_touched() {
        use std::str::FromStr;

        let gateway_only = actix_web::test::init_service(
            actix_web::App::new().service(
                web::scope("/api")
                    .wrap(actix_web::middleware::from_fn(business_gateway))
                    .route("/configs", web::get().to(host_sentinel))
                    .route("/git/branches", web::get().to(host_sentinel)),
            ),
        )
        .await;
        let baseline = actix_web::test::init_service(
            actix_web::App::new().service(
                web::scope("/api")
                    .route("/configs", web::get().to(host_sentinel))
                    .route("/git/branches", web::get().to(host_sentinel)),
            ),
        )
        .await;

        for (method, uri) in [
            ("POST", "/api/configs"),
            ("GET", "/api/not-a-business-endpoint"),
            ("PUT", "/api/git/branches?session_id=s-1"),
            ("DELETE", "/api/git/checkout"),
        ] {
            let mk = || {
                actix_web::test::TestRequest::default()
                    .method(actix_web::http::Method::from_str(method).unwrap())
                    .uri(uri)
                    .to_request()
            };
            let with = actix_web::test::call_service(&gateway_only, mk()).await;
            let without = actix_web::test::call_service(&baseline, mk()).await;
            assert_eq!(
                with.status(),
                without.status(),
                "{method} {uri}：挂上网关后的状态码必须与不挂时一致（网关不得介入）"
            );
        }
    }

    /// 已登记别名 + 无 AppContext（无头/测试：插件面不可判定）→ 原样放行（哨兵应答）
    #[actix_web::test]
    async fn registered_alias_falls_through_when_plugin_surface_unavailable() {
        let owner = "test-gw-fallthrough";
        register_test_alias(owner, "configs", "/api/configs", &["GET"], EndpointAuth::Jwt);
        let app = actix_web::test::init_service(actix_web::App::new().service(test_scope())).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get()
                .uri("/api/configs")
                .insert_header(("Authorization", format!("Bearer {}", bearer_token())))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(
            body,
            serde_json::json!({ "code": 0, "message": "ok", "data": { "configs": [] } }),
            "无 AppContext 时不得把请求交给不存在的插件（原样放行）"
        );
        purge(owner);
    }

    /// 「插件未激活」响应形状：HTTP 200 + `{code:1007,message}`，与既有未激活响应同口径
    #[actix_web::test]
    async fn plugin_required_response_keeps_existing_error_shape() {
        let entry = registry::register(
            "test-gw-shape",
            "configs",
            Some("/api/configs-shape"),
            &["GET".to_string()],
            EndpointAuth::Jwt,
        )
        .expect("register");
        let resp = plugin_unavailable_response(&entry);
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let ct = resp
            .headers()
            .get(actix_web::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            ct.starts_with("application/json"),
            "错误响应必须是 JSON 信封, got: {ct}"
        );
        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "code": CODE_PLUGIN_AUTH_FAILED,
                "message": "Plugin test-gw-shape is not activated",
            })
        );
        purge("test-gw-shape");
    }

    /// 票 08：「要验签而未验签」的响应形状是 401 + 1007 + 点名对外路径
    #[actix_web::test]
    async fn auth_required_response_says_authentication_not_activation() {
        let resp = unauthorized_response("/api/configs");
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], CODE_PLUGIN_AUTH_FAILED as u64);
        assert_eq!(
            json["message"],
            serde_json::json!("Authentication required for /api/configs"),
            "文案按对外别名路径说，不暴露插件端点段"
        );
        assert!(
            !json["message"].as_str().unwrap_or_default().contains("not activated"),
            "不得把未登录报成插件未激活"
        );
    }

    /// 降级分支不消费 payload：网关若在判定阶段读 body，宿主 handler 就会拿到空载荷并
    /// 400——那是最隐蔽的「实现搬走、行为变味」形态，故单列一条。
    #[actix_web::test]
    async fn fallback_branch_leaves_the_payload_intact() {
        async fn echo(body: web::Json<Value>) -> HttpResponse {
            HttpResponse::Ok().json(serde_json::json!({ "echo": body.0 }))
        }
        let owner = "test-gw-payload";
        register_test_alias(owner, "file-tree", "/api/file-tree", &["POST"], EndpointAuth::Jwt);
        let app = actix_web::test::init_service(
            actix_web::App::new().service(
                web::scope("/api")
                    .wrap(actix_web::middleware::from_fn(business_gateway))
                    .route("/file-tree", web::post().to(echo)),
            ),
        )
        .await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/api/file-tree")
                .insert_header(("content-type", "application/json"))
                .set_json(serde_json::json!({ "session_id": "s-9", "depth": 3 }))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["echo"]["session_id"], "s-9");
        assert_eq!(body["echo"]["depth"], 3);
        purge(owner);
    }

    // ==================== 形状契约锁（对外响应逐字节一致） ====================

    /// 业务端点响应形状 golden：宿主侧当前输出即契约，插件面必须逐字段复刻。
    ///
    /// 锁的是「JSON 形状」而不是「实现在哪」：DTO 的 serde 表示就是移动端看到的字节。
    /// 票 02/03/04 的插件端点回包必须与本用例逐字段相同（含可选字段的缺席形态）。
    #[test]
    fn business_endpoint_shapes_are_locked() {
        use crate::server::http::dtos::config_dto::{
            ConfigItem, ConfigListResponseData, QuickActionItem, QuickActionListResponseData,
        };
        use crate::server::http::dtos::file_dto::{
            FileContentResponseData, FileDiffLine, FileDiffResponseData, FileTreeNode, FileTreeResponseData,
        };
        use crate::server::http::dtos::git_dto::{
            GitBranchesResponseData, GitCheckoutResponseData, GitStatusResponseData,
        };

        // GET /api/configs
        assert_shape(
            ApiResponse::ok_with_data(ConfigListResponseData {
                configs: vec![ConfigItem {
                    id: "c1".into(),
                    name: "工作台".into(),
                    environment: "linux".into(),
                    wsl_distro: None,
                    working_dir: "/srv/app".into(),
                    command: "bash".into(),
                }],
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "configs": [{
                    "id": "c1", "name": "工作台", "environment": "linux", "wslDistro": null,
                    "workingDir": "/srv/app", "command": "bash"
                }] }
            }),
        );
        // wslDistro 为 None 时是 **显式 null**（ConfigItem 没有 skip_serializing_if）
        assert_shape(
            ApiResponse::ok_with_data(ConfigListResponseData {
                configs: vec![ConfigItem {
                    id: "c2".into(),
                    name: "wsl".into(),
                    environment: "wsl".into(),
                    wsl_distro: Some("Ubuntu-24.04".into()),
                    working_dir: "/home/u".into(),
                    command: "claude".into(),
                }],
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "configs": [{
                    "id": "c2", "name": "wsl", "environment": "wsl", "wslDistro": "Ubuntu-24.04",
                    "workingDir": "/home/u", "command": "claude"
                }] }
            }),
        );

        // GET /api/quick-actions
        assert_shape(
            ApiResponse::ok_with_data(QuickActionListResponseData {
                actions: vec![
                    QuickActionItem {
                        id: "a1".into(),
                        name: "提交".into(),
                        content: "git commit".into(),
                        icon: None,
                        color: None,
                    },
                    QuickActionItem {
                        id: "a2".into(),
                        name: "推送".into(),
                        content: "git push".into(),
                        icon: Some("upload".into()),
                        color: Some("#ff0000".into()),
                    },
                ],
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "actions": [
                    { "id": "a1", "name": "提交", "content": "git commit", "icon": null, "color": null },
                    { "id": "a2", "name": "推送", "content": "git push", "icon": "upload", "color": "#ff0000" }
                ] }
            }),
        );

        // POST /api/file-tree 与 POST /api/diff-tree 共用同一树形状
        let tree = ApiResponse::ok_with_data(FileTreeResponseData {
            tree: vec![
                FileTreeNode {
                    name: "src".into(),
                    node_type: "directory".into(),
                    path: Some("src".into()),
                    children: Some(vec![FileTreeNode {
                        name: "main.rs".into(),
                        node_type: "file".into(),
                        path: Some("src/main.rs".into()),
                        children: None,
                    }]),
                },
                FileTreeNode {
                    name: ".git".into(),
                    node_type: "directory".into(),
                    path: None,
                    children: None,
                },
            ],
        });
        assert_shape(
            tree.clone(),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "tree": [
                    { "name": "src", "nodeType": "directory", "path": "src", "children": [
                        { "name": "main.rs", "nodeType": "file", "path": "src/main.rs" }
                    ] },
                    { "name": ".git", "nodeType": "directory" }
                ] }
            }),
        );

        // POST /api/file-content
        assert_shape(
            ApiResponse::ok_with_data(FileContentResponseData {
                content: "hello".into(),
                file_name: "b.txt".into(),
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "content": "hello", "fileName": "b.txt" }
            }),
        );

        // POST /api/file-diff
        assert_shape(
            ApiResponse::ok_with_data(FileDiffResponseData {
                file_name: "a.rs".into(),
                lines: vec![
                    FileDiffLine {
                        line_type: "removed".into(),
                        content: "let a = 1;".into(),
                        old_line_no: Some(3),
                        new_line_no: None,
                    },
                    FileDiffLine {
                        line_type: "added".into(),
                        content: "let a = 2;".into(),
                        old_line_no: None,
                        new_line_no: Some(3),
                    },
                    FileDiffLine {
                        line_type: "context".into(),
                        content: "".into(),
                        old_line_no: Some(4),
                        new_line_no: Some(4),
                    },
                ],
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "fileName": "a.rs", "lines": [
                    { "type": "removed", "content": "let a = 1;", "oldLineNo": 3 },
                    { "type": "added", "content": "let a = 2;", "newLineNo": 3 },
                    { "type": "context", "content": "", "oldLineNo": 4, "newLineNo": 4 }
                ] }
            }),
        );

        // GET /api/git/branches：非 git 仓库与仓库两态
        assert_shape(
            ApiResponse::ok_with_data(GitBranchesResponseData {
                current_branch: None,
                branches: vec![],
                is_git_repo: false,
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "currentBranch": null, "branches": [], "isGitRepo": false }
            }),
        );
        assert_shape(
            ApiResponse::ok_with_data(GitBranchesResponseData {
                current_branch: Some("main".into()),
                branches: vec!["main".into(), "dev".into()],
                is_git_repo: true,
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "currentBranch": "main", "branches": ["main", "dev"], "isGitRepo": true }
            }),
        );

        // GET /api/git/status
        assert_shape(
            ApiResponse::ok_with_data(GitStatusResponseData {
                has_changes: true,
                changed_count: 2,
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "hasChanges": true, "changedCount": 2 }
            }),
        );

        // POST /api/git/checkout
        assert_shape(
            ApiResponse::ok_with_data(GitCheckoutResponseData { branch: "dev".into() }),
            serde_json::json!({ "code": 0, "message": "ok", "data": { "branch": "dev" } }),
        );

        // 错误信封：这些端点今天全部是 HTTP 200 + 业务码。插件面必须同口径
        assert_shape(
            ApiResponse::<()>::error(404, "Session not found"),
            serde_json::json!({ "code": 404, "message": "Session not found" }),
        );

        // ABI v29（sessions REST 下沉）：/api/sessions* 七条由插件 sessions_http 域
        // 复刻旧宿主控制器形状——SessionItem / StartSessionResponseData /
        // SessionHistoryData 的 serde 表示即移动端看到的字节，插件面必须逐字段同形
        use crate::server::http::dtos::session_dto::{
            SessionHistoryData, SessionItem, SessionListResponseData, StartSessionResponseData,
        };
        assert_shape(
            ApiResponse::ok_with_data(SessionListResponseData {
                sessions: vec![SessionItem {
                    id: "s-1".into(),
                    name: "工作台".into(),
                    status: "Running".into(),
                    created_at: "2026-09-25T00:00:00Z".into(),
                    started_at: Some("2026-09-25T00:00:01Z".into()),
                    session_type: Some("pty".into()),
                    config_id: Some("c1".into()),
                    task_status: Some("idle".into()),
                    task_reason: None,
                }],
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "sessions": [{
                    "id": "s-1", "name": "工作台", "status": "Running",
                    "createdAt": "2026-09-25T00:00:00Z", "startedAt": "2026-09-25T00:00:01Z",
                    "sessionType": "pty", "configId": "c1", "taskStatus": "idle"
                }] }
            }),
        );
        assert_shape(
            ApiResponse::ok_with_data(StartSessionResponseData {
                session_id: "s-2".into(),
                status: "running".into(),
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": { "sessionId": "s-2", "status": "running" }
            }),
        );
        assert_shape(
            ApiResponse::ok_with_data(SessionHistoryData {
                min_offset: 0,
                snapshot_offset: 1024,
                history_bytes: 2048,
                data_base64: "aGVsbG8=".into(),
            }),
            serde_json::json!({
                "code": 0, "message": "ok",
                "data": {
                    "minOffset": 0, "snapshotOffset": 1024, "historyBytes": 2048,
                    "dataBase64": "aGVsbG8="
                }
            }),
        );
    }

    fn assert_shape<T: serde::Serialize>(value: ApiResponse<T>, expected: Value) {
        assert_eq!(
            serde_json::to_value(&value).expect("DTO 必须可序列化"),
            expected,
            "响应形状漂移：移动端会看到不同字节"
        );
    }
}
