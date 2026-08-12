//! Terminal WebSocket Actor
//!
//! 处理终端 I/O 的 WebSocket 连接
//! 使用 actix-web-actors 的 WS actor 模式

use actix::prelude::*;
use actix_web_actors::ws;
use actix_web_actors::ws::{Message as WsMessage, ProtocolError};
use tauri::Emitter;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::server::ws::session::WsSession;
use crate::server::ws::registry::WsSessionRegistry;
use crate::server::message::Message;
use crate::system::app_context::AppContext;
use crate::session::GlobalOutputManager;
use crate::utils::auth::jwt::JwtService;
use crate::enums::{SessionControlPayload, TerminalPayload};
use crate::system::config::AppConfig;
use crate::system::constants::server::{HEARTBEAT_INTERVAL_SECS, CLIENT_TIMEOUT_SECS, REMOTE_CLIENT_TIMEOUT_SECS};
use crate::system::constants::event;

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(HEARTBEAT_INTERVAL_SECS);
/// 心跳超时
const CLIENT_TIMEOUT: Duration = Duration::from_secs(CLIENT_TIMEOUT_SECS);

/// 订阅结果消息（actor 内部消息，用于从异步任务传回订阅结果）
#[derive(Message)]
#[rtype(result = "()")]
struct SubscribeResult {
    session_id: String,
    /// 原始请求的 message_id，用于匹配客户端的 pending 请求
    request_id: String,
    result: Option<crate::session::SubscribeResponse>,
}

/// 取消订阅结果消息
#[derive(Message)]
#[rtype(result = "()")]
struct UnsubscribeResult {
    session_id: String,
    /// 原始请求的 message_id，用于匹配客户端的 pending 请求
    request_id: String,
    success: bool,
}

/// 终端输出消息（从输出转发任务传回，文本 JSON 形态，供移动端 WS 使用）
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutput {
    text: String,
}

/// 终端输出消息（从输出转发任务传回，二进制帧形态，供桌面端本地 WS 使用）
/// `data` 为已编码的完整帧（含 20 字节头），直接 ctx.binary 发送
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutputBinary {
    data: Vec<u8>,
}

/// 认证响应消息（从异步 auth_service 传回）
#[derive(Message)]
#[rtype(result = "()")]
struct AuthResponse {
    /// 是否将客户端标记为已认证
    authenticated: bool,
    /// 设备 ID（认证成功时设置）
    device_id: Option<String>,
    /// 设备名称（认证成功时设置）
    device_name: Option<String>,
    /// 设备指纹（认证成功时设置）
    fingerprint: Option<String>,
    /// 响应 JSON 文本
    response_json: Option<String>,
}

/// 外部推送消息（用于广播/定向发送，由 WsSessionRegistry 调用）
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendTextMessage {
    pub text: String,
}

/// Terminal WebSocket Actor
pub struct TerminalWs {
    session: WsSession,
    hb: Instant,
    /// 是否为本地环回通道（桌面端 WebView 直连，免 JWT、输出走二进制帧）
    local: bool,
    /// 输出转发任务表（key = `client_id:session_id` → forward_loop JoinHandle）
    ///
    /// 订阅者被替换 / 取消订阅 / 连接断开时 abort：旧订阅者的 send_queue 被替换
    /// drop 后，其 forward_loop 仍会把通道中已缓冲的历史帧排空投递到同一 WS，
    /// 客户端字节游标必然不匹配 → 连续性违反 → 重订阅风暴（自持循环）。
    /// abort 直接丢弃残留帧，保证同连接同一会话始终只有一条输出流
    output_forwarders: std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
    /// 订阅任务表（key = `client_id:session_id` → subscribe task JoinHandle）
    ///
    /// 订阅者被替换 / 取消订阅时 abort：旧任务的 subscribe() 已完成占位并在
    /// 发送历史，其历史发送循环重新读取 subscribers 会拿到替换后的新订阅者，
    /// 把旧历史注入新通道 → 客户端收到重复字节（游标连续不触发自愈，重复
    /// 内容直接显示）。abort 直接终止旧任务的发送循环
    subscribe_tasks: std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
}

