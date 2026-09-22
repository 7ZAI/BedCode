//! Actix Web Application Configuration
//!
//! 配置路由、中间件和服务器启动
//! HTTP REST API + WebSocket 终端在同一端口上运行

use actix_cors::Cors;
use actix_web::{dev::Service, http::KeepAlive, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws as actix_ws;
use serde_json::json;
use std::time::Duration;

use crate::server::controllers::{plugin_controller, session_controller};
use crate::server::ws::channel::plugin::PluginChannel;
use crate::server::ws::conn::{ConnSpec, WsConnBase};
use crate::server::ws::registry::{ChannelKind, WsSessionRegistry};
use crate::system::constants::server::{
    API_HEALTH_PATH, BIND_ADDRESS, CORS_MAX_AGE_SECS, PLACEHOLDER_PEER_ADDR, WS_EVENT_PATH,
};

/// WS 帧/消息大小上限（字节）
///
/// max_size 同时限制 frame 和 message 大小，取两者中较大的值；
/// 两条 WS 路由（session / event）共用同一计算
///
/// `pub(crate)`：host-websocket（ABI v14）客户端域/服务端域的帧上限
/// 与终端链路取同一事实源（spec §4.4）
pub(crate) fn ws_frame_limit() -> usize {
    let config = crate::system::config::AppConfig::global();
    std::cmp::max(
        config.network.ws_max_frame_size_kb * 1024,
        config.network.ws_max_message_size_mb * 1024 * 1024,
    )
}

/// 每会话终端 WS 握手端点 — 连接创建即绑定 session_id（spec §5.1）
///
/// 移动端前端直连（P2）：首消息 JWT 认证（§4.3 规则），输出帧为 TB v3
/// 二进制（§5.3），订阅即连接（无多路复用）。会话不存在 → 认证通过后
/// error(SESSION_NOT_FOUND) 并关闭。旧 /ws/terminal 兼容路由已随旧 v2.0.0
/// 客户端下线删除（§7 D2）
async fn session_terminal_ws(
    path: web::Path<String>,
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    let addr = req
        .peer_addr()
        .unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
    let ws_actor = WsConnBase::new_for_session(addr, path.into_inner());
    actix_ws::WsResponseBuilder::new(ws_actor, &req, stream)
        .frame_size(ws_frame_limit())
        .start()
}

/// WS 事件通道握手端点 — 常驻事件通道（设备在线判定基准 + 同步广播接收方）
///
/// 认证同样在 WS 首消息完成（JWT 重连或配对流程），与 /ws/terminal 一致；
/// 路由在 /api scope 外，不经 HTTP JWT 中间件
async fn event_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let addr = req
        .peer_addr()
        .unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
    let ws_actor = WsConnBase::new_event(addr);
    actix_ws::WsResponseBuilder::new(ws_actor, &req, stream)
        .frame_size(ws_frame_limit())
        .start()
}

/// 插件端点 WS 握手端点 — `/ws/plugin/{plugin_id}/{path}`（spec D5）
///
/// 通配单点分发（不依赖 actix 动态加路由）：路径 → 端点表反查 → 未注册端点 /
/// 属主未激活 → 404；入站客户端数超上限 → 503（**协议升级前**拒绝，不产生
/// 连接事件，spec §4.4）。与 `/ws/event` 一样落在 `/api` scope 之外，
/// 不经 HTTP JWT 中间件——认证策略由端点声明（`auth: none | jwt`，spec D8）。
async fn plugin_endpoint_ws(
    path: web::Path<(String, String)>,
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    let (plugin_id, suffix) = path.into_inner();
    let mount = crate::server::ws::endpoint::mount_path(&plugin_id, &suffix);
    let Some(entry) = crate::server::ws::endpoint::find_by_mount(&mount) else {
        tracing::debug!(mount_path = %mount, "plugin ws endpoint not registered, rejecting 404");
        return Ok(HttpResponse::NotFound().finish());
    };

    // 属主未激活 → 404（停用流程已回收端点，此处为防御性门禁）
    if !endpoint_owner_activated(&plugin_id).await {
        tracing::debug!(
            plugin_id = %plugin_id,
            mount_path = %mount,
            "plugin ws endpoint owner is not activated, rejecting 404"
        );
        return Ok(HttpResponse::NotFound().finish());
    }

    // 入站连接数上限：升级前拒绝（503，不产生连接事件）
    let online = WsSessionRegistry::global()
        .endpoint_client_count(&entry.endpoint_id)
        .await;
    if online >= entry.max_clients {
        tracing::warn!(
            plugin_id = %plugin_id,
            endpoint_id = %entry.endpoint_id,
            online,
            limit = entry.max_clients,
            "plugin ws endpoint client limit reached, rejecting before upgrade (503)"
        );
        return Ok(HttpResponse::ServiceUnavailable().finish());
    }

    let addr = req
        .peer_addr()
        .unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
    let channel = PluginChannel::new(&entry, addr);
    let ws_actor = WsConnBase::new(
        ConnSpec {
            owner: Some(entry.owner.clone()),
            endpoint_id: Some(entry.endpoint_id.clone()),
            ..ConnSpec::new(addr, ChannelKind::Plugin)
        },
        Box::new(channel),
    );
    actix_ws::WsResponseBuilder::new(ws_actor, &req, stream)
        .frame_size(entry.max_message_bytes)
        .start()
}

