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

use crate::enums::{SessionControlPayload, TerminalPayload};
use crate::server::filter::{Direction, FilterContext, TrafficChannel, TrafficFilterChain};
use crate::server::link_crypto;
use crate::server::message::Message;
use crate::server::ws::registry::{ChannelType, WsSessionRegistry};
use crate::server::ws::session::WsSession;
use crate::session::{GlobalOutputManager, RendererSource, SessionStatus};
use crate::system::app_context::AppContext;
use crate::system::constants::event;
use crate::system::constants::server::{HEARTBEAT_INTERVAL_SECS, REMOTE_CLIENT_TIMEOUT_SECS, WS_AUTH_TIMEOUT_SECS};
use crate::system::error_boundary::spawn_with_error_boundary;
use crate::utils::auth::jwt::JwtService;
use control_frame::ServerFrame;

mod control_frame;

/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(HEARTBEAT_INTERVAL_SECS);

/// 转发统计打点帧数（链路调试字节对账；不打逐帧 WS 发送日志，防输出风暴刷屏）
const FORWARD_STATS_FRAMES: u64 = 100;

/// 拉取模型订阅就绪（异步装配任务 → actor）
///
/// 装配在异步任务里完成（读会话管理器 + 登记句柄 + 起订阅者执行体），
/// 任务把「执行体 JoinHandle + 交接通道接收端」交回 actor 持有，才能与
/// 连接生命周期绑定（abort 于替换/退订/断连）
#[derive(Message)]
#[rtype(result = "()")]
struct PullSubscriberReady {
    /// `client_id:session_id`（任务表/代数表键）
    key: String,
    session_id: String,
    /// 本链路的输出流代数（残留帧按代数丢弃）
    generation: u64,
    /// 旧 Message 路由的 message_id（新路由为 None —— 控制帧协议无请求-响应）
    request_id: Option<String>,
    /// None = 会话不存在（订阅失败）
    ready: Option<PullReadyParts>,
}

/// 订阅链路装配产物
struct PullReadyParts {
    response: crate::session::SubscribeResponse,
    subscriber_task: tokio::task::JoinHandle<()>,
    out_rx: tokio::sync::mpsc::Receiver<forward::ForwardOutput>,
}

/// 一条订阅链路的任务组（订阅者执行体 + 桥接）
///
/// 替换/退订/断连时整体 abort：旧链路的残留帧另有流代数门控兜底
pub(crate) struct PullTasks {
    subscriber: tokio::task::JoinHandle<()>,
    bridge: tokio::task::JoinHandle<()>,
}

