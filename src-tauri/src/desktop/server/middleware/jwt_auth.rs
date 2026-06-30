//! JWT Authentication Middleware for Actix Web
//!
//! 从 Authorization header 提取 Bearer token 并验证 JWT
//! 验证通过后将 claims 存入 request extensions

use actix_web::{dev::ServiceRequest, Error, HttpMessage};
use actix_web::error::ErrorUnauthorized;
use crate::desktop::auth::jwt::{JwtService, JwtClaims};

/// JWT 认证验证器
///
/// 从 Authorization: Bearer <token> 提取并验证 JWT
/// 验证成功后将 JwtClaims 注入 request extensions
pub fn validate_jwt(req: &ServiceRequest) -> Result<JwtClaims, Error> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ErrorUnauthorized("Missing Authorization header"))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ErrorUnauthorized("Invalid Authorization format, expected Bearer <token>"))?;

    let jwt_service = JwtService::new();
    let claims = jwt_service
        .verify_token_with_expiry(token)
        .map_err(|e| {
            let msg = match e {
                crate::desktop::auth::jwt::JwtError::TokenExpired => "Token expired",
                _ => "Invalid token",
            };
            ErrorUnauthorized(msg)
        })?;

    req.extensions_mut().insert(claims.clone());

    Ok(claims)
}

/// 从 request extensions 提取 JWT claims
pub fn get_claims_from_request(req: &actix_web::HttpRequest) -> Option<JwtClaims> {
    req.extensions().get::<JwtClaims>().cloned()
}