impl TerminalWs {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            session: WsSession::new(addr),
            hb: Instant::now(),
            local: false,
            output_forwarders: std::collections::HashMap::new(),
            subscribe_tasks: std::collections::HashMap::new(),
        }
    }

    /// 本地环回通道：直接标记已认证，跳过配对/JWT 流程
    /// （路由层已校验 peer 为环回地址，见 server/app.rs local_terminal_ws）
    pub fn new_local(addr: SocketAddr) -> Self {
        let mut ws = Self::new(addr);
        ws.session.authenticated = true;
        ws.local = true;
        ws
    }

    /// 心跳检测
    ///
    /// 本地环回通道（桌面 WebView）保持 10s 超时；远程通道（移动端）放宽到
    /// 45s——移动端在输出风暴/高负载/弱网下 Pong 回复可能延迟，收紧的超时
    /// 会造成断连-重连-再订阅的循环（每次循环都触发前端断连提示）
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let timeout = if self.local {
            CLIENT_TIMEOUT
        } else {
            Duration::from_secs(REMOTE_CLIENT_TIMEOUT_SECS)
        };
        ctx.run_interval(HEARTBEAT_INTERVAL, move |act, ctx| {
            if Instant::now().duration_since(act.hb) > timeout {
                tracing::warn!("WebSocket heartbeat timeout for {}", act.session.addr);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for TerminalWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Terminal WS connected: {}", self.session.addr);
        self.start_heartbeat(ctx);

        // 注册到 WsSessionRegistry
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();
        let socket_addr = self.session.addr;
        actix::spawn(async move {
            use crate::server::ws::registry::WsSessionRegistry;
            let registry = WsSessionRegistry::global();
            registry.register(client_id, socket_addr, addr).await;
        });
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("Terminal WS disconnected: {}", self.session.addr);

        // 中止所有输出转发任务：连接已断开，残留缓冲帧不再需要投递
        for (_, handle) in self.output_forwarders.drain() {
            handle.abort();
        }
        // 中止所有订阅任务：连接已断开，旧任务的历史发送不再需要（其
        // 占位订阅者残留也会随断连清理移除）
        for (_, handle) in self.subscribe_tasks.drain() {
            handle.abort();
        }

        // 通知前端设备下线（与 DEVICE_CONNECTED 对称；仅已认证连接有 device_id）
        if let Some(device_id) = self.session.device_id.clone() {
            let app_ctx = crate::system::app_context::AppContext::global();
            let _ = app_ctx.app_handle().emit(
                crate::system::constants::event::DEVICE_DISCONNECTED,
                &crate::server::connection_types::DeviceConnectionEvent {
                    addr: self.session.addr.to_string(),
                    device_id,
                    device_name: self.session.device_name.clone(),
                    fingerprint: self.session.fingerprint.clone(),
                    event: "disconnected".to_string(),
                },
            );
        }

        // 注销 WsSessionRegistry + 取消所有订阅 + 清理对端文件服务记录
        let client_id = self.session.addr.to_string();
        let sessions: Vec<String> = self.session.subscribed_sessions.iter().cloned().collect();
        // 断连清理：移除该设备公告的文件服务（避免插件访问已不可达的端点）
        let device_id = self.session.device_id.clone();
        // 断连清理：清除该连接的生物认证挑战值 + 回填连接历史断开时间
        let socket_addr = self.session.addr;
        actix::spawn(async move {
            let app_ctx = crate::system::app_context::AppContext::global();
            app_ctx.biometric_challenges().clear(&socket_addr.to_string()).await;
            if let Some(device_id) = device_id.clone() {
                let db_guard = app_ctx.db().lock().await;
                if let Err(e) = db_guard.close_open_connection_event(&device_id) {
                    tracing::warn!(device_id = %device_id, error = %e, "Failed to close connection history");
                }
            }

            use crate::server::ws::registry::WsSessionRegistry;
            let registry = WsSessionRegistry::global();
            registry.unregister(&client_id).await;

            let global_manager = GlobalOutputManager::global();
            for session_id in sessions {
                global_manager.unsubscribe(&session_id, &client_id).await;
            }

            if let Some(device_id) = device_id {
                app_ctx
                    .file_service()
                    .remove_peer(&device_id)
                    .await;
            }
        });

        Running::Stop
    }
}

/// 处理 WebSocket 消息
impl StreamHandler<Result<WsMessage, ProtocolError>> for TerminalWs {
    fn handle(&mut self, msg: Result<WsMessage, ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!(error = %e, "WS protocol error, closing connection");
                ctx.stop();
                return;
            }
        };

        match msg {
            WsMessage::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            WsMessage::Pong(_) => {
                self.hb = Instant::now();
            }
            WsMessage::Text(text) => {
                crate::server::metrics::MetricsCollector::global().inc_ws_received();
                self.handle_text_message(text.to_string(), ctx);
            }
            WsMessage::Binary(_) => {}
            WsMessage::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl TerminalWs {
    /// 处理文本消息（JSON 格式的 Message）
    fn handle_text_message(&mut self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
        let metrics = crate::server::metrics::MetricsCollector::global();
        let message = match Message::from_json(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, addr = %self.session.addr, "Failed to parse WS message");
                let error = Message::error("PARSE_ERROR", &e.to_string());
                if let Ok(json) = error.to_json() {
                    metrics.inc_ws_sent();
                    ctx.text(json);
                }
                return;
            }
        };

        match message {
            Message::Auth { payload, message_id, .. } => {
                self.handle_auth(payload, message_id, ctx);
            }
            Message::Terminal { session_id, payload, message_id, expect_response, .. } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        metrics.inc_ws_sent();
                        ctx.text(json);
                    }
                    return;
                }
                self.handle_terminal(session_id, payload, message_id, expect_response, ctx);
            }
            Message::SessionControl { payload, message_id, expect_response, .. } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        metrics.inc_ws_sent();
                        ctx.text(json);
                    }
                    return;
                }
                self.handle_session_control(payload, message_id, expect_response, ctx);
            }
            Message::FileService { payload, message_id, .. } => {
                // 文件服务控制面（移动端 → 桌面，规格阶段 2）：
                // Announce → 登记对端文件服务；Withdraw → 移除
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        metrics.inc_ws_sent();
                        ctx.text(json);
                    }
                    return;
                }
                self.handle_file_service(payload);
            }
            _ => {
                tracing::debug!("Unsupported WS message type from {}", self.session.addr);
            }
        }
    }

    /// 处理文件服务控制面消息（仅已认证连接可调用）
    ///
    /// - Announce：取连接 peer_addr IP + 载荷 → 写入 FileServiceRegistry.peers（key=device_id）
    /// - Withdraw：移除对端记录
    fn handle_file_service(&self, payload: crate::enums::FileServicePayload) {
        use crate::enums::FileServicePayload;

        let Some(device_id) = self.session.device_id.clone() else {
            tracing::warn!(addr = %self.session.addr, "file service message from connection without device_id, ignored");
            return;
        };
        let file_service = crate::system::app_context::AppContext::global().file_service().clone();

        match payload {
            FileServicePayload::Announce { port, token, device_name, mounts } => {
                // IP 取连接 peer_addr（移动端 bind 0.0.0.0，公告不含 IP）
                let ip = self.session.addr.ip().to_string();
                let info = bedcode_plugin_api::PeerFileService {
                    ip: ip.clone(),
                    port,
                    token,
                    device_name,
                    mounts: mounts
                        .into_iter()
                        .map(|m| bedcode_plugin_api::PeerMountAnnouncement {
                            plugin_id: m.plugin_id,
                            mount_path: m.mount_path,
                            operations: m.operations,
                        })
                        .collect(),
                };
                tracing::info!(
                    device_id = %device_id,
                    ip = %ip,
                    port = port,
                    "mobile file service announced"
                );
                actix::spawn(async move {
                    file_service.set_peer(&device_id, info).await;
                });
            }
            FileServicePayload::Withdraw {} => {
                tracing::info!(device_id = %device_id, "mobile file service withdrawn");
                actix::spawn(async move {
                    file_service.remove_peer(&device_id).await;
                });
            }
            FileServicePayload::Query {} => {
                // 主动探测：向该客户端回复当前挂载快照（有挂载 → Announce；无 → Withdraw）
                let addr = self.session.addr;
                let device_id = device_id.clone();
                tracing::info!(device_id = %device_id, "file service query received, replying snapshot");
                actix::spawn(async move {
                    // 强制推送该设备当前记录（Query = 显式刷新请求，绕过 set_peer
                    // 去重：插件 activate 后主动探测时信息未变会被吞掉推送）
                    if let Some(info) = file_service.get_peer(&device_id).await {
                        file_service.push_peer(&device_id, info).await;
                    }
                    send_file_service_snapshot_to(addr).await;
                });
            }
        }
    }

    /// 认证成功后向该客户端补发当前文件服务挂载快照
    ///
    /// 修复先挂载后连接的场景：挂载广播发生在客户端连接之前会丢失，
    /// 认证成功时补发一次，移动端经 FileServiceHandler 更新 peer 记录
    pub(crate) fn push_file_service_snapshot(&self, _ctx: &mut ws::WebsocketContext<Self>) {
        let addr = self.session.addr;
        actix::spawn(async move {
            send_file_service_snapshot_to(addr).await;
        });
    }

    /// 处理认证消息 — 根据阶段路由到不同处理器
    ///
    /// - RequestPairing / VerifyCode / QrConnect / ExchangeCertificate / BiometricRequest / BiometricVerify
    ///   → auth_service::handle_auth（配对/生物认证流程）
    /// - Authenticated（JWT re-auth）→ 内联 JWT 验证（快速路径，无需异步）
    fn handle_auth(
        &mut self,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.stage {
            // 配对/生物认证流程：需要异步调用 auth_service（涉及 PairingService、QrTokenManager 等）
            crate::enums::AuthStage::RequestPairing
            | crate::enums::AuthStage::VerifyCode
            | crate::enums::AuthStage::QrConnect
            | crate::enums::AuthStage::ExchangeCertificate
            | crate::enums::AuthStage::BiometricRequest
            | crate::enums::AuthStage::BiometricVerify => {
                self.handle_auth_pairing(payload, message_id, ctx);
            }
            // JWT 重新认证：同步路径，直接验证 JWT token
            crate::enums::AuthStage::Authenticated
            | crate::enums::AuthStage::Reauthenticate => {
                self.handle_auth_jwt(payload, message_id, ctx);
            }
            _ => {
                let error = Message::error_with_id(&message_id, "INVALID_AUTH_STAGE", "Unsupported auth stage");
                if let Ok(json) = error.to_json() { ctx.text(json); }
            }
        }
    }

    /// 处理配对认证（RequestPairing / VerifyCode / QrConnect）
    ///
    /// 通过 actix::spawn 桥接异步 auth_service::handle_auth 调用
    fn handle_auth_pairing(
        &mut self,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let addr = self.session.addr;
        let actor_addr = ctx.address();

        actix::spawn(async move {
            let app_ctx = AppContext::global();
            let pairing_service = app_ctx.pairing_service().clone();
            let qr_manager = app_ctx.qr_manager().clone();
            let app_handle: Option<std::sync::Arc<tauri::AppHandle>> = Some(app_ctx.app_handle().clone());
            let ws_manager = crate::server::ws::WebSocketManager::global();
            let jwt_service = JwtService::new();
            let db = app_ctx.db().clone();

            let result = crate::server::services::auth_service::handle_auth(
                payload,
                message_id,
                addr,
                &pairing_service,
                &qr_manager,
                &jwt_service,
                ws_manager,
                &app_handle,
                &db,
            ).await;

            let auth_response = match result {
                Ok(Some(response_msg)) => {
                    // 从响应中提取认证状态
                    let (authenticated, device_id, device_name, fingerprint) = if let Message::Auth { payload, .. } = &response_msg {
                        match payload.stage {
                            crate::enums::AuthStage::Authenticated => (
                                true,
                                payload.device_id.clone(),
                                payload.device_name.clone(),
                                payload.device_fingerprint.clone(),
                            ),
                            _ => (false, None, None, None),
                        }
                    } else {
                        (false, None, None, None)
                    };

                    AuthResponse {
                        authenticated,
                        device_id,
                        device_name,
                        fingerprint,
                        response_json: response_msg.to_json().ok(),
                    }
                }
                Ok(None) => AuthResponse {
                    authenticated: false,
                    device_id: None,
                    device_name: None,
                    fingerprint: None,
                    response_json: None,
                },
                Err(e) => {
                    tracing::error!(error = %e, addr = %addr, "Auth service error");
                    let error = Message::error("AUTH_ERROR", &e.to_string());
                    AuthResponse {
                        authenticated: false,
                        device_id: None,
                        device_name: None,
                        fingerprint: None,
                        response_json: error.to_json().ok(),
                    }
                }
            };

            let _ = actor_addr.send(auth_response).await;
        });
    }

    /// 处理 JWT 重新认证（快速同步路径）
    fn handle_auth_jwt(
        &mut self,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let metrics = crate::server::metrics::MetricsCollector::global();
        let jwt_service = JwtService::new();
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                let error = Message::error_with_id(&message_id, "NO_TOKEN", "No JWT token provided");
                if let Ok(json) = error.to_json() {
                    metrics.inc_ws_sent();
                    ctx.text(json);
                }
                return;
            }
        };

        match jwt_service.verify_token_with_expiry(&token) {
            Ok(claims) => {
                self.session.authenticated = true;
                self.session.device_id = Some(claims.sub.clone());
                self.session.device_name = claims.device_name.clone();
                self.session.fingerprint = claims.fingerprint.clone();

                // 注册认证状态到 WsSessionRegistry
                let client_id = self.session.addr.to_string();
                let device_name = claims.device_name.clone();
                let fp = claims.fingerprint.clone();
                actix::spawn(async move {
                    use crate::server::ws::registry::WsSessionRegistry;
                    let registry = WsSessionRegistry::global();
                    registry.set_authenticated(&client_id, device_name, fp).await;
                });

                // 更新配对设备的 last_seen 和 connect_count，并同步设备展示名
                // （重连携带真实设备名时刷新历史记录，避免旧名残留；空串视为未上报，保留原值）
                let fingerprint = claims.fingerprint.clone();
                let display_name = claims.device_name.as_deref().filter(|n| !n.trim().is_empty()).map(|n| {
                    crate::server::services::auth_service::format_device_display_name(
                        n,
                        &self.session.addr.to_string(),
                    )
                });
                actix::spawn(async move {
                    if let Some(fp) = fingerprint {
                        let app_ctx = AppContext::global();
                        let db = app_ctx.db().clone();
                        let db_guard = db.lock().await;
                        if let Err(e) = db_guard.update_pairing_last_seen(&fp, display_name.as_deref()) {
                            tracing::warn!(fingerprint = %fp, error = %e, "Failed to update pairing last_seen");
                        }
                    }
                });

                // 通知桌面端
                let app_ctx = AppContext::global();
                let _ = app_ctx.app_handle().emit(event::DEVICE_CONNECTED, &crate::server::connection_types::DeviceConnectionEvent {
                    addr: self.session.addr.to_string(),
                    device_id: claims.sub,
                    device_name: self.session.device_name.clone(),
                    fingerprint: self.session.fingerprint.clone(),
                    event: "authenticated".to_string(),
                });

                let response = Message::Auth {
                    message_id,
                    expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: crate::enums::AuthPayload {
                        stage: crate::enums::AuthStage::Authenticated,
                        device_id: self.session.device_id.clone(),
                        device_name: self.session.device_name.clone(),
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        ..Default::default()
                    },
                };
                if let Ok(json) = response.to_json() {
                    metrics.inc_ws_sent();
                    ctx.text(json);
                }

                // 补发文件服务挂载快照（JWT 重认证成功：修复先挂载后连接的广播丢失）
                self.push_file_service_snapshot(ctx);
            }
            Err(e) => {
                let msg = match e {
                    crate::utils::auth::jwt::JwtError::TokenExpired => "Token expired",
                    _ => "Invalid token",
                };
                let error = Message::error_with_id(&message_id, "AUTH_FAILED", msg);
                if let Ok(json) = error.to_json() {
                    metrics.inc_ws_sent();
                    ctx.text(json);
                }
            }
        }
    }

    /// 处理终端消息 — 路由到 subscribe/unsubscribe/input
    fn handle_terminal(
        &mut self,
        session_id: String,
        payload: TerminalPayload,
        message_id: String,
        expect_response: bool,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.action {
            crate::enums::TerminalAction::Input { data, special_key } => {
                let app_ctx = AppContext::global();
                let sm = app_ctx.session_manager().clone();
                actix::spawn(async move {
                    if let Err(e) = crate::server::services::terminal_service::handle_input(
                        &session_id,
                        TerminalPayload { action: crate::enums::TerminalAction::Input { data, special_key } },
                        &Some(sm),
                    ).await {
                        tracing::error!(session_id = %session_id, error = %e, "Terminal input error");
                    }
                });

                // 输入消息需要立即回复 Ack，避免移动端 send_and_wait 超时断开
                if expect_response {
                    let ack = Message::ack(&message_id);
                    if let Ok(json) = ack.to_json() {
                        crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                        ctx.text(json);
                    }
                }
            }
            crate::enums::TerminalAction::Subscribe { start_seq } => {
                self.handle_subscribe(session_id, start_seq, message_id, ctx);
            }
            crate::enums::TerminalAction::Unsubscribe => {
                self.handle_unsubscribe(session_id, message_id, ctx);
            }
            _ => {}
        }
    }

    /// 订阅会话输出 — 使用 actix::spawn 桥接异步调用
    ///
    /// - `start_seq = None` 或 `0`：从头补完所有历史
    /// - `start_seq = N (N > 0)`：从断点继续（用于断线重连）
    fn handle_subscribe(
        &mut self,
        session_id: String,
        start_seq: Option<u64>,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();

        // 替换订阅者前先中止旧转发任务：旧任务的 send_queue 被替换 drop 后，
        // 其 forward_loop 仍会把通道中已缓冲的历史帧排空投递到同一 WS——
        // 客户端游标必然不匹配 → 连续性违反 → 重订阅风暴（自持循环）。
        // abort 直接丢弃残留帧，保证同连接同一会话始终只有一条输出流
        let fwd_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.output_forwarders.remove(&fwd_key) {
            tracing::debug!("[TerminalWs] Aborting previous output forwarder: {}", fwd_key);
            prev.abort();
        }

        // 替换订阅者前先中止旧订阅任务：旧任务的 subscribe() 已完成占位并
        // 在发送历史，其历史发送循环重新读取 subscribers 会拿到替换后的
        // 新订阅者，把旧历史注入新通道 → 客户端收到重复字节（游标连续，
        // 不触发自愈，重复内容直接显示）。abort 直接终止旧任务的发送循环
        let sub_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.subscribe_tasks.remove(&sub_key) {
            tracing::debug!("[TerminalWs] Aborting previous subscribe task: {}", sub_key);
            prev.abort();
        }

        // 创建输出转发通道
        // 容量 8192：历史回放 + 实时输出并发到达时，subscribe() 的历史发送
        // 会被 send_queue 背压阻塞（历史发不完 → subscribe_response 不回 →
        // 客户端 send_and_wait 超时误判断开）。大容量显著降低背压概率；
        // 连续违反重订阅（客户端增量续传）期间也给实时输出留足缓冲余量
        let (output_tx, output_rx) =
            tokio::sync::mpsc::channel::<crate::session::OutputEvent>(8192);

        let session_id_for_sub = session_id.clone();
        let request_id = message_id.clone();

        // 订阅响应经 oneshot 提前返回：subscribe() 在历史入队前发响应，
        // 不被历史背压阻塞——大历史 + 慢链路时响应延迟会让客户端 10s 订阅
        // 超时误判失败（订阅实际已建立，后续重订阅产生孤儿任务 → 重复流）
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        let addr_for_resp = addr.clone();
        let session_id_for_resp = session_id.clone();
        let request_id_for_resp = request_id.clone();

        // 在 Tokio 运行时中执行异步订阅（tokio::spawn 而非 actix::spawn：
        // 需要可 abort 的 JoinHandle，订阅者被替换时终止旧任务的历史发送，
        // 防止旧历史注入新订阅者通道造成客户端重复字节）
        let subscribe_handle = tokio::spawn(async move {
            let result = global_manager
                .subscribe(&session_id_for_sub, &client_id, output_tx, start_seq, Some(resp_tx))
                .await;
            // 响应已通过 resp_tx 前置返回；此处仅处理会话不存在（resp_tx 已丢弃）
            if result.is_none() {
                let _ = addr.send(SubscribeResult {
                    session_id: session_id_for_sub,
                    request_id,
                    result: None,
                }).await;
            }
        });
        self.subscribe_tasks.insert(sub_key, subscribe_handle);

        // 响应转发任务：订阅建立后立即把裁决消息送回客户端
        actix::spawn(async move {
            if let Ok(response) = resp_rx.await {
                let _ = addr_for_resp.send(SubscribeResult {
                    session_id: session_id_for_resp,
                    request_id: request_id_for_resp,
                    result: Some(response),
                }).await;
            }
        });

        // 启动输出转发任务：将 OutputEvent 转为 WS 消息发到 actor
        // 本地通道 → 二进制帧（桌面端原始字节直通）；远程通道 → base64 JSON（移动端兼容）
        let addr = ctx.address();
        let config = AppConfig::global();
        let flush_interval = Duration::from_millis(config.terminal.flush_interval_ms);
        let max_buffer_size = config.terminal.max_buffer_size;
        let local = self.local;
        let merge_output = config.terminal.merge_output;

        // 本地通道（桌面端环回，延迟敏感）恒零缓冲直通；远程通道按开关决定：
        // 合并开启 → 有界延迟合并；关闭 → 零缓冲直通。合并/直通语义与
        // 时序保证集中在 forward_loop（有单测覆盖）
        let interval = if local || !merge_output {
            Duration::ZERO
        } else {
            flush_interval
        };
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ForwardOutput>(64);
        let session_id_for_fwd = session_id.clone();
        let fwd_handle = tokio::spawn(forward_loop(
            output_rx,
            out_tx,
            interval,
            max_buffer_size,
            local,
            session_id_for_fwd,
        ));
        // 注册转发任务：替换订阅 / 取消订阅 / 断连时 abort
        self.output_forwarders.insert(fwd_key, fwd_handle);

        // 消费循环：转发结果经 actor 发送（失败 = actor 停止，终止转发）
        actix::spawn(async move {
            while let Some(out) = out_rx.recv().await {
                match out {
                    ForwardOutput::Text(text) => {
                        if addr.send(TerminalOutput { text }).await.is_err() {
                            tracing::debug!("[OutputForwarder] Actor stopped, exiting loop");
                            break;
                        }
                    }
                    ForwardOutput::Binary(data) => {
                        if addr.send(TerminalOutputBinary { data }).await.is_err() {
                            tracing::debug!("[OutputForwarder] Actor stopped, exiting loop");
                            break;
                        }
                    }
                }
            }
        });
    }

    /// 取消订阅
    fn handle_unsubscribe(
        &mut self,
        session_id: String,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();
        let request_id = message_id;

        // 中止该会话的输出转发任务：取消订阅后旧任务残留缓冲帧无意义
        let fwd_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.output_forwarders.remove(&fwd_key) {
            tracing::debug!("[TerminalWs] Aborting output forwarder on unsubscribe: {}", fwd_key);
            prev.abort();
        }
        // 中止在途订阅任务：取消订阅后旧订阅完成会重新插入占位订阅者
        let sub_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.subscribe_tasks.remove(&sub_key) {
            tracing::debug!("[TerminalWs] Aborting subscribe task on unsubscribe: {}", sub_key);
            prev.abort();
        }

        actix::spawn(async move {
            let success = global_manager.unsubscribe(&session_id, &client_id).await;
            let _ = addr.send(UnsubscribeResult {
                session_id,
                request_id,
                success,
            }).await;
        });
    }

    /// 处理会话控制消息 — 路由到 session_control service
    fn handle_session_control(
        &mut self,
        payload: SessionControlPayload,
        message_id: String,
        expect_response: bool,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let addr = self.session.addr;
        let device_name = self.session.device_name.clone();
        let actor_addr = ctx.address();
        let app_handle = AppContext::global().app_handle().clone();

        actix::spawn(async move {
            let app_ctx = AppContext::global();
            let session_manager = Some(app_ctx.session_manager().clone());

            let result = crate::server::services::session_control::handle_control_message(
                message_id.clone(),
                None, // session_id
                chrono::Utc::now().timestamp_millis(),
                payload.action,
                &session_manager,
                addr,
                device_name,
                Some(app_handle),
            ).await;

            match result {
                Ok(Some(response_msg)) => {
                    if let Ok(json) = response_msg.to_json() {
                        let _ = actor_addr.send(SendTextMessage { text: json }).await;
                    }
                }
                Ok(None) => {
                    // 无响应消息（如 fire-and-forget 的 ResizeSession）
                    if expect_response {
                        let ack = Message::ack(&message_id);
                        if let Ok(json) = ack.to_json() {
                            let _ = actor_addr.send(SendTextMessage { text: json }).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "[TerminalWs] Session control error");
                    let error = Message::error_with_id(&message_id, "SESSION_CONTROL_ERROR", &e.to_string());
                    if let Ok(json) = error.to_json() {
                        let _ = actor_addr.send(SendTextMessage { text: json }).await;
                    }
                }
            }
        });
    }
}

