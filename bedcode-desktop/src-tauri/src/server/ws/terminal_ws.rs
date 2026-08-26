//! Terminal WebSocket Actor
//!
//! 处理终端 I/O 的 WebSocket 连接
//! 使用 actix-web-actors 的 WS actor 模式

use actix::prelude::*;
use actix_web_actors::ws;
use actix_web_actors::ws::{Message as WsMessage, ProtocolError};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tauri::Emitter;

use crate::enums::{SessionControlPayload, SubscribeMode, TerminalPayload};
use crate::server::filter::{Direction, FilterContext, TrafficChannel, TrafficFilterChain};
use crate::server::link_crypto;
use crate::server::message::Message;
use crate::server::ws::registry::{ChannelType, WsSessionRegistry};
use crate::server::ws::session::WsSession;
use crate::session::{GlobalOutputManager, OutputFrame, RendererSource, SessionStatus};
use crate::system::app_context::AppContext;
use crate::system::config::AppConfig;
use crate::system::constants::event;
use crate::system::constants::server::{
    CLIENT_TIMEOUT_SECS, HEARTBEAT_INTERVAL_SECS, REMOTE_CLIENT_TIMEOUT_SECS, WS_AUTH_TIMEOUT_SECS,
};
use crate::utils::auth::jwt::JwtService;
use control_frame::ServerFrame;

mod control_frame;

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

/// 新路由（/ws/terminal/session/{id}）订阅结果：连接绑定单会话，
/// 无需 message_id（控制帧协议无请求-响应机制，spec §5.3）
#[derive(Message)]
#[rtype(result = "()")]
struct SessionSubscribeOutcome {
    session_id: String,
    result: Option<crate::session::SubscribeResponse>,
}

