//! HTTP 网关中间件 — JWT 认证
//!
//! 挂载在 /api scope 上，统一拦截认证：
//! - /api/auth/* — 放行（公开路由，配对/登录）
//! - /api/plugin/* — 优先校验 JWT，无 JWT 时放行（hook 脚本由插件注入 PTY 环境、
//!   无法持有 JWT；handler 不校验任何凭证，仅检查插件激活状态）
//! - 其余 /api/* — 必须通过 JWT 校验
//!
//! 信任边界：服务监听 BIND_ADDRESS（0.0.0.0），局域网内任意设备均可无凭证调用
//! 已激活插件的 HTTP 端点（含写操作）。插件端点的安全增益只能来自插件自身的
//! 业务校验，本中间件对此不提供保护。
//!
//! 校验通过后将 JwtClaims 注入 request extensions，handler 通过 get_claims_from_request 提取。

use actix_web::HttpMessage;

use crate::utils::auth::jwt::{JwtClaims, JwtService};

/// 从 request extensions 提取 JWT claims
///
/// 供 handler 使用，中间件校验通过后 claims 已注入
pub fn get_claims_from_request(req: &actix_web::HttpRequest) -> Option<JwtClaims> {
    req.extensions().get::<JwtClaims>().cloned()
}

/// 从 Authorization header 提取并验证 JWT
///
/// 返回 Some(claims) 表示校验通过，None 表示无 token 或校验失败
pub fn extract_and_verify_jwt(req: &actix_web::dev::ServiceRequest) -> Option<JwtClaims> {
    let auth_header = req.headers().get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let jwt_service = JwtService::new();
    jwt_service.verify_token_with_expiry(token).ok()
}

/// 判断请求路径是否属于公开路由（无需认证）
pub fn is_public_path(path: &str) -> bool {
    path.starts_with("/api/auth/") || path == "/api/health" || path == "/health"
}

/// 判断请求路径是否属于插件端点
///
/// 插件端点优先走 JWT 校验；无 JWT 的本地调用方（如 Claude Code hook 脚本）放行，
/// handler 仅校验插件激活状态。
pub fn is_plugin_path(path: &str) -> bool {
    path.starts_with("/api/plugin/")
}
