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

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// 心跳超时
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

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

/// 终端输出消息（从输出转发任务传回）
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutput {
    text: String,
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
}

impl TerminalWs {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            session: WsSession::new(addr),
            hb: Instant::now(),
        }
    }

    /// 心跳检测
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
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

        // 注销 WsSessionRegistry + 取消所有订阅
        let client_id = self.session.addr.to_string();
        let sessions: Vec<String> = self.session.subscribed_sessions.iter().cloned().collect();
        actix::spawn(async move {
            use crate::server::ws::registry::WsSessionRegistry;
            let registry = WsSessionRegistry::global();
            registry.unregister(&client_id).await;

            let global_manager = GlobalOutputManager::global();
            for session_id in sessions {
                global_manager.unsubscribe(&session_id, &client_id).await;
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
            _ => {
                tracing::debug!("Unsupported WS message type from {}", self.session.addr);
            }
        }
    }

    /// 处理认证消息 — 根据阶段路由到不同处理器
    ///
    /// - RequestPairing / VerifyCode / QrConnect → auth_service::handle_auth（配对流程）
    /// - Authenticated（JWT re-auth）→ 内联 JWT 验证（快速路径，无需异步）
    fn handle_auth(
        &mut self,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.stage {
            // 配对流程：需要异步调用 auth_service（涉及 PairingService、QrTokenManager 等）
            crate::enums::AuthStage::RequestPairing
            | crate::enums::AuthStage::VerifyCode
            | crate::enums::AuthStage::QrConnect => {
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

                // 更新配对设备的 last_seen 和 connect_count
                let fingerprint = claims.fingerprint.clone();
                actix::spawn(async move {
                    if let Some(fp) = fingerprint {
                        let app_ctx = AppContext::global();
                        let db = app_ctx.db().clone();
                        let db_guard = db.lock().await;
                        if let Err(e) = db_guard.update_pairing_last_seen(&fp) {
                            tracing::warn!(fingerprint = %fp, error = %e, "Failed to update pairing last_seen");
                        }
                    }
                });

                // 通知桌面端
                let app_ctx = AppContext::global();
                let _ = app_ctx.app_handle().emit("device-connected", &crate::server::connection_types::DeviceConnectionEvent {
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

        // 创建输出转发通道
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<crate::session::OutputEvent>(256);

        let session_id_for_sub = session_id.clone();
        let session_id_for_fwd = session_id.clone();
        let request_id = message_id.clone();

        // 在 Actix 运行时中执行异步订阅
        actix::spawn(async move {
            let result = global_manager.subscribe(&session_id_for_sub, &client_id, output_tx, start_seq).await;
            let _ = addr.send(SubscribeResult {
                session_id: session_id_for_sub.clone(),
                request_id,
                result,
            }).await;
        });

        // 启动输出转发任务：将 OutputEvent 转为 WS 消息发到 actor
        let addr = ctx.address();
        let config = AppConfig::global();
        let flush_interval = Duration::from_millis(config.terminal.flush_interval_ms);
        let max_buffer_size = config.terminal.max_buffer_size;

        actix::spawn(async move {
            let mut buffer = OutputBuffer::new();

            loop {
                match tokio::time::timeout(flush_interval, output_rx.recv()).await {
                    Ok(Some(event)) => {
                        buffer.append(&event);
                        if buffer.data.len() >= max_buffer_size {
                            let text = buffer.flush(&session_id_for_fwd);
                            let _ = addr.send(TerminalOutput { text }).await;
                        }
                    }
                    Ok(None) => {
                        // channel 关闭
                        if !buffer.is_empty() {
                            let text = buffer.flush(&session_id_for_fwd);
                            let _ = addr.send(TerminalOutput { text }).await;
                        }
                        break;
                    }
                    Err(_) => {
                        // 超时，flush 缓冲区
                        if !buffer.is_empty() {
                            let text = buffer.flush(&session_id_for_fwd);
                            let _ = addr.send(TerminalOutput { text }).await;
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

/// 输出缓冲区 — 累积多条 PTY 输出，减少 WS 消息数量
struct OutputBuffer {
    data: Vec<u8>,
    start_index: u64,
    end_index: u64,
    last_is_waiting: bool,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            end_index: 0,
            last_is_waiting: false,
        }
    }

    fn append(&mut self, event: &crate::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
        }
        // 始终更新 end_index 为最新事件的 index
        self.end_index = event.index;
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为 WS 消息 JSON
    ///
    /// 合并多条事件时，index 为起始索引，end_index 为结束索引，
    /// 前端可用 end_index 精确更新去重游标，支持增量同步
    fn flush(&mut self, session_id: &str) -> String {
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
        );
        self.data.clear();
        message.to_json().unwrap_or_default()
    }
}