/// 新路由认证结果：认证通过后校验绑定会话是否存在（spec §5.1：
/// 不存在的会话 → 认证通过后 error(SESSION_NOT_FOUND) + 关闭）
#[derive(Message)]
#[rtype(result = "()")]
struct SessionAuthOutcome {
    session_id: String,
    exists: bool,
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

/// 终端输出消息（从输出转发任务传回，二进制帧形态，供桌面端本地 WS 使用）
/// `data` 为已编码的完整帧（含 16 字节 TB v2 帧头），直接 ctx.binary 发送
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutputBinary {
    data: Vec<u8>,
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
    /// 绑定会话（新路由 /ws/terminal/session/{id}）：连接创建即绑定，
    /// 订阅即连接、无多路复用；None = 本地环回 /ws/terminal/local
    /// （无预绑定会话，经 subscribe 控制帧订阅）
    bound_session: Option<String>,
    /// 会话停止监听任务（新路由）：bound 会话 Stopped 时推送 session_stopped 帧
    session_stopped_watcher: Option<tokio::task::JoinHandle<()>>,
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
    /// 输出流代数（key = `client_id:session_id` → AtomicU64）
    ///
    /// 订阅 / 取消订阅 / 断连时递增；forward_loop 每次转发前校验代数，
    /// 旧代 forward_loop 的残留帧（abort 异步取消窗口内已投递到 actor 邮箱
    /// 的帧）直接丢弃——与 abort 互补，杜绝旧流帧注入新订阅通道（移动端
    /// 字节游标错位 → 连续性违反 → 重订阅风暴的根源）
    stream_generations: std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU64>>,
    /// 通道类型（注册时定死）：终端 I/O 路由 → Terminal；事件路由（ticket 02）→ Event
    channel_type: ChannelType,
    /// 链路加密待处理协商（issue 04）：auth 帧携带的客户端临时公钥，
    /// 认证成功后派生并回执（新路由在 SessionAuthOutcome 消费，旧路由同步消费）
    pending_ws_crypto: Option<crate::enums::auth::CryptoProposal>,
}

impl TerminalWs {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            session: WsSession::new(addr),
            hb: Instant::now(),
            local: false,
            bound_session: None,
            session_stopped_watcher: None,
            output_forwarders: std::collections::HashMap::new(),
            subscribe_tasks: std::collections::HashMap::new(),
            stream_generations: std::collections::HashMap::new(),
            channel_type: ChannelType::Terminal,
            pending_ws_crypto: None,
        }
    }

    /// 每会话终端路由构造（spec §5.1）：连接创建即绑定 session_id，
    /// 订阅即连接（无 subscribed_sessions 多路复用），控制帧走简化
    /// JSON 协议（无 message_id/expect_response），输出帧为 TB v2 二进制
    pub fn new_for_session(addr: SocketAddr, session_id: String) -> Self {
        let mut ws = Self::new(addr);
        ws.bound_session = Some(session_id);
        ws
    }

    /// 本地环回通道：直接标记已认证，跳过配对/JWT 流程
    /// （路由层已校验 peer 为环回地址，见 server/app.rs local_terminal_ws）
    pub fn new_local(addr: SocketAddr) -> Self {
        // 本地环回也承载终端 I/O，通道类型保持 Terminal（new() 默认值）
        let mut ws = Self::new(addr);
        ws.session.authenticated = true;
        ws.local = true;
        ws
    }

    /// 事件通道构造：设备在线判定基准 + 同步广播接收方
    ///
    /// channel_type 在注册时定死（Event），广播过滤与 stopping() 的离线
    /// 判定都依赖它；事件通道不承载终端 I/O，其余构造体保持默认
    pub fn new_event(addr: SocketAddr) -> Self {
        let mut ws = Self::new(addr);
        ws.channel_type = ChannelType::Event;
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
                ctx.close(None);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    // ==================== Traffic Filter Hooks（流量过滤责任链接线） ====================

    /// 本连接对应的流量通道类型（本地环回优先于通道类型判断）
    fn traffic_channel(&self) -> TrafficChannel {
        if self.local {
            TrafficChannel::WsLocal
        } else {
            match self.channel_type {
                ChannelType::Terminal => TrafficChannel::WsTerminal,
                ChannelType::Event => TrafficChannel::WsEvent,
            }
        }
    }

    /// 链路加密失败收尾：Close(4003) 并停止 actor（spec：WS 解密失败不丢帧，
    /// TBv2 序列流丢帧会破坏 ack 环与渲染序，必须断连重建）
    fn close_link_crypto_failure(&self, reason: String, ctx: &mut ws::WebsocketContext<Self>) {
        tracing::warn!(addr = %self.session.addr, %reason, "link crypto failure, closing 4003");
        ctx.close(Some(ws::CloseReason {
            code: ws::CloseCode::Other(4003),
            description: Some(reason),
        }));
        ctx.stop();
    }

    /// 入站帧过滤：None = 被拒（已关连接），调用方应立即返回
    fn filter_inbound_data(
        &self,
        data: Vec<u8>,
        kind: &'static str,
        ctx: &mut ws::WebsocketContext<Self>,
    ) -> Option<Vec<u8>> {
        let chain = TrafficFilterChain::global();
        if chain.is_empty() {
            return Some(data);
        }
        let peer = self.session.addr.to_string();
        let mut fctx = FilterContext {
            channel: self.traffic_channel(),
            direction: Direction::Inbound,
            peer: &peer,
            route: kind,
            negotiation: "",
            data,
        };
        match chain.run_inbound(&mut fctx) {
            Ok(()) => Some(fctx.data),
            Err(rej) => {
                tracing::warn!(
                    addr = %peer,
                    channel = self.traffic_channel().as_str(),
                    frame = kind,
                    %rej,
                    "WS inbound frame rejected by traffic filter"
                );
                self.close_link_crypto_failure(rej.to_string(), ctx);
                None
            }
        }
    }

    /// 入站文本帧过滤（JSON 控制帧 / 业务消息）
    fn filter_inbound_text(
        &self,
        text: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) -> Option<String> {
        self.filter_inbound_data(text.into_bytes(), "text", ctx)
            .map(|data| String::from_utf8_lossy(&data).into_owned())
    }

    /// 出站文本帧：经过滤链后写出（含 metrics 计数）；被拒 → 丢弃 + warn。
    /// 全部业务文本帧的唯一写出口，广播/推送经 Handler<SendTextMessage> 汇入
    fn send_text_filtered(&self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
        let chain = TrafficFilterChain::global();
        if chain.is_empty() {
            crate::server::metrics::MetricsCollector::global().inc_ws_sent();
            ctx.text(text);
            return;
        }
        let peer = self.session.addr.to_string();
        let mut fctx = FilterContext {
            channel: self.traffic_channel(),
            direction: Direction::Outbound,
            peer: &peer,
            route: "text",
            negotiation: "",
            data: text.into_bytes(),
        };
        match chain.run_outbound(&mut fctx) {
            Ok(()) => {
                crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                ctx.text(String::from_utf8_lossy(&fctx.data).into_owned());
            }
            Err(rej) => {
                tracing::warn!(
                    addr = %peer,
                    channel = self.traffic_channel().as_str(),
                    %rej,
                    "WS outbound text frame rejected by traffic filter"
                );
                self.close_link_crypto_failure(rej.to_string(), ctx);
            }
        }
    }

    /// 出站二进制帧：经过滤链后写出（含 metrics 计数）；被拒 → 丢弃 + warn。
    /// 承载 TB v2 终端输出流（spec §5.3），加密过滤器的主战场
    fn send_binary_filtered(&self, data: Vec<u8>, ctx: &mut ws::WebsocketContext<Self>) {
        let chain = TrafficFilterChain::global();
        if chain.is_empty() {
            crate::server::metrics::MetricsCollector::global().inc_ws_sent();
            ctx.binary(data);
            return;
        }
        let peer = self.session.addr.to_string();
        let mut fctx = FilterContext {
            channel: self.traffic_channel(),
            direction: Direction::Outbound,
            peer: &peer,
            route: "binary",
            negotiation: "",
            data,
        };
        match chain.run_outbound(&mut fctx) {
            Ok(()) => {
                crate::server::metrics::MetricsCollector::global().inc_ws_sent();
                ctx.binary(fctx.data);
            }
            Err(rej) => {
                tracing::warn!(
                    addr = %peer,
                    channel = self.traffic_channel().as_str(),
                    %rej,
                    "WS outbound binary frame rejected by traffic filter"
                );
                self.close_link_crypto_failure(rej.to_string(), ctx);
            }
        }
    }
}

