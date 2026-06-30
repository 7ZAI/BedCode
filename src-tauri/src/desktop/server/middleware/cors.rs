//! CORS Middleware for Actix Web

use actix_cors::Cors;

/// 创建 CORS 配置
///
/// 允许所有来源（移动端 IP 不固定），支持认证 header
pub fn cors_config() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .max_age(3600)
}
