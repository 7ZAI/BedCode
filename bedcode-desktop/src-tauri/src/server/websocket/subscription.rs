//! WS 连接级「输出订阅」原语（拉取模型链路 + 背压 ack）
//!
//! 一段订阅 = 订阅者执行体（环上按游标拉取 + 合帧 + 窗口门控） + 桥接
//! （`ForwardOutput` → WS actor 消息）。任务组与连接生命周期绑定：订阅者被替换 /
//! 取消订阅 / 连接断开时整体 abort，旧链路已投递到 actor 邮箱的残留帧由**流代数**
//! 门控丢弃，保证同连接同一会话始终只有一条输出流（移动端字节游标错位 →
//! 连续性违反 → 重订阅风暴的根源）。
//!
//! 该原语被两个通道共用：终端通道（`/ws/terminal/session/{id}` 控制帧 subscribe）
//! 与事件通道（`/ws/event` 旧 `Message::Terminal(Subscribe)` 兼容面）。

use actix::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use super::conn::{ConnCtx, TerminateConnection, WsConnBase};
use super::message::Message;
use super::terminal_ws::control_frame::{self, ServerFrame};
use super::terminal_ws::{forward, subscriber};
use crate::session::{GlobalOutputManager, RendererSource};
use crate::system::error_boundary::spawn_with_error_boundary;

/// 转发统计打点帧数（链路调试字节对账；不打逐帧 WS 发送日志，防输出风暴刷屏）
const FORWARD_STATS_FRAMES: u64 = 100;

// ==================== 订阅链路消息 ====================

/// 拉取模型订阅就绪（异步装配任务 → actor）
///
/// 装配在异步任务里完成（读会话管理器 + 登记句柄 + 起订阅者执行体），
/// 任务把「执行体 JoinHandle + 交接通道接收端」交回 actor 持有，才能与
/// 连接生命周期绑定（abort 于替换/退订/断连）
#[derive(Message)]
#[rtype(result = "()")]
pub(crate) struct PullSubscriberReady {
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

/// 取消订阅结果消息
#[derive(Message)]
#[rtype(result = "()")]
struct UnsubscribeResult {
    session_id: String,
    /// 原始请求的 message_id，用于匹配客户端的 pending 请求
    request_id: String,
    success: bool,
}

// ==================== 订阅状态 ====================

/// 单连接内的订阅状态（任务表 / 流代数 / 传播模式）
pub(crate) struct SubscriptionState {
    /// 拉取模型订阅链路任务表（key = `client_id:session_id`）
    ///
    /// 每链路 = 订阅者执行体（环拉取）+ 桥接（ForwardOutput → WS actor）。
    /// 订阅者被替换 / 取消订阅 / 连接断开时整体 abort：旧链路已投递到 actor
    /// 邮箱的残留帧由流代数门控丢弃，保证同连接同一会话始终只有一条输出流
    pull_tasks: HashMap<String, PullTasks>,
    /// 输出流代数（key = `client_id:session_id` → AtomicU64）
    ///
    /// 订阅 / 取消订阅 / 断连时递增；桥接下发的每一帧都携带代数，
    /// 旧代残留帧（abort 异步取消窗口内已投递到 actor 邮箱的帧）直接丢弃——
    /// 与 abort 互补，杜绝旧流帧注入新订阅通道（移动端字节游标错位 →
    /// 连续性违反 → 重订阅风暴的根源）
    stream_generations: HashMap<String, Arc<AtomicU64>>,
    /// 订阅者实时传播模式（key = `client_id:session_id` → AtomicU8；双速，
    /// 用户需求 3）：realtime（进终端页，读即传）/ batch（退出终端页但
    /// 会话未停，满 terminal.batch_bytes 才转发）。由 SetMode 控制帧实时
    /// 切换，订阅者执行体每次循环读取；重订阅时重置为 realtime
    subscriber_modes: HashMap<String, Arc<AtomicU8>>,
}

impl SubscriptionState {
    pub(crate) fn new() -> Self {
        Self {
            pull_tasks: HashMap::new(),
            stream_generations: HashMap::new(),
            subscriber_modes: HashMap::new(),
        }
    }