impl Actor for TerminalWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("Terminal WS connected: {}", self.session.addr);
        self.start_heartbeat(ctx);

        // 首消息认证超时：连接建立后 10s 内未完成认证（JWT 或配对流程）→
        // 服务端主动关闭（spec §4.3「10s 未完成首消息认证」）。local 通道
        // 构造时已标记 authenticated，此闭包自动 no-op，无需特判
        let auth_timeout = Duration::from_secs(WS_AUTH_TIMEOUT_SECS);
        ctx.run_later(auth_timeout, |act, ctx| {
            if !act.session.authenticated {
                tracing::warn!(
                    addr = %act.session.addr,
                    "WS auth timeout: no first-message auth within {}s",
                    WS_AUTH_TIMEOUT_SECS
                );
                ctx.stop();
            }
        });

        // 注册到 WsSessionRegistry（携带通道类型：广播过滤与在线判定依据）
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();
        let socket_addr = self.session.addr;
        let channel_type = self.channel_type;
        actix::spawn(async move {
            use crate::server::ws::registry::WsSessionRegistry;
            let registry = WsSessionRegistry::global();
            registry.register(client_id, socket_addr, addr, channel_type).await;
        });

        // 新路由：监听绑定会话的停止事件，主动推送 session_stopped 帧
        // （会话停止后不再有输出，前端据此提示并断开，避免悬挂等待）
        if let Some(session_id) = self.bound_session.clone() {
            let addr = ctx.address();
            let handle = tokio::spawn(async move {
                let app_ctx = AppContext::global();
                let session_manager = app_ctx.session_manager();
                let mut rx = session_manager.subscribe_status();
                while let Ok(event) = rx.recv().await {
                    if event.session_id == session_id && matches!(event.new_status, SessionStatus::Stopped) {
                        let frame = ServerFrame::SessionStopped {
                            session_id: session_id.clone(),
                        };
                        if addr.send(SendTextMessage { text: frame.to_json() }).await.is_err() {
                            break;
                        }
                    }
                }
            });
            self.session_stopped_watcher = Some(handle);
        }
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::info!("Terminal WS disconnected: {}", self.session.addr);

        // 中止会话停止监听任务：连接已断开，通知不再需要
        if let Some(handle) = self.session_stopped_watcher.take() {
            handle.abort();
        }

        // 中止所有输出转发任务：连接已断开，残留缓冲帧不再需要投递
        for (_, handle) in self.output_forwarders.drain() {
            handle.abort();
        }
        // 中止所有订阅任务：连接已断开，旧任务的历史发送不再需要（其
        // 占位订阅者残留也会随断连清理移除）
        for (_, handle) in self.subscribe_tasks.drain() {
            handle.abort();
        }
        // 流代数全部失效：abort 异步取消窗口内仍可能发出的残留帧直接丢弃
        for (_, gen) in self.stream_generations.drain() {
            gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        // 注销 WsSessionRegistry + 取消所有订阅 + 清理对端文件服务记录。
        // 离线判定（DEVICE_DISCONNECTED + 连接历史回填）迁入 async 块：
        // 需要先 unregister 再按「断开后剩余连接数」判定，见注入逻辑
        let client_id = self.session.addr.to_string();
        let sessions: Vec<String> = self.session.subscribed_sessions.iter().cloned().collect();
        // 新路由：绑定单会话、无 subscribed_sessions 集合，但 subscribe 时
        // 已按 client_id 注册占位订阅者——断连必须整体清理，否则泄漏
        let bound_session = self.bound_session.is_some();
        let device_id = self.session.device_id.clone();
        let fingerprint = self.session.fingerprint.clone();
        let channel_type = self.channel_type;
        let addr = self.session.addr.to_string();
        let device_name = self.session.device_name.clone();
        actix::spawn(async move {
            let app_ctx = crate::system::app_context::AppContext::global();
            let registry = WsSessionRegistry::global();
            // 先注销本连接：后续计数判定基于「断开后」的剩余连接，本连接不再计入
            registry.unregister(&client_id).await;

            // spec §4.2 在线语义（指纹键控）：设备在线 ⇔ 至少一条已认证事件 WS 存活。
            // 最后一条事件通道断开 → DEVICE_DISCONNECTED + 连接历史回填。
            // R1 回退（旧 v2.0.0 客户端无事件通道）：终端通道断开时，仅当该设备
            // 「事件连接与终端连接均为零」才判定离线——纯终端形态的设备也正确下线
            let is_offline = match (&device_id, fingerprint.as_deref()) {
                (Some(device_id), Some(fp)) => {
                    let event_count = registry.event_connection_count(fp).await;
                    let terminal_count = registry.terminal_connection_count(fp).await;
                    let offline = match channel_type {
                        ChannelType::Event => event_count == 0,
                        ChannelType::Terminal => event_count == 0 && terminal_count == 0,
                    };
                    if offline {
                        // 通知前端设备下线（与 DEVICE_CONNECTED 对称）；无头/测试
                        // 上下文无 AppHandle：跳过（保持 let _ 丢弃错误语义）
                        if let Some(handle) = app_ctx.app_handle() {
                            let _ = handle.emit(
                                crate::system::constants::event::DEVICE_DISCONNECTED,
                                &crate::server::connection_types::DeviceConnectionEvent {
                                    addr: addr.clone(),
                                    device_id: device_id.clone(),
                                    device_name,
                                    fingerprint: Some(fp.to_string()),
                                    event: "disconnected".to_string(),
                                },
                            );
                        }
                        // 回填连接历史断开时间
                        let db_guard = app_ctx.db().lock().await;
                        if let Err(e) = db_guard.close_open_connection_event(device_id) {
                            tracing::warn!(device_id = %device_id, error = %e, "Failed to close connection history");
                        }
                    }
                    offline
                }
                // 未认证连接（如被拒后关闭）从未在线：不触发离线语义
                _ => false,
            };
            tracing::debug!(
                addr = %addr, channel = ?channel_type, offline = is_offline,
                "WS disconnect online-state decision"
            );

            // 断连清理：清除该连接的生物认证挑战值（ticket 01 起按键为
            // fingerprint；addr 键已是空操作，改用指纹键精确清理）
            if let Some(fp) = fingerprint {
                app_ctx.biometric_challenges().clear(&fp).await;
            }

            // 断连清理：链路加密密码表（issue 04）——必须在连接标识失效前移除
            link_crypto::ws_remove_ciphers(&client_id);

            // 取消所有订阅
            let global_manager = GlobalOutputManager::global();
            for session_id in sessions {
                global_manager.unsubscribe(&session_id, &client_id).await;
            }
            if bound_session {
                global_manager.unsubscribe_all_for_client(&client_id).await;
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
                ctx.close(None);
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
                // 入站先过流量过滤链（解密/审计）；被拒即链路加密失败 → 已 Close 4003
                let Some(text) = self.filter_inbound_text(text.to_string(), ctx) else {
                    return;
                };
                self.handle_text_message(text, ctx);
            }
            WsMessage::Binary(data) => {
                crate::server::metrics::MetricsCollector::global().inc_ws_received();
                // 入站先过流量过滤链，被拒即链路加密失败 → 已 Close 4003（不丢帧续跑）
                let Some(data) = self.filter_inbound_data(data.to_vec(), "binary", ctx) else {
                    return;
                };
                self.handle_ack_binary(&data, ctx);
            }
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
        // 新路由（绑定单会话）：简化 JSON 控制帧协议（spec §5.3），
        // 无 message_id/expect_response——与旧路由的 Message 枚举互不相干
        if self.bound_session.is_some() {
            self.handle_session_control_frame(text, ctx);
            return;
        }

        let message = match Message::from_json(&text) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, addr = %self.session.addr, "Failed to parse WS message");
                let error = Message::error("PARSE_ERROR", &e.to_string());
                if let Ok(json) = error.to_json() {
                    self.send_text_filtered(json, ctx);
                }
                return;
            }
        };

        match message {
            Message::Auth {
                payload, message_id, ..
            } => {
                self.handle_auth(payload, message_id, ctx);
            }
            Message::Terminal {
                session_id,
                payload,
                message_id,
                expect_response,
                ..
            } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        self.send_text_filtered(json, ctx);
                    }
                    // spec §4.3 拒绝对称：未认证连接发业务消息 → 回错误后关闭连接。
                    // 显式 close：仅 stop() 时 socket 要等下一个 heartbeat tick 才关闭
                    ctx.close(None);
                    ctx.stop();
                    return;
                }
                self.handle_terminal(session_id, payload, message_id, expect_response, ctx);
            }
            Message::SessionControl {
                payload,
                message_id,
                expect_response,
                ..
            } => {
                if !self.session.authenticated {
                    let error = Message::error_with_id(&message_id, "AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        self.send_text_filtered(json, ctx);
                    }
                    // spec §4.3 拒绝对称：未认证连接发业务消息 → 回错误后关闭连接。
                    // 显式 close：仅 stop() 时 socket 要等下一个 heartbeat tick 才关闭
                    ctx.close(None);
                    ctx.stop();
                    return;
                }
                self.handle_session_control(payload, message_id, expect_response, ctx);
            }
            _ => {
                if !self.session.authenticated {
                    // 未认证连接首条消息必须走 Auth 分派；其他消息类型（SessionConfig
                    // 等）一律拒绝并关闭。无 message_id 的通知类消息用 Message::error
                    // （无 id），spec §4.3 拒绝对称
                    let error = Message::error("AUTH_REQUIRED", "Please authenticate first");
                    if let Ok(json) = error.to_json() {
                        self.send_text_filtered(json, ctx);
                    }
                    ctx.stop();
                    return;
                }
                tracing::debug!("Unsupported WS message type from {}", self.session.addr);
            }
        }
    }

    /// 处理新路由控制帧（/ws/terminal/session/{id}，spec §5.3）
    ///
    /// 连接级状态机：auth（首消息 JWT，未认证前拒绝一切业务帧并关闭）→
    /// subscribe（无参快照订阅）→ 输出流；input 直通 PTY。
    /// 拒绝对称（spec §4.3）：未认证发业务帧 → error(AUTH_REQUIRED) + 关闭
    fn handle_session_control_frame(&mut self, text: String, ctx: &mut ws::WebsocketContext<Self>) {
        let frame = match control_frame::parse_client_frame(&text) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(addr = %self.session.addr, error = %e, "Failed to parse session control frame");
                let error = ServerFrame::Error {
                    code: "PARSE_ERROR".to_string(),
                    message: e,
                };
                self.send_text_filtered(error.to_json(), ctx);
                return;
            }
        };

        match frame {
            control_frame::ClientFrame::Auth { token, crypto } => {
                // 幂等：已认证连接重复发 auth 直接忽略
                if self.session.authenticated {
                    return;
                }
                self.pending_ws_crypto = crypto;
                self.handle_session_auth(token, ctx);
            }
            control_frame::ClientFrame::Subscribe => {
                if !self.require_session_auth(ctx) {
                    return;
                }
                self.handle_session_subscribe(ctx);
            }
            control_frame::ClientFrame::Input { data, special_key } => {
                if !self.require_session_auth(ctx) {
                    return;
                }
                self.handle_session_input(data, special_key, ctx);
            }
        }
    }

    /// 新路由认证守卫：未认证 → error(AUTH_REQUIRED) + 关闭连接（spec §4.3 拒绝对称）
    ///
    /// 返回 false 表示连接已被关闭，调用方应立即返回
    fn require_session_auth(&mut self, ctx: &mut ws::WebsocketContext<Self>) -> bool {
        if self.session.authenticated {
            return true;
        }
        let error = ServerFrame::Error {
            code: "AUTH_REQUIRED".to_string(),
            message: "Please authenticate first".to_string(),
        };
        self.send_text_filtered(error.to_json(), ctx);
        ctx.close(None);
        ctx.stop();
        false
    }

    /// 新路由认证：JWT 验证（与旧路由共享 `authenticate_jwt` 核心）→
    /// 认证通过后校验绑定会话存在（spec §5.1：不存在 → error(SESSION_NOT_FOUND) + 关闭）
    fn handle_session_auth(&mut self, token: String, ctx: &mut ws::WebsocketContext<Self>) {
        if token.is_empty() {
            let error = ServerFrame::Error {
                code: "NO_TOKEN".to_string(),
                message: "No JWT token provided".to_string(),
            };
            self.send_text_filtered(error.to_json(), ctx);
            // spec §4.3 拒绝对称：JWT 认证失败（缺 token）→ 回错误后关闭连接
            ctx.close(None);
            ctx.stop();
            return;
        }

        match self.authenticate_jwt(&token) {
            Ok(_) => {
                // 会话存在性校验放异步块：has_session 需持 GlobalOutputManager 锁；
                // 加密协商回执与密码表注册延后到 SessionAuthOutcome（auth_ok 发出后生效）
                let session_id = self.bound_session.clone().unwrap();
                let actor_addr = ctx.address();
                actix::spawn(async move {
                    let exists = GlobalOutputManager::global().has_session(&session_id).await;
                    let _ = actor_addr.send(SessionAuthOutcome { session_id, exists }).await;
                });
            }
            Err((code, message)) => {
                let error = ServerFrame::Error { code, message };
                self.send_text_filtered(error.to_json(), ctx);
                // spec §4.3 拒绝对称：JWT 认证失败（无效/过期 token）→ 回错误后关闭连接。
                // 显式 close：仅 ctx.stop() 时 socket 要等下一个 heartbeat tick 才关闭
                // （测试实测延迟 5s，偶发更久），close 立即发 Close 帧并关闭 TCP
                ctx.close(None);
                ctx.stop();
            }
        }
    }

    /// 新路由订阅：绑定单会话直接订阅，无多路复用（spec §5.1「订阅即连接」）
    ///
    /// - 每会话 mpsc 容量 32768（spec §5.4 D7：远程通道背压余量，旧路由保持 8192）
    /// - subscribe_ok 经 oneshot 前置返回（05 模式：不被历史发送背压阻塞）
    /// - forward_loop 输出 TB v2 二进制帧（spec §5.3），合并策略 30ms/64KB
    /// - 重订阅（前端 seq 缺口自愈）abort 旧 forwarder + 流代数递增，防双流
    fn handle_session_subscribe(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        let session_id = self.bound_session.clone().unwrap();
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let addr = ctx.address();

        // 替换订阅者前先中止旧转发任务（残留帧会与新生订阅流交错，
        // 前端 seq 游标错位 → 重订阅风暴自持循环），语义与旧路由一致
        let fwd_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.output_forwarders.remove(&fwd_key) {
            tracing::debug!("[TerminalWs] Aborting previous output forwarder: {}", fwd_key);
            prev.abort();
        }
        let sub_key = format!("{}:{}", client_id, session_id);
        if let Some(prev) = self.subscribe_tasks.remove(&sub_key) {
            tracing::debug!("[TerminalWs] Aborting previous subscribe task: {}", sub_key);
            prev.abort();
        }
        let generation = self
            .stream_generations
            .entry(fwd_key.clone())
            .or_insert_with(|| std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)))
            .clone();
        let my_gen = generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

        // 每会话独立输出通道（spec §5.4）：容量 32768，大历史重播 + 实时
        // 并发到达时留足缓冲余量；on_output 保持 try_send 背压丢弃
        let (output_tx, output_rx) = tokio::sync::mpsc::channel::<OutputFrame>(32768);

        let session_id_for_sub = session_id.clone();
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        let addr_for_resp = addr.clone();
        let session_id_for_resp = session_id.clone();

        // subscribe() 在历史入队前经 oneshot 前置返回响应（不被历史背压
        // 阻塞）；仅当会话不存在（已从认证时的快照移除）时走 None 分支
        let subscribe_handle = tokio::spawn(async move {
            let result = global_manager
                .subscribe(&session_id_for_sub, &client_id, output_tx, Some(resp_tx))
                .await;
            if result.is_none() {
                let _ = addr
                    .send(SessionSubscribeOutcome {
                        session_id: session_id_for_sub,
                        result: None,
                    })
                    .await;
            }
        });
        self.subscribe_tasks.insert(sub_key, subscribe_handle);

        // 响应转发任务：订阅建立后立即把 subscribe_ok 送回客户端
        actix::spawn(async move {
            if let Ok(response) = resp_rx.await {
                let _ = addr_for_resp
                    .send(SessionSubscribeOutcome {
                        session_id: session_id_for_resp,
                        result: Some(response),
                    })
                    .await;
            }
        });

        // 输出转发任务：OutputEvent 流 → TB v2 二进制帧（spec §5.3），
        // 远程通道按 merge_output 开关决定合并/直通（语义与旧路由一致）
        let addr = ctx.address();
        let config = AppConfig::global();
        let flush_interval = Duration::from_millis(config.terminal.flush_interval_ms);
        let max_buffer_size = config.terminal.max_buffer_size;
        let merge_output = config.terminal.merge_output;
        let interval = if merge_output { flush_interval } else { Duration::ZERO };
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<forward::ForwardOutput>(64);
        let fwd_handle = tokio::spawn(forward::forward_loop(
            output_rx,
            out_tx,
            interval,
            max_buffer_size,
            generation,
            my_gen,
        ));
        self.output_forwarders.insert(fwd_key, fwd_handle);

        // 消费循环：二进制帧经 actor 直发；HistoryEnd 编码 JSON 控制帧
        // （05 注释的落点：新路由在此编码，旧路由消费侧仍吞掉）
        actix::spawn(async move {
            while let Some(out) = out_rx.recv().await {
                match out {
                    forward::ForwardOutput::Binary(data) => {
                        if addr.send(TerminalOutputBinary { data }).await.is_err() {
                            tracing::debug!("[OutputForwarder] Actor stopped, exiting loop");
                            break;
                        }
                    }
                    forward::ForwardOutput::HistoryEnd { snapshot_seq, .. } => {
                        let frame = ServerFrame::HistoryEnd { snapshot_seq };
                        if addr.send(SendTextMessage { text: frame.to_json() }).await.is_err() {
                            tracing::debug!("[OutputForwarder] Actor stopped, exiting loop");
                            break;
                        }
                    }
                }
            }
        });
    }

    /// 处理客户端背压 ack 帧（spec 04-06 渲染反馈环）：解析后交给全局输出
    /// 管理器推进该会话未 ack 记账（释放 ≤ last_rendered_seq 的输出字节），
    /// 使 PTY 读取得以恢复。非法帧（未知二进制）仅记日志，不中断连接——
    /// ack 尽力而为，丢失时由水位暂停兜底，不缺字节不丢帧
    ///
    /// 来源身份：本地环回通道（桌面 WebView）为 Desktop；远程通道（移动端）
    /// 取认证时的 device_name——服务端据此做背压门控（仅正统渲染端的 ack
    /// 推进记账，见 GlobalOutputManager::ack）
    fn handle_ack_binary(&self, bytes: &[u8], _ctx: &mut ws::WebsocketContext<Self>) {
        // 提前解析来源（actix::spawn 需要 'static）
        let source = if self.local {
            RendererSource::Desktop
        } else {
            match self.session.device_name.clone() {
                Some(name) => RendererSource::Mobile { device_name: name },
                None => {
                    tracing::debug!(
                        addr = %self.session.addr,
                        "ack from unauthenticated remote channel, treating as Desktop source"
                    );
                    RendererSource::Desktop
                }
            }
        };
        match control_frame::parse_ack_frame(bytes) {
            Ok((acked_seq, session_id)) => {
                actix::spawn(async move {
                    GlobalOutputManager::global()
                        .ack(&session_id, acked_seq, source)
                        .await;
                });
            }
            Err(()) => {
                tracing::debug!(
                    addr = %self.session.addr,
                    len = bytes.len(),
                    "non-ack binary frame ignored"
                );
            }
        }
    }

    /// 新路由输入：控制帧 input → PTY（data 为 Base64，与旧路由 wire 一致）
    fn handle_session_input(
        &mut self,
        data: String,
        special_key: Option<crate::enums::special_key::KeyCombo>,
        _ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let session_id = self.bound_session.clone().unwrap();
        let app_ctx = AppContext::global();
        let sm = app_ctx.session_manager().clone();
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
                action: crate::enums::TerminalAction::Input { data, special_key },
            };
            if let Err(e) =
                crate::server::services::terminal_service::handle_input(&session_id, payload, &Some(sm)).await
            {
                tracing::error!(session_id = %session_id, error = %e, "Terminal input error");
            }
        });
    }

    /// JWT 认证共享核心（旧路由 handle_auth_jwt 与新路由 handle_session_auth 共用）
    ///
    /// 验证 token → 设置会话认证状态 → 注册到 WsSessionRegistry + 更新
    /// 配对 last_seen + 通知前端设备上线。成功返回 claims（调用方各自
    /// 构造响应帧：旧路由 Message::Auth JSON，新路由 auth_ok 控制帧）
    fn authenticate_jwt(&mut self, token: &str) -> Result<crate::utils::auth::jwt::JwtClaims, (String, String)> {
        let jwt_service = JwtService::new();
        let claims = match jwt_service.verify_token_with_expiry(token) {
            Ok(c) => c,
            Err(e) => {
                let msg = match e {
                    crate::utils::auth::jwt::JwtError::TokenExpired => "Token expired",
                    _ => "Invalid token",
                };
                return Err(("AUTH_FAILED".to_string(), msg.to_string()));
            }
        };

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
            crate::server::services::auth_service::format_device_display_name(n, &self.session.addr.to_string())
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

        // 通知桌面端（无头/测试上下文无 AppHandle：跳过前端事件）
        let app_ctx = AppContext::global();
        if let Some(handle) = app_ctx.app_handle() {
            let _ = handle.emit(
                event::DEVICE_CONNECTED,
                &crate::server::connection_types::DeviceConnectionEvent {
                    addr: self.session.addr.to_string(),
                    device_id: claims.sub.clone(),
                    device_name: self.session.device_name.clone(),
                    fingerprint: self.session.fingerprint.clone(),
                    event: "authenticated".to_string(),
                },
            );
        }

        Ok(claims)
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
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        match payload.stage {
            // JWT 重新认证：同步路径，直接验证 JWT token
            crate::enums::AuthStage::Authenticated | crate::enums::AuthStage::Reauthenticate => {
                self.handle_auth_jwt(payload, message_id, ctx);
            }
            _ => {
                let error = Message::error_with_id(&message_id, "INVALID_AUTH_STAGE", "Unsupported auth stage");
                if let Ok(json) = error.to_json() {
                    self.send_text_filtered(json, ctx);
                }
            }
        }
    }

    /// 处理 JWT 重新认证（快速同步路径）
    fn handle_auth_jwt(
        &mut self,
        payload: crate::enums::AuthPayload,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        let token = match &payload.session_token {
            Some(t) if !t.is_empty() => t.clone(),
            _ => {
                let error = Message::error_with_id(&message_id, "NO_TOKEN", "No JWT token provided");
                if let Ok(json) = error.to_json() {
                    self.send_text_filtered(json, ctx);
                }
                // spec §4.3 拒绝对称：JWT 认证失败（缺 token）→ 回错误后关闭连接
                ctx.close(None);
                ctx.stop();
                return;
            }
        };

        match self.authenticate_jwt(&token) {
            Ok(claims) => {
                // 链路加密协商（issue 04）：回执随 auth 响应明文下发，
                // 密码表注册在发送之后——此后的帧才进入加密模式
                let mut ws_handshake = None;
                if let Some(proposal) = self.pending_ws_crypto.take() {
                    if link_crypto::current_config().enabled {
                        match link_crypto::derive_ws_session_ciphers(&proposal.ek) {
                            Ok(hs) => ws_handshake = Some(hs),
                            Err(e) => tracing::warn!(
                                addr = %self.session.addr,
                                error = %e,
                                "ws link crypto handshake failed, staying plaintext"
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
                        device_id: self.session.device_id.clone(),
                        device_name: self.session.device_name.clone(),
                        device_fingerprint: claims.fingerprint,
                        session_token: Some(token),
                        error: None,
                        crypto: ws_handshake.as_ref().map(|hs| crate::enums::auth::CryptoProposal {
                            v: 1,
                            ek: hs.server_ek_b64.clone(),
                        }),
                        ..Default::default()
                    },
                };
                if let Ok(json) = response.to_json() {
                    self.send_text_filtered(json, ctx);
                }
                if let Some(hs) = ws_handshake {
                    link_crypto::ws_register_ciphers(&self.session.addr.to_string(), hs.ciphers);
                    tracing::info!(
                        addr = %self.session.addr,
                        "ws link encryption negotiated (legacy route), frames encrypted from now on"
                    );
                }
            }
            Err((code, message)) => {
                let error = Message::error_with_id(&message_id, &code, &message);
                if let Ok(json) = error.to_json() {
                    self.send_text_filtered(json, ctx);
                }
                // spec §4.3 拒绝对称：JWT 认证失败（无效/过期 token）→ 回错误后关闭连接
                ctx.close(None);
                ctx.stop();
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
                        TerminalPayload {
                            action: crate::enums::TerminalAction::Input { data, special_key },
                        },
                        &Some(sm),
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
                        self.send_text_filtered(json, ctx);
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
    /// 05 快照协议：wire start_seq 仅作兼容保留（恒忽略），历史全量重播
    fn handle_subscribe(
        &mut self,
        session_id: String,
        start_seq: Option<u64>,
        message_id: String,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        // 05 快照协议忽略 wire start_seq（恒全量重播），留日志便于排查
        if start_seq.is_some() {
            tracing::debug!(
                "[TerminalWs] 05 snapshot protocol ignores wire start_seq={:?}",
                start_seq
            );
        }

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

        // 输出流代数递增：旧 forward_loop 立即失效（即使 abort 异步取消
        // 窗口内仍有帧投递到 actor 邮箱，也会被代数校验丢弃）
        let generation = self
            .stream_generations
            .entry(fwd_key.clone())
            .or_insert_with(|| std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)))
            .clone();
        let my_gen = generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;

        // 创建输出转发通道
        // 容量 8192：历史回放 + 实时输出并发到达时，subscribe() 的历史发送
        // 会被 send_queue 背压阻塞（历史发不完 → subscribe_response 不回 →
        // 客户端 send_and_wait 超时误判断开）。大容量显著降低背压概率；
        // 重订阅全量重播期间也给实时输出留足缓冲余量
        let (output_tx, output_rx) = tokio::sync::mpsc::channel::<OutputFrame>(8192);

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
                .subscribe(&session_id_for_sub, &client_id, output_tx, Some(resp_tx))
                .await;
            // 响应已通过 resp_tx 前置返回；此处仅处理会话不存在（resp_tx 已丢弃）
            if result.is_none() {
                let _ = addr
                    .send(SubscribeResult {
                        session_id: session_id_for_sub,
                        request_id,
                        result: None,
                    })
                    .await;
            }
        });
        self.subscribe_tasks.insert(sub_key, subscribe_handle);

        // 响应转发任务：订阅建立后立即把裁决消息送回客户端
        actix::spawn(async move {
            if let Ok(response) = resp_rx.await {
                let _ = addr_for_resp
                    .send(SubscribeResult {
                        session_id: session_id_for_resp,
                        request_id: request_id_for_resp,
                        result: Some(response),
                    })
                    .await;
            }
        });

        // 启动输出转发任务：将 OutputEvent 转为 TB v2 二进制帧发到 actor。
        // 旧 /ws/terminal 兼容路由（base64 JSON 文本帧）已删除，仅剩 TB v2
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
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<forward::ForwardOutput>(64);
        let fwd_handle = tokio::spawn(forward::forward_loop(
            output_rx,
            out_tx,
            interval,
            max_buffer_size,
            generation,
            my_gen,
        ));
        // 注册转发任务：替换订阅 / 取消订阅 / 断连时 abort
        self.output_forwarders.insert(fwd_key, fwd_handle);

        // 消费循环：转发结果经 actor 发送（失败 = actor 停止，终止转发）
        actix::spawn(async move {
            while let Some(out) = out_rx.recv().await {
                match out {
                    forward::ForwardOutput::Binary(data) => {
                        if addr.send(TerminalOutputBinary { data }).await.is_err() {
                            tracing::debug!("[OutputForwarder] Actor stopped, exiting loop");
                            break;
                        }
                    }
                    // 本地环回客户端不识别 history_end 控制帧，直接吞掉
                    forward::ForwardOutput::HistoryEnd { .. } => {}
                }
            }
        });
    }

    /// 取消订阅
    fn handle_unsubscribe(&mut self, session_id: String, message_id: String, ctx: &mut ws::WebsocketContext<Self>) {
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
        // 流代数失效：即使旧 forward_loop 在 abort 异步取消窗口内仍发出帧，
        // 也会被代数校验丢弃，不会与后续新订阅的流交错
        if let Some(gen) = self.stream_generations.get(&fwd_key) {
            gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }

        actix::spawn(async move {
            let success = global_manager.unsubscribe(&session_id, &client_id).await;
            let _ = addr
                .send(UnsubscribeResult {
                    session_id,
                    request_id,
                    success,
                })
                .await;
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
        // 无头/测试上下文可能无 AppHandle：handle_control_message 签名本身就是 Option，直接透传
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
                app_handle,
            )
            .await;

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
                // 05 快照协议恒全量重播：mode 恒 Reset、offsets 恒 0，
                // 合成常量保旧客户端（本地终端/移动端）wire 解析兼容；
                // 06 新路由不走此路径（控制帧 subscribe_ok 无这些字段）
                let ws_msg = Message::subscribe_response_with_request_id(
                    &msg.session_id,
                    response.min_seq,
                    response.snapshot_seq,
                    response.history_count,
                    SubscribeMode::Reset,
                    0,
                    0,
                    &msg.request_id,
                );
                if let Ok(json) = ws_msg.to_json() {
                    self.send_text_filtered(json, ctx);
                }
            }
            None => {
                let error = Message::error_with_id(
                    &msg.request_id,
                    "SESSION_NOT_FOUND",
                    &format!("Session {} not found", msg.session_id),
                );
                if let Ok(json) = error.to_json() {
                    self.send_text_filtered(json, ctx);
                }
            }
        }
    }
}