/// 属主插件是否处于激活态
///
/// 跳过闸门的两种情形（端点本身只可能由运行中的插件注册，端点表存在性已是
/// 最强证据；停用流程会 `purge_for_plugin` 回收端点，本闸门只是防御性兜底）：
/// - 无 `AppContext` 的运行上下文（库级测试 / 初始化中间态）；
/// - 宿主对该 `plugin_id` **没有任何记录**——此时无从判定，且说明该 id 从未
///   在本进程注册过（测试替身宿主 / 外来上下文注入的全局 AppContext）。
///   仅当宿主有记录且状态非激活时否决。
async fn endpoint_owner_activated(plugin_id: &str) -> bool {
    let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
        return true;
    };
    let host = ctx.plugin_host();
    if host.get_plugin(plugin_id).await.is_none() {
        return true;
    }
    host.is_activated(plugin_id).await
}

/// 健康检查端点 — 移动端 WS 连接前探测桌面端是否可达
async fn health_check() -> HttpResponse {
    let supervisor = crate::server::core::supervisor::ServerSupervisor::global();
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

    let data_dir = match crate::system::app_context::AppContext::global().app_handle() {
        // 无头/测试上下文无 AppHandle：没有应用数据目录，视为未设置背景图
        Some(handle) => match handle.path().app_data_dir() {
            Ok(dir) => dir,
            Err(e) => {
                tracing::error!("解析应用数据目录失败: {e}");
                return HttpResponse::InternalServerError().finish();
            }
        },
        None => return HttpResponse::NotFound().finish(),
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
    // WebSocket 每会话终端端点（spec §5.1）：连接创建即绑定 session_id，订阅即连接。
    // 旧 /ws/terminal 兼容路由（多会话订阅 + base64 JSON 文本帧 + 旧 WS 配对认证）
    // 已随旧 v2.0.0 客户端下线删除
    cfg.route("/ws/terminal/session/{session_id}", web::get().to(session_terminal_ws));

    // WebSocket 事件通道端点（常驻，在线判定 + 广播接收，认证在 WS 首消息完成）
    cfg.route(WS_EVENT_PATH, web::get().to(event_ws));

    // 插件端点通配路由（spec D5）：命名空间段 `{plugin_id}` 由宿主注入，
    // 插件只给后缀；未注册 / 属主未激活 → 404，连接数超限 → 503
    cfg.route("/ws/plugin/{plugin_id}/{path:.*}", web::get().to(plugin_endpoint_ws));

    // 健康检查（公开，无需 JWT，供移动端探测连通性）
    cfg.route(API_HEALTH_PATH, web::get().to(health_check));

    // 终端背景图片（公开，无需 JWT；CSS background-image 无法携带认证头）
    cfg.route("/static/terminal-bg", web::get().to(terminal_bg_image));

    // /api scope — 中间件顺序即请求顺序：JWT 验签 → HTTP 协议网关 → 路由
    // 顺序是硬约束（票 01）：网关只能在验签之后介入，业务 JWT 的验签执行点不下沉；
    // 由 server/gateway.rs 的 unverified_requests_never_reach_gateway 在真实 actix 栈上钉死
    cfg.service(
        web::scope("/api")
            // 注册顺序 = 由内到外（`Scope::wrap` 后注册者先执行），故网关在前、验签在后。
            // 顺序语义由 server/gateway.rs 的中间件用例钉死；即便写反，网关的「已验签」
            // 前置也会把未验签的业务请求挡在插件之外（降级宿主，由验签中间件 401）。
            .wrap(actix_web::middleware::from_fn(crate::server::gateway::business_gateway))
            .wrap(actix_web::middleware::from_fn(
                crate::server::middleware::jwt_auth::jwt_gateway,
            ))
            // 票 07 contract：/api/auth/* 七端点编排已下沉 session 插件（公开路由——
            // JWT 之前的入口经网关免验签转发），宿主不再注册认证业务路由
            // 受 JWT 保护的业务路由
            .route("/sessions", web::get().to(session_controller::list_sessions))
            .route("/sessions/start", web::post().to(session_controller::start_session))
            .route("/sessions/{id}/stop", web::post().to(session_controller::stop_session))
            .route(
                "/sessions/{id}/resize",
                web::post().to(session_controller::resize_session),
            )
            .route(
                "/sessions/{id}/input",
                web::post().to(session_controller::send_session_input),
            )
            .route(
                "/sessions/{id}/history",
                web::get().to(session_controller::get_session_history),
            )
            .route(
                "/sessions/{id}/remove",
                web::delete().to(session_controller::remove_session),
            )
            // 票 02/03/04 contract：/api/configs / /api/quick-actions / 文件浏览五端点 /
            // /api/git/* 三端点真源已全部下沉 session 插件，宿主不再注册任何业务路由
            // （网关别名表接管：插件激活即转发，未激活返回明确错误而非假数据）
            // 插件动态 HTTP 端点代理 — 中间件允许 JWT 或 plugin token
            .route(
                "/plugin/{plugin_id}/{path:.*}",
                web::route().to(plugin_controller::plugin_http_endpoint),
            ),
    );
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
            .wrap(crate::server::middleware::http_filter::TrafficFilter)
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