// ==================== Actor Message Handlers ====================

/// 处理订阅结果
impl Handler<SubscribeResult> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SubscribeResult, ctx: &mut Self::Context) {
        match msg.result {
            Some(response) => {
                self.session.subscribed_sessions.insert(msg.session_id.clone());
                let ws_msg = Message::subscribe_response_with_request_id(
                    &msg.session_id,
                    response.min_seq,
                    response.max_seq,
                    response.history_count,
                    response.mode,
                    response.min_offset,
                    response.max_offset,
                    &msg.request_id,
                );
                if let Ok(json) = ws_msg.to_json() {
                    crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                    ctx.text(json);
                }
            }
            None => {
                let error = Message::error_with_id(&msg.request_id, "SESSION_NOT_FOUND", &format!("Session {} not found", msg.session_id));
                if let Ok(json) = error.to_json() {
                    crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                    ctx.text(json);
                }
            }
        }
    }
}

/// 处理取消订阅结果
impl Handler<UnsubscribeResult> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: UnsubscribeResult, ctx: &mut Self::Context) {
        if msg.success {
            self.session.subscribed_sessions.remove(&msg.session_id);
            let ws_msg = Message::unsubscribe_response_with_request_id(&msg.session_id, &msg.request_id);
            if let Ok(json) = ws_msg.to_json() {
                crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                ctx.text(json);
            }
        }
    }
}

