//! Actix Web Application Configuration
//!
//! 配置路由、中间件和服务器启动
//! HTTP REST API + WebSocket 终端在同一端口上运行

use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse, Error, dev::Service, http::KeepAlive};
use actix_cors::Cors;
use actix_web_actors::ws as actix_ws;
use serde_json::json;
use std::time::Duration;

use crate::server::controllers::{
    auth_controller, session_controller, config_controller, file_controller,
    plugin_controller, git_controller,
};
use crate::server::ws::terminal_ws::TerminalWs;

/// WS 握手端点 — 升级为 WebSocket 连接处理终端 I/O
async fn terminal_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let addr = req.peer_addr().unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
    let ws_actor = TerminalWs::new(addr);
    let config = crate::system::config::AppConfig::global();
    // max_size 同时限制 frame 和 message 大小，取两者中较大的值
    let max_size = std::cmp::max(
        config.network.ws_max_frame_size_kb * 1024,
        config.network.ws_max_message_size_mb * 1024 * 1024,
    );
    actix_ws::WsResponseBuilder::new(ws_actor, &req, stream)
        .frame_size(max_size)
        .start()
}

/// 健康检查端点 — 移动端 WS 连接前探测桌面端是否可达
async fn health_check() -> HttpResponse {
    let supervisor = crate::server::supervisor::ServerSupervisor::global();
    let status_info = supervisor.get_status_info().await;
    HttpResponse::Ok().json(json!({
        "status": "ok",
        "port": status_info.port,
        "uptime_secs": status_info.uptime_secs,
    }))
}

/// 构建路由配置
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // WebSocket 终端端点
    cfg.route("/ws/terminal", web::get().to(terminal_ws));

    // 健康检查（公开，无需 JWT，供移动端探测连通性）
    cfg.route("/api/health", web::get().to(health_check));

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
            .route("/sessions/{id}/input", web::post().to(session_controller::send_session_input))
            .route("/sessions/{id}/remove", web::delete().to(session_controller::remove_session))
            .route("/configs", web::get().to(config_controller::list_configs))
            .route("/quick-actions", web::get().to(config_controller::list_quick_actions))
            .route("/file-tree", web::post().to(file_controller::get_file_tree))
            .route("/file-content", web::post().to(file_controller::get_file_content))
            .route("/diff-tree", web::post().to(file_controller::get_diff_tree))
            .route("/file-diff", web::post().to(file_controller::get_file_diff))
            .route("/git/branches", web::get().to(git_controller::get_branches))
            .route("/git/status", web::get().to(git_controller::get_status))
            .route("/git/checkout", web::post().to(git_controller::checkout))
    );

    // 插件专用路由（token 认证，非 JWT）
    cfg.route("/plugin/task-status", web::post().to(plugin_controller::update_task_status));
    cfg.route("/plugin/session-mode", web::post().to(plugin_controller::set_session_mode));
    cfg.route("/plugin/session-mode", web::get().to(plugin_controller::get_session_mode));
}

/// 启动 Actix Web 服务器（HTTP + WebSocket 统一端口）
///
/// 返回 `ServerHandle` 用于优雅停机
/// 调用方通过 oneshot channel 获取 handle，然后继续 await server 保持运行
pub async fn start_http_server(
    port: u16,
    config: &crate::system::config::NetworkConfig,
) -> std::io::Result<(actix_web::dev::ServerHandle, impl std::future::Future<Output = std::io::Result<()>>)> {
    tracing::info!("Starting Actix Web server (HTTP + WS) on port {}", port);

    let keep_alive = if config.keep_alive_secs == 0 {
        KeepAlive::Disabled
    } else {
        KeepAlive::Timeout(Duration::from_secs(config.keep_alive_secs))
    };

    let mut server_builder = HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .wrap_fn(|req, srv| {
                crate::server::metrics::MetricsCollector::global().inc_http_request();
                srv.call(req)
            })
            .configure(configure_routes)
    })
    .bind(format!("0.0.0.0:{}", port))?
    .keep_alive(keep_alive)
    .client_request_timeout(Duration::from_secs(config.client_request_timeout_secs))
    .client_disconnect_timeout(Duration::from_secs(config.client_disconnect_timeout_secs))
    .max_connections(config.max_connections)
    .backlog(config.backlog)
    .tcp_nodelay(config.tcp_nodelay)
    .shutdown_timeout(config.shutdown_timeout_secs);

    if config.workers > 0 {
        server_builder = server_builder.workers(config.workers);
    }

    let server = server_builder.run();

    // 在 await 之前获取 handle，用于后续优雅停机
    let handle = server.handle();

    Ok((handle, server))
}

// 注意：由于 Tauri crate-type = ["cdylib", "rlib"] 的限制，
// Windows 上 cargo test 无法运行（STATUS_ENTRYPOINT_NOT_FOUND）。
// 连接链路测试通过手动运行桌面端 + curl/移动端实际连接来验证：
//
// 验证步骤：
// 1. 启动桌面端应用，确保服务器运行中
// 2. 在同一网络内的移动端或浏览器访问 http://<desktop-ip>:8765/api/health
// 3. 预期返回: {"status":"ok","port":8765,"uptime_secs":123}
// 4. 手动输入 IP 连接应能通过 HTTP 探测后继续 WS 连接
// 5. 如果 HTTP 探测失败，3秒内返回"无法连接"错误而非10秒WS超时
