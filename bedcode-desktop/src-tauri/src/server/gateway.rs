//! HTTP 协议网关 — 平台基础服务（宿主业务清零票 01）
//!
//! ## 它是什么
//!
//! 宿主 HTTP 面混着两类端点：**引擎端点**（auth / sessions / health / 插件代理 / static）
//! 与**业务端点**（配置、快捷指令、文件浏览、git——产品概念，按 ADR 0022 裁剪线该归插件）。
//! 本模块只处理后者：一张「业务 URL → 目标插件端点」的别名路由表，命中后把请求交给插件的
//! `_http_endpoint`，让业务实现整块搬进插件工程而移动端一行不改。
//!
//! ## 三条不变量
//!
//! 1. **形状不变**：URL、方法、响应 JSON 与今天逐字节一致。落表即锁契约——[`BUSINESS_ROUTES`]
//!    每条带归属插件与业务域，形状 golden 见 [`business_endpoint_shapes_are_locked_for_dual_track`]。
//! 2. **验签不移动**：移动端 JWT 仍在宿主 `/api` scope 的中间件里统一校验（AGENTS.md §8 认证
//!    红线），网关只挂在它**之后**（[`unverified_requests_never_reach_gateway`]）；转发只透传
//!    claims 派生的设备标识与 [`caller`] 三档调用方身份（票 08），**JWT 本体与指纹不出宿主**。
//!    别名条目自带的 [`RouteAuth`] 与插件端点声明的档位**取较严者**（见 [`decide`]）——
//!    两个声明面都不得单方面把宿主要求验签的业务端点放开。
//! 3. **不发明传输机制**：转发复用 `controllers/plugin_controller` 的同一内核
//!    （[`forward_to_plugin`]）与同一端点声明治理（manifest `contributes.httpEndpoints`）。
//!
//! ## 双轨与降级
//!
//! 别名目标**未激活**或**未声明该端点** → 请求原样落到宿主旧实现（响应字节不变）；两条都满足
//! 才切插件路径。判定 [`decide`] 是纯函数，切换条件与两种降级路径全部可单测。宿主旧实现退役后
//! （票 02/03/04 的 contract 步骤）条目 [`FallbackPolicy`] 翻到 [`FallbackPolicy::PluginRequired`]：
//! 业务数据真源已在插件，宿主不留副本，插件不在时返回明确的「插件未激活」错误而非假数据。
//!
//! ## 为什么是中间件而不是新路由
//!
//! 降级分支必须让 actix 继续按原路由表分发：宿主 handler 的类型化提取器（`web::Json<T>` /
//! `web::Query<T>`）连同 400/405 语义一字都不能改。在 handler 层做回落就得手工复现这些提取器
//! 行为，那正是形状漂移的源头。中间件形态下「决定回落」时连 payload 都不碰——只有 Forward
//! 分支才 [`ServiceRequest::into_parts`] 消费载荷。

use actix_web::body::MessageBody;
use actix_web::dev::{Payload, ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{web, Error, FromRequest, HttpResponse};
use serde_json::Value;

use crate::server::controllers::plugin_controller::{
    caller_identity, filter_plugin_request_headers, forward_to_plugin, HttpCaller, PluginHttpRequest,
};
use crate::server::dtos::{ApiResponse, CODE_INVALID_REQUEST, CODE_PLUGIN_AUTH_FAILED};
use crate::system::app_context::AppContext;
use bedcode_plugin_api::EndpointAuth;

/// session 插件 id（配置 / 快捷指令 / 文件浏览 / git 四域的接管方）
const SESSION_PLUGIN: &str = "com.bedcode.terminal-session";

// ==================== 业务 URL 别名路由表 ====================

/// 业务域归属（审计面：网关条目必须说清这是哪块业务、归哪个插件）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessDomain {
    /// 会话配置查询面（与桌面命令面同真源，消除双轨数据源）
    SessionConfig,
    /// 快捷指令
    QuickAction,
    /// 本地文件浏览（文件树 / 内容 / diff）
    FileBrowse,
    /// 工作区 git
    Git,
    /// 认证链（票 07：公开路由，编排归插件）
    Auth,
}

impl BusinessDomain {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BusinessDomain::SessionConfig => "session-config",
            BusinessDomain::QuickAction => "quick-action",
            BusinessDomain::FileBrowse => "file-browse",
            BusinessDomain::Git => "git",
            BusinessDomain::Auth => "auth",
        }
    }
}

/// 别名目标不可用时的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackPolicy {
    /// 双轨期：请求原样落到宿主旧实现
    HostImplementation,
    /// contract 后：宿主实现与宿主数据面已退役，明确报「插件未激活」
    PluginRequired,
}

/// 别名的认证前置（票 07）：大多数业务端点要求宿主已验签（JWT 中间件先行），
/// `/api/auth/*` 是**公开路由**——它们本身在 JWT 之前（移动端拿 token 的入口），
/// 必须免验签转发给插件（验签执行点在插件 auth 域 + host-auth 原语）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteAuth {
    /// 宿主 JWT 中间件验签通过后才可转发（默认；claims 派生设备上下文随转发注入）
    Authenticated,
    /// 公开路由：无 JWT 前置，直接按别名判定转发（`/api/auth/*` 专用）
    Public,
}

/// 一条业务 URL 别名
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessRoute {
    /// 宿主对外路径（逐字等于今天注册的路由，移动端零改动的锚点）
    pub path: &'static str,
    /// 归属插件 id
    pub plugin_id: &'static str,
    /// 目标端点段（插件 `contributes.httpEndpoints` 里的相对段）
    pub endpoint: &'static str,
    /// 该别名对外暴露的方法（与今天的宿主路由一致）
    pub methods: &'static [&'static str],
    pub domain: BusinessDomain,
    pub fallback: FallbackPolicy,
    /// 认证前置（票 07）：Authenticated = 需宿主验签；Public = 免验签转发
    pub auth: RouteAuth,
}

