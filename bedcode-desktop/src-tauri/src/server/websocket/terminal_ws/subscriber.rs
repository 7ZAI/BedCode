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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Notify};

use super::forward::{ForwardOutput, OutputBuffer, MODE_REALTIME};
use crate::system::config::AppConfig;

/// 引擎环订阅者（P3 形态 B，票 06）：宿主广播面直读同进程 [`PtyRing`]
/// 的输出订阅执行体——帧语义为 subscribe_ok / history_end / resync / TB v3（原内核环
/// `subscriber_loop` 的对应实现随票 11 退役，本实现即唯一形态），数据源是引擎环
/// （经票 05 的 `hostBroadcastSessionId` 声明映射），零跨 WASM 边界（subscribe_ok /
/// history_end / resync / TB v3 帧形状），但数据源是引擎环（经票 05 的
/// `hostBroadcastSessionId` 声明映射），零跨 WASM 边界。
///
/// 与内核环的差异只有「读源形态」：
/// - 内核环有 watch 通道（新数据即时唤醒）；引擎环**无任何通知机制**（纯拉取，
///   插件/前端输出面同样是轮询）→ 本循环以自适应节奏轮询（快档 50ms / 慢档
///   250ms，镜像 `output.rs` 的前端轮询节奏）；
/// - 引擎环 `fetch` 一次返回连续字节块（无块边界事件语义）→ 直接进
///   [`OutputBuffer`] 合帧（区间连续由环不变量保证）；
/// - 终态信号来自 `PtySession::subscribe_lifecycle()`（broadcast），而非 watch
///   sender drop；终态后做**宽限排空**（P0 已记：终态事件 ≠ sink 已收尾帧，
///   读线程入队先于消费任务投递）再发 `SessionStopped`。
///
/// 窗口门控 / ack / 模式 / 统计复用 [`SubscriberHandle`]（自持，无需内核
/// 管理器登记）。

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
    pub response: SubscribeResponse,
    /// 交接通道接收端（调用方负责桥接到具体下游）
    pub out_rx: mpsc::Receiver<ForwardOutput>,
    /// 执行体任务句柄（替换/退订/断连时 abort）
    pub task: tokio::task::JoinHandle<()>,
    /// 引擎环订阅句柄（票 06）：引擎订阅者自持、未在内核管理器登记；
    /// 经订阅就绪消息回传连接侧供 ack 路由与退订清理。内核环订阅者 → None。
    pub engine_handle: Option<Arc<SubscriberHandle>>,
}

/// 启动一条拉取模型订阅链路：登记句柄 + 起订阅者执行体
///
/// 调用方持有 `out_rx` 并负责桥接到具体下游（WS actor / Tauri Channel）；
/// 丢弃 `out_rx` 会让执行体的发送失败并自然退出（无孤儿任务）。
// ==================== 引擎环订阅者（P3 形态 B，票 06） ====================

/// 引擎环轮询快档间隔（活跃输出期）：50ms（镜像前端 `output.pull` 快档 100ms，
/// 宿主直读无 WASM 边界开销，可更密；07 实测后可调）
const ENGINE_POLL_FAST_INTERVAL: Duration = Duration::from_millis(50);
/// 引擎环轮询慢档间隔（连续空闲后）：250ms（镜像前端空闲退避）
const ENGINE_POLL_IDLE_INTERVAL: Duration = Duration::from_millis(250);
/// 连续追平次数达到该值 → 降为慢档轮询（省唤醒）
const ENGINE_IDLE_THRESHOLD: u32 = 5;
/// 终态宽限排空窗口：`PtyTerminated` 事件后仍可能收到尾帧（P0 已记：读线程入队
/// 先于消费任务投递 sink），在此窗口内继续轮询直到环水印稳定或超时
const ENGINE_TERMINAL_GRACE: Duration = Duration::from_millis(300);
/// 引擎环单次拉取预算（宿主直读，不受 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` 的
/// WASM 边界限额约束——那是插件经 ring-fetch 的拷贝上限，宿主直读无此成本）
const ENGINE_FETCH_BUDGET: usize = 64 * 1024;

