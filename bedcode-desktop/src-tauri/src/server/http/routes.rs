//! HTTP 传输面路由装配：公开端点 + `/api` scope
//!
//! 从 `core/app.rs` 拆出（票 07）：两个公开 HTTP 端点（`health_check` /
//! `terminal_bg_image`）与 `/api` scope（含其 `wrap` 链）整块搬入——注册顺序与
//! `wrap` 相对次序**一字不改**（`Scope::wrap` 后注册者先执行，网关写在验签之前
//! 是硬约束，见 `configure_routes` 内注释）。此后 HTTP 面的路由改动只落在本文件。
//!
//! 常量语义分工：`API_HEALTH_PATH` 归本面（`/api/health` 健康检查路由）；
//! `WS_EVENT_PATH` 归 WS 面（`websocket/routes.rs`）——端点常量不留在组合物里，
//! 避免「core 知道具体路由」的错觉。
//!
//! 依赖方向（不变量 I2 / I1）：本文件只**向下**依赖 `crate::server::core` 与系统常量，
//! 与 `websocket` 面零横向 import（I1，票 08 加锁）。

use actix_web::{web, HttpResponse};
use serde_json::json;

use crate::server::http::controllers::{plugin_controller, session_controller};
use crate::system::constants::server::API_HEALTH_PATH;

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

/// 构建 HTTP 路由配置：两个公开端点 + `/api` scope
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
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
            .wrap(actix_web::middleware::from_fn(crate::server::http::gateway::business_gateway))
            .wrap(actix_web::middleware::from_fn(
                crate::server::http::middleware::jwt_auth::jwt_gateway,
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
