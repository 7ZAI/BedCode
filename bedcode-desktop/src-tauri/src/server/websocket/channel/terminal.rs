//! 终端通道处理器（`/ws/terminal/session/{id}`，spec §5.3 控制帧协议）
//!
//! 通道语义：连接创建即绑定 session_id，订阅即连接（无多路复用）；首消息
//! JWT 认证 → `auth_ok` → `subscribe` 快照订阅 → TB v3 二进制输出流；
//! `input` 直通 PTY。控制帧为简化协议（无 message_id / expect_response）。
//!
//! 连接生命周期（心跳 / 认证超时 / 帧级过滤链 / 注册表）全部由骨架
//! [`crate::server::websocket::conn`] 承担，本文件只实现通道协议。

use actix::prelude::*;
use std::any::Any;

use super::super::conn::{AuthMode, ChannelHandler, ChannelMessage, ConnCtx, WsConnBase};
use crate::enums::special_key::KeyCombo;
use crate::enums::{TerminalAction, TerminalPayload};
use crate::server::core::link_crypto;
use crate::server::websocket::terminal_ws::control_frame::{self, ServerFrame};
use crate::wasm_core::host_api::pty::broadcast_handle_for_session;

/// 新路由认证结果：认证通过后校验绑定会话是否存在（spec §5.1：
/// 不存在的会话 → 认证通过后 error(SESSION_NOT_FOUND) + 关闭）
struct SessionAuthOutcome {
    session_id: String,
    exists: bool,
}

/// 终端通道处理器
///
/// 只持有通道自有资源（会话停止监听任务）；其余状态（订阅链路、认证态、
/// 链路加密协商）在骨架中，经 `&mut WsConnBase` 访问。
pub struct TerminalChannel {
    /// 会话停止监听任务（终端路由）：bound 会话 Stopped 时推送 session_stopped 帧
    session_stopped_watcher: Option<tokio::task::JoinHandle<()>>,
}

impl TerminalChannel {
    pub fn new() -> Self {
        Self {
            session_stopped_watcher: None,
        }
    }