impl BusinessRoute {
    /// 端点声明全路径：按属主插件的声明清单做精确匹配时用的键
    ///
    /// 与 `/api/plugin/{id}/{path}` 那条路由同一形态——声明面只有一张表，网关不为自己的
    /// 别名开第二条判定口径。
    pub fn declared_path(&self) -> String {
        format!("/api/plugin/{}/{}", self.plugin_id, self.endpoint)
    }
}

/// 业务 URL 别名路由表
///
/// 新增条目 = 新增平台编排，必须同时补归属插件声明与该业务的形状契约测试
/// （[`route_table_golden`] + [`business_endpoint_shapes_are_locked_for_dual_track`]）。
/// 引擎端点（`/api/auth/*`、`/api/sessions*`、`/api/health`、`/api/plugin/*`、`/static/*`）
/// **永不入表**：它们是协议面不是业务面（spec 决策 2）。
pub const BUSINESS_ROUTES: &[BusinessRoute] = &[
    BusinessRoute {
        path: "/api/configs",
        plugin_id: SESSION_PLUGIN,
        endpoint: "configs",
        methods: &["GET"],
        domain: BusinessDomain::SessionConfig,
        // 票 02 contract：会话配置查询面真源已下沉插件，宿主不再持有业务副本
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/quick-actions",
        plugin_id: SESSION_PLUGIN,
        endpoint: "quick-actions",
        methods: &["GET"],
        domain: BusinessDomain::QuickAction,
        // 票 02 contract：快捷指令真源已下沉插件（私有库），宿主旧表契约退役
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/file-tree",
        plugin_id: SESSION_PLUGIN,
        endpoint: "file-tree",
        methods: &["POST"],
        domain: BusinessDomain::FileBrowse,
        // 票 03 contract：文件浏览真源已下沉插件（host-fs + host-process）
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/file-tree-children",
        plugin_id: SESSION_PLUGIN,
        endpoint: "file-tree-children",
        methods: &["GET"],
        domain: BusinessDomain::FileBrowse,
        // 票 03 contract
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/file-content",
        plugin_id: SESSION_PLUGIN,
        endpoint: "file-content",
        methods: &["POST"],
        domain: BusinessDomain::FileBrowse,
        // 票 03 contract
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/diff-tree",
        plugin_id: SESSION_PLUGIN,
        endpoint: "diff-tree",
        methods: &["POST"],
        domain: BusinessDomain::FileBrowse,
        // 票 03 contract
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/file-diff",
        plugin_id: SESSION_PLUGIN,
        endpoint: "file-diff",
        methods: &["POST"],
        domain: BusinessDomain::FileBrowse,
        // 票 03 contract
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/git/branches",
        plugin_id: SESSION_PLUGIN,
        endpoint: "git/branches",
        methods: &["GET"],
        domain: BusinessDomain::Git,
        // 票 04 contract：git 分支查询面真源下沉插件（host-process run-sync 执行），
        // 宿主 git_controller 已退役
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/git/status",
        plugin_id: SESSION_PLUGIN,
        endpoint: "git/status",
        methods: &["GET"],
        domain: BusinessDomain::Git,
        // 票 04 contract：工作区状态面同上退役
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    BusinessRoute {
        path: "/api/git/checkout",
        plugin_id: SESSION_PLUGIN,
        endpoint: "git/checkout",
        methods: &["POST"],
        domain: BusinessDomain::Git,
        // 票 04 contract：checkout 编排（含分支名白名单）随插件走
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Authenticated,
    },
    // ==================== 票 07：认证链（公开路由——JWT 之前的入口） ====================
    BusinessRoute {
        path: "/api/auth/pairing",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/pairing",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/verify",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/verify",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/qr-connect",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/qr-connect",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/reauth",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/reauth",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/biometric-challenge",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/biometric-challenge",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/biometric-verify",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/biometric-verify",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
    BusinessRoute {
        path: "/api/auth/biometric-bind",
        plugin_id: SESSION_PLUGIN,
        endpoint: "auth/biometric-bind",
        methods: &["POST"],
        domain: BusinessDomain::Auth,
        fallback: FallbackPolicy::PluginRequired,
        auth: RouteAuth::Public,
    },
];

/// 按「路径 + 方法」查别名条目（纯函数）
///
/// 路径**全等**匹配，绝不做前缀匹配（`/api/configsx` 不得命中 `/api/configs`）。
/// 路径命中而方法不命中时不命中别名：那是调用方打错了方法，属宿主路由面的 405 语义，
/// 交给旧路由表原样应答。
pub fn route_for_request(path: &str, method: &str) -> Option<&'static BusinessRoute> {
    BUSINESS_ROUTES
        .iter()
        .find(|r| r.path == path && r.methods.contains(&method))
}

/// 网关对一次业务请求的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayDecision<'a> {
    /// 目标插件在位且已声明该端点、认证前置齐备 → 切插件路径
    Forward(&'a BusinessRoute),
    /// 未激活 / 未声明 → 原样落宿主旧实现
    HostFallback,
    /// 宿主实现已退役且插件不可用 → 明确报错（不给假数据）
    PluginRequired(&'a BusinessRoute),
    /// 端点要求已验签（宿主条目或插件声明任一）而本次请求没有 → 401
    ///
    /// 单独一档而不是回 [`GatewayDecision::PluginRequired`]：那会把「你没登录」报成
    /// 「插件未激活」，指错方向。
    AuthRequired(&'a BusinessRoute),
}

/// 降级判定（纯函数，双轨语义的单一事实源）
///
/// - `verified`：宿主 JWT 中间件是否已验签并注入 claims（[`caller_identity`] 的 device 档）。
///   **需要验签时绝不转发**——这道守卫是网关自身的性质，不依赖中间件注册顺序
///   （顺序错乱时最多多拒一次，不会漏验签）。
/// - `declared`：属主插件 manifest 声明的端点全路径清单。**空清单按「未声明」处理**——与
///   `/api/plugin/*` 那条路由同一判据（票 08 起两处都不再放行未声明）：网关交出去的
///   是**宿主自有业务**，插件没显式声明这个业务端点就绝不交出去。
/// - `endpoint_auth`：属主插件对该端点声明的认证档位（票 08）。与别名条目自带的
///   [`RouteAuth`] **取较严者**——`Public` 条目不因插件声明 `jwt` 而被放宽，
///   `Authenticated` 条目也不因插件声明 `none` 而放松：两个声明面谁都不能单方面开门。
pub fn decide<'a>(
    route: &'a BusinessRoute,
    verified: bool,
    activated: bool,
    declared: &[String],
    endpoint_auth: EndpointAuth,
) -> GatewayDecision<'a> {
    let target = route.declared_path();
    if !(activated && declared.contains(&target)) {
        return match route.fallback {
            FallbackPolicy::HostImplementation => GatewayDecision::HostFallback,
            FallbackPolicy::PluginRequired => GatewayDecision::PluginRequired(route),
        };
    }
    let requires_verification = route.auth == RouteAuth::Authenticated || endpoint_auth == EndpointAuth::Jwt;
    if requires_verification && !verified {
        return GatewayDecision::AuthRequired(route);
    }
    GatewayDecision::Forward(route)
}

/// 「插件未激活」响应：与 `/api/plugin/*` 路由的未激活响应同口径（HTTP 200 + 业务码 1007）
fn plugin_unavailable_response(route: &BusinessRoute) -> HttpResponse {
    tracing::warn!(
        plugin_id = %route.plugin_id,
        path = %route.path,
        domain = route.domain.as_str(),
        "业务端点不可用（宿主实现已退役且目标插件未激活）"
    );
    HttpResponse::Ok().json(ApiResponse::<()>::error(
        CODE_PLUGIN_AUTH_FAILED,
        &format!("Plugin {} is not activated", route.plugin_id),
    ))
}

/// 转发失败时的显式错误响应（与宿主业务端点的错误口径一致：HTTP 200 + 业务码）
fn bad_request(message: &str) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::<()>::error(CODE_INVALID_REQUEST, message))
}

