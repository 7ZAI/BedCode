//! 事件通道处理器（`/ws/event`，旧 `Message` 协议兼容面）
//!
//! 通道语义：常驻事件通道——设备在线判定基准 + 同步/通知广播接收方。
//! 首消息 JWT 认证后：
//!
//! - `Message::Auth`：JWT 重新认证（活路径，移动端事件通道首消息）；
//! - `Message::Terminal`：输入（移动端 TUI 滚动经 `ws_send_input_async` 走此路径）
//!   与订阅/退订（`ws_join_session` 兼容面）；
//! - `Message::SessionControl`：会话控制（移动端会话列表等请求经此转发）。
//!
//! 移动端常驻连接即本通道（`/ws/event`），因此上述消息**不是死代码**（见
//! `.scratch/2026-09-18-ws-base-service/issues/01-dead-code-removal.md` Comments）。
//!
//! 连接生命周期（心跳 / 认证超时 / 帧级过滤链 / 注册表）全部由骨架
//! [`crate::server::websocket::conn`] 承担，本文件只实现通道协议。

use actix::prelude::*;

use super::super::conn::{AuthMode, ChannelHandler, ConnCtx, WsConnBase};
use crate::enums::{SessionControlPayload, TerminalPayload};
use crate::server::core::link_crypto;
use crate::server::websocket::message::Message;
use crate::system::app_context::AppContext;

/// 事件通道处理器（无自有状态：旧协议面全在骨架与订阅原语上）
pub struct EventChannel;

impl EventChannel {
    pub fn new() -> Self {
        Self
    }