    /// 处理控制帧（`/ws/terminal/session/{id}`，spec §5.3）
    ///
    /// 连接级状态机：auth（首消息 JWT，未认证前拒绝一切业务帧并关闭）→
    /// subscribe（无参快照订阅）→ 输出流；input 直通 PTY。
    /// 拒绝对称（spec §4.3）：未认证发业务帧 → error(AUTH_REQUIRED) + 关闭
    fn handle_session_control_frame(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        let frame = match control_frame::parse_client_frame(&text) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(addr = %conn.session.addr, error = %e, "Failed to parse session control frame");
                let error = ServerFrame::Error {
                    code: "PARSE_ERROR".to_string(),
                    message: e,
                };
                conn.send_text_filtered(error.to_json(), ctx);
                return;
            }
        };

        // 未认证 + 业务帧 → 拒绝对称：error(AUTH_REQUIRED) + 关闭（require_session_auth 内部处理）
        if Self::should_reject_unauthenticated(&frame, conn.session.authenticated) {
            self.require_session_auth(conn, ctx);
            return;
        }

        match frame {
            control_frame::ClientFrame::Auth { token, crypto } => {
                // 幂等：已认证连接重复发 auth 直接忽略
                if conn.session.authenticated {
                    return;
                }
                conn.pending_ws_crypto = crypto;
                self.handle_session_auth(conn, token, ctx);
            }
            control_frame::ClientFrame::Subscribe { from_offset } => {
                if !self.require_session_auth(conn, ctx) {
                    return;
                }
                self.handle_session_subscribe(conn, from_offset, ctx);
            }
            control_frame::ClientFrame::SetMode { mode } => {
                if !self.require_session_auth(conn, ctx) {
                    return;
                }
                self.handle_session_mode(conn, mode, ctx);
            }
            control_frame::ClientFrame::Input { data, special_key } => {
                if !self.require_session_auth(conn, ctx) {
                    return;
                }
                self.handle_session_input(conn, data, special_key, ctx);
            }
        }
    }

    /// 控制帧认证需求判定（纯函数，供测试；分派契约的单一事实源）
    ///
    /// spec §4.3 拒绝对称：Auth 帧无需认证（首消息即认证握手），
    /// Subscribe/SetMode/Input 必须在认证后发送
    pub(crate) fn frame_needs_auth(frame: &control_frame::ClientFrame) -> bool {
        !matches!(frame, control_frame::ClientFrame::Auth { .. })
    }

    /// 未认证连接收到业务帧 → 应拒绝（纯函数，供测试）
    ///
    /// 拒绝语义 = 该帧需要认证 且 本连接尚未认证
    pub(crate) fn should_reject_unauthenticated(frame: &control_frame::ClientFrame, authenticated: bool) -> bool {
        Self::frame_needs_auth(frame) && !authenticated
    }

    /// 认证守卫：未认证 → error(AUTH_REQUIRED) + 关闭连接（spec §4.3 拒绝对称）
    ///
    /// 返回 false 表示连接已被关闭，调用方应立即返回
    fn require_session_auth(&mut self, conn: &mut WsConnBase, ctx: &mut ConnCtx) -> bool {
        if conn.session.authenticated {
            return true;
        }
        let error = ServerFrame::Error {
            code: "AUTH_REQUIRED".to_string(),
            message: "Please authenticate first".to_string(),
        };
        conn.send_text_filtered(error.to_json(), ctx);
        ctx.close(None);
        ctx.stop();
        false
    }

    /// 认证：JWT 验证（共享骨架 `authenticate_jwt` 核心）→
    /// 认证通过后校验绑定会话存在（spec §5.1：不存在 → error(SESSION_NOT_FOUND) + 关闭）
    fn handle_session_auth(&mut self, conn: &mut WsConnBase, token: String, ctx: &mut ConnCtx) {
        if token.is_empty() {
            let error = ServerFrame::Error {
                code: "NO_TOKEN".to_string(),
                message: "No JWT token provided".to_string(),
            };
            conn.send_text_filtered(error.to_json(), ctx);
            // spec §4.3 拒绝对称：JWT 认证失败（缺 token）→ 回错误后关闭连接
            ctx.close(None);
            ctx.stop();
            return;
        }

        match conn.authenticate_jwt(&token) {
            Ok(_) => {
                // 会话存在性校验放异步块（同步上下文不可 await）。票 11：**唯一真源
                // 是宿主 PTY 引擎的广播句柄**（会话输出环在引擎，票 05 声明 + 票 06
                // 直读）——内核 `has_session` 兜底腿随 `session/` 目录删除。
                // 加密协商回执与密码表注册延后到 SessionAuthOutcome（auth_ok 后生效）
                let session_id = conn.bound_session.clone().unwrap();
                let actor_addr = ctx.address();
                actix::spawn(async move {
                    let exists = broadcast_handle_for_session(&session_id).is_some();
                    let _ = actor_addr
                        .send(ChannelMessage(Box::new(SessionAuthOutcome { session_id, exists })))
                        .await;
                });
            }
            Err((code, message)) => {
                let error = ServerFrame::Error { code, message };
                conn.send_text_filtered(error.to_json(), ctx);
                // spec §4.3 拒绝对称：JWT 认证失败（无效/过期 token）→ 回错误后关闭连接。
                // 显式 close：仅 ctx.stop() 时 socket 要等下一个 heartbeat tick 才关闭
                // （测试实测延迟 5s，偶发更久），close 立即发 Close 帧并关闭 TCP
                ctx.close(None);
                ctx.stop();
            }
        }
    }

    /// 订阅绑定会话输出（spec §5.4「订阅即连接」）：控制帧 subscribe 可携带
    /// from_offset（字节锚点）→ 历史自该游标起播；重订阅（前端 offset 缺口自愈）
    /// 由订阅原语做代数递增 + abort 旧任务组，防双流
    fn handle_session_subscribe(&mut self, conn: &mut WsConnBase, from_offset: Option<u64>, ctx: &mut ConnCtx) {
        let Some(session_id) = conn.bound_session.clone() else {
            return;
        };
        // 终端路由：启用双速传播模式（SetMode 实时切换），回执为 subscribe_ok 控制帧
        conn.subscribe_output(session_id, from_offset, None, true, ctx);
    }

    /// 传播模式切换（双速，用户需求 3）：更新绑定会话订阅者的 mode 原子，
    /// 订阅者执行体每次循环读取实现即时生效；未订阅（连接建立后未发 subscribe）
    /// 时忽略（订阅时统一重置为 realtime）
    fn handle_session_mode(&mut self, conn: &mut WsConnBase, mode: control_frame::WatchMode, _ctx: &mut ConnCtx) {
        let Some(session_id) = conn.bound_session.clone() else {
            return;
        };
        let client_id = conn.session.addr.to_string();
        let fwd_key = format!("{client_id}:{session_id}");
        if conn.set_output_mode(&session_id, mode.as_u8()) {
            tracing::debug!(fwd_key = %fwd_key, mode = ?mode, "terminal propagate mode updated");
        } else {
            tracing::debug!(fwd_key = %fwd_key, mode = ?mode, "SetMode before subscribe, ignored");
        }
    }

    /// 输入：控制帧 input → PTY（data 为 Base64，与旧路由 wire 一致）
    fn handle_session_input(
        &mut self,
        conn: &mut WsConnBase,
        data: String,
        special_key: Option<KeyCombo>,
        _ctx: &mut ConnCtx,
    ) {
        // 会话绑定缺失（异常时序：认证通过后会话被销毁）时不得 panic——actix
        // 任务内 panic 会中断该连接的后续处理且无用户可见反馈，这里记日志丢弃
        let Some(session_id) = conn.bound_session.clone() else {
            tracing::warn!(
                addr = %conn.session.addr,
                "terminal input dropped: session binding missing"
            );
            return;
        };
        actix::spawn(async move {
            // wire 协议：新路由 input 的 data 为 Base64（UTF-8 → Standard，移动端
            // btoa/TextEncoder 编码）。handle_input → write_input 全链路按明文透传，
            // 此处先解码，避免把 base64 字符串原样写入 PTY（修复：快捷命令 /new 被
            // 回显为 L25ldw==）。解码失败按明文透传，兼容误用此路由的明文客户端
            let data = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    tracing::warn!(
                        session_id = %session_id, error = %e, data = %data,
                        "input data 非合法 Base64，按明文透传"
                    );
                    data
                }
            };
            let payload = TerminalPayload {
                action: TerminalAction::Input { data, special_key },
            };
            if let Err(e) =
                crate::server::websocket::services::terminal_service::handle_input(&session_id, payload).await
            {
                tracing::error!(session_id = %session_id, error = %e, "Terminal input error");
            }
        });
    }

    /// 处理认证结果：会话存在 → auth_ok；不存在 → error(SESSION_NOT_FOUND) + 关闭
    fn handle_auth_outcome(&mut self, conn: &mut WsConnBase, msg: SessionAuthOutcome, ctx: &mut ConnCtx) {
        if msg.exists {
            // 加密协商回执：auth_ok 本身保持明文（客户端需先读到服务端临时公钥
            // 才能派生密钥），注册在发送之后——此后的所有帧进入加密模式
            let mut handshake = None;
            if let Some(proposal) = conn.pending_ws_crypto.take() {
                if link_crypto::current_config().enabled {
                    // 票 05 套件参数化：先按协商套件名解析（缺省→默认，未知名→拒绝告警），
                    // 成功后落审计日志（只记套件名，不含任何密钥材料），再做握手。
                    match link_crypto::resolve_link_suite(proposal.suite.as_deref()) {
                        Ok(suite) => {
                            match link_crypto::derive_ws_session_ciphers(&proposal.ek) {
                                Ok(hs) => {
                                    handshake = Some(hs);
                                    tracing::info!(
                                        addr = %conn.session.addr,
                                        suite = suite.name,
                                        "ws link encryption negotiated, frames encrypted from now on"
                                    );
                                }
                                Err(e) => tracing::warn!(addr = %conn.session.addr, error = %e, "ws link crypto handshake failed, staying plaintext"),
                            }
                        }
                        Err(e) => {
                            // 未知套件名：fail-visible，拒绝此协商（不静默降级到默认套件）
                            tracing::warn!(
                                addr = %conn.session.addr,
                                error = %e,
                                "ws link crypto suite rejected, staying plaintext"
                            );
                        }
                    }
                }
            }
            let frame = ServerFrame::AuthOk {
                crypto: handshake.as_ref().map(|hs| control_frame::CryptoEcho {
                    v: 1,
                    ek: hs.server_ek_b64.clone(),
                }),
            };
            conn.send_text_filtered(frame.to_json(), ctx);
            if let Some(hs) = handshake {
                link_crypto::ws_register_ciphers(&conn.session.addr.to_string(), hs.ciphers);
            }
        } else {
            tracing::warn!(
                addr = %conn.session.addr,
                session_id = %msg.session_id,
                "Session WS rejected: session not found after auth"
            );
            let frame = ServerFrame::Error {
                code: "SESSION_NOT_FOUND".to_string(),
                message: format!("Session {} not found", msg.session_id),
            };
            conn.send_text_filtered(frame.to_json(), ctx);
            // spec §5.1：会话不存在 → 认证通过后 error + 关闭
            ctx.close(None);
            ctx.stop();
        }
    }
}