/// 处理终端输出转发
impl Handler<TerminalOutput> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: TerminalOutput, ctx: &mut Self::Context) {
        crate::server::metrics::MetricsCollector::global().inc_ws_sent();
        ctx.text(msg.text);
    }
}

/// 处理终端输出转发（二进制帧，本地通道）
impl Handler<TerminalOutputBinary> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: TerminalOutputBinary, ctx: &mut Self::Context) {
        crate::server::metrics::MetricsCollector::global().inc_ws_sent();
        ctx.binary(msg.data);
    }
}

/// 处理认证响应（从 auth_service 异步调用返回）
impl Handler<AuthResponse> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: AuthResponse, ctx: &mut Self::Context) {
        // 更新会话认证状态
        if msg.authenticated {
            self.session.authenticated = true;
            self.session.device_id = msg.device_id.clone();
            self.session.device_name = msg.device_name.clone();
            self.session.fingerprint = msg.fingerprint.clone();

            // 补发文件服务挂载快照（配对认证成功：修复先挂载后连接的广播丢失）
            self.push_file_service_snapshot(ctx);

            // 注册到 WsSessionRegistry
            let client_id = self.session.addr.to_string();
            let device_name = msg.device_name.clone();
            let fingerprint = msg.fingerprint.clone();
            actix::spawn(async move {
                use crate::server::ws::registry::WsSessionRegistry;
                let registry = WsSessionRegistry::global();
                registry.set_authenticated(&client_id, device_name, fingerprint).await;
            });
        }

