//! 拉取模型订阅者执行体（每订阅链路一个独立执行体）
//!
//! spec §4.1/§4.3：源只负责「读 PTY + 入环 + 通告水印」；本模块的执行体持
//! **自己的位置指针**在会话输出环上循环拉取 —— 合帧 → 窗口门控 → 转发。
//! ack 背压归属该任务自身，落后/驻留/截断只影响它这一路（I4/I5），
//! 不会以任何形式阻塞源侧产出或其他订阅者。
//!
//! 测试友好：注入 mock 环 + 内存 `out_tx` 即可脱离 WS 断言全部不变量
//!（见模块内 tests：顺序零丢失 / 窗口门控 / 驻留不影响源 / 多订阅者隔离 /
//! 截断重同步 / 空历史边界 / 僵尸回收 / 模式切换）。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch, RwLock};

use super::forward::{ForwardOutput, OutputBuffer, MODE_REALTIME};
use crate::session::{SubscriberHandle, UnifiedOutputQueue};
use crate::system::config::AppConfig;

/// 订阅者统计打点间隔（定期对账用；不打逐帧日志，防输出风暴刷屏）
const SUBSCRIBER_STATS_INTERVAL: Duration = Duration::from_secs(5);

/// 拉取订阅者执行体配置（默认值来源见 spec §8 / `TerminalConfig`）
#[derive(Debug, Clone)]
pub struct SubscriberCfg {
    /// 窗口高位水：`已发游标 − ack ≥ high_water` → 驻留等待 ack
    pub high_water: u64,
    /// 窗口低位水（滞回下沿）：驻留后 `窗口 ≤ low_water` → 解除
    pub low_water: u64,
    /// 驻留兜底轮询间隔（防丢失唤醒 + 僵尸判定节拍）
    pub park_poll: Duration,
    /// 僵尸判定：窗口持续不降超过该时长 → 回收该订阅者
    pub zombie_timeout: Duration,
    /// 合帧时间窗（`Duration::ZERO` = 零缓冲直通，本地环回通道语义）
    pub flush_interval: Duration,
    /// 字节窗：realtime 模式下批次达该字节数立即 flush
    pub max_buffer_size: usize,
    /// batch 模式批次阈值（无时间窗）
    pub batch_bytes: usize,
}

impl SubscriberCfg {
    /// 从全局配置构造；并给出水位预算违规告警（ticket 06）
    pub fn from_app_config() -> Self {
        let cfg = AppConfig::global();
        let term = &cfg.terminal;
        if let Some(violation) = term.subscriber_budget_violation(cfg.channels.global_queue_max_bytes) {
            tracing::warn!(
                violation = %violation,
                high_water = term.subscriber_high_water_bytes,
                low_water = term.subscriber_low_water_bytes,
                "subscriber watermark budget invalid, park/unpark may deadlock"
            );
        }
        let high = term.subscriber_high_water_bytes;
        let high_usize = usize::try_from(high).unwrap_or(usize::MAX);
        // 单帧上限不得超过窗口高位水：大于窗口的帧无法被窗口节流约束
        //（预算关系见 TerminalConfig::subscriber_budget_violation）。钳制不静默——
        // 它会让 batch 模式提前出帧，必须留痕便于对账
        let max_buffer_size = term.max_buffer_size.min(high_usize);
        let batch_bytes = term.batch_bytes.min(high_usize);
        if max_buffer_size != term.max_buffer_size || batch_bytes != term.batch_bytes {
            tracing::warn!(
                configured_max_buffer_size = term.max_buffer_size,
                configured_batch_bytes = term.batch_bytes,
                high_water_bytes = high,
                max_buffer_size,
                batch_bytes,
                "subscriber frame size clamped to window high water"
            );
        }
        Self {
            high_water: high,
            low_water: term.subscriber_low_water_bytes,
            park_poll: Duration::from_millis(term.subscriber_park_poll_ms.max(1)),
            zombie_timeout: Duration::from_millis(term.subscriber_zombie_timeout_ms.max(1)),
            flush_interval: Duration::from_millis(term.flush_interval_ms),
            max_buffer_size,
            batch_bytes,
        }
    }

    /// 远程订阅通道（移动端 WS / 旧 Message 路由）：按时合并开关决定
    /// 合并窗口；关闭合并（`merge_output=false`）时零缓冲直通
    pub fn for_remote_route() -> Self {
        let mut cfg = Self::from_app_config();
        if !AppConfig::global().terminal.merge_output {
            cfg.flush_interval = Duration::ZERO;
        }
        cfg
    }

    /// 桌面本地通道（Tauri Channel）：沿用该通道既有的固定合并窗口
    pub fn for_local_route(flush_interval: Duration) -> Self {
        let mut cfg = Self::from_app_config();
        cfg.flush_interval = flush_interval;
        cfg
    }
}

/// 每订阅链路输出交接通道容量
///
/// 短途交接而非背压依据：真正的节流是订阅者窗口门控。容量 × 单帧上限
/// 构成窗口之外的有界余量（预算见 `TerminalConfig` 注释）
pub(crate) const OUT_CHANNEL_CAPACITY: usize = 4;

/// 已启动的订阅者链路（执行体任务 + 交接通道接收端）
pub(crate) struct SpawnedSubscriber {
    /// 订阅成功元数据（subscribe_ok 三件套）
    pub response: crate::session::SubscribeResponse,
    /// 交接通道接收端（调用方负责桥接到具体下游）
    pub out_rx: mpsc::Receiver<ForwardOutput>,
    /// 执行体任务句柄（替换/退订/断连时 abort）
    pub task: tokio::task::JoinHandle<()>,
}