/// 「调用方未验签」响应：HTTP 401 + 业务码 1007，与 JWT 中间件的 401 同一码
///
/// 这条分支生产链路正常走不到（中间件在网关之外已把无 JWT 的业务请求 401），
/// 它是「中间件顺序被改错 / 插件把 `Authenticated` 条目的端点声明成 `none` 之外的档」
/// 时的兜底，所以回 401 而不是回「插件未激活」。
fn unauthorized_response(route: &BusinessRoute) -> HttpResponse {
    HttpResponse::Unauthorized().json(ApiResponse::<()>::error(
        CODE_PLUGIN_AUTH_FAILED,
        &format!("Authentication required for {}", route.path),
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
///
/// 判据取「字节是不是 JSON」而非 content-type：比宿主宽松一档，但绝不丢数据；移动端
/// 这些端点一律 `application/json`，差异面只在畸形请求上。
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

/// 网关中间件：业务 URL 别名 → 插件，其余请求原样放行
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
    let Some(route) = route_for_request(&path, &method) else {
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    };

    // 调用方身份（票 08）：device 档 = 宿主 JWT 中间件已验签并注入 claims。
    // 环回 / 匿名两档只在插件把该端点显式声明成 `auth: "none"` 时才可能走到转发。
    let (caller, device) = caller_identity(req.request());
    let verified = matches!(caller, HttpCaller::Device);
    // 无 AppContext（无头 / 库级测试 / 初始化中间态）= 插件面不可判定 → 交回链条，
    // 绝不凭别名表就把请求递给不存在的插件。
    let decision = match AppContext::try_global() {
        None => GatewayDecision::HostFallback,
        Some(ctx) => {
            let host = ctx.plugin_host();
            let activated = host.is_activated(route.plugin_id).await;
            let registry = host.registry();
            let declared = registry.list_http_endpoint_paths(route.plugin_id).await;
            // 档位按属主声明查；查不到条目（未声明）时给最严档——那种情形下面
            // `declared` 判定先落，档位取值不影响结果，只是不许有「查不到=免凭证」的形状
            let endpoint_auth = registry
                .find_http_endpoint(&route.declared_path())
                .await
                .map(|e| e.auth)
                .unwrap_or(EndpointAuth::Jwt);
            decide(route, verified, activated, &declared, endpoint_auth)
        }
    };
    tracing::debug!(
        path = %path,
        plugin_id = %route.plugin_id,
        domain = route.domain.as_str(),
        caller = %caller.as_str(),
        forward = matches!(decision, GatewayDecision::Forward(_)),
        "HTTP 协议网关判定"
    );

    match decision {
        GatewayDecision::HostFallback => next.call(req).await.map(|res| res.map_into_boxed_body()),
        GatewayDecision::PluginRequired(_) => {
            let (http_req, _payload) = req.into_parts();
            Ok(ServiceResponse::new(http_req, plugin_unavailable_response(route)))
        }
        GatewayDecision::AuthRequired(_) => {
            tracing::warn!(
                path = %path,
                plugin_id = %route.plugin_id,
                caller = %caller.as_str(),
                "业务端点要求已验签调用方，本次请求未通过宿主 JWT 验签"
            );
            let (http_req, _payload) = req.into_parts();
            Ok(ServiceResponse::new(http_req, unauthorized_response(route)))
        }
        GatewayDecision::Forward(_) => {
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
            let request = PluginHttpRequest {
                endpoint_path: route.endpoint,
                method: &method,
                headers,
                body,
                query,
                caller,
                device,
            };
            let resp = forward_to_plugin(route.plugin_id, &request).await;
            Ok(ServiceResponse::new(http_req, resp))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::controllers::plugin_controller::build_plugin_http_args;
    use crate::server::middleware::jwt_auth::jwt_gateway;
    use crate::utils::auth::jwt::JwtService;

    fn route(path: &str, method: &str) -> &'static BusinessRoute {
        route_for_request(path, method).expect("已登记的别名条目")
    }

    // ==================== 路由表 ====================

    /// 别名表 golden 清单：路径 / 归属插件 / 端点段 / 方法 / 业务域逐项钉死
    ///
    /// 这张表是「宿主 HTTP 面 = 引擎端点 + 网关」这条审计线上唯一的业务清单，条目增减
    /// 必须交代为什么（票 01 的初始集合 = 领域审计判为业务滞留的四域全部端点）。
    #[test]
    fn route_table_golden() {
        let rows: Vec<(&str, &str, &str, &[&str], &str)> = BUSINESS_ROUTES
            .iter()
            .map(|r| (r.path, r.plugin_id, r.endpoint, r.methods, r.domain.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "/api/configs",
                    SESSION_PLUGIN,
                    "configs",
                    &["GET"][..],
                    "session-config"
                ),
                (
                    "/api/quick-actions",
                    SESSION_PLUGIN,
                    "quick-actions",
                    &["GET"][..],
                    "quick-action"
                ),
                (
                    "/api/file-tree",
                    SESSION_PLUGIN,
                    "file-tree",
                    &["POST"][..],
                    "file-browse"
                ),
                (
                    "/api/file-tree-children",
                    SESSION_PLUGIN,
                    "file-tree-children",
                    &["GET"][..],
                    "file-browse"
                ),
                (
                    "/api/file-content",
                    SESSION_PLUGIN,
                    "file-content",
                    &["POST"][..],
                    "file-browse"
                ),
                (
                    "/api/diff-tree",
                    SESSION_PLUGIN,
                    "diff-tree",
                    &["POST"][..],
                    "file-browse"
                ),
                (
                    "/api/file-diff",
                    SESSION_PLUGIN,
                    "file-diff",
                    &["POST"][..],
                    "file-browse"
                ),
                ("/api/git/branches", SESSION_PLUGIN, "git/branches", &["GET"][..], "git"),
                ("/api/git/status", SESSION_PLUGIN, "git/status", &["GET"][..], "git"),
                (
                    "/api/git/checkout",
                    SESSION_PLUGIN,
                    "git/checkout",
                    &["POST"][..],
                    "git"
                ),
                (
                    "/api/auth/pairing",
                    SESSION_PLUGIN,
                    "auth/pairing",
                    &["POST"][..],
                    "auth"
                ),
                ("/api/auth/verify", SESSION_PLUGIN, "auth/verify", &["POST"][..], "auth"),
                (
                    "/api/auth/qr-connect",
                    SESSION_PLUGIN,
                    "auth/qr-connect",
                    &["POST"][..],
                    "auth"
                ),
                ("/api/auth/reauth", SESSION_PLUGIN, "auth/reauth", &["POST"][..], "auth"),
                (
                    "/api/auth/biometric-challenge",
                    SESSION_PLUGIN,
                    "auth/biometric-challenge",
                    &["POST"][..],
                    "auth"
                ),
                (
                    "/api/auth/biometric-verify",
                    SESSION_PLUGIN,
                    "auth/biometric-verify",
                    &["POST"][..],
                    "auth"
                ),
                (
                    "/api/auth/biometric-bind",
                    SESSION_PLUGIN,
                    "auth/biometric-bind",
                    &["POST"][..],
                    "auth"
                ),
            ]
        );
    }

    /// 表自身一致性：路径唯一、方法非空、端点段与路径的对应关系成立
    #[test]
    fn route_table_is_self_consistent() {
        let mut seen = std::collections::HashSet::new();
        for r in BUSINESS_ROUTES {
            assert!(seen.insert(r.path), "业务路径重复登记: {}", r.path);
            assert!(!r.methods.is_empty(), "{} 必须声明方法", r.path);
            assert!(
                r.path.starts_with("/api/"),
                "业务端点必须挂在 /api 下（JWT 中间件作用域）: {}",
                r.path
            );
            assert_eq!(
                r.endpoint,
                r.path.strip_prefix("/api/").expect("/api/ 前缀"),
                "端点段 = 路径去 /api/ 前缀是约定，网关条目不得自造端点名, got: {}",
                r.endpoint
            );
            assert_eq!(r.declared_path(), format!("/api/plugin/{}/{}", r.plugin_id, r.endpoint));
        }
    }

    /// 引擎端点绝不被收编（spec 决策 2：`/api/sessions` 保持宿主壳形态）
    #[test]
    fn engine_endpoints_are_never_aliased() {
        // /api/auth/* 自票 07 起随用户裁定进业务别名表（认证编排归插件）：
        // 免验签的「公开路由」语义由 `RouteAuth::Public` 显式声明，不再是引擎面
        for prefix in ["/api/sessions", "/api/health", "/api/plugin/", "/static/"] {
            for r in BUSINESS_ROUTES {
                assert!(!r.path.starts_with(prefix), "引擎端点不得进业务别名表, got: {}", r.path);
            }
        }
    }

    /// 别名的归属插件必须是「无业务内核」路线上的业务插件，且与业务域一一对应
    ///
    /// 别名的意义是把业务交给插件；`plugin_id` 写成宿主自身或写错命名空间，就是
    /// 把业务路由指向不存在/不该存在的实现（用户故事 12：一条别名 = 一个归属声明）。
    #[test]
    fn every_alias_declares_a_business_plugin_owner() {
        const BUSINESS_PLUGINS: &[&str] = &["com.bedcode.terminal-session", "com.bedcode.file-transfer"];
        for r in BUSINESS_ROUTES {
            assert!(
                BUSINESS_PLUGINS.contains(&r.plugin_id),
                "{} 的归属插件必须是业务插件, got: {}",
                r.path,
                r.plugin_id
            );
            let domain_plugin = match r.domain {
                BusinessDomain::SessionConfig
                | BusinessDomain::QuickAction
                | BusinessDomain::FileBrowse
                | BusinessDomain::Git
                | BusinessDomain::Auth => SESSION_PLUGIN,
            };
            assert_eq!(
                r.plugin_id,
                domain_plugin,
                "{} 的业务域 {} 与归属插件不符",
                r.path,
                r.domain.as_str()
            );
        }
    }

    // ==================== 别名锁自校准前置 ====================

    /// 两条别名锁共同的扫描目标：宿主路由装配源码
    ///
    /// 单一取源点：`server/` 三层化把路由搬走后（票 04/05/06/07），只需重指这一行
    /// `include_str!`，下面的前置会当场验证新目标仍是「活的宿主路由面」。
    const APP_RS: &str = include_str!("app.rs");

    /// 失败消息里显示的目标名（`include_str!` 的相对路径无法自报，故单独记一份）
    const APP_RS_LABEL: &str = "server/app.rs";

    /// 扫描目标 `configure_routes` 函数体的路由标识符基线数
    ///
    /// 现值 14 = 12 条路径字面量 + `API_HEALTH_PATH` + `WS_EVENT_PATH` 两个常量标识符。
    /// **常量必须纳入计数**：只匹配 `"/…"` 字面量的话，`/api/health` 整行删掉照样绿。
    ///
    /// HTTP/WS 路由面拆分（票 07）后本锁的扫描目标收窄为 HTTP 侧，基线随之改钉 **11**——
    /// 判据与新旧清单见票 07。这是实测钉出来的数，禁止当魔法数放宽成「>= 1」。
    const HOST_ROUTE_IDENTIFIERS_BASELINE: usize = 14;

    /// 取 `configure_routes` 函数体：签名行之后到首个顶格 `}`
    ///
    /// 计数必须限定在函数体：整文件会把 `use` 行与文档注释里的路径字面量算进来，
    /// 基线被撑虚高之后，「指错文件即红」的判别力就没了。
    fn configure_routes_body(src: &str) -> Option<&str> {
        let head = src.find("fn configure_routes")?;
        let open = src[head..].find('{').map(|i| head + i + 1)?;
        let close = src[open..].find("\n}").map(|i| open + i)?;
        Some(&src[open..close])
    }

    /// 数函数体里的路由标识符，与 spec §7 的门禁命令
    /// `grep -oE '"/[^"]*"|API_HEALTH_PATH|WS_EVENT_PATH'` 同形
    fn route_identifier_count(body: &str) -> usize {
        let mut count = body.matches("API_HEALTH_PATH").count() + body.matches("WS_EVENT_PATH").count();
        let mut rest = body;
        while let Some(open) = rest.find("\"/") {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            count += 1;
            rest = &after[close + 1..];
        }
        count
    }

    /// 取扫描目标里的 `configure_routes` 函数体；取不到即目标已不含宿主路由装配
    fn configure_routes_body_or_panic<'a>(label: &str, src: &'a str) -> &'a str {
        configure_routes_body(src)
            .unwrap_or_else(|| panic!("锁已空转：扫描目标 {label} 里没有 configure_routes 函数体"))
    }

    /// 前置（第一条锁）：扫描目标必须命中基线数量的路由标识符
    ///
    /// 别名表现有 17 条**全部** `PluginRequired`，而那条锁对每条断言的是
    /// `!APP_RS.contains(path)`——只做 17 次否定断言。`include_str!` 指到一个「存在但没有
    /// 路由字面量」的文件时，17 条全部恒真、全绿；路径**不存在**编译器会抓，指错文件只有
    /// 这条前置能抓。
    fn assert_host_route_surface_is_live(label: &str, src: &str) {
        let hit = route_identifier_count(configure_routes_body_or_panic(label, src));
        assert!(
            hit >= HOST_ROUTE_IDENTIFIERS_BASELINE,
            "锁已空转：扫描目标 {label} 的 configure_routes 只命中 {hit} 条路由标识符，\
             低于基线 {HOST_ROUTE_IDENTIFIERS_BASELINE}（12 条路径字面量 + API_HEALTH_PATH + WS_EVENT_PATH）",
        );
    }

    /// 前置（第二条锁）：扫描目标非空，且确实执行「挂路由」这个动作
    ///
    /// 那条锁扫的 `git_controller::` / `auth_controller::` 两个 handler 名已随业务下沉退役，
    /// 零命中是**合法现状**、不能拿它当判据；能判的只有「被扫文件仍是宿主路由装配处」。
    fn assert_host_route_surface_not_empty(label: &str, src: &str) {
        let body = configure_routes_body_or_panic(label, src);
        assert!(
            !body.trim().is_empty() && body.contains(".route("),
            "锁已空转：扫描目标 {label} 的 configure_routes 函数体内没有任何 .route( 绑定",
        );
    }

    /// 计数式自身：路径字面量与两个路由常量都算一项，非路径文本不算
    ///
    /// 与 spec §7 的门禁命令同形（含其「注释里的引号路径也算一项」的口径——只会虚高不会漏，
    /// 放宽方向与门禁一致）。这条锁的全部判别力来自「常量纳入计数」，
    /// 故把该行为永久钉成用例，而不只靠一次变异验证。
    #[test]
    fn calibration_counts_path_literals_and_route_constants() {
        let live = "\
    cfg.route(\"/sessions\", web::get().to(h));
    cfg.route(\"/static/terminal-bg\", web::get().to(h));
    cfg.route(WS_EVENT_PATH, web::get().to(event_ws));
    cfg.route(API_HEALTH_PATH, web::get().to(health_check));
";
        assert_eq!(route_identifier_count(live), 4);
        assert_eq!(route_identifier_count("use crate::server::controllers;"), 0);
    }

    /// 目标取函数体而不是整文件：`use` 行与函数体外的常量不得混进计数
    #[test]
    fn calibration_scans_only_the_configure_routes_body() {
        let src = "\
use crate::server::app::{API_HEALTH_PATH, WS_EVENT_PATH};
/// 文档注释里的 \"/api/doc-comment\" 不是路由
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route(\"/sessions\", web::get().to(h));
}
const ELSEWHERE: &str = \"/api/elsewhere\";
";
        let body = configure_routes_body(src).expect("函数体");
        assert_eq!(
            route_identifier_count(body),
            1,
            "整文件计数会把 use 行的两个常量、文档注释与函数体后的字面量都算进来"
        );
        assert!(
            body.contains("/sessions")
                && !body.contains("API_HEALTH_PATH")
                && !body.contains("doc-comment")
                && !body.contains("elsewhere"),
            "计数范围越界，实际取到: {body}"
        );
        assert!(configure_routes_body("pub fn nothing() {}").is_none());
    }

    /// 前置必须咬得动：死目标（有 `configure_routes` 但函数体是空的）上跑第一条锁的前置 → 红
    #[test]
    #[should_panic(expected = "锁已空转")]
    fn calibration_panics_when_route_surface_is_dead() {
        assert_host_route_surface_is_live(
            "server/_dead_target.rs",
            "pub fn configure_routes(cfg: &mut web::ServiceConfig) {}\n",
        );
    }

    /// 前置必须咬得动：死目标上跑第二条锁的前置 → 红
    #[test]
    #[should_panic(expected = "锁已空转")]
    fn calibration_panics_when_route_surface_mounts_nothing() {
        assert_host_route_surface_not_empty(
            "server/_dead_target.rs",
            "pub fn configure_routes(cfg: &mut web::ServiceConfig) {\n    // 空\n}\n",
        );
    }

    /// 别名表与宿主路由面同源：每条业务路径都必须仍注册在 `app.rs` 的 `/api` scope 里
    ///
    /// 网关是「加一层」而不是「换路由」——降级分支依赖旧路由原样存在。这条静态扫描把依赖
    /// 关系钉死：删宿主路由而没同时删别名表条目，立刻红。
    ///
    /// 双轨期（HostImplementation）条目必须有宿主路由——降级分支才有落点；
    /// contract（PluginRequired）条目必须**没有**宿主路由——真源已在插件，
    /// 宿主再挂同路径 handler 就是死业务面（票 02/03/04 contract 的反向守护）。
    #[test]
    fn every_alias_still_has_a_host_route() {
        assert_host_route_surface_is_live(APP_RS_LABEL, APP_RS);
        for r in BUSINESS_ROUTES {
            let literal = format!("\"{}\"", r.path.strip_prefix("/api").expect("/api/ 前缀"));
            match r.fallback {
                FallbackPolicy::HostImplementation => {
                    assert!(
                        APP_RS.contains(&literal),
                        "{} 的宿主路由（{}）必须存在，降级分支才有落点",
                        r.path,
                        literal
                    );
                }
                FallbackPolicy::PluginRequired => {
                    assert!(
                        !APP_RS.contains(&literal),
                        "{} 已 contract（PluginRequired），宿主路由必须注销（真源在插件，宿主不该再挂同路径 handler）",
                        r.path
                    );
                }
            }
        }
    }

    /// 业务 controller 的 handler 只能挂在网关已收编的路径上（防「宿主长回业务路由」）
    ///
    /// 扫描口径选「handler 绑定处」而不是「路由字面量全集」：别名表的职责就是把业务出口
    /// 收在一处，只要业务 handler 没被挂上未登记的 path，宿主业务面就不会重新长出来。
    /// 新增业务端点的正路只有两条：下沉插件 + 上表，或论证它确属引擎协议面（不碰业务 handler）。
    #[test]
    fn business_handlers_are_only_mounted_on_aliased_paths() {
        assert_host_route_surface_not_empty(APP_RS_LABEL, APP_RS);
        const BUSINESS_HANDLERS: &[&str] = &["git_controller::", "auth_controller::"];
        let mut offenders = Vec::new();
        for marker in BUSINESS_HANDLERS {
            for (idx, _) in find_all(APP_RS, marker) {
                // handler 名前最近的字符串字面量即其挂载路径（多行 .route( 排版同样成立）
                let Some(path) = nearest_string_literal(&APP_RS[..idx]) else {
                    offenders.push(format!("{marker}@{idx} 找不到路径字面量"));
                    continue;
                };
                let full = if path.starts_with("/api/") {
                    path
                } else {
                    format!("/api{path}")
                };
                if !BUSINESS_ROUTES.iter().any(|r| r.path == full) {
                    offenders.push(full);
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "业务 handler 挂到了别名表之外的路径（应先下沉插件再上表）: {offenders:?}"
        );
    }

    fn find_all(haystack: &str, needle: &str) -> Vec<(usize, String)> {
        haystack
            .match_indices(needle)
            .map(|(i, s)| (i, s.to_string()))
            .collect()
    }

    /// 取 `src` 末尾往前最近的一个字符串字面量内容（跳过注释行，避免文档里的示例路径误命中）
    fn nearest_string_literal(src: &str) -> Option<String> {
        let close = src.rfind('"')?;
        let open = src[..close].rfind('"')?;
        let literal = &src[open + 1..close];
        // 字面量必须与 marker 同行/同段（跨行的注释文字不算路径）
        if literal.contains('\n') || !literal.starts_with('/') {
            return None;
        }
        Some(literal.to_string())
    }

    // ==================== 判定 ====================

    /// 路径全等匹配：前缀相似、多一段、方法不符都不得命中
    #[test]
    fn route_matching_is_exact_on_path_and_method() {
        assert!(route_for_request("/api/configs", "GET").is_some());
        assert!(route_for_request("/api/configsx", "GET").is_none());
        assert!(route_for_request("/api/configs/", "GET").is_none());
        assert!(route_for_request("/api/configs/1", "GET").is_none());
        assert!(route_for_request("/api/git/branches", "POST").is_none());
        assert!(route_for_request("/api/git/checkout", "GET").is_none());
        assert!(route_for_request("/api/file-tree", "GET").is_none());
        assert!(route_for_request("", "GET").is_none());
    }

    /// 双轨判定：只有「认证前置齐备 + 在位 + 已声明」才切插件
    ///
    /// 判定语义与具体条目的 contract 状态无关，故用**合成的** HostImplementation
    /// 条目验证降级分支（表内条目已全部随票 02/03/04 contract 翻 PluginRequired，
    /// 由 [`retired_host_implementation_reports_plugin_required`] 覆盖该形态）。
    #[test]
    fn decide_forwards_only_when_verified_activated_and_declared() {
        const DUAL_TRACK: BusinessRoute = BusinessRoute {
            path: "/api/git/branches",
            plugin_id: SESSION_PLUGIN,
            endpoint: "git/branches",
            methods: &["GET"],
            domain: BusinessDomain::Git,
            fallback: FallbackPolicy::HostImplementation,
            auth: RouteAuth::Authenticated,
        };
        let r = &DUAL_TRACK;
        let declared = vec![r.declared_path()];

        assert_eq!(
            decide(r, true, true, &declared, EndpointAuth::Jwt),
            GatewayDecision::Forward(r)
        );
        // 未验签 → 绝不转发（即使命令齐备）：验签执行点在宿主，网关不替插件放行陌生请求
        assert_eq!(
            decide(r, false, true, &declared, EndpointAuth::Jwt),
            GatewayDecision::AuthRequired(r),
            "未验签要报「要认证」，不得静默回落宿主实现"
        );
        // 未激活 → 降级宿主
        assert_eq!(
            decide(r, true, false, &declared, EndpointAuth::Jwt),
            GatewayDecision::HostFallback
        );
        // 激活但未声明该业务端点 → 仍走宿主。票 01 落地时 session 插件正落在这一格
        // （已激活、只声明了任务域端点），所以业务端点不会提前切过去。
        assert_eq!(
            decide(r, true, true, &[], EndpointAuth::Jwt),
            GatewayDecision::HostFallback
        );
        assert_eq!(
            decide(
                r,
                true,
                true,
                &["/api/plugin/com.bedcode.terminal-session/task-status".to_string()],
                EndpointAuth::Jwt
            ),
            GatewayDecision::HostFallback
        );
        // 声明面禁止前缀匹配：更深的子路径不得借父声明命中
        assert_eq!(
            decide(
                r,
                true,
                true,
                &[format!("{}/extra", r.declared_path())],
                EndpointAuth::Jwt
            ),
            GatewayDecision::HostFallback
        );
    }

    /// 票 08：宿主条目档位与插件声明档位**取较严者**，两向都不许单方面开门
    #[test]
    fn decide_takes_the_stricter_of_route_and_endpoint_auth() {
        // Authenticated 条目 + 插件声明 none：仍要求验签（插件不能放开宿主要求）
        const GUARDED: BusinessRoute = BusinessRoute {
            path: "/api/configs",
            plugin_id: SESSION_PLUGIN,
            endpoint: "configs",
            methods: &["GET"],
            domain: BusinessDomain::SessionConfig,
            fallback: FallbackPolicy::PluginRequired,
            auth: RouteAuth::Authenticated,
        };
        // Public 条目（/api/auth/* 在 JWT 之前）+ 插件声明 jwt：仍要求验签
        const PUBLIC: BusinessRoute = BusinessRoute {
            path: "/api/auth/pairing",
            plugin_id: SESSION_PLUGIN,
            endpoint: "auth/pairing",
            methods: &["POST"],
            domain: BusinessDomain::Auth,
            fallback: FallbackPolicy::PluginRequired,
            auth: RouteAuth::Public,
        };
        let guarded_declared = vec![GUARDED.declared_path()];
        let public_declared = vec![PUBLIC.declared_path()];

        assert_eq!(
            decide(&GUARDED, false, true, &guarded_declared, EndpointAuth::None),
            GatewayDecision::AuthRequired(&GUARDED),
            "插件声明 none 不得放开宿主 Authenticated 条目的验签前置"
        );
        assert_eq!(
            decide(&PUBLIC, false, true, &public_declared, EndpointAuth::Jwt),
            GatewayDecision::AuthRequired(&PUBLIC),
            "公开别名遇到插件的 jwt 声明必须改判为要验签"
        );
        // 两档都为 none 时才免验签转发（配对入口的真实形态）
        assert_eq!(
            decide(&PUBLIC, false, true, &public_declared, EndpointAuth::None),
            GatewayDecision::Forward(&PUBLIC)
        );
        // 未声明条目优先按 fallback 处置，档位判定不参与（否则未声明变成 401 而非「未激活」）
        assert_eq!(
            decide(&PUBLIC, false, true, &[], EndpointAuth::Jwt),
            GatewayDecision::PluginRequired(&PUBLIC)
        );
    }

    /// contract 阶段（宿主实现退役）后插件不可用必须明确报错，不给假数据
    #[test]
    fn retired_host_implementation_reports_plugin_required() {
        const RETIRED: BusinessRoute = BusinessRoute {
            path: "/api/configs",
            plugin_id: SESSION_PLUGIN,
            endpoint: "configs",
            methods: &["GET"],
            domain: BusinessDomain::SessionConfig,
            fallback: FallbackPolicy::PluginRequired,
            auth: RouteAuth::Authenticated,
        };
        assert_eq!(
            decide(&RETIRED, true, false, &[], EndpointAuth::Jwt),
            GatewayDecision::PluginRequired(&RETIRED)
        );
        assert_eq!(
            decide(&RETIRED, true, true, &[], EndpointAuth::Jwt),
            GatewayDecision::PluginRequired(&RETIRED),
            "在位但未声明同样算不可用：真源已不在宿主"
        );
        // 未验签：条目已声明也要认证，报 401 而不是「插件未激活」（方向不能指错）。
        // 生产链路里这一格由 JWT 中间件先 401（见 unverified_requests_never_reach_gateway），
        // 本条锁的是中间件顺序错乱时网关自己的兜底。
        assert_eq!(
            decide(&RETIRED, false, true, &[RETIRED.declared_path()], EndpointAuth::Jwt),
            GatewayDecision::AuthRequired(&RETIRED)
        );
        // 在位且已声明后照旧切换：翻策略只改「不可用时怎么答」，不改切换条件
        let declared = vec![RETIRED.declared_path()];
        assert_eq!(
            decide(&RETIRED, true, true, &declared, EndpointAuth::Jwt),
            GatewayDecision::Forward(&RETIRED)
        );
    }

    /// 「插件未激活」响应形状：HTTP 200 + `{code:1007,message}`，与既有未激活响应同口径
    #[actix_web::test]
    async fn plugin_required_response_keeps_existing_error_shape() {
        let r = route("/api/configs", "GET");
        let resp = plugin_unavailable_response(r);
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
                "message": "Plugin com.bedcode.terminal-session is not activated",
            })
        );
    }

    /// 票 08：「要验签而未验签」的响应形状是 401 + 1007 + 点名对外路径，
    /// 与「插件未激活」（200 + 1007）分得开——方向不能指错
    #[actix_web::test]
    async fn auth_required_response_says_authentication_not_activation() {
        let r = route("/api/configs", "GET");
        let resp = unauthorized_response(r);
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

    // ==================== 转发入参 ====================

    /// 转发入参形状：与 `/api/plugin/*` 共用同一构造器，故两条路径不可能各答一版
    ///
    /// `device` 只在已验签时出现；未验签的调用方不得凭空多出设备上下文，但仍必须带
    /// `caller`（票 08）——插件据此区分环回 hook 与局域网匿名。
    #[test]
    fn forwarded_request_shape_locks_device_and_headers() {
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
                "caller": "device",
                "device": { "deviceId": "device-1" },
            })
        );
        let no_device = build_plugin_http_args(&PluginHttpRequest {
            endpoint_path: "configs",
            method: "GET",
            headers: serde_json::Map::new(),
            body: Value::Null,
            query: Value::Object(serde_json::Map::new()),
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
    ///
    /// 边界必须分清：把畸形 body 当空 body 转给插件，等于把「客户端发了错东西」
    /// 伪装成「客户端什么都没发」。
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
    /// 链路顺序，不是测试自己搭的近似物。哨兵 handler 复刻 `/api/configs` 今天的宿主回包
    /// 形状，用来断言降级分支确实原样落到宿主。
    ///
    /// `Scope::wrap_fn` 是**后注册者在外**（见 [`scope_wrap_registration_puts_jwt_outermost`]），
    /// 所以网关在前、验签在后——与 `app.rs` 的挂载顺序保持一致。
    fn test_scope() -> impl actix_web::dev::HttpServiceFactory + 'static {
        use actix_web::middleware::from_fn;
        web::scope("/api")
            .wrap(from_fn(business_gateway))
            .wrap(from_fn(jwt_gateway))
            .route("/configs", web::get().to(host_sentinel))
            .route("/git/branches", web::get().to(host_sentinel))
    }

    /// `Scope::wrap` 的注册顺序语义（app.rs 挂载顺序的依据，第三方 API 的承重假设）
    ///
    /// 生产接线依赖「最后注册 = 最外层 = 先执行」。这条不是给自己补覆盖率，而是把 actix
    /// 的这条语义钉住：哪天升级把它翻过来，app.rs 的「先验签后网关」会静默失效，本用例先红。
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
    ///
    /// 这条同时锁中间件顺序：若网关挂在 JWT 之前，本用例拿到的会是宿主哨兵响应而不是 401。
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

    /// 降级分支：验签通过 + 无 AppContext（插件面不可判定）→ 宿主旧实现原样应答
    #[actix_web::test]
    async fn verified_request_falls_through_to_host_when_plugin_unavailable() {
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
            "降级分支的响应必须与今天逐字段一致"
        );
    }

    /// 方法不匹配 / 未登记路径：网关不介入，状态码与「同一 scope 不挂网关」完全一致
    ///
    /// 用对照组而不是写死 405：`Scope` 挂了中间件之后，actix 对「路径在、方法不在」的
    /// 应答并不是教科书上的 405（今天宿主面就是这一格行为）。对照组把标准钉在「与不挂
    /// 网关时相同」，比钉一个记错的数字更诚实。
    #[actix_web::test]
    async fn engine_and_wrong_method_requests_are_not_touched() {
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

    /// 降级分支不消费 payload：宿主 handler 仍能读到完整 JSON body
    ///
    /// 网关若在判定阶段读body，宿主 handler 就会拿到空载荷并 400——那是最隐蔽的
    /// 「实现搬走、行为变味」形态，故单列一条。
    #[actix_web::test]
    async fn fallback_branch_leaves_the_payload_intact() {
        async fn echo(body: web::Json<Value>) -> HttpResponse {
            HttpResponse::Ok().json(serde_json::json!({ "echo": body.0 }))
        }
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
    }

    // ==================== 形状契约锁（网关切换前后逐字节一致） ====================

    /// 业务端点响应形状 golden：宿主侧当前输出即契约，插件面必须逐字段复刻
    ///
    /// 锁的是「JSON 形状」而不是「实现在哪」：DTO 的 serde 表示就是移动端看到的字节。
    /// 票 02/03/04 的插件端点回包必须与本用例逐字段相同（含可选字段的缺席形态）。
    #[test]
    fn business_endpoint_shapes_are_locked_for_dual_track() {
        use crate::server::dtos::config_dto::{
            ConfigItem, ConfigListResponseData, QuickActionItem, QuickActionListResponseData,
        };
        use crate::server::dtos::file_dto::{
            FileContentResponseData, FileDiffLine, FileDiffResponseData, FileTreeNode, FileTreeResponseData,
        };
        use crate::server::dtos::git_dto::{GitBranchesResponseData, GitCheckoutResponseData, GitStatusResponseData};

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
        // wslDistro 为 None 时是 **显式 null**（ConfigItem 没有 skip_serializing_if）——
        // 移动端 `?? 空串` 依赖这一格，插件面若改成省略字段就是形状漂移
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

        // 错误信封：这些端点今天全部是 HTTP 200 + 业务码。插件面必须同口径——
        // 直接套 SDK 的 http_response::error(404, …) 会把 HTTP 状态码一起改掉，即形状漂移。
        assert_shape(
            ApiResponse::<()>::error(404, "Session not found"),
            serde_json::json!({ "code": 404, "message": "Session not found" }),
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
