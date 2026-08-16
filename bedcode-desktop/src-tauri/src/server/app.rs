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
    file_service_controller, plugin_controller, git_controller,
};
use crate::server::ws::terminal_ws::TerminalWs;
use crate::system::constants::server::{
    WS_TERMINAL_PATH, API_HEALTH_PATH, LOCAL_WS_TERMINAL_PATH, PLACEHOLDER_PEER_ADDR,
    CORS_MAX_AGE_SECS, BIND_ADDRESS,
};

/// WS 握手端点 — 升级为 WebSocket 连接处理终端 I/O
async fn terminal_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let addr = req.peer_addr().unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
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

/// 本地 WS 握手端点 — 仅供桌面端 WebView 直连
///
/// 双重防线：
/// 1. 环回地址校验：服务器绑定 0.0.0.0 供移动端访问，本地通道必须显式限定环回
/// 2. 短期一次性令牌校验（?token=）：防止本机其他进程（恶意网页/脚本）连本地端口
async fn local_terminal_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let addr = req.peer_addr().unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
    if !addr.ip().is_loopback() {
        tracing::warn!(addr = %addr, "Local WS rejected: peer is not loopback");
        return Ok(HttpResponse::Forbidden().finish());
    }

    // 校验短期一次性令牌（由 get_local_ws_token command 签发）
    let token = req.query_string().split('&').find_map(|kv| {
        let mut parts = kv.split('=');
        match (parts.next(), parts.next()) {
            (Some("token"), Some(v)) if !v.is_empty() => Some(v.to_string()),
            _ => None,
        }
    });
    match token {
        Some(t) if crate::server::local_token::LocalTokenManager::global().verify_and_consume(&t) => {}
        _ => {
            tracing::warn!(addr = %addr, "Local WS rejected: missing or invalid token");
            return Ok(HttpResponse::Forbidden().finish());
        }
    }

    let ws_actor = TerminalWs::new_local(addr);
    let config = crate::system::config::AppConfig::global();
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

/// 背景图片扩展名 → Content-Type 映射
fn terminal_bg_content_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// 终端背景图片静态端点 — 公开，无需 JWT（CSS background-image 无法携带认证头）
///
/// 返回应用数据目录中的 `terminal_bg.<ext>`；未设置时返回 404。
/// 仅扫描白名单扩展名的固定前缀文件，不接受任意路径参数，无目录穿越风险。
/// 图片为用户自选的壁纸，不含敏感信息，局域网可见可接受。
async fn terminal_bg_image() -> HttpResponse {
    use crate::system::constants::terminal::{TERMINAL_BG_EXTENSIONS, TERMINAL_BG_FILE_PREFIX};
    use tauri::Manager;

    let data_dir = match crate::system::app_context::AppContext::global()
        .app_handle()
        .path()
        .app_data_dir()
    {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("解析应用数据目录失败: {e}");
            return HttpResponse::InternalServerError().finish();
        }
    };

    // 扫描目录找到当前背景图片（扩展名在选图时可能变化，不能写死）
    let entries = match tokio::fs::read_dir(&data_dir).await {
        Ok(entries) => entries,
        Err(_) => return HttpResponse::NotFound().finish(),
    };

    let prefix = format!("{TERMINAL_BG_FILE_PREFIX}.");
    let mut found: Option<std::path::PathBuf> = None;
    let mut iter = entries;
    while let Ok(Some(entry)) = iter.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(ext) = name.strip_prefix(&prefix) {
            if TERMINAL_BG_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                found = Some(entry.path());
                break;
            }
        }
    }

    let Some(path) = found else {
        return HttpResponse::NotFound().finish();
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    match tokio::fs::read(&path).await {
        Ok(bytes) => HttpResponse::Ok()
            .content_type(terminal_bg_content_type(&ext))
            // 前端通过 ?t= 时间戳防缓存，服务端不额外下发长缓存头
            .insert_header(("Cache-Control", "no-cache"))
            .body(bytes),
        Err(e) => {
            tracing::error!("读取终端背景图片失败 {}: {e}", path.display());
            HttpResponse::InternalServerError().finish()
        }
    }
}