        // 发送响应给客户端
        if let Some(json) = msg.response_json {
            crate::server::metrics::MetricsCollector::global().inc_ws_sent();
            ctx.text(json);
        }
    }
}

/// 处理外部推送消息（广播/定向发送）
impl Handler<SendTextMessage> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SendTextMessage, ctx: &mut Self::Context) {
        crate::server::metrics::MetricsCollector::global().inc_ws_sent();
        ctx.text(msg.text);
    }
}

// ==================== Output Buffer ====================

/// 转发输出形态：文本 JSON（移动端 WS）或二进制帧（桌面端本地 WS）
#[derive(Debug)]
enum ForwardOutput {
    Text(String),
    Binary(Vec<u8>),
}

/// 二进制帧头：magic(2) + version(1) + flags(1) + start_offset(8 LE) + end_offset(8 LE) = 20 字节
const BINARY_FRAME_HEADER_LEN: usize = 20;
const BINARY_FRAME_MAGIC: [u8; 2] = [0x54, 0x42]; // "TB"
const BINARY_FRAME_VERSION: u8 = 1;
const BINARY_FRAME_FLAG_WAITING: u8 = 0x01;

/// 编码输出二进制帧（客户端据此做字节级连续性校验）
fn encode_output_frame(start_offset: u64, end_offset: u64, is_waiting: bool, data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(BINARY_FRAME_HEADER_LEN + data.len());
    frame.extend_from_slice(&BINARY_FRAME_MAGIC);
    frame.push(BINARY_FRAME_VERSION);
    frame.push(if is_waiting { BINARY_FRAME_FLAG_WAITING } else { 0 });
    frame.extend_from_slice(&start_offset.to_le_bytes());
    frame.extend_from_slice(&end_offset.to_le_bytes());
    frame.extend_from_slice(data);
    frame
}