/// 引擎环订阅链路启动（票 06）：经票 05 的广播声明直读同进程 [`PtyRing`]
///
/// 返回形状（subscribe_ok 三件套 + 执行体 + 交接通道）即订阅链路的**唯一**形态
/// （原内核环 `spawn_subscriber` 随票 11 退役，调用方已无第二种数据源可区分）。
/// `session_id` 用于订阅句柄寻址与终态 SessionStopped 帧载荷（引擎环自身
/// 不知道会话 id——那是业务标识，映射由票 05 的广播声明持有）。
pub(crate) async fn spawn_engine_subscriber(
    handle: &crate::wasm_core::host_api::pty::BroadcastHandle,
    session_id: &str,
    client_id: &str,
    from_offset: Option<u64>,
    mode: Arc<std::sync::atomic::AtomicU8>,
    cfg: SubscriberCfg,
) -> SpawnedSubscriber {
    // 与内核 `register_subscriber` 同语义的水印读取：订阅时刻的水位即快照边界
    let (min_offset, max_offset) = {
        let ring = handle.ring.lock().unwrap_or_else(|e| e.into_inner());
        ring.watermarks()
    };
    let requested = from_offset.unwrap_or(min_offset);
    let truncated = requested < min_offset;
    let start_offset = requested.clamp(min_offset, max_offset);
    let snapshot_offset = max_offset;

    let sub = Arc::new(SubscriberHandle::new(
        session_id.to_string(),
        client_id.to_string(),
        start_offset,
        snapshot_offset,
        mode,
    ));
    if truncated {
        sub.stats.truncated_on_subscribe.store(true, Ordering::SeqCst);
    }

    // 终态订阅在循环外建立；**循环内持有 `PtySession` clone**——终态事件由
    // `PtyTerminationGate` 的 broadcast sender 发出，sender 存活期 = PtySession
    // 存活期：PTYS 注册表在终态时摘除条目并 drop 自己的 session 引用，若订阅者
    // 不持 clone，sender 先行关闭 → recv() 返回 Err(Closed) → 尾帧宽限排空与
    // SessionStopped 全被跳过。持 clone 同时保证「活跃订阅者存在期间进程不提前
    // 回收」（Drop 语义：最后一个引用 drop 才杀子进程）。
    let lifecycle_rx = handle.session.subscribe_lifecycle();
    let already_terminated = handle.session.output_terminated();
    let session_holder = Some(handle.session.clone());
    let (out_tx, out_rx) = mpsc::channel::<ForwardOutput>(OUT_CHANNEL_CAPACITY);
    let task = crate::system::error_boundary::spawn_with_error_boundary(
        "engine_terminal_subscriber",
        engine_subscriber_loop(
            Arc::clone(&handle.ring),
            session_holder,
            lifecycle_rx,
            already_terminated,
            Arc::clone(&sub),
            out_tx,
            cfg,
        ),
    );
    SpawnedSubscriber {
        response: SubscribeResponse {
            min_offset,
            snapshot_offset,
            history_bytes: max_offset.saturating_sub(min_offset),
        },
        out_rx,
        task,
        // 引擎句柄自持（未在内核管理器登记）：经订阅就绪消息回传给连接侧，
        // 供 ack 路由与退订清理（见 subscription.rs）
        engine_handle: Some(sub),
    }
}