/// 启动一条拉取模型订阅链路：登记句柄 + 起订阅者执行体
///
/// 调用方持有 `out_rx` 并负责桥接到具体下游（WS actor / Tauri Channel）；
/// 丢弃 `out_rx` 会让执行体的发送失败并自然退出（无孤儿任务）。
pub(crate) async fn spawn_subscriber(
    manager: &Arc<crate::session::SessionOutputManager>,
    client_id: &str,
    from_offset: Option<u64>,
    mode: Arc<std::sync::atomic::AtomicU8>,
    cfg: SubscriberCfg,
) -> SpawnedSubscriber {
    let pull = manager.register_subscriber(client_id, from_offset, mode).await;
    let (out_tx, out_rx) = mpsc::channel::<ForwardOutput>(OUT_CHANNEL_CAPACITY);
    let handle = pull.handle.clone();
    let task = crate::system::error_boundary::spawn_with_error_boundary(
        "terminal_subscriber",
        subscriber_loop(manager.ring(), pull.max_watch, handle, out_tx, cfg),
    );
    SpawnedSubscriber {
        response: pull.response,
        out_rx,
        task,
    }
}

/// 订阅者执行体（`spawn_with_error_boundary("terminal_subscriber", …)` 启动）
///
/// 生命周期：`out_tx` 关闭（WS actor 退出）或会话产出端结束（watch 发送端 drop）
/// 或僵尸回收 → 返回。
pub(crate) async fn subscriber_loop(
    ring: Arc<RwLock<UnifiedOutputQueue>>,
    mut max_watch: watch::Receiver<u64>,
    sub: Arc<SubscriberHandle>,
    out_tx: mpsc::Sender<ForwardOutput>,
    cfg: SubscriberCfg,
) {
    let mut next = sub.start_offset;
    let mut snapshot = sub.snapshot_offset;
    let mut history_done = false;
    let mut planner = OutputBuffer::new();
    let mut last_flush = tokio::time::Instant::now();
    let mut last_mode = sub.mode.load(Ordering::SeqCst);
    let mut parked_since: Option<tokio::time::Instant> = None;
    let mut stats_tick = tokio::time::interval(SUBSCRIBER_STATS_INTERVAL);
    stats_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // 订阅起点早于驻留起点（订阅即截断）：先发显式重同步信号（§4.7），
    // 客户端清屏 + 以 min_offset 重锚；随后 min..snapshot 由重播帧覆盖
    if sub.stats.truncated_on_subscribe.load(Ordering::SeqCst) {
        let (min, max) = ring.read().await.watermarks();
        sub.stats.truncated_count.fetch_add(1, Ordering::SeqCst);
        if emit_resync(&out_tx, &sub, min, snapshot).await.is_err() {
            return;
        }
        next = next.clamp(min, max);
        sub.set_next_offset(next);
        // resync 帧自带历史边界：不再补发 HistoryEnd（§4.7；重播帧即恢复负载）
        history_done = true;
    }

    'outer: loop {
        // 退订/被替换（句柄已移除）→ 任务退出（下一次唤醒检查点）
        if sub.is_retired() {
            tracing::debug!(
                session_id = %sub.session_id,
                client_id = %sub.client_id,
                "subscriber loop exiting: handle retired (unsubscribed or replaced)"
            );
            break 'outer;
        }

        // ---------- 截断局部化（I4）：所需字节已淘汰 → 仅本订阅者重同步 ----------
        {
            let (min, max) = ring.read().await.watermarks();
            if next < min {
                sub.stats.truncated_count.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    next_offset = next,
                    min_offset = min,
                    lag_bytes = max.saturating_sub(next),
                    "subscriber truncation: cursor evicted from ring, resync"
                );
                // 先落盘残留批次（帧头区间必须与负载一致），再清屏重锚
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                if emit_resync(&out_tx, &sub, min, max).await.is_err() {
                    break 'outer;
                }
                next = min;
                snapshot = max;
                // resync 帧自带历史边界：此后无需再发 HistoryEnd
                history_done = true;
                sub.set_next_offset(next);
                last_flush = tokio::time::Instant::now();
            }
        }

        // ---------- 推进（合帧 → 窗口门控 → 转发） ----------
        loop {
            let (cur_min, cur_max) = ring.read().await.watermarks();

            // 历史边界（I7）必须在「追平即退出」之前判定：订阅时恰好无历史
            // （next == snapshot == max）时若先 break，客户端永远等不到
            // history_end → 历史拼接卡到超时（移动端表现为「进页面不出内容」）
            if !history_done && next >= snapshot {
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
                if out_tx
                    .send(ForwardOutput::HistoryEnd {
                        snapshot_offset: snapshot,
                        min_offset: cur_min,
                        history_bytes: cur_max.saturating_sub(cur_min),
                    })
                    .await
                    .is_err()
                {
                    break 'outer;
                }
                history_done = true;
                tracing::debug!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    snapshot_offset = snapshot,
                    min_offset = cur_min,
                    "subscriber history segment done"
                );
            }

            if next >= cur_max {
                break; // 追平产出端，回去等唤醒
            }

            // 窗口门控（I5）：越位即驻留——只停自己，源侧产出与其他订阅者无感
            let window = next.saturating_sub(sub.acked_offset());
            if window >= cfg.high_water {
                if parked_since.is_none() {
                    parked_since = Some(tokio::time::Instant::now());
                    sub.stats.park_count.fetch_add(1, Ordering::SeqCst);
                    tracing::debug!(
                        session_id = %sub.session_id,
                        client_id = %sub.client_id,
                        next_offset = next,
                        acked_offset = sub.acked_offset(),
                        window_bytes = window,
                        high_water = cfg.high_water,
                        "subscriber park entered"
                    );
                }
                match park_until_ack(&sub, &mut max_watch, &cfg, next).await {
                    ParkExit::Ack => {}
                    ParkExit::Zombie => {
                        // 僵尸订阅者回收（§4.8）：窗口持续不降 = ack 不动 → 客户端已死。
                        // 判据只看窗口（不看 next_offset），避免把「渲染慢」误判为僵尸
                        tracing::warn!(
                            session_id = %sub.session_id,
                            client_id = %sub.client_id,
                            lag_bytes = next.saturating_sub(sub.acked_offset()),
                            parked_ms = parked_since.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0),
                            zombie_timeout_ms = cfg.zombie_timeout.as_millis() as u64,
                            "zombie subscriber reclaimed (window never decreased)"
                        );
                        // 尽力下发终止信号（下游已关则忽略）：路由侧据此关闭连接/回收句柄
                        if out_tx
                            .send(ForwardOutput::Terminate {
                                code: "lag_truncated".to_string(),
                                message: "subscriber stalled: no ack within zombie timeout".to_string(),
                            })
                            .await
                            .is_err()
                        {
                            tracing::debug!(
                                session_id = %sub.session_id,
                                client_id = %sub.client_id,
                                "subscriber terminate frame dropped (downstream closed)"
                            );
                        }
                        break 'outer;
                    }
                    // 退订/被替换或会话产出端结束：安静退出（不是僵尸，不下发终止帧）
                    ParkExit::Stop => break 'outer,
                }
                continue;
            }
            if let Some(since) = parked_since.take() {
                let parked = since.elapsed().as_millis() as u64;
                sub.stats.parked_ms.fetch_add(parked, Ordering::SeqCst);
                tracing::debug!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    next_offset = next,
                    acked_offset = sub.acked_offset(),
                    parked_ms = parked,
                    "subscriber park exited"
                );
            }

            // 模式翻转即时 flush 残留批次（双速语义：不把累积批次滞留到下一次触发）
            let cur_mode = sub.mode.load(Ordering::SeqCst);
            if cur_mode != last_mode {
                last_mode = cur_mode;
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
                tracing::debug!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    mode = cur_mode,
                    "subscriber mode switched, residual flushed"
                );
            }

            // 取一块（零拷贝视图；单块返回，跨块合帧由本任务按连续性决定）
            let slice = match ring.read().await.read_at(next) {
                Ok(Some(slice)) => slice,
                Ok(None) => break,            // 追平
                Err(_min) => continue 'outer, // 读锁间隙被淘汰 → 外层重同步
            };
            // 连续性优先：带洞/重叠先落盘当前批次，保证帧头区间 = 负载
            if !planner.is_contiguous_with(slice.start_offset) {
                tracing::debug!(
                    session_id = %sub.session_id,
                    slice_start = slice.start_offset,
                    buffer_end = planner.end_offset,
                    "subscriber batch split on offset hole"
                );
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }
            planner.append_slice(slice.start_offset, &slice.bytes, slice.end_is_waiting);
            next = slice.end_offset();
            sub.set_next_offset(next);

            if planner.should_flush(
                last_mode,
                cfg.batch_bytes,
                cfg.max_buffer_size,
                last_flush.elapsed(),
                cfg.flush_interval,
            ) {
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }
        }

        // 退订/被替换：进入等待前再查一次（`notify_waiters` 对尚未注册的
        // waiter 是 no-op，故不能只依赖唤醒信号）
        if sub.is_retired() {
            tracing::debug!(
                session_id = %sub.session_id,
                client_id = %sub.client_id,
                "subscriber loop exiting: handle retired (unsubscribed or replaced)"
            );
            break 'outer;
        }

        // ---------- 等唤醒：新数据（watch）/ ack / 时间窗到 / 统计打点 ----------
        // realtime 且缓冲有残留：时间窗到即 flush（延迟有界 ≤ flush_interval）；
        // batch 不因时间窗 flush（纯批次语义，未满批次的数据按设计滞留环中）
        let flush_deadline = if !planner.is_empty() && last_mode == MODE_REALTIME && !cfg.flush_interval.is_zero()
        {
            Some(last_flush + cfg.flush_interval)
        } else {
            None
        };
        tokio::select! {
            r = max_watch.changed() => {
                if r.is_err() {
                    // 会话产出端结束（watch 发送端 drop）→ 任务自然退出
                    tracing::debug!(
                        session_id = %sub.session_id,
                        client_id = %sub.client_id,
                        "subscriber loop exiting: output producer closed"
                    );
                    break 'outer;
                }
            }
            _ = sub.wait_ack() => {}
            _ = sleep_until_opt(flush_deadline) => {
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }
            _ = stats_tick.tick() => {
                let (cur_min, cur_max) = ring.read().await.watermarks();
                tracing::debug!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    next_offset = sub.next_offset(),
                    acked_offset = sub.acked_offset(),
                    lag_bytes = cur_max.saturating_sub(next),
                    stream_min_offset = cur_min,
                    park_count = sub.stats.park_count.load(Ordering::SeqCst),
                    parked_ms = sub.stats.parked_ms.load(Ordering::SeqCst),
                    truncated_count = sub.stats.truncated_count.load(Ordering::SeqCst),
                    frames_sent = sub.stats.frames_sent.load(Ordering::SeqCst),
                    bytes_sent = sub.stats.bytes_sent.load(Ordering::SeqCst),
                    "subscriber stats (periodic)"
                );
            }
        }
    }

    // 收尾：残留缓冲尽力落盘（通道已关则丢弃），并入帧/字节统计
    let _ = flush_planner(&mut planner, &out_tx, &sub).await;
    tracing::debug!(
        session_id = %sub.session_id,
        client_id = %sub.client_id,
        next_offset = sub.next_offset(),
        acked_offset = sub.acked_offset(),
        frames_sent = sub.stats.frames_sent.load(Ordering::SeqCst),
        bytes_sent = sub.stats.bytes_sent.load(Ordering::SeqCst),
        "subscriber loop exited"
    );
}