impl ChannelHandler for TerminalChannel {
    /// 终端路由首消息 JWT 认证必需（spec §4.3）
    fn auth_mode(&self) -> AuthMode {
        AuthMode::Required
    }

    fn on_started(&mut self, conn: &mut WsConnBase, ctx: &mut ConnCtx) {
        // 终端路由：监听绑定会话的停止事件，主动推送 session_stopped 帧
        // （会话停止后不再有输出，前端据此提示并断开，避免悬挂等待）
        if conn.bound_session.is_some() {
            // 票 11：会话停止通知**只有一条来路**——引擎订阅者在宽限排空后发
            // `ForwardOutput::SessionStopped`（尾帧先行、顺序保证）。原先「无广播声明的
            // 旧内核会话」走内核 status 兜底 watcher 的分支随 `session/` 目录删除：
            // 该分支的前提（宿主内核还能创建会话）已不存在，会话一律由插件经
            // `host-pty` 创建并声明广播。
        }
    }

    fn on_text(&mut self, conn: &mut WsConnBase, text: String, ctx: &mut ConnCtx) {
        self.handle_session_control_frame(conn, text, ctx);
    }

    fn on_binary(&mut self, conn: &mut WsConnBase, data: Vec<u8>, ctx: &mut ConnCtx) {
        // 终端通道的二进制入站 = 客户端背压 ack 帧
        conn.handle_ack_binary(&data, ctx);
    }