/// 引擎环订阅者执行体（票 06）：直读同进程 [`PtyRing`]，帧语义与
/// 原内核环 `subscriber_loop` 逐字一致（subscribe_ok / history_end / resync / TB v3），
/// 差异只在读源与唤醒机制：
/// - 引擎环**无 watch 通道**（纯拉取，插件/前端输出面同样轮询）→ 以自适应
///   节奏轮询（快档 50ms / 连续空闲 5 次后慢档 250ms）；
/// - `fetch` 一次返回连续字节块（无事件/块边界语义）→ 直接进合帧缓冲，
///   区间连续由环不变量保证；
/// - 终态信号 = `PtyTerminated`（生命周期 broadcast）：进入**宽限排空**
///   （P0 已记：终态事件 ≠ sink 已收尾帧，读线程入队先于消费任务投递），
///   环水印稳定或宽限超时后落盘残留 + 发 `SessionStopped` 帧 + 退出。
pub(crate) async fn engine_subscriber_loop(
    ring: Arc<std::sync::Mutex<crate::pty::PtyRing>>,
    _session_holder: Option<crate::pty::PtySession>,
    mut lifecycle_rx: tokio::sync::broadcast::Receiver<crate::pty::PtyTerminated>,
    already_terminated: bool,
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

    // 轮询节奏（引擎环无 watch 通道）：快档起步，连续空闲后降慢档
    let mut poll_interval = ENGINE_POLL_FAST_INTERVAL;
    let mut idle_streak: u32 = 0;
    let mut terminal_grace: Option<tokio::time::Instant> = None;
    let mut last_grace_max: Option<u64> = None;

    // 订阅起点早于驻留起点（订阅即截断）：先发显式重同步（§4.7 同内核路径）
    if sub.stats.truncated_on_subscribe.load(Ordering::SeqCst) {
        let (min, max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();
        sub.stats.truncated_count.fetch_add(1, Ordering::SeqCst);
        if emit_resync(&out_tx, &sub, min, snapshot).await.is_err() {
            return;
        }
        next = next.clamp(min, max);
        sub.set_next_offset(next);
        // resync 帧自带历史边界：不再补发 HistoryEnd（§4.7；重播帧即恢复负载）
        history_done = true;
    }

    // 会话已死（读取侧 EOF）但生命周期事件可能已错过：直接进入宽限排空
    if already_terminated && terminal_grace.is_none() {
        terminal_grace = Some(tokio::time::Instant::now() + ENGINE_TERMINAL_GRACE);
        last_grace_max = None;
    }

    'outer: loop {
        // 退订/被替换（句柄已移除）→ 任务退出（下一次唤醒检查点）
        if sub.is_retired() {
            tracing::debug!(
                session_id = %sub.session_id,
                client_id = %sub.client_id,
                "engine subscriber loop exiting: handle retired (unsubscribed or replaced)"
            );
            break 'outer;
        }

        // ---------- 终态宽限排空（P0：事件 ≠ sink 已收尾帧） ----------
        // 事件后仍可能收到少量尾帧：在宽限窗口内继续轮询，直到环水印连续两轮
        // 不再推进（产出端已定）或宽限超时 → 落盘残留 + SessionStopped + 退出
        if let Some(deadline) = terminal_grace {
            let (_, cur_max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();
            let stable = last_grace_max == Some(cur_max);
            last_grace_max = Some(cur_max);
            if (next >= cur_max && stable) || tokio::time::Instant::now() >= deadline {
                let _ = flush_planner(&mut planner, &out_tx, &sub).await;
                if out_tx
                    .send(ForwardOutput::SessionStopped {
                        session_id: sub.session_id.clone(),
                    })
                    .await
                    .is_err()
                {
                    tracing::debug!(
                        session_id = %sub.session_id,
                        client_id = %sub.client_id,
                        "engine subscriber session_stopped dropped (downstream closed)"
                    );
                }
                break 'outer;
            }
        }

        // ---------- 截断局部化（I4）：所需字节已淘汰 → 仅本订阅者重同步 ----------
        {
            let (min, max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();
            if next < min {
                sub.stats.truncated_count.fetch_add(1, Ordering::SeqCst);
                tracing::warn!(
                    session_id = %sub.session_id,
                    client_id = %sub.client_id,
                    next_offset = next,
                    min_offset = min,
                    lag_bytes = max.saturating_sub(next),
                    "engine subscriber truncation: cursor evicted from ring, resync"
                );
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                if emit_resync(&out_tx, &sub, min, max).await.is_err() {
                    break 'outer;
                }
                next = min;
                snapshot = max;
                history_done = true;
                sub.set_next_offset(next);
                last_flush = tokio::time::Instant::now();
            }
        }

        // ---------- 推进（合帧 → 窗口门控 → 转发） ----------
        loop {
            let (cur_min, cur_max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();

            // 历史边界（I7）：追平即退出之前判定（空历史也必发 history_end）
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
                    "engine subscriber history segment done"
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
                        "engine subscriber park entered"
                    );
                }
                match engine_park_until_ack(&sub, &mut lifecycle_rx, &cfg, next).await {
                    ParkExit::Ack => {}
                    ParkExit::Zombie => {
                        tracing::warn!(
                            session_id = %sub.session_id,
                            client_id = %sub.client_id,
                            lag_bytes = next.saturating_sub(sub.acked_offset()),
                            parked_ms = parked_since.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0),
                            zombie_timeout_ms = cfg.zombie_timeout.as_millis() as u64,
                            "engine zombie subscriber reclaimed (window never decreased)"
                        );
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
                                "engine subscriber terminate frame dropped (downstream closed)"
                            );
                        }
                        break 'outer;
                    }
                    ParkExit::Stop => {
                        // 已退订（句柄移除）→ 安静退出；会话产出端结束（生命周期
                        // 事件）→ 继续外层循环进入终态宽限排空（不直接退出）
                        if sub.is_retired() {
                            break 'outer;
                        }
                        if terminal_grace.is_none() {
                            terminal_grace = Some(tokio::time::Instant::now() + ENGINE_TERMINAL_GRACE);
                            last_grace_max = None;
                        }
                        break;
                    }
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
                    "engine subscriber park exited"
                );
            }

            // 模式翻转即时 flush 残留批次（双速语义同内核路径）
            let cur_mode = sub.mode.load(Ordering::SeqCst);
            if cur_mode != last_mode {
                last_mode = cur_mode;
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }

            // 取一块（引擎环 fetch：连续字节；`truncated` 时已自 min 起返回 →
            // 外层截断分支处理；空数据 = 追平）
            let fetched = {
                let ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                ring.fetch(next, ENGINE_FETCH_BUDGET)
            };
            if fetched.truncated {
                continue 'outer; // 游标已淘汰 → 外层重同步
            }
            if fetched.data.is_empty() {
                break; // 追平
            }
            // 区间连续由环不变量保证：fetch 返回 [next, next_offset) 连续块。
            // 兜底连续性校验（与内核路径同策略）：带洞先落盘当前批次
            let slice_start = fetched.next_offset.saturating_sub(fetched.data.len() as u64);
            if !planner.is_contiguous_with(slice_start) {
                tracing::debug!(
                    session_id = %sub.session_id,
                    slice_start,
                    buffer_end = planner.end_offset,
                    "engine subscriber batch split on offset hole"
                );
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }
            planner.append_slice(slice_start, &fetched.data, false);
            next = fetched.next_offset;
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

        // 退订/被替换：进入等待前再查一次
        if sub.is_retired() {
            break 'outer;
        }

        // ---------- 等唤醒：轮询 / ack / 生命周期 / 时间窗 / 统计 ----------
        let flush_deadline = if !planner.is_empty() && last_mode == MODE_REALTIME && !cfg.flush_interval.is_zero() {
            Some(last_flush + cfg.flush_interval)
        } else {
            None
        };
        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {
                // 自适应档位：追平（无新数据）累计到阈值 → 慢档；有数据回快档
                let (_, cur_max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();
                if next >= cur_max {
                    idle_streak = idle_streak.saturating_add(1);
                } else {
                    idle_streak = 0;
                }
                poll_interval = if idle_streak >= ENGINE_IDLE_THRESHOLD {
                    ENGINE_POLL_IDLE_INTERVAL
                } else {
                    ENGINE_POLL_FAST_INTERVAL
                };
            }
            _ = sub.wait_ack() => {}
            r = lifecycle_rx.recv() => {
                // 终态：Ok = 事件到达；Err(Closed) = sender 关闭（会话对象已销毁，
                // 正常终止路径）→ 两者都进入宽限排空窗口（尾帧仍可能未投递完）
                if r.is_err() {
                    tracing::debug!(
                        session_id = %sub.session_id,
                        client_id = %sub.client_id,
                        "engine subscriber lifecycle closed, entering terminal grace drain"
                    );
                }
                if terminal_grace.is_none() {
                    terminal_grace = Some(tokio::time::Instant::now() + ENGINE_TERMINAL_GRACE);
                    last_grace_max = None;
                }
            }
            _ = sleep_until_opt(flush_deadline) => {
                if flush_planner(&mut planner, &out_tx, &sub).await.is_err() {
                    break 'outer;
                }
                last_flush = tokio::time::Instant::now();
            }
            _ = stats_tick.tick() => {
                let (cur_min, cur_max) = ring.lock().unwrap_or_else(|e| e.into_inner()).watermarks();
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
                    "engine subscriber stats (periodic)"
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
        "engine subscriber loop exited"
    );
}

