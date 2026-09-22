//! Actix Web Application Configuration
//!
//! 配置路由、中间件和服务器启动
//! HTTP REST API + WebSocket 终端在同一端口上运行
//!
//! **单端口组合物（spec D4 / I3 豁免点）**：HTTP 与 WS 共用一个端口、一个
//! `HttpServer`，组合物必然同时认识两面——本文件是 `core/` 层唯一被允许 import
//! 传输面的文件，白名单三条引用：`http::configure_routes` / `websocket::configure_routes` /
//! `http::middleware::http_filter::TrafficFilter`（后者挂在 `App` 级是现状红线，
//! 覆盖 WS 升级请求与公开路由，收进 `/api` scope 属覆盖收窄的行为变更，另票论证）。
//! 具体路由各自落在 `http/routes.rs` 与 `websocket/routes.rs`，本文件不承载任何端点。

use actix_cors::Cors;
use actix_web::{dev::Service, http::KeepAlive, web, App, HttpServer};
use std::time::Duration;

use crate::system::constants::server::{BIND_ADDRESS, CORS_MAX_AGE_SECS};

/// 构建路由配置 — 组合物：只做两侧装配，不承载任何端点
///
/// HTTP 侧（公开端点 + `/api` scope）与 WS 侧（三条握手路由）各自落在
/// `http/routes.rs` / `websocket/routes.rs`；本函数是 `core` 认识传输面的唯一
/// 合法形态（spec I3 白名单，票 08 结构锁钉死）。
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    crate::server::http::configure_routes(cfg);
    crate::server::websocket::configure_routes(cfg);
}

/// 启动 Actix Web 服务器（HTTP + WebSocket 统一端口）
///
/// 返回 `ServerHandle` 用于优雅停机
/// 调用方通过 oneshot channel 获取 handle，然后继续 await server 保持运行
pub async fn start_http_server(
    port: u16,
    config: &crate::system::config::NetworkConfig,
) -> std::io::Result<(
    actix_web::dev::ServerHandle,
    impl std::future::Future<Output = std::io::Result<()>>,
)> {
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
            .max_age(CORS_MAX_AGE_SECS);

        App::new()
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .wrap_fn(|req, srv| {
                crate::server::core::metrics::MetricsCollector::global().inc_http_request();
                srv.call(req)
            })
            // 流量过滤器责任链（HTTP 接入点）：请求体入站过滤 + 响应体出站过滤。
            // 挂在最内层：CORS/日志层拒绝的请求不进入缓冲逻辑；
            // 链为空时零开销透传（加密等扩展经 server::core::filter::TrafficFilterChain 注册）
            .wrap(crate::server::http::middleware::http_filter::TrafficFilter)
            .configure(configure_routes)
    })
    .bind(format!("{}:{}", BIND_ADDRESS, port))?
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

// 注意：由于 Tauri crate-type = ["cdylib", "rlib"] 的限制，Windows 上曾无法运行
// cargo test（STATUS_ENTRYPOINT_NOT_FOUND）。
// 已由 build.rs 通过 cargo:rustc-link-arg 将 resource.lib（tauri 默认清单）链接进
// lib 单元测试二进制解决，cargo test --lib 可直接运行。
// 连接链路测试通过手动运行桌面端 + curl/移动端实际连接来验证：
//
// 验证步骤：
// 1. 启动桌面端应用，确保服务器运行中
// 2. 在同一网络内的移动端或浏览器访问 http://<desktop-ip>:8765/api/health
// 3. 预期返回: {"status":"ok","port":8765,"uptime_secs":123}
// 4. 手动输入 IP 连接应能通过 HTTP 探测后继续 WS 连接
// 5. 如果 HTTP 探测失败，3秒内返回"无法连接"错误而非10秒WS超时