/// 处理新路由订阅结果（/ws/terminal/session/{id}）
///
/// subscribe_ok 控制帧携带快照元数据（spec §5.3）；响应经 oneshot 前置
/// 返回，保证先于历史帧到达（前端据此进入 HISTORY 分发模式）
impl Handler<SessionSubscribeOutcome> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SessionSubscribeOutcome, ctx: &mut Self::Context) {
        match msg.result {
            Some(response) => {
                let frame = ServerFrame::SubscribeOk {
                    snapshot_seq: response.snapshot_seq,
                    min_seq: response.min_seq,
                    history_count: response.history_count,
                };
                self.send_text_filtered(frame.to_json(), ctx);
            }
            None => {
                // 会话在认证后被移除（罕见竞态）：与认证时一致的错误流
                let frame = ServerFrame::Error {
                    code: "SESSION_NOT_FOUND".to_string(),
                    message: format!("Session {} not found", msg.session_id),
                };
                self.send_text_filtered(frame.to_json(), ctx);
                ctx.close(None);
                ctx.stop();
            }
        }
    }
}

/// 处理新路由认证结果：会话存在 → auth_ok；不存在 → error(SESSION_NOT_FOUND) + 关闭
impl Handler<SessionAuthOutcome> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SessionAuthOutcome, ctx: &mut Self::Context) {
        if msg.exists {
            // 加密协商回执：auth_ok 本身保持明文（客户端需先读到服务端临时公钥
            // 才能派生密钥），注册在发送之后——此后的所有帧进入加密模式
            let mut handshake = None;
            if let Some(proposal) = self.pending_ws_crypto.take() {
                if link_crypto::current_config().enabled {
                    match link_crypto::derive_ws_session_ciphers(&proposal.ek) {
                        Ok(hs) => {
                            handshake = Some(hs);
                            tracing::info!(
                                addr = %self.session.addr,
                                "ws link encryption negotiated, frames encrypted from now on"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(addr = %self.session.addr, error = %e, "ws link crypto handshake failed, staying plaintext")
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
            self.send_text_filtered(frame.to_json(), ctx);
            if let Some(hs) = handshake {
                link_crypto::ws_register_ciphers(&self.session.addr.to_string(), hs.ciphers);
            }
        } else {
            tracing::warn!(
                addr = %self.session.addr,
                session_id = %msg.session_id,
                "Session WS rejected: session not found after auth"
            );
            let frame = ServerFrame::Error {
                code: "SESSION_NOT_FOUND".to_string(),
                message: format!("Session {} not found", msg.session_id),
            };
            self.send_text_filtered(frame.to_json(), ctx);
            // spec §5.1：会话不存在 → 认证通过后 error + 关闭
            ctx.close(None);
            ctx.stop();
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
                self.send_text_filtered(json, ctx);
            }
        }
    }
}

/// 处理终端输出转发（二进制帧，本地通道）
impl Handler<TerminalOutputBinary> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: TerminalOutputBinary, ctx: &mut Self::Context) {
        self.send_binary_filtered(msg.data, ctx);
    }
}

/// 处理外部推送消息（广播/定向发送）
impl Handler<SendTextMessage> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SendTextMessage, ctx: &mut Self::Context) {
        self.send_text_filtered(msg.text, ctx);
    }
}
mod forward;