/// 引擎环驻留等待（票 06）：与 [`park_until_ack`] 同语义（窗口 ≤ low_water 解除 /
/// 窗口停滞超僵尸时回收 / 退订或产出端结束安静退出），差异只在产出端信号源——
/// 引擎环无 watch 通道，以生命周期 broadcast 代替 watch sender drop。
async fn engine_park_until_ack(
    sub: &SubscriberHandle,
    lifecycle_rx: &mut tokio::sync::broadcast::Receiver<crate::pty::PtyTerminated>,
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
            _ = lifecycle_rx.recv() => {
                // 会话结束（Ok = 事件 / Err = 发送端关闭）→ 安静退出，不误判僵尸
                return ParkExit::Stop;
            }
            _ = tokio::time::sleep(cfg.park_poll) => {}
        }
    }
}

/// 订阅者执行体（`spawn_with_error_boundary("terminal_subscriber", …)` 启动）
///
/// 生命周期：`out_tx` 关闭（WS actor 退出）或会话产出端结束（watch 发送端 drop）
/// 或僵尸回收 → 返回。
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

// ==================== 合帧与睡眠辅助（订阅执行体使用） ====================

/// 驻留等待结束原因
enum ParkExit {
    /// ack 推进使窗口回落到低位水以下 → 继续发送
    Ack,
    /// 窗口持续不降超僵尸超时 → 回收该订阅者
    Zombie,
    /// 退订/被替换，或会话产出端结束 → 安静退出（非僵尸，不下发终止帧）
    Stop,
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

/// 可选截止时刻的睡眠：None 时永不就绪（用于 `select!` 分支占位）
async fn sleep_until_opt(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(t) => tokio::time::sleep_until(t).await,
        None => std::future::pending::<()>().await,
    }
}

