//! HTTP 网关中间件 — JWT 认证
//!
//! 挂载在 /api scope 上，统一拦截认证：
//! - /api/auth/* — 放行（公开路由，配对/登录）
//! - /api/plugin/* — 有 JWT 就验签并注入 claims；无 JWT 一律**放行到 handler**，
//!   由 handler 按**端点声明的档位**决定要不要真的到达插件（票 08）：manifest
//!   `contributes.httpEndpoints` 未声明 `auth` 即最严档 `jwt`（无凭证 → 401），
//!   免凭证必须逐条显式声明 `auth: "none"`（环回 hook 脚本无法持有 JWT，正是这一格）。
//!   本中间件不做端点级判定，因为它还不知道属主插件是谁（旧前缀兜底在 handler 里解）。
//! - 其余 /api/* — 必须通过 JWT 校验
//!
//! 信任边界：服务监听 BIND_ADDRESS（0.0.0.0）。票 08 前「局域网内任意设备可无凭证
//! 调用已激活插件的 HTTP 端点（含写操作）」；现在这一面由插件的逐端点声明承担——
//! 未显式声明 `auth: "none"` 的端点要求验签，且插件拿得到宿主判定的调用方身份
//! （`caller` = device / localhost / anonymous）用于自行收紧。
//!
//! 校验通过后将 JwtClaims 注入 request extensions，handler 通过 get_claims_from_request 提取。

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{Error, HttpMessage, HttpResponse};
use serde_json::json;

use crate::utils::auth::jwt::{JwtClaims, JwtService};

/// 从 request extensions 提取 JWT claims
///
/// 供 handler 使用，中间件校验通过后 claims 已注入
pub fn get_claims_from_request(req: &actix_web::HttpRequest) -> Option<JwtClaims> {
    req.extensions().get::<JwtClaims>().cloned()
}

/// 从 Authorization header 提取并验证 JWT
///
/// 验签执行留宿主中间件（密码学引擎不移动，票 12 C3）；验签通过后取认证中心
/// 策略（`auth-policy` capability 导出），认证中心未激活/调用失败 → 宿主策略
/// 回退（验签通过即放行，无单点）。
///
/// 返回 Some(claims) 表示校验通过（含策略放行），None 表示无 token / 验签失败 / 策略拒绝
pub fn extract_and_verify_jwt(req: &actix_web::dev::ServiceRequest) -> Option<JwtClaims> {
    let auth_header = req.headers().get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let jwt_service = JwtService::new();
    let claims = jwt_service.verify_token_with_expiry(token).ok()?;

    // 票 12 C3：验签后取认证中心策略。无 AppContext（无头/单测）→ 宿主策略。
    if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
        if let Err(reason) = crate::utils::auth::auth_center::enforce_connection_policy(ctx.plugin_host(), token) {
            tracing::warn!(
                device_id = %claims.sub,
                %reason,
                "HTTP /api request denied by auth center policy"
            );
            return None;
        }
    }
    Some(claims)
}

/// 判断请求路径是否属于公开路由（无需认证）
pub fn is_public_path(path: &str) -> bool {
    path.starts_with("/api/auth/") || path == "/api/health" || path == "/health"
}

/// 判断请求路径是否属于插件端点
///
/// 插件端点**有 JWT 就验签**（claims 注入后由 handler 按端点档位判定）；无 JWT 的
/// 请求放行到 handler——端点级认证在 `plugin_controller::plugin_http_endpoint`，
/// 不在这里（本层还解析不出属主插件，旧前缀兜要按接管方的声明判）。
pub fn is_plugin_path(path: &str) -> bool {
    path.starts_with("/api/plugin/")
}