/// 下发重同步信号（§4.7）：同一入口保证「截断留痕 + 信号发送」两件事同源
async fn emit_resync(
    out_tx: &mpsc::Sender<ForwardOutput>,
    sub: &SubscriberHandle,
    min_offset: u64,
    snapshot_offset: u64,
) -> Result<(), ()> {
    tracing::warn!(
        session_id = %sub.session_id,
        client_id = %sub.client_id,
        next_offset = sub.next_offset(),
        min_offset,
        snapshot_offset,
        "subscriber cursor truncated, resync from min_offset"
    );
    out_tx
        .send(ForwardOutput::Resync {
            min_offset,
            snapshot_offset,
        })
        .await
        .map_err(|_| ())
}

/// 落盘合帧缓冲（空缓冲 no-op），并累计发送统计
async fn flush_planner(
    planner: &mut OutputBuffer,
    out_tx: &mpsc::Sender<ForwardOutput>,
    sub: &SubscriberHandle,
) -> Result<(), ()> {
    if planner.is_empty() {
        return Ok(());
    }
    let bytes = planner.len() as u64;
    let frame = planner.flush();
    sub.stats.frames_sent.fetch_add(1, Ordering::SeqCst);
    sub.stats.bytes_sent.fetch_add(bytes, Ordering::SeqCst);
    out_tx.send(frame).await.map_err(|_| ())
}