    /// 断连清理纯逻辑（`stopping` 前半段，供测试）：中止全部输出转发/订阅任务、
    /// 流代数全部失效（+1）、清空订阅者模式表。返回后三个表均为空——abort 的
    /// JoinHandle 在异步取消窗口内仍可能把残留帧投递到 actor 邮箱，代数递增与
    /// abort 互补：actor 侧按代数丢弃旧代残留帧，杜绝旧流帧注入
    /// 新订阅通道（移动端字节游标错位 → 连续性违反 → 重订阅风暴的根源）
    pub(crate) fn cleanup_subscription_state(
        pull_tasks: &mut HashMap<String, PullTasks>,
        stream_generations: &mut HashMap<String, Arc<AtomicU64>>,
        subscriber_modes: &mut HashMap<String, Arc<AtomicU8>>,
    ) {
        // 中止所有订阅链路任务：连接已断开，残留缓冲帧不再需要投递
        for (_, tasks) in pull_tasks.drain() {
            tasks.abort();
        }
        // 流代数全部失效：abort 异步取消窗口内仍可能发出的残留帧直接丢弃
        for (_, gen) in stream_generations.drain() {
            gen.fetch_add(1, Ordering::SeqCst);
        }
        // 订阅者模式原子随连接销毁（SetMode 仅存活于连接生命周期）
        subscriber_modes.clear();
    }

    /// 连接关闭时的整体清理
    pub(crate) fn cleanup(&mut self) {
        Self::cleanup_subscription_state(
            &mut self.pull_tasks,
            &mut self.stream_generations,
            &mut self.subscriber_modes,
        );
    }

    /// 订阅前替换链路：代数递增（旧流残留帧立即失效）+ 中止旧任务组。
    /// 返回本链路的新代数（桥接携带它下发帧；actor 侧按代数丢弃旧流帧）
    fn bump_generation(&mut self, key: &str) -> u64 {
        if let Some(prev) = self.pull_tasks.remove(key) {
            tracing::debug!(key = %key, "[WsConnBase] aborting previous subscriber tasks");
            prev.abort();
        }
        let generation = self
            .stream_generations
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(AtomicU64::new(0)))
            .clone();
        generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 流代数是否仍为当前代：false = 旧流残留帧，直接丢弃
    fn generation_current(&self, key: &str, generation: u64) -> bool {
        match self.stream_generations.get(key) {
            Some(cur) => cur.load(Ordering::SeqCst) == generation,
            None => false,
        }
    }

    /// 取（必要时创建）该链路的传播模式原子，并重置为 realtime
    ///
    /// 重订阅 / 新建订阅统一重置（进终端页即读即传），随后由 SetMode 实时切换
    fn reset_mode_entry(&mut self, key: &str) -> Arc<AtomicU8> {
        let mode = self
            .subscriber_modes
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(AtomicU8::new(forward::MODE_REALTIME)))
            .clone();
        mode.store(forward::MODE_REALTIME, Ordering::SeqCst);
        mode
    }

    /// 更新该链路的传播模式；链路未订阅（无模式原子）→ false
    fn set_mode(&self, key: &str, mode: u8) -> bool {
        match self.subscriber_modes.get(key) {
            Some(atomic) => {
                atomic.store(mode, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    /// 登记本链路的任务组（替换旧任务组）
    fn insert_tasks(&mut self, key: String, tasks: PullTasks) {
        if let Some(prev) = self.pull_tasks.insert(key, tasks) {
            // 极端竞态（并发重订阅）：后到者仍以最新一代为准
            prev.abort();
        }
    }

    /// 摘除该链路的模式原子（退订）
    fn remove_mode(&mut self, key: &str) {
        self.subscriber_modes.remove(key);
    }
}

// ==================== 订阅原语（连接级） ====================

impl WsConnBase {
    /// 订阅本连接指定会话的输出流（拉取模型）
    ///
    /// - `request_id`：Some = 旧 Message 路由（回 subscribe_response 并登记
    ///   subscribed_sessions）；None = 终端路由控制帧协议（回 subscribe_ok）
    /// - `from_offset`：字节锚点（None = 全量回放）
    /// - `track_mode`：是否启用双速传播模式（终端路由启用；旧路由不启用，
    ///   使用一次性 mode 原子）
    ///
    /// 装配（读会话管理器 + 登记句柄 + 起订阅者执行体）在异步任务中完成，
    /// 产物交回 actor 持有（任务组与连接生命周期绑定）
    pub(crate) fn subscribe_output(
        &mut self,
        session_id: String,
        from_offset: Option<u64>,
        request_id: Option<String>,
        track_mode: bool,
        ctx: &mut ConnCtx,
    ) {
        let client_id = self.session.addr.to_string();
        let key = format!("{client_id}:{session_id}");
        // 替换链路：代数递增（旧流残留帧立即失效）+ abort 旧任务组
        let my_gen = self.subscriptions.bump_generation(&key);

        // 传播模式原子：终端路由重订阅/新建订阅重置为 realtime（进终端页即读即传），
        // 由 SetMode 控制帧实时切换为 batch（退出终端页），支持双速传播；
        // 旧 Message 路由不支持 SetMode，用不登记的一次性原子
        let mode = if track_mode {
            self.subscriptions.reset_mode_entry(&key)
        } else {
            Arc::new(AtomicU8::new(forward::MODE_REALTIME))
        };

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
                    request_id,
                    ready,
                })
                .await;
        });
    }