    fn on_channel_msg(&mut self, conn: &mut WsConnBase, msg: Box<dyn Any + Send>, ctx: &mut ConnCtx) {
        match msg.downcast::<SessionAuthOutcome>() {
            Ok(outcome) => self.handle_auth_outcome(conn, *outcome, ctx),
            Err(_) => tracing::warn!("[TerminalChannel] unknown channel message dropped"),
        }
    }

    fn on_close(&mut self, _conn: &mut WsConnBase) {
        // 中止会话停止监听任务：连接已断开，通知不再需要
        if let Some(handle) = self.session_stopped_watcher.take() {
            handle.abort();
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 分派契约（handle_session_control_frame 守卫） ====================

    #[test]
    fn frame_needs_auth_only_business_frames() {
        // Auth 帧无需认证（首消息即认证握手）
        let auth = control_frame::ClientFrame::Auth {
            token: "jwt".to_string(),
            crypto: None,
        };
        assert!(!TerminalChannel::frame_needs_auth(&auth), "auth 帧不应要求已认证");

        // 业务帧（subscribe / set_mode / input）必须认证后发送
        let subscribe = control_frame::ClientFrame::Subscribe { from_offset: None };
        let set_mode = control_frame::ClientFrame::SetMode {
            mode: control_frame::WatchMode::Realtime,
        };
        let input = control_frame::ClientFrame::Input {
            data: "aGk=".to_string(),
            special_key: None,
        };
        for frame in [&subscribe, &set_mode, &input] {
            assert!(TerminalChannel::frame_needs_auth(frame), "业务帧必须要求已认证");
        }
    }

    #[test]
    fn should_reject_unauthenticated_only_when_both_conditions_hold() {
        let subscribe = control_frame::ClientFrame::Subscribe { from_offset: None };
        let auth = control_frame::ClientFrame::Auth {
            token: "jwt".to_string(),
            crypto: None,
        };

        // 未认证 + 业务帧 → 拒绝（拒绝对称 spec §4.3）
        assert!(TerminalChannel::should_reject_unauthenticated(&subscribe, false));
        // 未认证 + auth 帧 → 放行（认证握手本身）
        assert!(!TerminalChannel::should_reject_unauthenticated(&auth, false));
        // 已认证 + 业务帧 → 放行
        assert!(!TerminalChannel::should_reject_unauthenticated(&subscribe, true));
        // 已认证 + auth 帧 → 放行（幂等忽略由 handle_session_control_frame 处理）
        assert!(!TerminalChannel::should_reject_unauthenticated(&auth, true));
    }
}