/// 驻留等待结束原因
enum ParkExit {
    /// ack 推进使窗口回落到低位水以下 → 继续发送
    Ack,
    /// 窗口持续不降超僵尸超时 → 回收该订阅者
    Zombie,
    /// 退订/被替换，或会话产出端结束 → 安静退出（非僵尸，不下发终止帧）
    Stop,
}

/// 驻留等待：ack 推进使窗口回落（≤ low_water）解除；窗口持续不降超时 → 僵尸
///
/// 判据只看「窗口是否下降」（= ack 是否推进），不看 `next_offset`——它已被
/// 驻留停住，用它判僵尸会把「客户端在渲染但很慢」误杀。丢失的 ack 唤醒由
/// `park_poll` 兜底（200ms 级，最坏代价仅是解锁延迟上限，不是功能损失）。
///
/// 同时观察产出端（`max_watch` drop = 会话结束）与句柄退订状态：否则会话已停止
/// 仍会在超时后下发 `lag_truncated` 并关连接（§4.9「watch drop → 任务退出」）
async fn park_until_ack(
    sub: &SubscriberHandle,
    max_watch: &mut watch::Receiver<u64>,
    cfg: &SubscriberCfg,
    next: u64,
) -> ParkExit {
    let mut last_window = next.saturating_sub(sub.acked_offset());
    let mut last_window_change = tokio::time::Instant::now();
    loop {
        if sub.is_retired() {
            return ParkExit::Stop;
        }
        let window = next.saturating_sub(sub.acked_offset());
        if window <= cfg.low_water {
            return ParkExit::Ack;
        }
        if window < last_window {
            last_window = window;
            last_window_change = tokio::time::Instant::now();
        } else if last_window_change.elapsed() >= cfg.zombie_timeout {
            return ParkExit::Zombie;
        }
        tokio::select! {
            _ = sub.wait_ack() => {}
            r = max_watch.changed() => {
                if r.is_err() {
                    return ParkExit::Stop; // 产出端已结束
                }
            }
            _ = tokio::time::sleep(cfg.park_poll) => {}
        }
    }
}