// ==================== 订阅者公共状态（票 11 自主线程会话层迁入） ====================
//
// `SubscribeResponse` / `SubscriberStats` / `SubscriberHandle` 与两个模式常量原先住在
// 内核会话层 `session/session_output.rs`（业务输出环），但它们的**真实归属就是本模块**：
// 句柄是订阅执行体与连接 actor 之间的契约，响应是订阅握手的回执，双速模式是合帧策略的
// 输入。票 11 删除会话目录时把它们迁到唯一消费者身边（调用点 `subscriber::X`）。

/// 订阅响应（TB v3 字节三件套）
#[derive(Debug, Clone)]
pub struct SubscribeResponse {
    /// 队列中最早存续字节位置（环形淘汰后推进；客户端游标 < min_offset → 截断）
    pub min_offset: u64,
    /// 订阅时刻的累计字节数（= 当时队列 max_offset，历史边界元数据）
    pub snapshot_offset: u64,
    /// 驻留历史总字节数
    pub history_bytes: u64,
}


/// 订阅者观测统计（结构化日志对账用，均为单调累加）
#[derive(Debug, Default)]
pub struct SubscriberStats {
    /// 已发出帧数
    pub frames_sent: AtomicU64,
    /// 已发出负载字节数
    pub bytes_sent: AtomicU64,
    /// 进入驻留（窗口越界等待 ack）次数
    pub park_count: AtomicU64,
    /// 驻留累计时长（毫秒）
    pub parked_ms: AtomicU64,
    /// 截断（游标早于驻留起点 → 重同步）次数
    pub truncated_count: AtomicU64,
    /// 首次订阅起始点早于驻留起点（订阅即截断）标记
    pub truncated_on_subscribe: AtomicBool,
}

/// 拉取模型订阅者句柄（会话管理器持有，per client_id）
///
/// 「位置指针」是订阅者任务的私有状态（局部 `next`），句柄侧的
/// `next_offset` 仅为观测镜像（供统计/日志对账，不参与判定）。
/// `acked_offset` 为**该订阅者私有**的 ack 水位（I6）：单调前移，
/// 只解除/施加本订阅者的窗口驻留，不做任何会话级共享记账。
pub struct SubscriberHandle {
    pub session_id: String,
    pub client_id: String,
    /// 订阅起点（首订阅的 from_offset；仅供日志/观测）
    pub start_offset: u64,
    /// 本次订阅的历史边界（订阅时刻 max_offset；HistoryEnd 依据，I7）
    pub snapshot_offset: u64,
    /// 私有 ack 水位（客户端 ack 帧推进；I6）
    acked_offset: AtomicU64,
    /// 订阅者游标观测镜像（仅任务自己写，句柄侧只读）
    next_offset: AtomicU64,
    /// ack 唤醒（park 期间等它；丢失唤醒由 park 轮询兜底）
    ack_notify: Notify,
    /// 已退订/已被替换（句柄已从管理器移除）：任务在下一个检查点退出
    ///
    /// 不能只靠 `ack_notify` 唤醒——任务可能正阻塞在 `watch::changed()` 上等待
    /// 新数据，`notify_waiters` 唤不醒它（退订后仍会继续转发），故用显式标志
    retired: AtomicBool,
    /// 双速模式（realtime/batch）
    pub mode: Arc<std::sync::atomic::AtomicU8>,
    /// 观测统计
    pub stats: SubscriberStats,
}

impl SubscriberHandle {
    pub fn new(
        session_id: String,
        client_id: String,
        start_offset: u64,
        snapshot_offset: u64,
        mode: Arc<std::sync::atomic::AtomicU8>,
    ) -> Self {
        Self {
            session_id,
            client_id,
            start_offset,
            snapshot_offset,
            acked_offset: AtomicU64::new(start_offset),
            next_offset: AtomicU64::new(start_offset),
            ack_notify: Notify::new(),
            retired: AtomicBool::new(false),
            mode,
            stats: SubscriberStats::default(),
        }
    }

    /// 标记退订/被替换（管理器移除句柄时调用）：唤醒 + 任务在检查点退出
    pub fn retire(&self) {
        self.retired.store(true, Ordering::SeqCst);
        self.ack_notify.notify_waiters();
    }