    /// 更新绑定会话订阅者的传播模式（双速，用户需求 3）
    ///
    /// 订阅者执行体每次循环读取该原子实现即时生效；未订阅（连接建立后未发
    /// subscribe）时返回 false（订阅时统一重置为 realtime）
    pub(crate) fn set_output_mode(&self, session_id: &str, mode: u8) -> bool {
        let client_id = self.session.addr.to_string();
        let key = format!("{client_id}:{session_id}");
        self.subscriptions.set_mode(&key, mode)
    }

    /// 取消订阅指定会话（旧 Message 路由）：代数递增 + 中止本链路任务组，
    /// 结果经 [`UnsubscribeResult`] 回 actor
    pub(crate) fn unsubscribe_output(&mut self, session_id: String, message_id: String, ctx: &mut ConnCtx) {
        let global_manager = GlobalOutputManager::global();
        let client_id = self.session.addr.to_string();
        let key = format!("{}:{}", client_id, session_id);
        let addr = ctx.address();
        let request_id = message_id;

        // 代数递增 + 中止本链路任务组：旧流残留帧被代数校验丢弃，
        // 不会与后续新订阅的流交错（其余订阅者不受影响）
        let _ = self.subscriptions.bump_generation(&key);
        self.subscriptions.remove_mode(&key);

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

    /// 处理客户端背压 ack 帧（spec §4.6 背压下移）：解析后推进**该订阅者私有**
    /// 的 ack 水位（只解除/施加本订阅者的窗口驻留，不参与任何共享记账）。
    /// 非法帧（未知二进制）仅记日志，不中断连接——ack 尽力而为，丢失由驻留
    /// 兜底轮询与僵尸回收兜底，不缺字节不丢帧
    pub(crate) fn handle_ack_binary(&self, bytes: &[u8], _ctx: &mut ConnCtx) {
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
    pub(crate) fn ack_frame_outcome(bytes: &[u8], source: RendererSource) -> Option<(u64, String, RendererSource)> {
        control_frame::parse_ack_frame(bytes)
            .ok()
            .map(|(acked_offset, session_id)| (acked_offset, session_id, source))
    }

    /// 输出桥接循环：订阅者交接通道（ForwardOutput）→ WS actor 消息
    ///
    /// - 二进制帧：`TerminalOutputBinary`（带流代数，旧流残留帧被 actor 丢弃）
    /// - 控制帧（history_end / resync / error）：仅终端路由透传（`forward_control`），
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
}

// ==================== Actor Message Handlers（订阅链路） ====================

/// 处理拉取模型订阅就绪（异步装配任务 → actor）
///
/// 关键顺序约束：**握手帧必须先于桥接启动**发出（前端据此进入 HISTORY 分发
/// 模式；桥接一旦启动即可能推送历史/实时帧）。此处先同步写握手帧，再起桥接
/// 任务，帧序天然成立（无需 oneshot 前置返回）。
impl Handler<PullSubscriberReady> for WsConnBase {
    type Result = ();

    fn handle(&mut self, msg: PullSubscriberReady, ctx: &mut Self::Context) {
        let PullSubscriberReady {
            key,
            session_id,
            generation,
            request_id,
            ready,
        } = msg;

        // 会话不存在：终端路由 error + 关闭（与认证时一致的错误流）；旧路由回 error 消息
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
                    let error =
                        Message::error_with_id(rid, "SESSION_NOT_FOUND", &format!("Session {session_id} not found"));
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

        // 桥接：交接通道 → actor 消息（控制帧仅终端路由透传）
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
        self.subscriptions.insert_tasks(key, tasks);
    }
}

/// 处理取消订阅结果
impl Handler<UnsubscribeResult> for WsConnBase {
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

/// 处理终端输出转发（二进制帧）
impl Handler<TerminalOutputBinary> for WsConnBase {
    type Result = ();

    fn handle(&mut self, msg: TerminalOutputBinary, ctx: &mut Self::Context) {
        // 流代数门控：abort 异步取消窗口内旧链路仍可能投递残留帧，
        // 代数不符直接丢弃（杜绝旧流帧注入新订阅通道）
        if !self.subscriptions.generation_current(&msg.stream_key, msg.generation) {
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
impl Handler<TerminalControlFrame> for WsConnBase {
    type Result = ();

    fn handle(&mut self, msg: TerminalControlFrame, ctx: &mut Self::Context) {
        if !self.subscriptions.generation_current(&msg.stream_key, msg.generation) {
            return;
        }
        self.send_text_filtered(msg.text, ctx);
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::websocket::terminal_ws::forward::MODE_REALTIME;

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

    // ==================== ack 帧处理（handle_ack_binary） ====================

    #[test]
    fn ack_source_for_remote_channel_uses_device_name() {
        match WsConnBase::ack_source_for(Some("Pixel 9")) {
            RendererSource::Mobile { device_name } => assert_eq!(device_name, "Pixel 9"),
            other => panic!("expected Mobile source, got {other:?}"),
        }
    }

    #[test]
    fn ack_source_for_unauthenticated_falls_back_to_desktop() {
        match WsConnBase::ack_source_for(None) {
            RendererSource::Desktop => {}
            other => panic!("expected Desktop source, got {other:?}"),
        }
    }

    #[test]
    fn ack_frame_outcome_accepts_valid_ack_with_source() {
        let bytes = build_ack("sv", 42);
        let outcome = WsConnBase::ack_frame_outcome(&bytes, RendererSource::Desktop);
        let (offset, session_id, source) = outcome.expect("合法 ack 帧应被接受");
        assert_eq!(offset, 42);
        assert_eq!(session_id, "sv");
        assert!(source.is_desktop());
    }

    #[test]
    fn ack_frame_outcome_ignores_malformed_frames() {
        // 截断帧（不足 16 字节帧头）
        assert!(WsConnBase::ack_frame_outcome(&[0x54, 0x42, 3, 0x02], RendererSource::Desktop).is_none());
        // 错误 magic
        let mut bad_magic = build_ack("sv", 1);
        bad_magic[0] = 0x00;
        assert!(WsConnBase::ack_frame_outcome(&bad_magic, RendererSource::Desktop).is_none());
        // 错误版本（既非 v2 也非 v3）
        let mut bad_version = build_ack("sv", 1);
        bad_version[2] = 4;
        assert!(WsConnBase::ack_frame_outcome(&bad_version, RendererSource::Desktop).is_none());
        // 非 ack 标志（flags 不含 0x02）
        let mut non_ack = build_ack("sv", 1);
        non_ack[3] = 0x01;
        assert!(WsConnBase::ack_frame_outcome(&non_ack, RendererSource::Desktop).is_none());
        // 非 UTF-8 payload（0xFF 非法 UTF-8）
        let mut bad_utf8 = Vec::new();
        bad_utf8.extend_from_slice(&[0x54, 0x42, 3, 0x02]);
        bad_utf8.extend_from_slice(&1u64.to_le_bytes());
        bad_utf8.extend_from_slice(&1u32.to_le_bytes());
        bad_utf8.push(0xFF);
        assert!(WsConnBase::ack_frame_outcome(&bad_utf8, RendererSource::Desktop).is_none());
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

        SubscriptionState::cleanup_subscription_state(&mut pull_tasks, &mut generations, &mut modes);

        // 任务表/代数表/模式表全部清空
        assert!(pull_tasks.is_empty(), "pull_tasks 必须全部清空");
        assert!(generations.is_empty(), "stream_generations 必须全部清空");
        assert!(modes.is_empty(), "subscriber_modes 必须全部清空");
        // 流代数全部 +1（残留帧校验依据）
        assert_eq!(gen1.load(Ordering::SeqCst), 1);
        assert_eq!(gen2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cleanup_subscription_state_empty_maps_is_noop() {
        let mut pull_tasks = HashMap::new();
        let mut generations = HashMap::new();
        let mut modes = HashMap::new();
        SubscriptionState::cleanup_subscription_state(&mut pull_tasks, &mut generations, &mut modes);
        assert!(pull_tasks.is_empty() && generations.is_empty() && modes.is_empty());
    }
}