/// 构建路由配置
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // WebSocket 终端端点
    cfg.route(WS_TERMINAL_PATH, web::get().to(terminal_ws));

    // 本地 WebSocket 终端端点（桌面端 WebView 直连，环回校验 + 免 JWT + 二进制帧）
    cfg.route(LOCAL_WS_TERMINAL_PATH, web::get().to(local_terminal_ws));

    // 健康检查（公开，无需 JWT，供移动端探测连通性）
    cfg.route(API_HEALTH_PATH, web::get().to(health_check));

    // 终端背景图片（公开，无需 JWT；CSS background-image 无法携带认证头）
    cfg.route("/static/terminal-bg", web::get().to(terminal_bg_image));

    // /api scope — 挂载 JWT 网关中间件
    // 中间件内部按路径区分：/api/auth/* 公开放行；/api/plugin/* 有 JWT 则校验、
    // 无 JWT 直接放行（hook 脚本等调用方无法持有 JWT，handler 仅校验插件激活状态）；
    // 其余要求 JWT
    cfg.service(
        web::scope("/api")
            .wrap_fn(|req, srv| {
                use crate::server::middleware::jwt_auth::{
                    is_public_path, is_plugin_path, extract_and_verify_jwt,
                };
                use actix_web::HttpMessage;

                let path = req.path().to_string();

                // 公开路由（/api/auth/*）直接放行
                if is_public_path(&path) {
                    return srv.call(req);
                }

                // 有效 JWT → 注入 claims 并放行
                if let Some(claims) = extract_and_verify_jwt(&req) {
                    req.extensions_mut().insert(claims);
                    return srv.call(req);
                }

                // 插件端点：无 JWT 时放行（handler 不校验任何凭证，仅检查插件激活状态；
                // 信任边界：服务监听 0.0.0.0，插件端点对局域网内任意设备可达）
                if is_plugin_path(&path) {
                    return srv.call(req);
                }

                // 其余受保护路由：无有效 JWT → 返回 401
                let (req, _payload) = req.into_parts();
                let response = actix_web::HttpResponse::Unauthorized()
                    .json(json!({
                        "code": 1007,
                        "message": "Authentication required"
                    }));
                let srv_response = actix_web::dev::ServiceResponse::new(req, response);
                Box::pin(std::future::ready(Ok(srv_response)))
            })
            // 公开路由（配对/认证）— 中间件按路径放行
            .route("/auth/pairing", web::post().to(auth_controller::request_pairing))
            .route("/auth/verify", web::post().to(auth_controller::verify_pairing_code))
            .route("/auth/qr-connect", web::post().to(auth_controller::qr_connect))
            .route("/auth/reauth", web::post().to(auth_controller::reauthenticate))
            // 受 JWT 保护的业务路由
            .route("/sessions", web::get().to(session_controller::list_sessions))
            .route("/sessions/start", web::post().to(session_controller::start_session))
            .route("/sessions/{id}/stop", web::post().to(session_controller::stop_session))
            .route("/sessions/{id}/resize", web::post().to(session_controller::resize_session))
            .route("/sessions/{id}/input", web::post().to(session_controller::send_session_input))
            .route("/sessions/{id}/remove", web::delete().to(session_controller::remove_session))
            .route("/configs", web::get().to(config_controller::list_configs))
            .route("/quick-actions", web::get().to(config_controller::list_quick_actions))
            .route("/file-tree", web::post().to(file_controller::get_file_tree))
            .route("/file-tree-children", web::get().to(file_controller::get_file_tree_children))
            .route("/file-content", web::post().to(file_controller::get_file_content))
            .route("/diff-tree", web::post().to(file_controller::get_diff_tree))
            .route("/file-diff", web::post().to(file_controller::get_file_diff))
            .route("/git/branches", web::get().to(git_controller::get_branches))
            .route("/git/status", web::get().to(git_controller::get_status))
            .route("/git/checkout", web::post().to(git_controller::checkout))
            // 插件文件服务（复数 /plugins，走正常 JWT 校验；
            // 单数 /api/plugin/* 是插件动态端点代理，两者互不影响）
            .service(
                web::scope("/plugins/{plugin_id}")
                    .route("/{mount}/list", web::get().to(file_service_controller::list_dir))
                    .route("/{mount}/file", web::get().to(file_service_controller::download_file))
                    .route("/{mount}/file", web::head().to(file_service_controller::head_file))
                    .route("/{mount}/upload", web::post().to(file_service_controller::create_upload))
                    .route("/{mount}/transfer-request", web::post().to(file_service_controller::transfer_request))
                    .route("/{mount}/upload/{sid}", web::put().to(file_service_controller::append_upload))
                    .route("/{mount}/upload/{sid}", web::get().to(file_service_controller::query_upload))
                    .route("/{mount}/upload/{sid}", web::delete().to(file_service_controller::cancel_upload))
                    .route("/{mount}/upload/{sid}/complete", web::post().to(file_service_controller::complete_upload)),
            )
            // 插件动态 HTTP 端点代理 — 中间件允许 JWT 或 plugin token
            .route("/plugin/{plugin_id}/{path:.*}", web::route().to(plugin_controller::plugin_http_endpoint))
    );
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
            .max_age(CORS_MAX_AGE_SECS);

        App::new()
            .wrap(cors)
            .wrap(actix_web::middleware::Logger::default())
            .wrap_fn(|req, srv| {
                crate::server::metrics::MetricsCollector::global().inc_http_request();
                srv.call(req)
            })
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