    pub fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst)
    }

    pub fn acked_offset(&self) -> u64 {
        self.acked_offset.load(Ordering::SeqCst)
    }

    pub fn next_offset(&self) -> u64 {
        self.next_offset.load(Ordering::SeqCst)
    }

    /// 任务内推进游标（同时刷新观测镜像）
    pub fn set_next_offset(&self, next: u64) {
        self.next_offset.store(next, Ordering::SeqCst);
    }

    /// 客户端 ack：水位只前进（I6，陈旧/乱序 ack 天然忽略）并唤醒驻留
    pub fn on_ack(&self, acked_offset: u64) {
        let prev = self.acked_offset.fetch_max(acked_offset, Ordering::SeqCst);
        if acked_offset > prev {
            self.ack_notify.notify_waiters();
        }
    }

    /// 取消订阅/连接结束时的唤醒：让驻留中的任务立即重查退出条件
    pub fn wake(&self) {
        self.ack_notify.notify_waiters();
    }

    /// 等一次 ack 唤醒（配 `tokio::time::timeout` 做驻留兜底轮询）
    pub async fn wait_ack(&self) {
        self.ack_notify.notified().await;
    }

    /// 当前订阅者窗口 `next - acked`（已发未确认字节数）
    pub fn window(&self) -> u64 {
        self.next_offset().saturating_sub(self.acked_offset())
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU8;
    use tokio::sync::mpsc;

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

    // ==================== 引擎环订阅者（票 06，P3 形态 B 直读 PtyRing） ====================

    /// 引擎环夹具：真实 `PtyRing` + 可注入终态的 lifecycle channel + 内存输出通道。
    /// `engine_subscriber_loop` 不依赖 `PtySession`（终态由调用方经 broadcast 注入），
    /// 单测可直接驱动，与 `Harness`（内核环）对称。
    struct EngineHarness {
        ring: Arc<std::sync::Mutex<crate::pty::PtyRing>>,
        handle: Arc<SubscriberHandle>,
        lifecycle_tx: tokio::sync::broadcast::Sender<crate::pty::PtyTerminated>,
        out_rx: mpsc::Receiver<ForwardOutput>,
        task: tokio::task::JoinHandle<()>,
    }

    impl EngineHarness {
        /// 挂一条引擎环订阅者执行体（可先写历史再订阅）
        async fn attach(
            ring: Arc<std::sync::Mutex<crate::pty::PtyRing>>,
            session_id: &str,
            from_offset: Option<u64>,
            cfg: SubscriberCfg,
        ) -> Self {
            let (out_tx, out_rx) = mpsc::channel::<ForwardOutput>(64);
            let (lifecycle_tx, lifecycle_rx) = tokio::sync::broadcast::channel::<crate::pty::PtyTerminated>(8);

            // 复刻 `spawn_engine_subscriber` 的水印读取 + 句柄构造（可测公共路径）
            let (min_offset, max_offset) = {
                let ring = ring.lock().unwrap_or_else(|e| e.into_inner());
                ring.watermarks()
            };
            let requested = from_offset.unwrap_or(min_offset);
            let truncated = requested < min_offset;
            let start_offset = requested.clamp(min_offset, max_offset);
            let snapshot_offset = max_offset;
            let handle = Arc::new(SubscriberHandle::new(
                session_id.to_string(),
                "engine-client".to_string(),
                start_offset,
                snapshot_offset,
                Arc::new(AtomicU8::new(MODE_REALTIME)),
            ));
            if truncated {
                handle.stats.truncated_on_subscribe.store(true, Ordering::SeqCst);
            }

            let task = tokio::spawn(engine_subscriber_loop(
                Arc::clone(&ring),
                None, // 测试夹具自持 lifecycle_tx，无需 PtySession holder 保活
                lifecycle_rx,
                truncated,
                handle.clone(),
                out_tx,
                cfg,
            ));
            Self {
                ring,
                handle,
                lifecycle_tx,
                out_rx,
                task,
            }
        }

        /// 向引擎环 push 一段输出（生产端入口，与 `PtyRingSink::on_bytes` 同语义）
        fn push(&self, data: &[u8]) {
            self.ring
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(data);
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
                    Ok(Some(_other)) => continue, // 控制帧：跳过
                    Ok(None) | Err(_) => return None,
                }
            }
        }

        /// 收控制帧（Resync / HistoryEnd / SessionStopped / Terminate），超时 → None
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
    }

    impl Drop for EngineHarness {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    /// 引擎环夹具的常驻环（容量充足，无淘汰）
    fn engine_ring() -> Arc<std::sync::Mutex<crate::pty::PtyRing>> {
        Arc::new(std::sync::Mutex::new(crate::pty::PtyRing::new(64 * 1024)))
    }

    /// I7 引擎环等价：订阅前写历史 → 历史帧严格早于 HistoryEnd（边界 = 订阅时刻
    /// 水印），随后实时帧起点 = snapshot（无重无漏）
    #[tokio::test]
    async fn engine_history_replays_then_history_end_then_live() {
        let ring = engine_ring();
        {
            let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
            ring.push(b"aaaabbbbcccc");
        }
        let mut h = EngineHarness::attach(ring, "e-history", None, fast_cfg()).await;

        // 按序收帧：历史二进制帧 → HistoryEnd（不能先 recv_control——它会跳过并
        // 丢弃二进制帧）
        let mut history = Vec::new();
        let mut snapshot = None;
        loop {
            let frame = tokio::time::timeout(Duration::from_millis(300), h.out_rx.recv())
                .await
                .expect("engine subscriber 必须产出历史帧/边界")
                .expect("通道存活");
            match frame {
                ForwardOutput::Binary(frame) => {
                    let start = u64::from_le_bytes(frame[4..12].try_into().unwrap());
                    let len = u32::from_le_bytes(frame[12..16].try_into().unwrap()) as usize;
                    assert!(start + len as u64 <= 12, "历史负载不得越过快照边界");
                    history.extend_from_slice(&frame[16..16 + len]);
                }
                ForwardOutput::HistoryEnd {
                    snapshot_offset,
                    min_offset,
                    history_bytes,
                } => {
                    assert_eq!(snapshot_offset, 12, "历史边界 = 订阅时刻 max_offset");
                    assert_eq!(min_offset, 0);
                    assert_eq!(history_bytes, 12);
                    snapshot = Some(snapshot_offset);
                    break;
                }
                other => panic!("expected history frames then HistoryEnd, got {other:?}"),
            }
        }
        assert_eq!(history, b"aaaabbbbcccc", "历史必须完整重播");

        // 实时帧严格晚于 HistoryEnd（区间起点 ≥ snapshot）
        h.push(b"live-data!");
        let (start, payload) = h.recv_frame().await.expect("实时帧必须到达");
        assert_eq!(start, snapshot.unwrap(), "实时帧起点 = 历史边界（无重无漏）");
        assert_eq!(payload, b"live-data!");
    }

    /// 引擎环等价：空历史也必须发 HistoryEnd（移动端历史拼接锚点）
    #[tokio::test]
    async fn engine_empty_history_still_emits_history_end() {
        let mut h = EngineHarness::attach(engine_ring(), "e-empty", None, fast_cfg()).await;
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

    /// I4 引擎环等价：环淘汰推进 min_offset 越过游标 → Resync + 从 min 重播。
    /// 引擎环 `fetch` 以 `truncated` 显式上报缺口（不清零、不假装连续）
    #[tokio::test]
    async fn engine_truncation_emits_resync_and_replays_from_min() {
        // 小环（8 字节上限）放大淘汰效果：先驻留 [4,12)（bbbb+cccc，aaaa 已淘汰）
        let ring: Arc<std::sync::Mutex<crate::pty::PtyRing>> =
            Arc::new(std::sync::Mutex::new(crate::pty::PtyRing::new(8)));
        {
            let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
            ring.push(b"aaaa");
            ring.push(b"bbbb");
            ring.push(b"cccc"); // 淘汰 [0,4) → min=4, max=12
        }
        let mut h = EngineHarness::attach(ring, "e-truncate", None, windowed_cfg()).await;
        // 订阅即未截断（游标 4 = min）：全量回放历史 [4,12)，随后 HistoryEnd
        let mut replayed = Vec::new();
        loop {
            let frame = tokio::time::timeout(Duration::from_millis(300), h.out_rx.recv())
                .await
                .expect("历史必须被回放")
                .expect("通道存活");
            match frame {
                ForwardOutput::Binary(f) => {
                    replayed.extend_from_slice(&f[16..]);
                }
                ForwardOutput::HistoryEnd { .. } => break,
                other => panic!("expected history then HistoryEnd, got {other:?}"),
            }
        }
        assert_eq!(replayed, b"bbbbcccc", "订阅时刻历史必须完整回放");

        // 继续产出 3 段：每次淘汰 4 字节 → min 越过已驻留游标 12 → 截断
        h.push(b"dddd"); // 淘汰 bbbb → [8,16)
        h.push(b"eeee"); // 淘汰 cccc → [12,20)
        h.push(b"ffff"); // 淘汰 dddd → min=16, max=24（游标 12 已被淘汰）

        // ack 覆盖全部产出 → 解除驻留 → 发现游标已被淘汰
        h.handle.on_ack(24);
        let control = h.recv_control().await.expect("截断必须产生重同步信号");
        match control {
            ForwardOutput::Resync {
                min_offset,
                snapshot_offset,
            } => {
                assert_eq!(min_offset, 16, "重同步锚点 = 环驻留起点");
                assert_eq!(snapshot_offset, 24, "重同步快照点 = 当前产出端游标");
            }
            other => panic!("expected Resync, got {other:?}"),
        }
        // 随后从 min_offset 连续重播当前驻留区间（[16,24) = eeee+ffff）
        let (start, payload) = h.recv_frame().await.expect("重同步后必须重播驻留段");
        assert_eq!(start, 16);
        assert_eq!(payload, b"eeeeffff", "重同步后必须从 min_offset 连续重播（本用例后两步驻留 eeee+ffff）");
        assert_eq!(h.handle.next_offset(), 24);
    }

    /// 终态收尾（票 06）：`PtyTerminated` 事件 → 宽限排空剩余字节 → 落盘残留 →
    /// `SessionStopped` 帧（尾帧先行，移动端据此断开终端视图）
    #[tokio::test]
    async fn engine_terminal_grace_drains_and_emits_session_stopped() {
        let ring = engine_ring();
        let mut h = EngineHarness::attach(ring, "e-terminal", None, fast_cfg()).await;
        h.recv_control().await; // HistoryEnd（空历史）

        // 事件前先有实时帧
        h.push(b"tail-bytes");
        let (_, payload) = h.recv_frame().await.expect("事件前实时帧必须到达");
        assert_eq!(payload, b"tail-bytes");

        // 触发终态：事件后仍可能收到尾帧（读线程入队先于消费任务投递，P0 已记）
        h.push(b"late-tail");
        let _ = h.lifecycle_tx.send(crate::pty::PtyTerminated {
            status: crate::enums::PtySessionStatus::Stopped,
            exit_code: Some(0),
            killed: false,
        });

        // 宽限排空：晚到的尾帧必须被收走并落盘
        let (_, payload) = h.recv_frame().await.expect("终态宽限必须排空晚到尾帧");
        assert_eq!(payload, b"late-tail");
        // 随后是 SessionStopped（帧序保证：尾帧先于停止帧）
        let control = h.recv_control().await.expect("终态后必须发 SessionStopped");
        match control {
            ForwardOutput::SessionStopped { session_id } => {
                assert_eq!(session_id, "e-terminal")
            }
            other => panic!("expected SessionStopped, got {other:?}"),
        }
        // 任务退出（不悬挂）
        tokio::time::timeout(Duration::from_millis(500), &mut h.task)
            .await
            .expect("终态后任务必须退出")
            .expect("任务正常结束");
    }

    /// I5 引擎环等价：窗口门控驻留 + ack 解除（ack 经连接侧句柄路由，I6 语义不变）。
    /// 引擎环 fetch 一次取净驻留段 → 首轮追赶后游标停在产出端；随后新产出使
    /// window ≥ high_water → 驻留（不再推进游标）；ack 使窗口 ≤ low_water 后补齐。
    #[tokio::test]
    async fn engine_window_gate_parks_and_resumes_after_ack() {
        let ring = engine_ring();
        let mut h = EngineHarness::attach(ring, "e-window", None, windowed_cfg()).await;
        h.recv_control().await; // HistoryEnd

        // 先产出一批（12 字节 > high_water 8）：fetch-all 首轮追赶即取净
        // （游标追平到 12，不进入驻留——驻留只在「追赶后有新产出」时触发）
        h.push(b"abcd");
        h.push(b"efgh");
        h.push(b"ijkl");
        let mut got = Vec::new();
        while let Some((_, payload)) = h.recv_frame().await {
            got.extend_from_slice(&payload);
        }
        assert_eq!(got, b"abcdefghijkl", "首轮追赶应取净当前驻留段（引擎 fetch 无块粒度）");
        assert_eq!(h.handle.next_offset(), 12, "首轮追赶游标应停在产出端");

        // 新产出使 window = 12 ≥ high_water 8 → 驻留（游标停驻，不取新字节）。
        // 注意：首轮追赶后的 drain（300ms 超时）已把空闲档位推到慢档（250ms），
        // 此处睡眠须跨过慢档周期才能保证观察到驻留（确定性判据：游标停驻）
        h.push(b"mnop");
        h.push(b"qrst");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            h.handle.stats.park_count.load(Ordering::SeqCst) >= 1,
            "window 越位必须进入驻留"
        );
        assert_eq!(h.handle.next_offset(), 12, "驻留期间不得推进游标（I5）");

        // ack 到 12：窗口 0 ≤ low_water → 解除驻留并补齐剩余字节
        h.handle.on_ack(12);
        let mut got = Vec::new();
        while let Some((_, payload)) = h.recv_frame().await {
            got.extend_from_slice(&payload);
        }
        assert_eq!(got, b"mnopqrst", "ack 后必须补齐驻留期间产出的字节（零丢失）");
    }
}