    /// 处理文本消息（JSON 格式的 Message）
    fn handle_text_message(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        let message = match Message::from_json(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, addr = %conn.session.addr, "Failed to parse WS message");
                let error = Message::error("PARSE_ERROR", &e.to_string());
                if let Ok(json) = error.to_json() {
                    conn.send_text_filtered(json, ctx);
                }
                return;
            }
        };

        match message {
            Message::Auth {
                payload, message_id, ..
            } => {
                self.handle_auth(conn, payload, message_id, ctx);
            }
            Message::Terminal {
                session_id,
                payload,
                message_id,
                expect_response,
                ..
            } => {
                if !conn.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        conn.send_text_filtered(json, ctx);
                    }
                    // spec §4.3 拒绝对称：未认证连接发业务消息 → 回错误后关闭连接。
                    // 显式 close：仅 stop() 时 socket 要等下一个 heartbeat tick 才关闭
                    ctx.close(None);
                    ctx.stop();
                    return;
                }
                self.handle_terminal(conn, session_id, payload, message_id, expect_response, ctx);
            }
            Message::SessionControl {
                payload,
                message_id,
                expect_response,
                ..
            } => {
                if !conn.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        conn.send_text_filtered(json, ctx);
                    }
                    // spec §4.3 拒绝对称：未认证连接发业务消息 → 回错误后关闭连接。
                    // 显式 close：仅 stop() 时 socket 要等下一个 heartbeat tick 才关闭
                    ctx.close(None);
                    ctx.stop();
                    return;
                }
                self.handle_session_control(conn, payload, message_id, expect_response, ctx);
            }
            _ => {
                if !conn.session.authenticated {
                    // 未认证连接首条消息必须走 Auth 分派；其他消息类型
                    // 一律拒绝并关闭。无 message_id 的通知类消息用 Message::error
                    // （无 id），spec §4.3 拒绝对称
                    let error = Message::error("AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        conn.send_text_filtered(json, ctx);
                    }
                    ctx.stop();
                    return;
                }
                tracing::debug!(client = %conn.session.addr, "Unsupported WS message type");
            }
        }
    }

    /// 处理认证消息 — 根据阶段路由到不同处理器
    ///
    /// - Authenticated（JWT re-auth）→ 内联 JWT 验证（快速路径，无需异步）
    ///
    /// 旧 v2.0.0 客户端的 WS 配对认证阶段（RequestPairing / VerifyCode /
    /// QrConnect / ExchangeCertificate / Biometric*）已随 /ws/terminal 兼容路由
    /// 下线删除；配对统一走 HTTP /api/auth/*，WS 首消息仅接受 JWT（Reauthenticate）
    fn handle_auth(
        &mut self,
        conn: &mut WsConnBase,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ConnCtx,
    ) {
        match payload.stage {
            // JWT 重新认证：同步路径，直接验证 JWT token
            crate::enums::AuthStage::Authenticated | crate::enums::AuthStage::Reauthenticate => {
                self.handle_auth_jwt(conn, payload, message_id, ctx);
            }
            _ => {
                let error = Message::error_with_id(&message_id, "INVALID_AUTH_STAGE", "Unsupported auth stage");
                if let Ok(json) = error.to_json() {
                    conn.send_text_filtered(json, ctx);
                }
            }
        }
    }

    /// 处理 JWT 重新认证（快速同步路径）
    fn handle_auth_jwt(
        &mut self,
        conn: &mut WsConnBase,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ConnCtx,
    ) {
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                let error = Message::error_with_id(&message_id, "NO_TOKEN", "No JWT token provided");
                if let Ok(json) = error.to_json() {
                    conn.send_text_filtered(json, ctx);
                }
                // spec §4.3 拒绝对称：JWT 认证失败（缺 token）→ 回错误后关闭连接
                ctx.close(None);
                ctx.stop();
                return;
            }
        };

        match conn.authenticate_jwt(&token) {
            Ok(claims) => {
                // 链路加密协商（issue 04）：回执随 auth 响应明文下发，
                // 密码表注册在发送之后——此后的帧才进入加密模式
                let mut ws_handshake = None;
                if let Some(proposal) = conn.pending_ws_crypto.take() {
                    if link_crypto::current_config().enabled {
                        // 票 05 套件参数化：先按协商套件名解析（缺省→默认，未知名→拒绝告警），
                        // 成功后落审计日志（只记套件名，不含任何密钥材料），再做握手。
                        match link_crypto::resolve_link_suite(proposal.suite.as_deref()) {
                            Ok(suite) => match link_crypto::derive_ws_session_ciphers(&proposal.ek) {
                                Ok(hs) => {
                                    ws_handshake = Some(hs);
                                    tracing::info!(
                                        addr = %conn.session.addr,
                                        suite = suite.name,
                                        "ws link encryption negotiated (legacy route)"
                                    );
                                }
                                Err(e) => tracing::warn!(
                                    addr = %conn.session.addr,
                                    error = %e,
                                    "ws link crypto handshake failed, staying plaintext"
                                ),
                            },
                            Err(e) => tracing::warn!(
                                addr = %conn.session.addr,
                                error = %e,
                                "ws link crypto suite rejected, staying plaintext"
                            ),
                        }
                    }
                }
                let response = Message::Auth {
                    message_id,
                    expect_response: false,
                    session_id: None,
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    token: String::new(),
                    payload: crate::enums::AuthPayload {
                        stage: crate::enums::AuthStage::Authenticated,
                        device_id: conn.session.device_id.clone(),
                        device_name: conn.session.device_name.clone(),
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        crypto: ws_handshake.as_ref().map(|hs| crate::enums::auth::CryptoProposal {
                            v: 1,
                            ek: hs.server_ek_b64.clone(),
                            suite: None,
                        }),
                        ..Default::default()
                    },
                };
                if let Ok(json) = response.to_json() {
                    conn.send_text_filtered(json, ctx);
                }
                if let Some(hs) = ws_handshake {
                    link_crypto::ws_register_ciphers(&conn.session.addr.to_string(), hs.ciphers);
                }
            }
            Err((code, message)) => {
                let error = Message::error_with_id(&message_id, &code, &message);
                if let Ok(json) = error.to_json() {
                    conn.send_text_filtered(json, ctx);
                }
                // spec §4.3 拒绝对称：JWT 认证失败（无效/过期 token）→ 回错误后关闭连接
                ctx.close(None);
                ctx.stop();
            }
        }
    }

    /// 处理终端消息 — 路由到 input / subscribe / unsubscribe（旧 Message 协议）
    fn handle_terminal(
        &mut self,
        conn: &mut WsConnBase,
        session_id: String,
        payload: TerminalPayload,
        message_id: String,
        expect_response: bool,
        ctx: &mut ConnCtx,
    ) {
        match payload.action {
            crate::enums::TerminalAction::Input { data, special_key } => {
                actix::spawn(async move {
                    if let Err(e) = crate::server::websocket::services::terminal_service::handle_input(
                        &session_id,
                        TerminalPayload {
                            action: crate::enums::TerminalAction::Input { data, special_key },
                        },
                    )
                    .await
                    {
                        tracing::error!(session_id = %session_id, error = %e, "Terminal input error");
                    }
                });

                // 输入消息需要立即回复 Ack，避免移动端 send_and_wait 超时断开
                if expect_response {
                    let ack = Message::ack(&message_id);
                    if let Ok(json) = ack.to_json() {
                        conn.send_text_filtered(json, ctx);
                    }
                }
            }
            crate::enums::TerminalAction::Subscribe => {
                // 旧 Message 路由：回 subscribe_response + 登记 subscribed_sessions，
                // 不启用双速传播模式（无 SetMode 控制帧）
                conn.subscribe_output(session_id, None, Some(message_id), false, ctx);
            }
            crate::enums::TerminalAction::Unsubscribe => {
                conn.unsubscribe_output(session_id, message_id, ctx);
            }
            _ => {}
        }
    }

    /// 处理会话控制消息 — 路由到 session_control service
    fn handle_session_control(
        &mut self,
        conn: &mut WsConnBase,
        payload: SessionControlPayload,
        message_id: String,
        expect_response: bool,
        ctx: &mut ConnCtx,
    ) {
        let addr = conn.session.addr;
        let device_name = conn.session.device_name.clone();
        let actor_addr = ctx.address();
        // 无头/测试上下文可能无 AppHandle：handle_control_message 签名本身就是 Option，直接透传
        let app_handle = AppContext::global().app_handle().clone();

        actix::spawn(async move {
            let result = crate::server::websocket::services::session_control::handle_control_message(
                message_id.clone(),
                None, // session_id
                chrono::Utc::now().timestamp_millis(),
                payload.action,
                addr,
                device_name,
                app_handle,
            )
            .await;

            match result {
                Ok(Some(response_msg)) => {
                    if let Ok(json) = response_msg.to_json() {
                        let _ = actor_addr
                            .send(super::super::conn::SendTextMessage { text: json })
                            .await;
                    }
                }
                Ok(None) => {
                    // 无响应消息（如 fire-and-forget 的 ResizeSession）
                    if expect_response {
                        let ack = Message::ack(&message_id);
                        if let Ok(json) = ack.to_json() {
                            let _ = actor_addr
                                .send(super::super::conn::SendTextMessage { text: json })
                                .await;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "[EventChannel] Session control error");
                    let error = Message::error_with_id(&message_id, "SESSION_CONTROL_ERROR", &e.to_string());
                    if let Ok(json) = error.to_json() {
                        let _ = actor_addr
                            .send(super::super::conn::SendTextMessage { text: json })
                            .await;
                    }
                }
            }
        });
    }
}

impl ChannelHandler for EventChannel {
    /// 事件通道首消息 JWT 认证必需（spec §4.3）
    fn auth_mode(&self) -> AuthMode {
        AuthMode::Required
    }

    fn on_text(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        self.handle_text_message(conn, text, ctx);
    }

    fn on_binary(&mut self, conn: &mut WsConnBase, data: Vec<u8>, ctx: &mut ConnCtx) {
        // 事件通道的二进制入站同样只承载背压 ack 帧
        conn.handle_ack_binary(&data, ctx);
    }
}
