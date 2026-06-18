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

use crate::desktop::server::ws::session::WsSession;
use crate::desktop::server::message::Message;
use crate::desktop::app_context::AppContext;
use crate::desktop::session::GlobalOutputManager;
use crate::desktop::auth::jwt::JwtService;
use crate::shared::enums::TerminalPayload;
use crate::shared::system::config::AppConfig;

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// 心跳超时
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// 订阅结果消息（actor 内部消息，用于从异步任务传回订阅结果）
#[derive(Message)]
#[rtype(result = "()")]
struct SubscribeResult {
    session_id: String,
    result: Option<crate::desktop::session::SubscribeResponse>,
}

/// 取消订阅结果消息
#[derive(Message)]
#[rtype(result = "()")]
struct UnsubscribeResult {
    session_id: String,
    success: bool,
}

/// 终端输出消息（从输出转发任务传回）
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutput {
    text: String,
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
            use crate::desktop::server::ws::registry::WsSessionRegistry;
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
            use crate::desktop::server::ws::registry::WsSessionRegistry;
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
                tracing::error!("WS protocol error: {}", e);
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
        let message = match Message::from_json(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to parse WS message: {}", e);
                let error = Message::error("PARSE_ERROR", &e.to_string());
                if let Ok(json) = error.to_json() {
                    ctx.text(json);
                }
                return;
            }
        };

        match message {
            Message::Auth { payload, message_id, .. } => {
                self.handle_auth(payload, message_id, ctx);
            }
            Message::Terminal { session_id, payload, message_id, .. } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() { ctx.text(json); }
                    return;
                }
                self.handle_terminal(session_id, payload, ctx);
            }
            _ => {
                tracing::debug!("Unsupported WS message type from {}", self.session.addr);
            }
        }
    }

    /// 处理认证消息 — JWT 验证
    fn handle_auth(
        &mut self,
        payload: crate::shared::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let jwt_service = JwtService::new();
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                let error = Message::error_with_id(&message_id, "NO_TOKEN", "No JWT token provided");
                if let Ok(json) = error.to_json() { ctx.text(json); }
                return;
            }
        };

        match jwt_service.verify_token_with_expiry(&token) {
            Ok(claims) => {
                self.session.authenticated = true;
                self.session.device_id = Some(claims.sub.clone());
                self.session.device_name = claims.device_name.clone();

                // 注册认证状态到 WsSessionRegistry
                let client_id = self.session.addr.to_string();
                let device_name = claims.device_name.clone();
                actix::spawn(async move {
                    use crate::desktop::server::ws::registry::WsSessionRegistry;
                    let registry = WsSessionRegistry::global();
                    registry.set_authenticated(&client_id, device_name).await;
                });

                // 通知桌面端
                let app_ctx = AppContext::global();
                let _ = app_ctx.app_handle().emit("device-connected", &crate::desktop::server::connection_types::DeviceConnectionEvent {
                    addr: self.session.addr.to_string(),
                    device_id: claims.sub,
                    device_name: claims.device_name,
                    event: "authenticated".to_string(),
                });

                let response = Message::Auth {
                    message_id,
                    expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: crate::shared::enums::AuthPayload {
                        stage: crate::shared::enums::AuthStage::Authenticated,
                        device_id: self.session.device_id.clone(),
                        device_name: self.session.device_name.clone(),
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        ..Default::default()
                    },
                };
                if let Ok(json) = response.to_json() { ctx.text(json); }
            }
            Err(e) => {
                let msg = match e {
                    crate::desktop::auth::jwt::JwtError::TokenExpired => "Token expired",
                    _ => "Invalid token",
                };
                let error = Message::error_with_id(&message_id, "AUTH_FAILED", msg);
                if let Ok(json) = error.to_json() { ctx.text(json); }
            }
        }
    }

    /// 处理终端消息 — 路由到 subscribe/unsubscribe/input
    fn handle_terminal(
        &mut self,
        session_id: String,
        payload: TerminalPayload,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.action {
            crate::shared::enums::TerminalAction::Input { data, special_key } => {
                let app_ctx = AppContext::global();
                let sm = app_ctx.session_manager().clone();
                actix::spawn(async move {
                    if let Err(e) = crate::desktop::server::services::terminal_service::handle_input(
                        &session_id,
                        TerminalPayload { action: crate::shared::enums::TerminalAction::Input { data, special_key } },
                        &Some(sm),
                    ).await {
                        tracing::error!("Terminal input error: {}", e);
                    }
                });
            }
            crate::shared::enums::TerminalAction::Subscribe { start_seq: _ } => {
                self.handle_subscribe(session_id, ctx);
            }
            crate::shared::enums::TerminalAction::Unsubscribe => {
                self.handle_unsubscribe(session_id, ctx);
            }
            _ => {}
        }
    }

    /// 订阅会话输出 — 使用 actix::spawn 桥接异步调用
    fn handle_subscribe(
        &mut self,
        session_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();

        // 创建输出转发通道
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<crate::desktop::session::OutputEvent>(256);

        let session_id_for_sub = session_id.clone();
        let session_id_for_fwd = session_id.clone();

        // 在 Actix 运行时中执行异步订阅
        actix::spawn(async move {
            let result = global_manager.subscribe(&session_id_for_sub, &client_id, output_tx).await;
            let _ = addr.send(SubscribeResult {
                session_id: session_id_for_sub.clone(),
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
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();

        actix::spawn(async move {
            let success = global_manager.unsubscribe(&session_id, &client_id).await;
            let _ = addr.send(UnsubscribeResult {
                session_id,
                success,
            }).await;
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
                let ws_msg = Message::subscribe_response(
                    &msg.session_id,
                    response.min_seq,
                    response.max_seq,
                    response.history_count,
                );
                if let Ok(json) = ws_msg.to_json() { ctx.text(json); }
            }
            None => {
                let error = Message::error("SESSION_NOT_FOUND", &format!("Session {} not found", msg.session_id));
                if let Ok(json) = error.to_json() { ctx.text(json); }
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
            let ws_msg = Message::unsubscribe_response(&msg.session_id);
            if let Ok(json) = ws_msg.to_json() { ctx.text(json); }
        }
    }
}

/// 处理终端输出转发
impl Handler<TerminalOutput> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: TerminalOutput, ctx: &mut Self::Context) {
        ctx.text(msg.text);
    }
}

/// 处理外部推送消息（广播/定向发送）
impl Handler<SendTextMessage> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SendTextMessage, ctx: &mut Self::Context) {
        ctx.text(msg.text);
    }
}

// ==================== Output Buffer ====================

/// 输出缓冲区 — 累积多条 PTY 输出，减少 WS 消息数量
struct OutputBuffer {
    data: Vec<u8>,
    start_index: u64,
    last_is_waiting: bool,
}

impl OutputBuffer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            start_index: 0,
            last_is_waiting: false,
        }
    }

    fn append(&mut self, event: &crate::desktop::session::OutputEvent) {
        if self.data.is_empty() {
            self.start_index = event.index;
        }
        self.data.extend_from_slice(&event.data);
        self.last_is_waiting = event.is_waiting;
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flush 缓冲区为 WS 消息 JSON
    fn flush(&mut self, session_id: &str) -> String {
        let data_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &self.data,
        );
        let message = Message::output_from_base64(
            session_id,
            &data_base64,
            self.last_is_waiting,
            self.start_index as usize,
        );
        self.data.clear();
        message.to_json().unwrap_or_default()
    }
}
