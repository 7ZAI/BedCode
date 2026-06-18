//! Actix Web Application Configuration
//!
//! 配置路由、中间件和服务器启动

use actix_web::{web, App, HttpServer};
use actix_cors::Cors;

use crate::desktop::server::controllers::{
    auth_controller, session_controller, config_controller, file_controller,
};

/// 构建路由配置
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // 公开路由（无需 JWT）
    cfg.service(
        web::scope("/api/auth")
            .route("/pairing", web::post().to(auth_controller::request_pairing))
            .route("/verify", web::post().to(auth_controller::verify_pairing_code))
            .route("/qr-connect", web::post().to(auth_controller::qr_connect))
            .route("/reauth", web::post().to(auth_controller::reauthenticate))
    );

    // 受保护路由（需要 JWT）— JWT 验证在各 handler 中通过 get_claims_from_request 实现
    cfg.service(
        web::scope("/api")
            .route("/sessions", web::get().to(session_controller::list_sessions))
            .route("/sessions/start", web::post().to(session_controller::start_session))
            .route("/sessions/{id}/stop", web::post().to(session_controller::stop_session))
            .route("/sessions/{id}/resize", web::post().to(session_controller::resize_session))
            .route("/sessions/{id}/remove", web::delete().to(session_controller::remove_session))
            .route("/configs", web::get().to(config_controller::list_configs))
            .route("/quick-actions", web::get().to(config_controller::list_quick_actions))
            .route("/file-tree", web::post().to(file_controller::get_file_tree))
    );
}

/// 启动 Actix Web HTTP 服务器
pub async fn start_http_server(port: u16) -> std::io::Result<()> {
    tracing::info!("Starting Actix Web HTTP server on port {}", port);

    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .configure(configure_routes)
    })
    .bind(format!("0.0.0.0:{}", port))?
    .run()
    .await
}