/// HTTP 网关中间件（`/api` scope）：验签通过注入 claims，否则按路径规则放行 / 401
///
/// 从 `server/app.rs` 的路由构造里提出来成为具名中间件，目的是让「协议网关挂在验签之后」
/// 这一顺序约束可被真实 actix 栈测到（见 `server/gateway.rs` 的中间件用例），而不是靠注释
/// 约定。业务 JWT 的验签执行点始终在这里，不下沉、不外移（AGENTS.md §8 认证红线）。
pub(crate) async fn jwt_gateway<B>(req: ServiceRequest, next: Next<B>) -> Result<ServiceResponse, Error>
where
    B: MessageBody + 'static,
{
    let path = req.path().to_string();

    // 公开路由（/api/auth/* 与 /api/health）直接放行
    if is_public_path(&path) {
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    }

    // 有效 JWT → 注入 claims 并放行
    if let Some(claims) = extract_and_verify_jwt(&req) {
        req.extensions_mut().insert(claims);
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    }

    // 插件端点：无 JWT 时放行到 handler（票 08）——真正的「这个端点要不要凭证」由
    // handler 按属主插件 manifest 声明的档位判定，环回 hook 才能免 JWT 命中显式声明
    // `auth: "none"` 的那几条
    if is_plugin_path(&path) {
        return next.call(req).await.map(|res| res.map_into_boxed_body());
    }

    // 其余受保护路由：无有效 JWT → 返回 401
    let (req, _payload) = req.into_parts();
    let response = HttpResponse::Unauthorized().json(json!({
        "code": 1007,
        "message": "Authentication required"
    }));
    Ok(ServiceResponse::new(req, response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_paths_do_not_require_auth() {
        assert!(is_public_path("/api/auth/pairing"));
        assert!(is_public_path("/api/auth/verify"));
        // ticket 01 新增的生物认证端点同样落在 /api/auth/ 前缀下
        assert!(is_public_path("/api/auth/biometric-challenge"));
        assert!(is_public_path("/api/auth/biometric-verify"));
        // biometric-bind 落在 /api/auth/ 前缀下（中间件放行，handler 内验 JWT）
        assert!(is_public_path("/api/auth/biometric-bind"));
        assert!(is_public_path("/api/health"));
        assert!(is_public_path("/health"));
    }

    #[test]
    fn non_public_paths_require_auth() {
        assert!(!is_public_path("/api/sessions"));
        assert!(!is_public_path("/api/settings"));
        assert!(!is_public_path("/"));
        // 前缀相似但路径不同，不应误放行
        assert!(!is_public_path("/api/authx"));
        // 生物认证端点前缀以下仍受保护：/api/auth/biometric 本身是公开前缀的
        // 成员（starts_with 语义正确），但拼写错误/其他路径不能钻前缀漏洞
        assert!(!is_public_path("/api/authbiometric"));
        assert!(!is_public_path("/api/healthz"));
    }

    #[test]
    fn plugin_paths_are_recognized() {
        assert!(is_plugin_path("/api/plugin/com.bedcode.demo/execute"));
        assert!(is_plugin_path("/api/plugin/"));
        // 非插件路径与仅前缀（无尾斜杠）不匹配
        assert!(!is_plugin_path("/api/sessions"));
        assert!(!is_plugin_path("/api/plugin"));
        assert!(!is_plugin_path("/api/plugin2/"));
    }

    // ==================== 票 12 C3：验签留宿主 + 策略门（中间件单测） ====================

    /// 签发一个宿主合法 token（单测无 AppContext → JwtService 进程内随机密钥
    /// 回退，签发/验签同进程稳定，jwt.rs 既有测试同模式）
    fn issue_token(sub: &str, fingerprint: Option<&str>) -> String {
        JwtService::new()
            .generate_token(
                sub.to_string(),
                Some("Pixel 9".to_string()),
                fingerprint.map(String::from),
            )
            .expect("issue token")
    }

    /// 构造带 Authorization: Bearer 头的 ServiceRequest
    fn srv_req_with_bearer(token: &str) -> actix_web::dev::ServiceRequest {
        use actix_web::test;
        test::TestRequest::get()
            .uri("/api/sessions")
            .insert_header(("Authorization", format!("Bearer {}", token)))
            .to_srv_request()
    }

    /// 验签仍执行于宿主：签名被篡改的 token（claims 合法但签名失效）→ None
    /// （策略门在验签之后，篡改 token 连策略都到不了）
    #[test]
    fn tampered_token_rejected_by_host_verification() {
        let token = issue_token("device-1", Some("fp-abc"));
        let tampered = format!("{}x", &token[..token.len() - 4]); // 破坏签名尾段
        let req = srv_req_with_bearer(&tampered);
        assert!(
            extract_and_verify_jwt(&req).is_none(),
            "篡改签名必须被宿主验签拒绝（验签执行点留宿主）"
        );
    }

    /// 验签仍执行于宿主：非 JWT 乱串 / 过期语义由宿主 `JwtService` 负责 → None
    #[test]
    fn garbage_token_rejected_by_host_verification() {
        let req = srv_req_with_bearer("not-a-jwt");
        assert!(extract_and_verify_jwt(&req).is_none());
    }

    /// 无 Authorization / 非 Bearer scheme → None（中间件入口解析）
    #[test]
    fn missing_or_non_bearer_header_returns_none() {
        use actix_web::test;
        let req = test::TestRequest::get().uri("/api/sessions").to_srv_request();
        assert!(extract_and_verify_jwt(&req).is_none(), "无 Authorization 头 → None");

        let req = test::TestRequest::get()
            .uri("/api/sessions")
            .insert_header(("Authorization", "Basic abc"))
            .to_srv_request();
        assert!(extract_and_verify_jwt(&req).is_none(), "非 Bearer scheme → None");
    }

    /// 宿主验签通过 + 单测上下文无 AppContext（认证中心不可判定）→ 宿主策略
    /// 回退放行（无单点）：合法 token 返回 claims，sub/device_name/fingerprint 注入
    #[test]
    fn valid_token_accepted_with_host_policy_fallback() {
        let token = issue_token("device-1", Some("fp-abc"));
        let req = srv_req_with_bearer(&token);
        let claims = extract_and_verify_jwt(&req).expect("合法 token 放行（宿主策略回退）");
        assert_eq!(claims.sub, "device-1");
        assert_eq!(claims.device_name.as_deref(), Some("Pixel 9"));
        assert_eq!(claims.fingerprint.as_deref(), Some("fp-abc"));
    }
}