/// 可选截止时刻的睡眠：None 时永不就绪（用于 `select!` 分支占位）
async fn sleep_until_opt(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(t) => tokio::time::sleep_until(t).await,
        None => std::future::pending::<()>().await,
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{SessionOutputManager, MODE_BATCH};
    use std::sync::atomic::AtomicU8;
    use tokio::sync::mpsc;

    /// 执行体测试夹具：真实会话管理器（环 + watch + 句柄）+ 内存输出通道
    struct Harness {
        manager: Arc<SessionOutputManager>,
        handle: Arc<SubscriberHandle>,
        out_rx: mpsc::Receiver<ForwardOutput>,
        task: tokio::task::JoinHandle<()>,
    }

    /// 向会话管理器入环一段输出（测试用；start_offset 由 on_output 分配）
    async fn push_to(manager: &SessionOutputManager, data: &[u8]) {
        manager
            .on_output(crate::session::OutputEvent {
                session_id: manager.session_id().to_string(),
                data: data.to_vec(),
                start_offset: 0,
                timestamp: 0,
                is_waiting: false,
            })
            .await;
    }

    impl Harness {
        /// 在既有会话管理器上挂一个订阅者执行体（可先写历史再订阅）
        async fn attach(manager: Arc<SessionOutputManager>, from_offset: Option<u64>, cfg: SubscriberCfg) -> Self {
            Self::attach_as(manager, "client-1", from_offset, cfg).await
        }

        /// 指定 client_id 挂订阅者（多订阅者隔离用例）
        async fn attach_as(
            manager: Arc<SessionOutputManager>,
            client_id: &str,
            from_offset: Option<u64>,
            cfg: SubscriberCfg,
        ) -> Self {
            let (out_tx, out_rx) = mpsc::channel::<ForwardOutput>(64);
            let pull = manager
                .register_subscriber(client_id, from_offset, Arc::new(AtomicU8::new(MODE_REALTIME)))
                .await;
            let handle = pull.handle.clone();
            let task = tokio::spawn(subscriber_loop(
                manager.ring(),
                pull.max_watch,
                handle.clone(),
                out_tx,
                cfg,
            ));
            Self {
                manager,
                handle,
                out_rx,
                task,
            }
        }

        async fn start(session_id: &str, from_offset: Option<u64>, cfg: SubscriberCfg) -> Self {
            Self::attach(Arc::new(SessionOutputManager::new(session_id)), from_offset, cfg).await
        }

        async fn push(&self, data: &[u8]) {
            push_to(&self.manager, data).await;
        }

        /// 收帧（超时视为「当前无更多帧」→ None），解出 (start_offset, payload)
        async fn recv_frame(&mut self) -> Option<(u64, Vec<u8>)> {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
            loop {
                match tokio::time::timeout_at(deadline, self.out_rx.recv()).await {
                    Ok(Some(ForwardOutput::Binary(frame))) => {
                        let start = u64::from_le_bytes(frame[4..12].try_into().unwrap());
                        let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
                        return Some((start, frame[16..16 + len].to_vec()));
                    }
                    Ok(Some(_other)) => continue, // 控制帧：跳过，继续找二进制帧
                    Ok(None) | Err(_) => return None,
                }
            }
        }

        /// 收控制帧（Resync / HistoryEnd / Terminate），超时 → None
        async fn recv_control(&mut self) -> Option<ForwardOutput> {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
            loop {
                match tokio::time::timeout_at(deadline, self.out_rx.recv()).await {
                    Ok(Some(ForwardOutput::Binary(_))) => continue,
                    Ok(Some(other)) => return Some(other),
                    Ok(None) | Err(_) => return None,
                }
            }
        }

        /// 排空当前已就绪的全部二进制帧，返回拼接负载
        async fn drain_payload(&mut self) -> Vec<u8> {
            let mut out = Vec::new();
            while let Some((_, payload)) = self.recv_frame().await {
                out.extend_from_slice(&payload);
            }
            out
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// 无窗口门控配置：零缓冲直通 + 窗口无限（只验证顺序/边界语义）
    fn fast_cfg() -> SubscriberCfg {
        SubscriberCfg {
            high_water: u64::MAX,
            low_water: u64::MAX,
            park_poll: Duration::from_millis(5),
            zombie_timeout: Duration::from_secs(5),
            flush_interval: Duration::ZERO,
            max_buffer_size: 64 * 1024,
            batch_bytes: 64 * 1024,
        }
    }

    /// 小窗口配置：高 8B / 低 4B + 短驻留轮询（门控/隔离用例；
    /// 僵尸超时放宽到远超测试时长，避免用例被误回收）
    fn windowed_cfg() -> SubscriberCfg {
        SubscriberCfg {
            high_water: 8,
            low_water: 4,
            park_poll: Duration::from_millis(5),
            zombie_timeout: Duration::from_secs(5),
            ..fast_cfg()
        }
    }

    /// 僵尸回收用例配置：短僵尸超时（窗口停滞 80ms 即回收）
    fn zombie_cfg() -> SubscriberCfg {
        SubscriberCfg {
            zombie_timeout: Duration::from_millis(80),
            ..windowed_cfg()
        }
    }

    // ==================== I1/I3：顺序零丢失 ====================

    #[tokio::test]
    async fn preserves_byte_order_and_completeness() {
        let mut h = Harness::start("s-order", None, fast_cfg()).await;

        // 空历史：仍必须发 HistoryEnd（I7，见用例 empty_history_still_emits_history_end）
        assert!(matches!(
            h.recv_control().await,
            Some(ForwardOutput::HistoryEnd { snapshot_offset: 0, .. })
        ));

        let payload: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
        for chunk in payload.chunks(37) {
            h.push(chunk).await;
        }

        let got = h.drain_payload().await;
        assert_eq!(got, payload, "收到的字节序列必须与产出完全一致（I1/I3）");
        assert_eq!(
            h.handle.next_offset() as usize,
            payload.len(),
            "游标必须严格推进到产出末端"
        );
    }

    // ==================== I7：历史边界 ====================

    #[tokio::test]
    async fn history_frames_precede_history_end_and_live_follows() {
        // 订阅前先写历史（12 字节），再由 harness 订阅：snapshot = 12
        let manager = Arc::new(SessionOutputManager::new("s-history"));
        push_to(&manager, b"aaaabbbbcccc").await;
        let mut h = Harness::attach(manager, None, fast_cfg()).await;

        // 历史帧严格早于 HistoryEnd：把控制帧之前收到的负载全部收集
        let mut history = Vec::new();
        loop {
            let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
            match tokio::time::timeout_at(deadline, h.out_rx.recv()).await {
                Ok(Some(ForwardOutput::Binary(frame))) => {
                    let start = u64::from_le_bytes(frame[4..12].try_into().unwrap());
                    let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
                    assert!(start + len as u64 <= 12, "历史负载不得越过快照边界");
                    history.extend_from_slice(&frame[16..16 + len]);
                }
                Ok(Some(ForwardOutput::HistoryEnd {
                    snapshot_offset,
                    min_offset,
                    history_bytes,
                })) => {
                    assert_eq!(snapshot_offset, 12, "历史边界 = 订阅时刻 max_offset");
                    assert_eq!(min_offset, 0);
                    assert_eq!(history_bytes, 12);
                    break;
                }
                other => panic!("expected history frames then HistoryEnd, got {other:?}"),
            }
        }
        assert_eq!(history, b"aaaabbbbcccc", "历史必须完整重播");

        // 实时帧严格晚于 HistoryEnd（区间起点 ≥ snapshot）
        h.push(b"live-data!").await;
        let (start, payload) = h.recv_frame().await.expect("实时帧必须到达");
        assert_eq!(start, 12, "实时帧起点 = 历史边界（无重无漏）");
        assert_eq!(payload, b"live-data!");
    }

    /// 订阅时 next == snapshot == max（无历史可发）：仍必须发 HistoryEnd，
    /// 否则客户端等不到历史边界，历史拼接卡到超时（移动端「进页面不出内容」）
    #[tokio::test]
    async fn empty_history_still_emits_history_end() {
        let mut h = Harness::start("s-empty", None, fast_cfg()).await;
        let control = h.recv_control().await.expect("空历史也必须发 HistoryEnd");
        assert!(
            matches!(
                control,
                ForwardOutput::HistoryEnd {
                    snapshot_offset: 0,
                    min_offset: 0,
                    history_bytes: 0
                }
            ),
            "空历史边界帧须携带零值元数据"
        );
    }

    // ==================== I5/I2：窗口门控 + 驻留不影响源 ====================

    #[tokio::test]
    async fn window_gate_parks_and_resumes_after_ack() {
        let mut h = Harness::start("s-window", None, windowed_cfg()).await;
        h.recv_control().await; // HistoryEnd

        // 每块 4 字节；窗口高位水 8 → 发两块后驻留（且下一块未发送）
        for _ in 0..3 {
            h.push(b"abcd").await;
        }
        let mut got = Vec::new();
        while let Some((_, payload)) = h.recv_frame().await {
            got.extend_from_slice(&payload);
        }
        assert_eq!(got, b"abcdabcd", "窗口越位后不得继续推进游标（I5）");
        assert!(h.handle.stats.park_count.load(Ordering::SeqCst) >= 1, "必须进入驻留");

        // ack 到 8：窗口 0 ≤ low_water → 解除驻留并补齐剩余字节
        h.handle.on_ack(8);
        let mut got = Vec::new();
        while let Some((_, payload)) = h.recv_frame().await {
            got.extend_from_slice(&payload);
        }
        assert_eq!(got, b"abcd", "ack 后必须补齐剩余字节（零丢失）");
    }

    /// 回归护栏（I2/I5）：A 驻留期间持续 push → 源产出照常增长、B 收齐
    #[tokio::test]
    async fn park_does_not_stall_source_or_other_subscriber() {
        let mut slow = Harness::start("s-isolation", None, windowed_cfg()).await;
        let mut fast = Harness::attach_as(slow.manager.clone(), "client-2", None, fast_cfg()).await;

        slow.recv_control().await; // HistoryEnd
        fast.recv_control().await; // HistoryEnd

        // A 不 ack：快速塞 40 块（160 字节）→ A 驻留在 8 字节窗口
        for _ in 0..40 {
            slow.push(b"abcd").await;
        }
        // B 正常 ack：收齐全部 160 字节
        let mut got = Vec::new();
        for _ in 0..40 {
            match fast.recv_frame().await {
                Some((_, payload)) => got.extend_from_slice(&payload),
                None => break,
            }
            fast.handle.on_ack(fast.handle.next_offset());
        }
        assert_eq!(got.len(), 160, "B 必须收齐全部产出（A 驻留不影响他人）");

        // 源产出照常推进：max_offset 单调增长到 160（A 驻留完全不改变源行为）
        let (_, max) = slow.manager.ring().read().await.watermarks();
        assert_eq!(max, 160, "源产出不得因任何订阅者驻留而停摆（I2）");

        // A 仍被窗口门控（窗口 ≤ 高位水 + 单块余量）
        assert_eq!(slow.handle.next_offset(), 8, "A 游标停在窗口边界");
        assert!(slow.handle.window() <= 12, "A 窗口不得失控增长");
    }

    // ==================== I4：截断本地化 + 重同步信号 ====================

    /// 环淘汰推进 min_offset 越过订阅者游标 → 该订阅者收到 Resync 并从 min 重播；
    /// 其他订阅者无感
    #[tokio::test]
    async fn truncation_emits_resync_and_replays_from_min() {
        let mut h = Harness::start("s-truncate", None, windowed_cfg()).await;
        h.recv_control().await; // HistoryEnd

        // 小环（8 字节上限）放大淘汰效果
        {
            let ring_arc = h.manager.ring();
            let mut ring = ring_arc.write().await;
            *ring = UnifiedOutputQueue::with_limits(8, 100);
        }

        h.push(b"aaaabbbb").await; // 驻留 [0,8)，游标推进到 8（窗口 8 → 驻留）
        // 继续产出：环淘汰最旧 → min_offset 推进越过驻留中的游标 8
        h.push(b"ccccdddd").await; // 淘汰 [0,8) → min=8, max=16
        h.push(b"eeeeffff").await; // 淘汰 [8,16) → min=16, max=24
        h.push(b"gggghhhh").await; // 淘汰 [16,24) → min=24, max=32

        // 客户端恢复 ack（覆盖全部产出）→ 解除驻留 → 发现游标已被淘汰
        h.handle.on_ack(32);

        let control = h.recv_control().await.expect("截断必须产生重同步信号");
        match control {
            ForwardOutput::Resync {
                min_offset,
                snapshot_offset,
            } => {
                assert_eq!(min_offset, 24, "重同步锚点 = 环驻留起点");
                assert_eq!(snapshot_offset, 32, "重同步快照点 = 当前产出端游标");
            }
            other => panic!("expected Resync, got {other:?}"),
        }
        // 随后从 min_offset 连续重播当前驻留区间（[24,32)）
        let got = h.drain_payload().await;
        assert_eq!(got, b"gggghhhh", "重同步后必须从 min_offset 连续重播");
        assert_eq!(h.handle.next_offset(), 32);
    }

    // ==================== I6：ack 单调私有 ====================

    #[tokio::test]
    async fn ack_is_monotonic_and_private() {
        let h = Harness::start("s-ack", None, fast_cfg()).await;
        h.handle.on_ack(100);
        assert_eq!(h.handle.acked_offset(), 100);
        // 陈旧/乱序 ack 不后退（I6）
        h.handle.on_ack(50);
        assert_eq!(h.handle.acked_offset(), 100, "陈旧 ack 必须被忽略");
    }

    // ==================== 零拷贝保活 ====================

    /// `read_at` 返回的 Bytes 视图在该块被环淘汰后仍可安全编码发送（Arc 保活）
    #[tokio::test]
    async fn ring_slice_survives_eviction() {
        let mut ring = UnifiedOutputQueue::with_limits(8, 100);
        ring.push(crate::session::OutputEvent {
            session_id: "s".to_string(),
            data: b"aaaabbbb".to_vec(),
            start_offset: 0,
            timestamp: 0,
            is_waiting: false,
        });
        let slice = ring.read_at(0).unwrap().unwrap();
        assert_eq!(&slice.bytes[..], b"aaaabbbb");

        // 淘汰该块（min_offset 越过 0）
        ring.push(crate::session::OutputEvent {
            session_id: "s".to_string(),
            data: b"ccccdddd".to_vec(),
            start_offset: 0,
            timestamp: 0,
            is_waiting: false,
        });
        assert_eq!(ring.min_offset(), 8, "旧块应被淘汰");

        // 视图仍完整可读（Arc 共享，零拷贝保活）
        assert_eq!(&slice.bytes[..], b"aaaabbbb");
        assert_eq!(slice.end_offset(), 8);
    }

    // ==================== 僵尸回收 ====================

    #[tokio::test]
    async fn zombie_subscriber_is_reclaimed_after_window_stalls() {
        let mut h = Harness::start("s-zombie", None, zombie_cfg()).await;
        h.recv_control().await; // HistoryEnd

        // 塞满窗口且永不 ack：窗口不降 → 超时（80ms）后被回收
        for _ in 0..4 {
            h.push(b"abcd").await;
        }
        let control = h.recv_control().await.expect("僵尸回收必须下发终止信号");
        match control {
            ForwardOutput::Terminate { code, .. } => assert_eq!(code, "lag_truncated"),
            other => panic!("expected Terminate, got {other:?}"),
        }
        tokio::time::timeout(Duration::from_millis(500), &mut h.task)
            .await
            .expect("僵尸判定后任务必须退出")
            .expect("任务正常结束");
    }

    // ==================== 模式切换 ====================

    /// realtime → batch：残留小批立即落盘，不滞留到下一批次
    #[tokio::test]
    async fn mode_switch_flushes_residual_batch() {
        let mut cfg = fast_cfg();
        // 时间窗远大于测试时长：唯一能触发落盘的就是「模式翻转」
        cfg.flush_interval = Duration::from_secs(60);
        cfg.max_buffer_size = 64 * 1024;
        cfg.batch_bytes = 4; // batch 阈值小于单块，避免二次滞留
        let mut h = Harness::start("s-mode", None, cfg).await;
        h.recv_control().await; // HistoryEnd

        h.push(b"ab").await; // realtime：未达字节窗/时间窗 → 滞留缓冲
        tokio::task::yield_now().await;
        h.handle.mode.store(MODE_BATCH, Ordering::SeqCst);
        h.push(b"cdef").await; // 触发循环 → 发现模式翻转 → 残留 "ab" 立即落盘

        let first = h.recv_frame().await.expect("模式翻转必须立即落盘残留批次");
        assert_eq!(first, (0, b"ab".to_vec()), "残留批次区间与负载必须一致");
    }

    // ==================== 订阅即截断（游标过旧） ====================

    /// 订阅起点早于驻留起点（客户端断网重连后游标过旧）→ 立即产出重同步信号
    #[tokio::test]
    async fn stale_start_offset_yields_resync() {
        let manager = Arc::new(SessionOutputManager::new("s-stale"));
        // 小环：3 块后 min_offset=4
        {
            let ring_arc = manager.ring();
            let mut ring = ring_arc.write().await;
            *ring = UnifiedOutputQueue::with_limits(8, 100);
        }
        for _ in 0..3 {
            manager
                .on_output(crate::session::OutputEvent {
                    session_id: "s-stale".to_string(),
                    data: b"abcd".to_vec(),
                    start_offset: 0,
                    timestamp: 0,
                    is_waiting: false,
                })
                .await;
        }
        let (out_tx, mut out_rx) = mpsc::channel::<ForwardOutput>(64);
        let pull = manager
            .register_subscriber("client-1", Some(0), Arc::new(AtomicU8::new(MODE_REALTIME)))
            .await;
        assert_eq!(pull.response.min_offset, 4, "响应须携带最新驻留起点");
        assert_eq!(pull.handle.start_offset, 4, "起播锚点须收敛到 min_offset");
        let task = tokio::spawn(subscriber_loop(
            manager.ring(),
            pull.max_watch,
            pull.handle.clone(),
            out_tx,
            fast_cfg(),
        ));

        // 首帧即 Resync（提示截断 + 清屏重锚）
        let first = tokio::time::timeout(Duration::from_millis(300), out_rx.recv())
            .await
            .expect("订阅即截断必须立即产出重同步信号")
            .expect("通道存活");
        match first {
            ForwardOutput::Resync { min_offset, .. } => assert_eq!(min_offset, 4),
            other => panic!("expected Resync first, got {other:?}"),
        }
        task.abort();
    }

    // ==================== 退订 / 被替换 / 产出端结束（安静退出） ====================

    /// 订阅即截断：重同步帧自带历史边界 → 重播之后**不得**再发 HistoryEnd
    /// （§4.7；客户端已在 resync 处重锚，多余边界帧会与其状态机打架）
    #[tokio::test]
    async fn subscribe_truncation_emits_resync_and_no_history_end() {
        let manager = Arc::new(SessionOutputManager::new("s-subtrunc"));
        {
            let ring_arc = manager.ring();
            let mut ring = ring_arc.write().await;
            *ring = UnifiedOutputQueue::with_limits(8, 100);
        }
        for _ in 0..3 {
            push_to(&manager, b"abcd").await;
        }
        // 游标 0 已被淘汰（min=4）：订阅即截断
        let mut h = Harness::attach_as(manager, "client-1", Some(0), windowed_cfg()).await;

        let control = h.recv_control().await.expect("订阅即截断必须立即下发重同步");
        match control {
            ForwardOutput::Resync {
                min_offset,
                snapshot_offset,
            } => {
                assert_eq!(min_offset, 4);
                assert_eq!(snapshot_offset, 12);
            }
            other => panic!("expected Resync first, got {other:?}"),
        }
        // 重播当前驻留区间，且**不再**补发 HistoryEnd
        assert_eq!(h.drain_payload().await, b"abcdabcd");
        assert!(
            h.recv_control().await.is_none(),
            "resync 后不得再发 HistoryEnd（历史边界由 resync 帧自带）"
        );
    }

    /// 退订（句柄被移除并 retire）：任务安静退出，不得下发终止帧/僵尸告警
    #[tokio::test]
    async fn retired_handle_exits_quietly_without_terminate() {
        let mut h = Harness::start("s-retire", None, windowed_cfg()).await;
        h.recv_control().await; // HistoryEnd

        // 塞满窗口进入驻留（不 ack）；执行体是异步的，轮询等待其真正驻留
        for _ in 0..3 {
            h.push(b"abcd").await;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        while h.handle.stats.park_count.load(Ordering::SeqCst) == 0 && tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        assert!(h.handle.stats.park_count.load(Ordering::SeqCst) >= 1, "应已驻留");

        // 退订：标记 retire + 唤醒 → 任务在检查点退出
        assert!(h.manager.unsubscribe_subscriber("client-1").await);
        tokio::time::timeout(Duration::from_millis(500), &mut h.task)
            .await
            .expect("退订后任务必须退出（不能停留在驻留等待）")
            .expect("任务正常结束");
        // 安静退出：无终止帧（僵尸路径才发）、无额外控制帧
        assert!(
            h.recv_control().await.is_none(),
            "退订退出不得下发 Terminate/错误帧"
        );
    }

    /// 会话产出端结束（watch 发送端 drop）：驻留中的任务安静退出，不算僵尸
    #[tokio::test]
    async fn park_exits_when_producer_closed() {
        let manager = Arc::new(SessionOutputManager::new("s-producer-close"));
        let pull = manager
            .register_subscriber("client-1", None, Arc::new(AtomicU8::new(MODE_REALTIME)))
            .await;
        let (out_tx, mut out_rx) = mpsc::channel::<ForwardOutput>(64);
        let task = tokio::spawn(subscriber_loop(
            manager.ring(),
            pull.max_watch,
            pull.handle.clone(),
            out_tx,
            windowed_cfg(),
        ));

        // 塞满窗口进入驻留，然后注销会话（管理器 drop → watch 发送端 drop）
        for _ in 0..3 {
            push_to(&manager, b"abcd").await;
        }
        drop(manager);

        tokio::time::timeout(Duration::from_millis(500), task)
            .await
            .expect("产出端结束时任务必须退出（不得驻留到僵尸超时）")
            .expect("任务正常结束");
        // 只能有历史帧（无 Terminate）
        let mut saw_terminate = false;
        while let Ok(out) = out_rx.try_recv() {
            if matches!(out, ForwardOutput::Terminate { .. }) {
                saw_terminate = true;
            }
        }
        assert!(!saw_terminate, "会话正常结束不得触发僵尸回收路径");
    }

    // ==================== 环读取助手 ====================

    #[test]
    fn read_at_contract_matrix() {
        let mut ring = UnifiedOutputQueue::with_limits(u64::MAX, 100);
        for _ in 0..3 {
            ring.push(crate::session::OutputEvent {
                session_id: "s".to_string(),
                data: b"abcd".to_vec(),
                start_offset: 0,
                timestamp: 0,
                is_waiting: false,
            });
        }
        // from 落在块内：半块裁头（零拷贝视图）
        let slice = ring.read_at(2).unwrap().unwrap();
        assert_eq!(slice.start_offset, 2);
        assert_eq!(&slice.bytes[..], b"cd");
        assert_eq!(slice.end_offset(), 4);
        // from 恰为块边界
        let slice = ring.read_at(4).unwrap().unwrap();
        assert_eq!(&slice.bytes[..], b"abcd");
        // from >= max：追平（None）
        assert!(ring.read_at(12).unwrap().is_none());
        assert!(ring.read_at(99).unwrap().is_none());
        // watermarks 一致
        assert_eq!(ring.watermarks(), (0, 12));
    }

    #[test]
    fn read_at_evicted_cursor_returns_min_offset() {
        let mut ring = UnifiedOutputQueue::with_limits(4, 100);
        ring.push(crate::session::OutputEvent {
            session_id: "s".to_string(),
            data: b"aaaa".to_vec(),
            start_offset: 0,
            timestamp: 0,
            is_waiting: false,
        });
        ring.push(crate::session::OutputEvent {
            session_id: "s".to_string(),
            data: b"bbbb".to_vec(),
            start_offset: 0,
            timestamp: 0,
            is_waiting: false,
        });
        assert_eq!(ring.min_offset(), 4);
        // 游标早于驻留起点：明确返回「已淘汰」而不是残缺数据
        assert_eq!(ring.read_at(2).unwrap_err(), 4);
        assert_eq!(ring.read_at(0).unwrap_err(), 4);
    }

    // ==================== 水位预算关系（ticket 06） ====================

    /// 默认水位必须满足「一次 ack 即解锁」预算：ack 阈值 ≤ low < high 且
    /// high − ack ≤ low；否则订阅者会驻留到僵尸回收
    #[test]
    fn default_watermark_budget_is_coherent() {
        let term = crate::system::config::TerminalConfig::default();
        assert_eq!(
            term.subscriber_budget_violation(50 * 1024 * 1024),
            None,
            "默认水位配置必须满足解锁预算"
        );

        // 反例：ack 阈值 ≥ 高位水 → 永远等不到能解锁的 ack
        let broken = crate::system::config::TerminalConfig {
            subscriber_high_water_bytes: 32 * 1024,
            subscriber_low_water_bytes: 16 * 1024,
            ..Default::default()
        };
        assert!(
            broken.subscriber_budget_violation(50 * 1024 * 1024).is_some(),
            "ack 阈值 ≥ 高位水必须被判定为违规组合"
        );

        // 反例：上游环比下游窗口还小 → 无谓截断
        assert!(
            term.subscriber_budget_violation(64 * 1024).is_some(),
            "高位水 ≥ 环上限必须被判定为违规组合"
        );
    }
}