/// 输出缓冲区 — 累积多条 PTY 输出，减少 WS 消息数量
struct OutputBuffer {
    data: Vec<u8>,
    start_index: u64,
    end_index: u64,
    start_offset: u64,
    end_offset: u64,
    last_is_waiting: bool,
    /// true = 二进制帧输出（本地通道）；false = base64 JSON（移动端通道）
    binary: bool,
}

impl OutputBuffer {
    fn new(binary: bool) -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            end_index: 0,
            start_offset: 0,
            end_offset: 0,
            last_is_waiting: false,
            binary,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
            self.start_offset = event.start_offset;
        }
        // 始终更新 end_index 为最新事件的 index
        self.end_index = event.index;
        self.end_offset = event.end_offset;
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为转发输出
    ///
    /// 文本形态：合并多条事件时，index 为起始索引，end_index 为结束索引，
    /// 前端可用 end_index 精确更新去重游标，支持增量同步
    /// 二进制形态：帧携带 [start_offset, end_offset)，客户端校验连续性
    fn flush(&mut self, session_id: &str) -> ForwardOutput {
        if self.binary {
            let frame = encode_output_frame(
                self.start_offset,
                self.end_offset,
                self.last_is_waiting,
                &self.data,
            );
            self.data.clear();
            ForwardOutput::Binary(frame)
        } else {
            let data_base64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &self.data,
            );
            // 仅在合并了多条事件（end_index > start_index）时附带 end_index
            let end_index = if self.end_index > self.start_index {
                Some(self.end_index as usize)
            } else {
                None
            };
            let message = Message::output_from_base64(
                session_id,
                &data_base64,
                self.last_is_waiting,
                self.start_index as usize,
                end_index,
                Some(self.start_offset),
                Some(self.end_offset),
            );
            self.data.clear();
            ForwardOutput::Text(message.to_json().unwrap_or_default())
        }
    }
}

/// 输出转发循环 — 将 OutputEvent 流编码为 ForwardOutput 经 out_tx 送出
///
/// 两种模式：
/// - `flush_interval = ZERO`：零缓冲直通，每条事件立即转发（本地环回通道 / 合并开关关闭）
/// - 有界延迟合并：字节达 `max_buffer_size` 或距上次 flush 超过 `flush_interval` 时
///   flush（先到先发），持续输出下延迟恒 ≤ flush_interval。不能用 timeout 重计时代替
///   时间窗——持续输出时 timeout 永不触发，flush 会退化成仅容量触发，慢速输出
///   延迟 = 容量/速率（可达数百 ms）
async fn forward_loop(
    mut output_rx: tokio::sync::mpsc::Receiver<crate::session::OutputEvent>,
    out_tx: tokio::sync::mpsc::Sender<ForwardOutput>,
    flush_interval: Duration,
    max_buffer_size: usize,
    binary: bool,
    session_id: String,
) {
    let mut buffer = OutputBuffer::new(binary);

    if flush_interval.is_zero() {
        // 零缓冲直通：每条事件立即转发，不等待
        while let Some(event) = output_rx.recv().await {
            buffer.append(&event);
            if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                break;
            }
        }
        return;
    }

    // 有界延迟合并：时间窗 / 字节窗双条件，先到先发
    let mut last_flush = tokio::time::Instant::now();
    loop {
        match tokio::time::timeout(flush_interval, output_rx.recv()).await {
            Ok(Some(event)) => {
                buffer.append(&event);
                if buffer.data.len() >= max_buffer_size
                    || last_flush.elapsed() >= flush_interval
                {
                    if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                        break;
                    }
                    last_flush = tokio::time::Instant::now();
                }
            }
            Ok(None) => {
                // channel 关闭，最终 flush
                if !buffer.is_empty() {
                    let _ = out_tx.send(buffer.flush(&session_id)).await;
                }
                break;
            }
            Err(_) => {
                // 空闲超时，flush 缓冲区；仅在确有内容发出时重置时间窗——
                // 空 buffer 也重置会把持续输出场景的时间窗进度抹掉（timeout 与
                // 事件同时就绪时 Err 分支先执行，内容 flush 将永远等不到）
                if !buffer.is_empty() {
                    if out_tx.send(buffer.flush(&session_id)).await.is_err() {
                        break;
                    }
                    last_flush = tokio::time::Instant::now();
                }
            }
        }
    }
}

