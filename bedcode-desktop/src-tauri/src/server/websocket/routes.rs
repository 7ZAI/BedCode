//! WebSocket 传输面路由装配：三条握手端点 + 帧上限
//!
//! 从 `core/app.rs` 拆出（票 07）：三个 WS 握手 handler（`session_terminal_ws` /
//! `event_ws` / `plugin_endpoint_ws`）、属主激活闸门（`endpoint_owner_activated`）
//! 与帧上限（`ws_frame_limit`）都是纯 WS 面语义，寄居单端口组合物是历史错位——
//! 此后 WS 面的路由改动只落在本文件。
//!
//! 常量语义分工：`WS_EVENT_PATH` 归本面（`/ws/event` 事件通道路由）；`API_HEALTH_PATH`
//! 归 HTTP 面（`http/routes.rs`）——端点常量不留在组合物里，避免「core 知道具体路由」的错觉。
//!
//! 依赖方向（不变量 I2 / I1）：本文件只**向下**依赖 `crate::server::core` 与系统常量，
//! 与 `http` 面零横向 import（I1，票 08 加锁）。

use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws as actix_ws;

use crate::server::websocket::channel::plugin::PluginChannel;
use crate::server::websocket::conn::{ConnSpec, WsConnBase};
use crate::server::websocket::registry::{ChannelKind, WsSessionRegistry};
use crate::system::constants::{PLACEHOLDER_PEER_ADDR, WS_EVENT_PATH};

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
    let mount = crate::server::websocket::endpoint::mount_path(&plugin_id, &suffix);
    let Some(entry) = crate::server::websocket::endpoint::find_by_mount(&mount) else {
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

    let addr = req
        .peer_addr()
        .unwrap_or_else(|| PLACEHOLDER_PEER_ADDR.parse().unwrap());
    let client_id = addr.to_string();
    if !WsSessionRegistry::global().reserve_endpoint_client(
        &entry.endpoint_id,
        &client_id,
        entry.max_clients,
    ) {
        tracing::warn!(
            plugin_id = %plugin_id,
            endpoint_id = %entry.endpoint_id,
            limit = entry.max_clients,
            "plugin ws endpoint client limit reached, rejecting before upgrade (503)"
        );
        return Ok(HttpResponse::ServiceUnavailable().finish());
    }

    let channel = PluginChannel::new(&entry, addr);
    let ws_actor = WsConnBase::new(
        ConnSpec {
            owner: Some(entry.owner.clone()),
            endpoint_id: Some(entry.endpoint_id.clone()),
            ..ConnSpec::new(addr, ChannelKind::Plugin)
        },
        Box::new(channel),
    );
    match actix_ws::WsResponseBuilder::new(ws_actor, &req, stream)
        .frame_size(entry.max_message_bytes)
        .start()
    {
        Ok(response) => Ok(response),
        Err(error) => {
            WsSessionRegistry::global().release_endpoint_reservation(&entry.endpoint_id, &client_id);
            Err(error)
        }
    }
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

/// 构建 WS 路由配置
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
}