impl PullTasks {
    fn abort(self) {
        self.subscriber.abort();
        self.bridge.abort();
    }
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

/// 终端输出消息（从订阅者桥接任务传回，二进制帧形态）
/// `data` 为已编码的完整帧（含 16 字节 TB v3 帧头），直接 ctx.binary 发送
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalOutputBinary {
    data: Vec<u8>,
    /// 输出流代数校验（`client_id:session_id`）：不符 → 旧流残留帧，丢弃
    stream_key: String,
    generation: u64,
}

/// 桥接任务产生的控制帧（history_end / resync / error）：同样做代数门控，
/// 防旧流控制帧（如过期 history_end）注入新订阅
#[derive(Message)]
#[rtype(result = "()")]
struct TerminalControlFrame {
    text: String,
    stream_key: String,
    generation: u64,
}

/// 终止本连接（订阅者链路回收，如僵尸订阅者）：停止 actor 并关闭 socket
#[derive(Message)]
#[rtype(result = "()")]
struct TerminateConnection;

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
    /// 绑定会话（新路由 /ws/terminal/session/{id}）：连接创建即绑定，
    /// 订阅即连接、无多路复用；None = 事件通道（/ws/event，仅认证 + 收广播）
    bound_session: Option<String>,
    /// 会话停止监听任务（新路由）：bound 会话 Stopped 时推送 session_stopped 帧
    session_stopped_watcher: Option<tokio::task::JoinHandle<()>>,
    /// 拉取模型订阅链路任务表（key = `client_id:session_id`）
    ///
    /// 每链路 = 订阅者执行体（环拉取）+ 桥接（ForwardOutput → WS actor）。
    /// 订阅者被替换 / 取消订阅 / 连接断开时整体 abort：旧链路已投递到 actor
    /// 邮箱的残留帧由流代数门控丢弃，保证同连接同一会话始终只有一条输出流
    pull_tasks: std::collections::HashMap<String, PullTasks>,
    /// 输出流代数（key = `client_id:session_id` → AtomicU64）
    ///
    /// 订阅 / 取消订阅 / 断连时递增；桥接下发的每一帧都携带代数，
    /// 旧代残留帧（abort 异步取消窗口内已投递到 actor 邮箱
    /// 的帧）直接丢弃——与 abort 互补，杜绝旧流帧注入新订阅通道（移动端
    /// 字节游标错位 → 连续性违反 → 重订阅风暴的根源）
    stream_generations: std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU64>>,
    /// 订阅者实时传播模式（key = `client_id:session_id` → AtomicU8；双速，
    /// 用户需求 3）：realtime（进终端页，读即传）/ batch（退出终端页但
    /// 会话未停，满 terminal.batch_bytes 才转发）。由 SetMode 控制帧实时
    /// 切换，订阅者执行体每次循环读取；重订阅时重置为 realtime
    subscriber_modes: std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU8>>,
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
            bound_session: None,
            session_stopped_watcher: None,
            pull_tasks: std::collections::HashMap::new(),
            stream_generations: std::collections::HashMap::new(),
            subscriber_modes: std::collections::HashMap::new(),
            channel_type: ChannelType::Terminal,
            pending_ws_crypto: None,
        }
    }

    /// 每会话终端路由构造（spec §5.1）：连接创建即绑定 session_id，
    /// 订阅即连接（无 subscribed_sessions 多路复用），控制帧走简化
    /// JSON 协议（无 message_id/expect_response），输出帧为 TB v3 二进制
    pub fn new_for_session(addr: SocketAddr, session_id: String) -> Self {
        let mut ws = Self::new(addr);
        ws.bound_session = Some(session_id);
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
        let timeout = Duration::from_secs(REMOTE_CLIENT_TIMEOUT_SECS);
        ctx.run_interval(HEARTBEAT_INTERVAL, move |act, ctx| {
            if Instant::now().duration_since(act.hb) > timeout {
                tracing::warn!(client = %act.session.addr, "WebSocket heartbeat timeout");
                ctx.close(None);
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    // ==================== Traffic Filter Hooks（流量过滤责任链接线） ====================

    /// 本连接对应的流量通道类型
    fn traffic_channel(&self) -> TrafficChannel {
        match self.channel_type {
            ChannelType::Terminal => TrafficChannel::WsTerminal,
            ChannelType::Event => TrafficChannel::WsEvent,
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
            outbound_headers: Vec::new(),
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
    fn filter_inbound_text(&self, text: String, ctx: &mut ws::WebsocketContext<Self>) -> Option<String> {
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
            outbound_headers: Vec::new(),
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
    /// 承载 TB v3 终端输出流（spec §5.3），加密过滤器的主战场
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
            outbound_headers: Vec::new(),
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
        // 链路追踪（05）：WS 连接生命周期 span，client 地址 + 绑定会话（若有）；
        // 后续帧级事件不再各自开 span（热点路径），连接级 span 保持调用链锚点
        let _span = tracing::info_span!(
            "terminal_ws",
            client = %self.session.addr,
            session_id = %self.bound_session.as_deref().unwrap_or("-"),
        )
        .entered();
        tracing::info!(client = %self.session.addr, "Terminal WS connected");
        self.start_heartbeat(ctx);

        // 首消息认证超时：连接建立后 10s 内未完成认证（JWT 或配对流程）→
        // 服务端主动关闭（spec §4.3「10s 未完成首消息认证」）；已认证
        // 连接（如事件通道首消息即认证）此闭包自动 no-op
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
            let handle = spawn_with_error_boundary("ws_session_stopped_monitor", async move {
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
        tracing::info!(client = %self.session.addr, "Terminal WS disconnected");

        // 中止会话停止监听任务：连接已断开，通知不再需要
        if let Some(handle) = self.session_stopped_watcher.take() {
            handle.abort();
        }

        // 中止所有订阅链路任务 + 流代数全部失效 + 清空订阅者模式表
        Self::cleanup_subscription_state(
            &mut self.pull_tasks,
            &mut self.stream_generations,
            &mut self.subscriber_modes,
        );

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
                        // 回填连接历史断开时间（连接历史按 pairings.id 键控，须按指纹
                        // 解析——claims.sub 是移动端自身 ID，直接传会匹配不到 open 行）
                        let db_guard = app_ctx.db().lock().await;
                        if let Err(e) = db_guard.close_open_connection_event_by_fingerprint(fp) {
                            tracing::warn!(fingerprint = %fp, error = %e, "Failed to close connection history");
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
    /// 断连清理纯逻辑（stopping 前半段，供测试）：中止全部输出转发/订阅任务、
    /// 流代数全部失效（+1）、清空订阅者模式表。返回后三个表均为空——abort 的
    /// JoinHandle 在异步取消窗口内仍可能把残留帧投递到 actor 邮箱，代数递增与
    /// abort 互补：actor 侧按代数丢弃旧代残留帧，杜绝旧流帧注入
    /// 新订阅通道（移动端字节游标错位 → 连续性违反 → 重订阅风暴的根源）
    pub(crate) fn cleanup_subscription_state(
        pull_tasks: &mut std::collections::HashMap<String, PullTasks>,
        stream_generations: &mut std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU64>>,
        subscriber_modes: &mut std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicU8>>,
    ) {
        // 中止所有订阅链路任务：连接已断开，残留缓冲帧不再需要投递
        for (_, tasks) in pull_tasks.drain() {
            tasks.abort();
        }
        // 流代数全部失效：abort 异步取消窗口内仍可能发出的残留帧直接丢弃
        for (_, gen) in stream_generations.drain() {
            gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        // 订阅者模式原子随连接销毁（SetMode 仅存活于连接生命周期）
        subscriber_modes.clear();
    }

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
                tracing::debug!(client = %self.session.addr, "Unsupported WS message type");
            }
        }
    }

    /// 处理新路由控制帧（/ws/terminal/session/{id}，spec §5.3）
    ///
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

        // 未认证 + 业务帧 → 拒绝对称：error(AUTH_REQUIRED) + 关闭（require_session_auth 内部处理）
        if Self::should_reject_unauthenticated(&frame, self.session.authenticated) {
            self.require_session_auth(ctx);
            return;
        }

        match frame {
            control_frame::ClientFrame::Auth { token, crypto } => {
                // 幂等：已认证连接重复发 auth 直接忽略
                if self.session.authenticated {
                    return;
                }
                self.pending_ws_crypto = crypto;
                self.handle_session_auth(token, ctx);
            }
            control_frame::ClientFrame::Subscribe { from_offset } => {
                if !self.require_session_auth(ctx) {
                    return;
                }
                self.handle_session_subscribe(from_offset, ctx);
            }
            control_frame::ClientFrame::SetMode { mode } => {
                if !self.require_session_auth(ctx) {
                    return;
                }
                self.handle_session_mode(mode, ctx);
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

    /// 订阅前替换链路：代数递增（旧流残留帧立即失效）+ 中止旧任务组
    ///
    /// 返回本链路的新代数（桥接携带它下发帧；actor 侧按代数丢弃旧流帧）
    fn bump_stream_generation(&mut self, key: &str) -> u64 {
        if let Some(prev) = self.pull_tasks.remove(key) {
            tracing::debug!(key = %key, "[TerminalWs] aborting previous subscriber tasks");
            prev.abort();
        }
        let generation = self
            .stream_generations
            .entry(key.to_string())
            .or_insert_with(|| std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)))
            .clone();
        generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    /// 流代数是否仍为当前代：false = 旧流残留帧，直接丢弃
    fn stream_generation_current(&self, key: &str, generation: u64) -> bool {
        match self.stream_generations.get(key) {
            Some(cur) => cur.load(std::sync::atomic::Ordering::SeqCst) == generation,
            None => false,
        }
    }

    /// 新路由订阅（/ws/terminal/session/{id}，spec §5.4）：绑定单会话直接订阅，
    /// 无多路复用（「订阅即连接」）
    ///
    /// - 起一条拉取模型订阅链路：订阅者执行体（环上按游标拉取 + 合帧 + 窗口门控）
    ///   + 桥接（ForwardOutput → actor 消息），任务组随连接生命周期 abort
    /// - 控制帧 subscribe 可携带 from_offset（字节锚点）→ 历史自该游标起播
    /// - 重订阅（前端 offset 缺口自愈）：代数递增 + abort 旧任务组，防双流
    /// - 订阅即截断（from_offset < 环驻留起点）时由执行体下发 resync（spec §4.7）
    fn handle_session_subscribe(&mut self, from_offset: Option<u64>, ctx: &mut ws::WebsocketContext<Self>) {
        let session_id = self.bound_session.clone().unwrap();
        let client_id = self.session.addr.to_string();
        let key = format!("{}:{}", client_id, session_id);
        // 替换链路：代数递增（旧流残留帧立即失效）+ abort 旧任务组
        let my_gen = self.bump_stream_generation(&key);

        // 订阅者模式原子：重订阅/新建订阅重置为 realtime（进终端页即读即传）；
        // 由 SetMode 控制帧实时切换为 batch（退出终端页），支持双速传播
        let mode = self
            .subscriber_modes
            .entry(key.clone())
            .or_insert_with(|| std::sync::Arc::new(std::sync::atomic::AtomicU8::new(forward::MODE_REALTIME)))
            .clone();
        mode.store(forward::MODE_REALTIME, std::sync::atomic::Ordering::SeqCst);

        // 装配（读会话管理器 + 登记句柄 + 起订阅者执行体）在异步任务中完成，
        // 产物交回 actor 持有（任务组与连接生命周期绑定）
        let addr = ctx.address();
        let cfg = subscriber::SubscriberCfg::for_remote_route();
        let session_for_task = session_id.clone();
        let client_id_for_task = client_id.clone();
        tokio::spawn(async move {
            let ready = match GlobalOutputManager::global().session(&session_for_task).await {
                Some(manager) => {
                    let spawned =
                        subscriber::spawn_subscriber(&manager, &client_id_for_task, from_offset, mode, cfg).await;
                    Some(PullReadyParts {
                        response: spawned.response,
                        subscriber_task: spawned.task,
                        out_rx: spawned.out_rx,
                    })
                }
                None => None,
            };
            let _ = addr
                .send(PullSubscriberReady {
                    key,
                    session_id: session_for_task,
                    generation: my_gen,
                    request_id: None,
                    ready,
                })
                .await;
        });
    }

    /// 输出桥接循环：订阅者交接通道（ForwardOutput）→ WS actor 消息
    ///
    /// - 二进制帧：`TerminalOutputBinary`（带流代数，旧流残留帧被 actor 丢弃）
    /// - 控制帧（history_end / resync / error）：仅新路由透传（`forward_control`），
    ///   旧 Message 路由客户端不识别未知类型 → 吞掉
    /// - 僵尸回收的 Terminate：尽力下发 error 后请求关闭本连接
    /// - 链路调试：解析帧头累计帧/字节，每 100 帧打点 + 退出兜底汇总
    fn spawn_bridge(
        addr: actix::Addr<Self>,
        key: String,
        session_id: String,
        generation: u64,
        forward_control: bool,
        mut out_rx: tokio::sync::mpsc::Receiver<forward::ForwardOutput>,
    ) -> tokio::task::JoinHandle<()> {
        spawn_with_error_boundary("terminal_subscriber_bridge", async move {
            let mut frames: u64 = 0;
            let mut payload_bytes: u64 = 0;
            let mut stream_end: u64 = 0;
            while let Some(out) = out_rx.recv().await {
                match out {
                    forward::ForwardOutput::Binary(data) => {
                        if data.len() >= forward::V3_FRAME_HEADER_LEN {
                            let start = u64::from_le_bytes(data[4..12].try_into().unwrap_or([0; 8]));
                            let len = u32::from_le_bytes(data[12..16].try_into().unwrap_or([0; 4])) as u64;
                            frames += 1;
                            payload_bytes += len;
                            stream_end = start + len;
                            if frames.is_multiple_of(FORWARD_STATS_FRAMES) {
                                tracing::debug!(
                                    session_id = %session_id,
                                    forwarded_frames = frames,
                                    forwarded_bytes = payload_bytes,
                                    stream_end_offset = stream_end,
                                    "terminal forward stats (periodic)"
                                );
                            }
                        }
                        if addr
                            .send(TerminalOutputBinary {
                                data,
                                stream_key: key.clone(),
                                generation,
                            })
                            .await
                            .is_err()
                        {
                            tracing::debug!("[SubscriberBridge] Actor stopped, exiting loop");
                            break;
                        }
                    }
                    forward::ForwardOutput::HistoryEnd { snapshot_offset, .. } => {
                        if forward_control {
                            tracing::debug!(
                                session_id = %session_id,
                                forwarded_frames = frames,
                                snapshot_offset,
                                "terminal history segment replayed, sending history_end"
                            );
                            let text = ServerFrame::HistoryEnd { snapshot_offset }.to_json();
                            if addr
                                .send(TerminalControlFrame {
                                    text,
                                    stream_key: key.clone(),
                                    generation,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    // 重同步信号（spec §4.7）：只增不改，老客户端忽略未知帧
                    forward::ForwardOutput::Resync {
                        min_offset,
                        snapshot_offset,
                    } => {
                        tracing::warn!(
                            session_id = %session_id,
                            min_offset,
                            snapshot_offset,
                            "terminal subscriber truncated, sending resync"
                        );
                        if forward_control {
                            let text = ServerFrame::Resync {
                                min_offset,
                                snapshot_offset,
                            }
                            .to_json();
                            if addr
                                .send(TerminalControlFrame {
                                    text,
                                    stream_key: key.clone(),
                                    generation,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    // 僵尸订阅者回收：尽力下发 error 后关闭本连接（只影响这一路）
                    forward::ForwardOutput::Terminate { code, message } => {
                        tracing::warn!(
                            session_id = %session_id,
                            code = %code,
                            message = %message,
                            "terminal subscriber terminated, closing connection"
                        );
                        if forward_control {
                            let text = ServerFrame::Error { code, message }.to_json();
                            let _ = addr
                                .send(TerminalControlFrame {
                                    text,
                                    stream_key: key.clone(),
                                    generation,
                                })
                                .await;
                        }
                        let _ = addr.send(TerminateConnection).await;
                        break;
                    }
                }
            }
            if frames > 0 {
                tracing::debug!(
                    session_id = %session_id,
                    forwarded_frames = frames,
                    forwarded_bytes = payload_bytes,
                    stream_end_offset = stream_end,
                    "terminal subscriber bridge exited, final totals"
                );
            }
        })
    }

    /// 新路由传播模式切换（双速，用户需求 3）：更新绑定会话订阅者的 mode 原子，
    /// 订阅者执行体每次循环读取实现即时生效；未订阅（连接建立后未发 subscribe）
    /// 时忽略（订阅时统一重置为 realtime）
    fn handle_session_mode(&self, mode: control_frame::WatchMode, _ctx: &mut ws::WebsocketContext<Self>) {
        let Some(session_id) = self.bound_session.clone() else {
            return;
        };
        let client_id = self.session.addr.to_string();
        let fwd_key = format!("{client_id}:{session_id}");
        let Some(atomic) = self.subscriber_modes.get(&fwd_key) else {
            tracing::debug!(fwd_key = %fwd_key, mode = ?mode, "SetMode before subscribe, ignored");
            return;
        };
        atomic.store(mode.as_u8(), std::sync::atomic::Ordering::SeqCst);
        tracing::debug!(fwd_key = %fwd_key, mode = ?mode, "terminal propagate mode updated");
    }

    /// ack 来源身份判定（纯函数，供测试）：远程通道（移动端）取认证时的
    /// device_name（仅用于日志/审计——ack 语义已改为「每订阅者私有水位」，
    /// 不做来源门控）；未认证/本地环回通道视为 Desktop 源
    pub(crate) fn ack_source_for(device_name: Option<&str>) -> RendererSource {
        match device_name {
            Some(name) => RendererSource::Mobile {
                device_name: name.to_string(),
            },
            None => RendererSource::Desktop,
        }
    }

    /// ack 帧处理结果（纯函数，供测试）：parse 成功 → Some((acked_offset,
    /// session_id, source))；失败 → None。None 语义 = 调用方仅记日志不中断
    /// 连接——ack 尽力而为，丢失时由水位暂停兜底，不缺字节不丢帧
    pub(crate) fn ack_frame_outcome(
        bytes: &[u8],
        source: RendererSource,
    ) -> Option<(u64, String, RendererSource)> {
        control_frame::parse_ack_frame(bytes)
            .ok()
            .map(|(acked_offset, session_id)| (acked_offset, session_id, source))
    }

    /// 处理客户端背压 ack 帧（spec §4.6 背压下移）：解析后推进**该订阅者私有**
    /// 的 ack 水位（只解除/施加本订阅者的窗口驻留，不参与任何共享记账）。
    /// 非法帧（未知二进制）仅记日志，不中断连接——ack 尽力而为，丢失由驻留
    /// 兜底轮询与僵尸回收兜底，不缺字节不丢帧
    fn handle_ack_binary(&self, bytes: &[u8], _ctx: &mut ws::WebsocketContext<Self>) {
        // 客户端标识 = 连接地址（本连接即一个订阅者，per-connection 订阅模型）
        let client_id = self.session.addr.to_string();
        // 提前解析来源（actix::spawn 需要 'static；仅日志用）
        let source = Self::ack_source_for(self.session.device_name.as_deref());
        match Self::ack_frame_outcome(bytes, source) {
            Some((acked_offset, session_id, source)) => {
                actix::spawn(async move {
                    let applied = GlobalOutputManager::global()
                        .ack_subscriber(&session_id, &client_id, acked_offset)
                        .await;
                    if applied {
                        tracing::trace!(
                            session_id = %session_id,
                            client_id = %client_id,
                            acked_offset,
                            source = ?source,
                            "subscriber ack applied"
                        );
                    }
                });
            }
            None => {
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
        // 会话绑定缺失（异常时序：认证通过后会话被销毁）时不得 panic——actix
        // 任务内 panic 会中断该连接的后续处理且无用户可见反馈，这里记日志丢弃
        let Some(session_id) = self.bound_session.clone() else {
            tracing::warn!(
                addr = %self.session.addr,
                "terminal input dropped: session binding missing"
            );
            return;
        };
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
                let msg = crate::utils::auth::jwt::jwt_error_message(&e);
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
            crate::enums::TerminalAction::Subscribe => {
                self.handle_subscribe(session_id, message_id, ctx);
            }
            crate::enums::TerminalAction::Unsubscribe => {
                self.handle_unsubscribe(session_id, message_id, ctx);
            }
            _ => {}
        }
    }

    /// 订阅会话输出（旧 Message 路由，v2.0.0 客户端兼容）— 拉取模型
    ///
    /// 05 快照协议：历史全量重播（wire 无游标参数；客户端按字节游标
    /// min_offset 裁过去重回放段）。推进模型与移动端新路由一致：环 →
    /// 订阅者执行体 → 桥接 → WS（控制帧不透传：老客户端不识别未知类型）
    fn handle_subscribe(&mut self, session_id: String, message_id: String, ctx: &mut ws::WebsocketContext<Self>) {
        let client_id = self.session.addr.to_string();
        let key = format!("{}:{}", client_id, session_id);
        // 替换链路：代数递增（旧流残留帧立即失效）+ abort 旧任务组
        let my_gen = self.bump_stream_generation(&key);

        let addr = ctx.address();
        let cfg = subscriber::SubscriberCfg::for_remote_route();
        let session_for_task = session_id.clone();
        let client_id_for_task = client_id.clone();
        // 装配在异步任务中完成，产物交回 actor 持有（任务组与连接生命周期绑定）
        tokio::spawn(async move {
            let ready = match GlobalOutputManager::global().session(&session_for_task).await {
                Some(manager) => {
                    let spawned = subscriber::spawn_subscriber(
                        &manager,
                        &client_id_for_task,
                        None,
                        std::sync::Arc::new(std::sync::atomic::AtomicU8::new(forward::MODE_REALTIME)),
                        cfg,
                    )
                    .await;
                    Some(PullReadyParts {
                        response: spawned.response,
                        subscriber_task: spawned.task,
                        out_rx: spawned.out_rx,
                    })
                }
                None => None,
            };
            let _ = addr
                .send(PullSubscriberReady {
                    key,
                    session_id: session_for_task,
                    generation: my_gen,
                    request_id: Some(message_id),
                    ready,
                })
                .await;
        });
    }

    /// 取消订阅
    fn handle_unsubscribe(&mut self, session_id: String, message_id: String, ctx: &mut ws::WebsocketContext<Self>) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let key = format!("{}:{}", client_id, session_id);
        let addr = ctx.address();
        let request_id = message_id;

        // 代数递增 + 中止本链路任务组：旧流残留帧被代数校验丢弃，
        // 不会与后续新订阅的流交错（其余订阅者不受影响）
        let _ = self.bump_stream_generation(&key);
        self.subscriber_modes.remove(&key);

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

/// 处理拉取模型订阅就绪（异步装配任务 → actor）
///
/// 关键顺序约束：**握手帧必须先于桥接启动**发出（前端据此进入 HISTORY 分发
/// 模式；桥接一旦启动即可能推送历史/实时帧）。此处先同步写握手帧，再起桥接
/// 任务，帧序天然成立（无需 oneshot 前置返回）。
impl Handler<PullSubscriberReady> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: PullSubscriberReady, ctx: &mut Self::Context) {
        let PullSubscriberReady {
            key,
            session_id,
            generation,
            request_id,
            ready,
        } = msg;

        // 会话不存在：新路由 error + 关闭（与认证时一致的错误流）；旧路由回 error 消息
        let Some(parts) = ready else {
            match &request_id {
                None => {
                    let frame = ServerFrame::Error {
                        code: "SESSION_NOT_FOUND".to_string(),
                        message: format!("Session {session_id} not found"),
                    };
                    self.send_text_filtered(frame.to_json(), ctx);
                    ctx.close(None);
                    ctx.stop();
                }
                Some(rid) => {
                    let error = Message::error_with_id(rid, "SESSION_NOT_FOUND", &format!("Session {session_id} not found"));
                    if let Ok(json) = error.to_json() {
                        self.send_text_filtered(json, ctx);
                    }
                }
            }
            return;
        };

        match &request_id {
            None => {
                // 链路调试（终端字节对账）：快照三件套是移动端历史拼接/截断判定的
                // 锚点，与移动端 subscribe_ok 收帧日志对照可验证元数据一致
                tracing::debug!(
                    session_id = %session_id,
                    snapshot_offset = parts.response.snapshot_offset,
                    min_offset = parts.response.min_offset,
                    history_bytes = parts.response.history_bytes,
                    "subscribe_ok sent to client"
                );
                let frame = ServerFrame::SubscribeOk {
                    protocol: 3,
                    snapshot_offset: parts.response.snapshot_offset,
                    min_offset: parts.response.min_offset,
                    history_bytes: parts.response.history_bytes,
                };
                self.send_text_filtered(frame.to_json(), ctx);
            }
            Some(rid) => {
                self.session.subscribed_sessions.insert(session_id.clone());
                // 旧路由 wire 字段名（min_seq/max_seq/history_count）不变，值承载
                // TB v3 字节语义（min_offset/snapshot_offset/history_bytes）
                let ws_msg = Message::subscribe_response_with_request_id(
                    &session_id,
                    parts.response.min_offset,
                    parts.response.snapshot_offset,
                    parts.response.history_bytes as usize,
                    rid,
                );
                if let Ok(json) = ws_msg.to_json() {
                    self.send_text_filtered(json, ctx);
                }
            }
        }

        // 桥接：交接通道 → actor 消息（控制帧仅新路由透传）
        let forward_control = request_id.is_none();
        let bridge = Self::spawn_bridge(
            ctx.address(),
            key.clone(),
            session_id,
            generation,
            forward_control,
            parts.out_rx,
        );
        let tasks = PullTasks {
            subscriber: parts.subscriber_task,
            bridge,
        };
        if let Some(prev) = self.pull_tasks.insert(key, tasks) {
            // 极端竞态（并发重订阅）：后到者仍以最新一代为准
            prev.abort();
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
        // 流代数门控：abort 异步取消窗口内旧链路仍可能投递残留帧，
        // 代数不符直接丢弃（杜绝旧流帧注入新订阅通道）
        if !self.stream_generation_current(&msg.stream_key, msg.generation) {
            tracing::debug!(
                key = %msg.stream_key,
                generation = msg.generation,
                "stale output frame dropped (stream generation mismatch)"
            );
            return;
        }
        self.send_binary_filtered(msg.data, ctx);
    }
}

/// 桥接控制帧（history_end / resync / error）：同样受流代数门控
impl Handler<TerminalControlFrame> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: TerminalControlFrame, ctx: &mut Self::Context) {
        if !self.stream_generation_current(&msg.stream_key, msg.generation) {
            return;
        }
        self.send_text_filtered(msg.text, ctx);
    }
}

/// 订阅者链路终止（僵尸回收等）：关闭连接并停止 actor。
/// 该动作只影响这一条订阅链路，源产出与其他订阅者不受影响
impl Handler<TerminateConnection> for TerminalWs {
    type Result = ();

    fn handle(&mut self, _msg: TerminateConnection, ctx: &mut Self::Context) {
        ctx.close(None);
        ctx.stop();
    }
}

/// 处理外部推送消息（广播/定向发送）
impl Handler<SendTextMessage> for TerminalWs {
    type Result = ();

    fn handle(&mut self, msg: SendTextMessage, ctx: &mut Self::Context) {
        self.send_text_filtered(msg.text, ctx);
    }
}
pub(crate) mod forward;
pub(crate) mod subscriber;

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::ws::terminal_ws::forward::MODE_REALTIME;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, AtomicU8};
    use std::sync::Arc;

    /// 构造合法 ack 帧（TB v3 布局：magic(2) + version(1) + flags(1) +
    /// offset(8 LE) + len(4 LE) + session_id UTF-8）
    fn build_ack(session_id: &str, offset: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x54, 0x42, 3, 0x02]);
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&(session_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(session_id.as_bytes());
        bytes
    }

    // ==================== 分派契约（handle_session_control_frame 守卫） ====================

    #[test]
    fn frame_needs_auth_only_business_frames() {
        // Auth 帧无需认证（首消息即认证握手）
        let auth = control_frame::ClientFrame::Auth {
            token: "jwt".to_string(),
            crypto: None,
        };
        assert!(!TerminalWs::frame_needs_auth(&auth), "auth 帧不应要求已认证");

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
            assert!(TerminalWs::frame_needs_auth(frame), "业务帧必须要求已认证");
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
        assert!(TerminalWs::should_reject_unauthenticated(&subscribe, false));
        // 未认证 + auth 帧 → 放行（认证握手本身）
        assert!(!TerminalWs::should_reject_unauthenticated(&auth, false));
        // 已认证 + 业务帧 → 放行
        assert!(!TerminalWs::should_reject_unauthenticated(&subscribe, true));
        // 已认证 + auth 帧 → 放行（幂等忽略由 handle_session_control_frame 处理）
        assert!(!TerminalWs::should_reject_unauthenticated(&auth, true));
    }

    // ==================== ack 帧处理（handle_ack_binary） ====================

    #[test]
    fn ack_source_for_remote_channel_uses_device_name() {
        match TerminalWs::ack_source_for(Some("Pixel 9")) {
            RendererSource::Mobile { device_name } => assert_eq!(device_name, "Pixel 9"),
            other => panic!("expected Mobile source, got {other:?}"),
        }
    }

    #[test]
    fn ack_source_for_unauthenticated_falls_back_to_desktop() {
        match TerminalWs::ack_source_for(None) {
            RendererSource::Desktop => {}
            other => panic!("expected Desktop source, got {other:?}"),
        }
    }

    #[test]
    fn ack_frame_outcome_accepts_valid_ack_with_source() {
        let bytes = build_ack("sv", 42);
        let outcome = TerminalWs::ack_frame_outcome(&bytes, RendererSource::Desktop);
        let (offset, session_id, source) = outcome.expect("合法 ack 帧应被接受");
        assert_eq!(offset, 42);
        assert_eq!(session_id, "sv");
        assert!(source.is_desktop());
    }

    #[test]
    fn ack_frame_outcome_ignores_malformed_frames() {
        // 截断帧（不足 16 字节帧头）
        assert!(TerminalWs::ack_frame_outcome(&[0x54, 0x42, 3, 0x02], RendererSource::Desktop).is_none());
        // 错误 magic
        let mut bad_magic = build_ack("sv", 1);
        bad_magic[0] = 0x00;
        assert!(TerminalWs::ack_frame_outcome(&bad_magic, RendererSource::Desktop).is_none());
        // 错误版本（既非 v2 也非 v3）
        let mut bad_version = build_ack("sv", 1);
        bad_version[2] = 4;
        assert!(TerminalWs::ack_frame_outcome(&bad_version, RendererSource::Desktop).is_none());
        // 非 ack 标志（flags 不含 0x02）
        let mut non_ack = build_ack("sv", 1);
        non_ack[3] = 0x01;
        assert!(TerminalWs::ack_frame_outcome(&non_ack, RendererSource::Desktop).is_none());
        // 非 UTF-8 payload（0xFF 非法 UTF-8）
        let mut bad_utf8 = Vec::new();
        bad_utf8.extend_from_slice(&[0x54, 0x42, 3, 0x02]);
        bad_utf8.extend_from_slice(&1u64.to_le_bytes());
        bad_utf8.extend_from_slice(&1u32.to_le_bytes());
        bad_utf8.push(0xFF);
        assert!(TerminalWs::ack_frame_outcome(&bad_utf8, RendererSource::Desktop).is_none());
    }

    // ==================== 断连清理（stopping 前半段） ====================

    #[tokio::test]
    async fn cleanup_subscription_state_aborts_all_and_bumps_generation() {
        let mut pull_tasks = HashMap::new();
        let mut generations = HashMap::new();
        let mut modes = HashMap::new();

        // 两条订阅链路（不同 key），各挂一对永不完成的执行体/桥接任务
        let gen1 = Arc::new(AtomicU64::new(0));
        let gen2 = Arc::new(AtomicU64::new(0));
        for (key, gen) in [("c1:sv", &gen1), ("c2:sv2", &gen2)] {
            pull_tasks.insert(
                key.to_string(),
                PullTasks {
                    subscriber: tokio::spawn(std::future::pending::<()>()),
                    bridge: tokio::spawn(std::future::pending::<()>()),
                },
            );
            generations.insert(key.to_string(), Arc::clone(gen));
            modes.insert(key.to_string(), Arc::new(AtomicU8::new(MODE_REALTIME)));
        }

        TerminalWs::cleanup_subscription_state(&mut pull_tasks, &mut generations, &mut modes);

        // 任务表/代数表/模式表全部清空
        assert!(pull_tasks.is_empty(), "pull_tasks 必须全部清空");
        assert!(generations.is_empty(), "stream_generations 必须全部清空");
        assert!(modes.is_empty(), "subscriber_modes 必须全部清空");
        // 流代数全部 +1（残留帧校验依据）
        assert_eq!(gen1.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(gen2.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn cleanup_subscription_state_empty_maps_is_noop() {
        let mut pull_tasks = HashMap::new();
        let mut generations = HashMap::new();
        let mut modes = HashMap::new();
        TerminalWs::cleanup_subscription_state(&mut pull_tasks, &mut generations, &mut modes);
        assert!(pull_tasks.is_empty() && generations.is_empty() && modes.is_empty());
    }
}