/// 向指定客户端发送当前文件服务挂载快照（认证成功补发 / Query 响应共用）
///
/// 有挂载 → Announce（port 取宿主 HTTP 端口，与 WS 同端口；token 为空，
/// 移动端经其全局 token 兜底）；无挂载 → Withdraw（告知对端移除 peer 记录）
async fn send_file_service_snapshot_to(addr: SocketAddr) {
    use crate::enums::FileServicePayload;
    use crate::server::ws::message::Message;
    use crate::server::ws::registry::WsSessionRegistry;
    use crate::system::app_context::AppContext;
    use crate::system::config::AppConfig;

    let ctx = AppContext::global();
    let mounts = ctx.file_service().mount_announcements().await;
    let payload = if mounts.is_empty() {
        FileServicePayload::Withdraw {}
    } else {
        FileServicePayload::Announce {
            port: AppConfig::global().network.port,
            token: String::new(),
            // 携带本机真实设备名（SystemInfo 兜底保证非空），供移动端文件传输展示
            device_name: ctx.system_info().device_name.clone(),
            mounts,
        }
    };
    let json = match Message::file_service(payload).to_json() {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(error = %e, addr = %addr, "send_file_service_snapshot: serialize failed");
            return;
        }
    };
    if let Err(e) = WsSessionRegistry::global().send_to_addr(&addr, json).await {
        tracing::warn!(addr = %addr, error = %e, "send_file_service_snapshot: send failed");
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn event(session_id: &str, data: &[u8], index: u64, start_offset: u64, end_offset: u64) -> crate::session::OutputEvent {
        crate::session::OutputEvent {
            session_id: session_id.to_string(),
            data: data.to_vec(),
            index,
            start_offset,
            end_offset,
            timestamp: 0,
            is_waiting: false,
        }
    }

    /// 帧头布局：magic(2) + version(1) + flags(1) + start(8 LE) + end(8 LE) + payload
    #[test]
    fn test_encode_output_frame_header() {
        let frame = encode_output_frame(100, 106, true, b"hello");

        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], 1); // version
        assert_eq!(frame[3], BINARY_FRAME_FLAG_WAITING); // is_waiting
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 100);
        assert_eq!(u64::from_le_bytes(frame[12..20].try_into().unwrap()), 106);
        assert_eq!(&frame[20..], b"hello");
        assert_eq!(frame.len(), BINARY_FRAME_HEADER_LEN + 5);
    }

    #[test]
    fn test_encode_output_frame_non_waiting_flag() {
        let frame = encode_output_frame(0, 1, false, b"x");
        assert_eq!(frame[3], 0);
    }

    /// 二进制形态：合并多条事件为一个帧，偏移取首事件 start / 尾事件 end
    #[test]
    fn test_output_buffer_binary_flush_merges_with_offsets() {
        let mut buf = OutputBuffer::new(true);
        buf.append(&event("s", b"ab", 0, 10, 12));
        buf.append(&event("s", b"cd", 1, 12, 14));
        buf.append(&event("s", b"ef", 2, 14, 16));

        let out = buf.flush("s");
        let ForwardOutput::Binary(frame) = out else {
            panic!("expected binary frame");
        };
        assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 10);
        assert_eq!(u64::from_le_bytes(frame[12..20].try_into().unwrap()), 16);
        assert_eq!(&frame[20..], b"abcdef");
        assert!(buf.is_empty()); // flush 后清空
    }

    /// 文本形态（移动端兼容）：base64 JSON 携带 start_offset/end_offset 与 end_index
    #[test]
    fn test_output_buffer_text_flush_carries_offsets() {
        let mut buf = OutputBuffer::new(false);
        buf.append(&event("s", b"ab", 3, 40, 42));
        buf.append(&event("s", b"cd", 4, 42, 44));

        let out = buf.flush("s");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 3);
        assert_eq!(end_index, Some(4));
        assert_eq!(start_offset, Some(40));
        assert_eq!(end_offset, Some(44));
        // 解码 base64 校验数据
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"abcd");
    }

    /// 单事件 flush：end_index 为 None，偏移取事件自身
    #[test]
    fn test_output_buffer_single_event_flush() {
        let mut buf = OutputBuffer::new(false);
        buf.append(&event("s", b"single", 7, 100, 106));

        let out = buf.flush("s");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 7);
        assert_eq!(end_index, None);
        assert_eq!(start_offset, Some(100));
        assert_eq!(end_offset, Some(106));
    }

    // ==================== forward_loop 转发循环（合并时序语义） ====================

    /// 启动 forward_loop 并返回 (事件发送端, 输出接收端)
    fn spawn_forward(
        flush_interval: Duration,
        max_buffer_size: usize,
        binary: bool,
    ) -> (
        tokio::sync::mpsc::Sender<crate::session::OutputEvent>,
        tokio::sync::mpsc::Receiver<ForwardOutput>,
        tokio::task::JoinHandle<()>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::channel::<crate::session::OutputEvent>(128);
        let (out_tx, out_rx) = tokio::sync::mpsc::channel::<ForwardOutput>(128);
        let fwd = tokio::spawn(forward_loop(rx, out_tx, flush_interval, max_buffer_size, binary, "s".into()));
        (tx, out_rx, fwd)
    }

    /// 持续输出（事件间隔 < flush_interval）：合并生效且首条消息延迟有界（≤ 时间窗）
    ///
    /// 使用 tokio 虚拟时钟（start_paused）精确控制事件节奏，避免真实定时器
    /// 精度（Windows 上 ~15ms 抖动）干扰批次断言；消费任务与发送并行，
    /// 记录的是消息实际发出的时刻而非测试开始接收的时刻
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_sustained_output_bounded_delay_and_merging() {
        let (tx, mut out_rx, fwd) = spawn_forward(Duration::from_millis(20), 64 * 1024, false);

        let start = tokio::time::Instant::now();
        let (res_tx, mut res_rx) = tokio::sync::mpsc::channel::<(usize, Option<Duration>)>(4);
        // 并行消费：记录每条消息的实际发出时刻（虚拟时钟）
        let collector = tokio::spawn(async move {
            let mut messages = 0;
            let mut first_at: Option<Duration> = None;
            while let Some(out) = out_rx.recv().await {
                if first_at.is_none() {
                    first_at = Some(start.elapsed());
                }
                let ForwardOutput::Text(_) = out else {
                    panic!("expected text output");
                };
                messages += 1;
            }
            let _ = res_tx.send((messages, first_at)).await;
        });

        // 每 5ms 一条小事件，共 30 条（持续 150ms，间隔远小于 20ms 时间窗）
        for i in 0..30u64 {
            tx.send(event("s", b"x", i, i, i + 1)).await.unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop(tx);

        let (messages, first_at) = res_rx.recv().await.expect("collector finished");
        let _ = collector.await;
        let _ = fwd.await;

        // 合并生效：30 条事件按 20ms 窗聚合成 ~7-8 批（5ms 间隔 → 每批 4-5 条）
        assert!(messages < 12, "expected merging, got {messages} messages");
        // 有界延迟：首批事件累积满 20ms 时间窗时发出（虚拟时钟精确）
        let first = first_at.expect("at least one message");
        assert!(
            first >= Duration::from_millis(15) && first <= Duration::from_millis(25),
            "first message delayed {first:?}, expected ~20ms"
        );
    }

    /// 字节窗：单条大事件 ≥ max_buffer_size 时立即 flush，不等待时间窗
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_byte_window_flushes_immediately() {
        let (tx, mut out_rx, fwd) = spawn_forward(Duration::from_millis(500), 8, false);

        let start = tokio::time::Instant::now();
        tx.send(event("s", b"0123456789", 0, 0, 10)).await.unwrap(); // 10 字节 > 8
        drop(tx);

        let out = tokio::time::timeout(Duration::from_millis(50), out_rx.recv())
            .await
            .expect("byte window must flush immediately")
            .expect("forward_loop exited");
        // 立即 flush：虚拟时间几乎未流逝，不等到 500ms 时间窗
        assert!(start.elapsed() < Duration::from_millis(100));

        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 0);
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"0123456789");
        let _ = fwd.await;
    }

    /// 空闲 flush：单条小事件后无后续，≤ flush_interval 后发出（不无限滞留）
    ///
    /// 注意保持 sender 存活：drop 会触发 final flush 路径（立即发出），
    /// 而非空闲超时路径
    #[tokio::test(start_paused = true)]
    async fn test_forward_loop_idle_flush_within_interval() {
        let (tx, mut out_rx, fwd) = spawn_forward(Duration::from_millis(30), 64 * 1024, false);

        let start = tokio::time::Instant::now();
        tx.send(event("s", b"hi", 0, 0, 2)).await.unwrap();

        let out = tokio::time::timeout(Duration::from_millis(80), out_rx.recv())
            .await
            .expect("idle flush within interval")
            .expect("forward_loop exited");
        // 空闲 flush 由时间窗触发：虚拟时间 ≈30ms（而非无限滞留）
        let elapsed = start.elapsed();
        assert!(
            (Duration::from_millis(25)..=Duration::from_millis(60)).contains(&elapsed),
            "idle flush elapsed: {elapsed:?}"
        );
        assert!(matches!(out, ForwardOutput::Text(_)));

        drop(tx); // 关闭通道，让循环退出
        let _ = fwd.await;
    }

    /// 零间隔（直通模式）：每条事件立即转发，消息数 = 事件数，无合并
    #[tokio::test]
    async fn test_forward_loop_zero_interval_passthrough() {
        let (tx, mut out_rx, fwd) = spawn_forward(Duration::ZERO, 64 * 1024, false);

        for i in 0..5u64 {
            tx.send(event("s", b"x", i, i, i + 1)).await.unwrap();
        }
        drop(tx);

        let mut messages = 0;
        while let Some(out) = out_rx.recv().await {
            let ForwardOutput::Text(_) = out else {
                panic!("expected text output");
            };
            messages += 1;
        }
        let _ = fwd.await;
        assert_eq!(messages, 5, "passthrough must forward each event unchanged");
    }

    /// 通道关闭：未达时间窗/字节窗的残留缓冲最终 flush（合并语义收尾）
    #[tokio::test]
    async fn test_forward_loop_final_flush_on_channel_close() {
        let (tx, mut out_rx, fwd) = spawn_forward(Duration::from_millis(60_000), 64 * 1024, false);

        tx.send(event("s", b"ab", 0, 0, 2)).await.unwrap();
        tx.send(event("s", b"cd", 1, 2, 4)).await.unwrap();
        drop(tx); // 未达时间窗/字节窗 → 关闭时合并两条最终发出

        let out = tokio::time::timeout(Duration::from_millis(100), out_rx.recv())
            .await
            .expect("final flush on channel close")
            .expect("forward_loop exited");
        let ForwardOutput::Text(json) = out else {
            panic!("expected text output");
        };
        let msg: Message = Message::from_json(&json).unwrap();
        let Message::Terminal { payload, .. } = msg else {
            panic!("expected terminal message");
        };
        let crate::enums::TerminalAction::Output { data, index, end_index, start_offset, end_offset, .. } = payload.action else {
            panic!("expected output action");
        };
        assert_eq!(index, 0);
        assert_eq!(end_index, Some(1), "merged two events");
        assert_eq!(start_offset, Some(0));
        assert_eq!(end_offset, Some(4));
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).unwrap();
        assert_eq!(decoded, b"abcd");

        // 循环在关闭后退出：recv 返回 Ok(None)（通道关闭）而非超时
        assert!(
            matches!(
                tokio::time::timeout(Duration::from_millis(50), out_rx.recv()).await,
                Ok(None)
            ),
            "forward_loop must exit after channel close"
        );
        let _ = fwd.await;
    }
}
