//! Terminal Link — 会话级终端 WebSocket（Rust 后端持有）
//!
//! ## 两段订阅（2026-09-17 改造）
//!
//! 移动端终端链路按「谁向谁订阅」拆成两段，生命周期彼此独立：
//!
//! ```text
//!  段1  Rust ↔ 桌面端 PTY 输出（会话级）
//!       订阅时机：会话 WS 连接认证成功（会话启动 / 断线重连后 onPaired）
//!       取消时机：会话停止、设备断开、手动取消
//!       载体：`terminal_subscribe` / `terminal_unsubscribe`
//!       产物：本模块的会话级字节缓存（真源，16MB LRU）——与页面无关，始终收帧
//!
//!  段2  前端 ↔ Rust 缓存/实时流（页面级）
//!       订阅时机：进入终端页
//!       取消时机：退出终端页
//!       载体：`terminal_page_subscribe` / `terminal_page_unsubscribe`
//!       产物：`terminal-frame` 事件推送开关——未订阅时只入缓存不推事件
//!             （IPC 零空转），重进页面经 `terminal_get_history` 回补
//! ```
//!
//! 段2 订阅态由 `TerminalLinkManager` 持有（会话级 `Arc<AtomicBool>`，独立于链路
//! 对象生命周期）：链路因会话停止/断开被重建（subscribe 新建实例）时沿用同一份
//! 订阅态，页面存活期间的终端不会因链路重建而静默断流。
//!
//! ## 其余契约
//!
//! - 终端数据真源 = 会话级字节缓存：WS 收帧 → 缓存 → （段2 已订阅时）事件转发；
//!   渲染 backpressure ack 由本模块持字节水位回发（`terminal_ack_rendered` 提升
//!   水位）；输入经 `terminal_send_input` → WS input 帧 → 桌面端 PTY。意外断开
//!   由本模块自动退避重连 + 按字节游标（from_offset）重订阅；手动断开不再重连。
//! - 字节连续（TB v3）：帧按 `[start_offset, end_offset)` 区间缓存与校验；
//!   历史 = 缓存区间（[head, snapshot)），一次性位移数据经 `terminal_get_history`
//!   提供（缓存优先，缓存头被淘汰时回退桌面 HTTP
//!   `GET /api/sessions/{id}/history` 一次性拉取）；实时帧（start ≥ snapshot）
//!   经 `terminal-frame` 事件推送——前端「拼完历史才消费实时」（契约见
//!   useTerminalBuffer 改造）。
//!
//! 链路加密：本版本 JWT 认证 + 明文帧（桌面端接受明文；与 v2 时代明文终端
//! WS 同安全位）。链路加密（ws-terminal 协商）后续 ticket 接入，见
//! `.scratch/mobile-ws-rust/issues/03`。

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

use crate::system::error_boundary::spawn_with_error_boundary;

// ==================== 常量 ====================

/// 重连退避（ms）：500 → 1000 → 2000 → 4000 → 8000 封顶（对齐原前端 socket）
const RECONNECT_BASE_MS: u64 = 500;
const RECONNECT_MAX_MS: u64 = 8000;

/// 会话不存在（启动中/已停止）连续重试上限，超出后停止等待外部恢复
const MAX_SESSION_MISSING_STRIKES: u32 = 3;

/// 会话级字节缓存上限（历史 + 实时缓冲，LRU 淘汰头部）
const CACHE_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// 渲染背压 ack 节流：累计待 ack 字节达阈值即回发（对齐桌面端水位节奏）
///
/// **与水位的解锁关系（禁单独调整，ticket 06）**：桌面端订阅者窗口为
/// `高 128KB / 低 64KB`，本阈值必须满足
/// `本阈值 ≤ 桌面低位水 < 桌面高位水` 且 `高位水 − 本阈值 ≤ 低位水`——
/// 即「一次 ack 必须能把窗口压到低位水以下解锁」。若本阈值 ≥ 高位水，
/// 订阅者永远等不到能解锁的 ack → 驻留到僵尸回收（桌面端
/// `TerminalConfig::subscriber_budget_violation` 在启动时校验该关系）。
/// 上界另受**本端 WS 接收缓冲**约束：水位必须低于接收缓冲，否则溢出丢消息
const ACK_BYTES_THRESHOLD: u64 = 64 * 1024;
/// ack 空闲兜底：距上次回发超此时长仍推进则强制回发
const ACK_MAX_IDLE_MS: u64 = 250;
/// ack 空闲兜底轮询间隔：连接循环用该周期调用 should_send_ack，使「无新帧到达」
/// 时积压的 ack 仍能被回发。必须 ≤ ACK_MAX_IDLE_MS——否则积压 ack 的送达被推迟
/// 到空闲窗口之外，上游被背压暂停的窗口内仍会停摆（见 should_send_ack 注释）
const ACK_IDLE_TICK_MS: u64 = ACK_MAX_IDLE_MS / 2;

// ==================== 段2 背压水位（Rust → 前端） ====================

/// 段2 未渲染窗口高位水：`cursor - 前端已渲染游标` 超过该值即暂停事件推送
/// （帧继续入缓存，零丢失——缓存即段2 的「内核管道」）。
///
/// 取值理由：段1 高位水 64KB 太小——段2 的消费端是 WebView（需 base64 解码 +
/// xterm 解析 + 渲染提交），ack 节奏约一帧一次（~60Hz），64KB 在高吞吐下会被
/// 反复击穿（暂停/恢复抖动）。1MB 对应「前端落后约一秒的渲染工作量」，
/// 正常输出（数十 KB/s）永不触发，风暴期（数 MB/s）才介入
const SEG2_HIGH_WATER_BYTES: u64 = 1024 * 1024;
/// 段2 未渲染窗口低位水：降到该值以下才解除暂停并补推滞留区间（滞回下沿，
/// 避免单阈值在临界点反复抖振）。补推量恒 ≤ 该值，单轮开销可控
const SEG2_LOW_WATER_BYTES: u64 = 256 * 1024;

/// TB v3 帧头长度
const TB_FRAME_HEADER_LEN: usize = 16;
const TB_MAGIC: [u8; 2] = [0x54, 0x42];
const TB_VERSION_V3: u8 = 3;
/// 客户端 ack 标志位
const TB_FLAG_ACK: u8 = 0x02;

// ==================== 链路调试节流（终端字节对账） ====================

/// 收帧统计打点间隔（有新数据才打；不打逐帧日志，防输出风暴期日志淹没链路）
const INGEST_STATS_INTERVAL_MS: u64 = 5000;
/// 缺口告警节流：真实缺口时后续每条 WS 消息都会持续 gap，逐条告警会刷屏
const GAP_LOG_INTERVAL_MS: u64 = 2000;

// ==================== 事件名（前端 listen） ====================

/// 实时输出帧（仅 phase=live 且 end_offset > snapshot 的帧）
pub(crate) const EVENT_TERMINAL_FRAME: &str = "terminal-frame";
/// 状态变更（phase / reconnect / truncated / stopped / session_missing）
pub(crate) const EVENT_TERMINAL_STATE: &str = "terminal-state";
/// 重同步（桌面端把本订阅者重锚到 min_offset 并重播，spec §4.7）：
/// 前端据此清屏 + 游标重锚 + 一次性提示
pub(crate) const EVENT_TERMINAL_RESYNC: &str = "terminal-resync";

// ==================== 状态定义 ====================

/// 连接/订阅阶段（与原前端 buffer.phase 语义对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPhase {
    Idle,
    Connecting,
    Auth,
    /// 已订阅、服务端历史段（重连时 WS 重播的 [cursor, snapshot) 段）
    History,
    Live,
}

impl LinkPhase {
    /// u8 → 阶段（emit_state / terminal_get_state 用）
    fn from_u8(v: u8) -> Self {
        match v {
            1 => LinkPhase::Connecting,
            2 => LinkPhase::Auth,
            3 => LinkPhase::History,
            4 => LinkPhase::Live,
            _ => LinkPhase::Idle,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            LinkPhase::Idle => 0,
            LinkPhase::Connecting => 1,
            LinkPhase::Auth => 2,
            LinkPhase::History => 3,
            LinkPhase::Live => 4,
        }
    }

    fn as_api_str(self) -> &'static str {
        match self {
            LinkPhase::Idle => "idle",
            LinkPhase::Connecting => "connecting",
            LinkPhase::Auth => "auth",
            LinkPhase::History => "history",
            LinkPhase::Live => "live",
        }
    }
}

/// 传播模式（双速；与桌面端 forward::MODE_REALTIME/MODE_BATCH 对齐）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMode {
    Realtime,
    Batch,
}

impl LinkMode {
    fn as_u8(self) -> u8 {
        match self {
            LinkMode::Realtime => 0,
            LinkMode::Batch => 1,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            LinkMode::Realtime => "realtime",
            LinkMode::Batch => "batch",
        }
    }
}

/// 会话级字节缓存（真源）：WS 收到帧即入缓存，历史 = [head, tail) 区间
struct SessionCache {
    entries: VecDeque<CacheEntry>,
    bytes: u64,
    /// 驻留最旧字节位置（LRU 淘汰后推进）
    head: u64,
    /// 最新收帧末尾（= 实时流游标）
    tail: u64,
    max_bytes: u64,
    /// 已知字节洞 `[洞首, 洞尾)`：上游丢帧（背压超时丢弃 / 重订阅窗口）后
    /// 区间不连续。洞内字节永远不会再到达（重连锚点 from = cursor > 洞首），
    /// 快照拼接会跨洞，消费端必须据此清屏重播而非静默接受错位内容
    gaps: VecDeque<(u64, u64)>,
}

struct CacheEntry {
    start: u64,
    end: u64,
    data: Vec<u8>,
}

impl SessionCache {
    fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            head: 0,
            tail: 0,
            max_bytes: CACHE_MAX_BYTES,
            gaps: VecDeque::new(),
        }
    }

    /// 收帧入缓存（区间连续性在此登记：本帧首越过已收帧尾即为字节洞）。
    /// 返回本次 LRU 淘汰的最旧字节数（对账口径：淘汰字节无法再经缓存供给前端）
    fn push(&mut self, start: u64, data: Vec<u8>) -> u64 {
        let end = start + data.len() as u64;
        // 缺口登记：tail != 0 表示已有前序帧，start > tail 即中间丢字节
        if self.tail != 0 && start > self.tail {
            self.gaps.push_back((self.tail, start));
        }
        // 淘汰：超出上限丢最旧（均摊 O(1)）；新帧无论如何保留
        self.entries.push_back(CacheEntry { start, end, data });
        self.bytes += end - start;
        let mut evicted = 0u64;
        while self.bytes > self.max_bytes {
            if let Some(front) = self.entries.pop_front() {
                self.bytes -= front.end - front.start;
                evicted += front.end - front.start;
                self.head = front.end;
            } else {
                break;
            }
        }
        // 已完全落在淘汰区（洞尾 ≤ head）的缺口不再上报：快照不可能命中
        while let Some(&(_, gap_end)) = self.gaps.front() {
            if gap_end <= self.head {
                self.gaps.pop_front();
            } else {
                break;
            }
        }
        self.tail = end;
        evicted
    }

    /// [from, tail) 范围内是否存在已知字节洞：快照产物跨洞，不是连续流，
    /// 消费端不得按「区间 = 负载」直接上屏（会错位/切断转义序列）
    fn has_gap(&self, from: u64) -> bool {
        let from = from.max(self.head);
        self.gaps
            .iter()
            .any(|&(gap_start, gap_end)| gap_end > from && gap_start < self.tail)
    }

    /// 指定 from 起的历史快照：`(min 可用起点, 快照尾(=tail), 驻留字节, 区间字节)`
    /// from < head（被淘汰）→ 从 head 起并携带 min=head 供消费端判截断
    fn snapshot(&self, from: u64) -> (u64, u64, u64, Vec<u8>) {
        let from = from.max(self.head);
        let mut out = Vec::new();
        for entry in &self.entries {
            if entry.end <= from {
                continue;
            }
            if entry.start >= self.tail {
                break;
            }
            let lo = from.saturating_sub(entry.start) as usize;
            if lo < entry.data.len() {
                out.extend_from_slice(&entry.data[lo..]);
            }
        }
        (self.head, self.tail, self.bytes, out)
    }

    /// 重同步重锚（spec §4.7）：丢弃全部驻留字节，把 head/tail 锚定到 `offset`
    ///
    /// 服务端已把本订阅者重锚到该点并从该点重播：旧缓存字节与重播流之间必然
    /// 存在字节洞（[旧游标, offset) 已被环淘汰），留着只会让 `has_gap` /
    /// `contiguous_runs` 报告错误区间。重锚后 `tail == offset`，重播首帧恰从
    /// `offset` 起 → 不产生缺口登记，`has_gap` 自然为假
    fn reset_to(&mut self, offset: u64) {
        self.entries.clear();
        self.bytes = 0;
        self.head = offset;
        self.tail = offset;
        self.gaps.clear();
    }

    /// [from, tail) 的**连续段**列表：每段 `(start, data)`，段内字节严格连续，
    /// 段与段之间是已知字节洞（上游丢帧）。
    ///
    /// 供段2 背压恢复时「一次性补推滞留区间」使用：补推必须按连续段切分——
    /// 把跨洞区间合成一帧推送，消费端按帧头区间推导的负载与真实负载不符，
    /// 转义序列会被接在半途（错位/空行）；按段切分后消费端按既有「帧首越过
    /// 游标 = 缺口」判定即可自愈（清屏 + 锚定重播）
    fn contiguous_runs(&self, from: u64) -> Vec<(u64, Vec<u8>)> {
        let from = from.max(self.head);
        let mut runs: Vec<(u64, Vec<u8>)> = Vec::new();
        for entry in &self.entries {
            if entry.end <= from || entry.start >= self.tail {
                continue;
            }
            let lo = from.saturating_sub(entry.start) as usize;
            if lo >= entry.data.len() {
                continue;
            }
            let seg_start = entry.start + lo as u64;
            let slice = &entry.data[lo..];
            match runs.last_mut() {
                // 与前段严格相接（前段尾 == 本段首）→ 并入同一段；有洞则新起一段
                Some((run_start, data)) if *run_start + data.len() as u64 == seg_start => {
                    data.extend_from_slice(slice);
                }
                _ => runs.push((seg_start, slice.to_vec())),
            }
        }
        runs
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ==================== TB 帧解析（v3） ====================

/// 单帧解析结果（字节区间语义）
struct ParsedFrame {
    start: u64,
    end: u64,
    data: Vec<u8>,
}

/// 解析 TB 二进制消息内全部帧
///
/// v3：start_offset(8 LE) + len(4 LE)，end = start + len；未知版本安全停止解析
fn parse_tb_frames(bytes: &[u8]) -> Vec<ParsedFrame> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while offset + TB_FRAME_HEADER_LEN <= bytes.len() {
        if bytes[offset] != TB_MAGIC[0] || bytes[offset + 1] != TB_MAGIC[1] {
            break;
        }
        let version = bytes[offset + 2];
        let raw = u64::from_le_bytes(bytes[offset + 4..offset + 12].try_into().unwrap_or([0; 8]));
        let len4 = u32::from_le_bytes(bytes[offset + 12..offset + 16].try_into().unwrap_or([0; 4])) as usize;
        if offset + TB_FRAME_HEADER_LEN + len4 > bytes.len() {
            break;
        }
        let (start, end) = match version {
            TB_VERSION_V3 => {
                let start = raw;
                (start, start + len4 as u64)
            }
            _ => break,
        };
        frames.push(ParsedFrame {
            start,
            end,
            data: bytes[offset + TB_FRAME_HEADER_LEN..offset + TB_FRAME_HEADER_LEN + len4].to_vec(),
        });
        offset += TB_FRAME_HEADER_LEN + len4;
    }
    frames
}

/// 实时帧是否推送前端（段2 订阅门控 + 段2 背压门控）。
///
/// 四个条件缺一不可：
/// - `end > live_snapshot`（或 `resynced`）：帧属实时段（历史段 [head, snapshot)
///   只入缓存，由 `terminal_get_history` 一次性供给——重连 WS 重播段同理）。
///   `resynced` 例外：桌面端截断重同步后，重播段**就是**恢复负载（resync 帧
///   自带历史边界，不会再发 history_end）——只入缓存等于把恢复内容退回给一次
///   `get_history` 往返，正是本信号要消除的间接路径
/// - `subscribe_ack`：本次连接已收到 `subscribe_ok`。握手窗口内 `live_snapshot`
///   仍是旧值（新链路为 0），把重播历史段误判为实时帧推给前端，会与其后的
///   `terminal_get_history` 结果重叠/错位
/// - `frontend_subscribed`：段2 已订阅（终端页在前台消费）。未订阅时只入缓存：
///   页面关闭期间的输出无人消费，逐帧 IPC 是纯空转——重进页面由历史拼接回补
/// - `!seg2_paused`：段2 未渲染窗口未越高位水（背压解压中）。越界即停推，
///   字节留在缓存——恢复时由 `seg2_drain` 一次性补推（不丢帧、不产生缺口）
///
/// 独立成纯函数：判据分属协议边界（历史/实时）、握手状态、页面生命周期与段2
/// 背压水位，任一被改动都直接决定「前端是否丢帧」，必须可单测锁定
fn should_emit_live_frame(
    frame_end: u64,
    live_snapshot: u64,
    subscribe_ack: bool,
    frontend_subscribed: bool,
    seg2_paused: bool,
    resynced: bool,
) -> bool {
    (resynced || frame_end > live_snapshot) && subscribe_ack && frontend_subscribed && !seg2_paused
}

/// 段2 水位滞回判定：给定「未渲染字节数」与当前暂停态，返回更新后的暂停态。
///
/// 与段1（桌面端 PTY 读）同构：未暂停时超高位水才暂停；已暂停时降到低位水才
/// 恢复；区间内保持现状，避免单阈值在临界点反复抖振（暂停/恢复本身携带一次
/// 缓存补推，抖动即重复拷贝）
fn seg2_paused_after(unrendered_bytes: u64, paused: bool) -> bool {
    if paused {
        unrendered_bytes > SEG2_LOW_WATER_BYTES
    } else {
        unrendered_bytes > SEG2_HIGH_WATER_BYTES
    }
}

/// ack 回发节流判定（64KB 阈值 + 250ms 空闲兜底）。
///
/// `pending == 0` 一律不发：ack 水位在收帧时同步到缓存游标（见 `ingest_frame`），
/// pending 为 0 即表示自上次回发后没有新收到的字节，重发不推进桌面端记账。
///
/// 该判据必须独立成纯函数：ack 回发原先只由「收帧」「前端渲染 ack」两个事件
/// 触发，空闲规则仅在事件到达时求值。末批字节不足 64KB 且距上次回发 <250ms 时
/// 判定为「保留待发」，若此后上游因背压暂停（不再有帧到达），pending 永远等不到
/// 下一次触发——桌面端 unacked 停在 8KB~64KB 区间内既收不到新 ack、也不恢复
/// PTY 读，生产端等 ack / 消费端等帧形成双向死锁（现场症状：运行中滑动失效 +
/// 输入无回显）。现由连接循环的空闲定时器周期性调用本判据兜底，故必须显式排除
/// pending=0，否则定时器会每 250ms 回发一次空 ack（无谓流量）。
fn should_send_ack(pending: u64, last_ack_at: u64, now: u64) -> bool {
    if pending == 0 {
        return false;
    }
    pending >= ACK_BYTES_THRESHOLD || (last_ack_at != 0 && now.saturating_sub(last_ack_at) >= ACK_MAX_IDLE_MS)
}

/// 构造客户端背压 ack 帧（TB v3 头 + ACK 标志 + acked_offset(8 LE) + session_id 负载）
fn build_ack_frame(session_id: &str, acked_offset: u64) -> Vec<u8> {
    let payload = session_id.as_bytes();
    let mut frame = Vec::with_capacity(TB_FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&TB_MAGIC);
    frame.push(TB_VERSION_V3);
    frame.push(TB_FLAG_ACK);
    frame.extend_from_slice(&acked_offset.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

// ==================== Terminal Link（每会话） ====================

/// 连接终止原因（区分意外断开与否，决定重连策略）
enum LinkExit {
    /// 意外断开 / 连接失败：退避重连
    Io,
    /// 会话不存在（启动中/已停止）：有限重试后停止
    SessionMissing,
}

/// 出站帧（IO 任务与命令侧交互通道）
#[derive(Debug)]
enum Outbound {
    Subscribe { from_offset: u64 },
    Input { data: String, special_key: Option<String> },
    Ack { offset: u64 },
    SetMode { mode: LinkMode },
    /// 优雅关闭（不再重连）
    Close,
}

/// 会话级终端链路（每会话一个实例；管理器持有）
pub struct TerminalLink {
    session_id: String,
    app: AppHandle,
    /// 出站通道（命令侧 → IO 任务）
    write_tx: mpsc::Sender<Outbound>,
    /// IO 任务句柄（manual stop 时 abort）
    ///
    /// 类型为 `tokio::task::JoinHandle`（`spawn_with_error_boundary` 返回值）：
    /// tauri async_runtime 的 spawn 底层同为 tokio（移动端无 spawn_local 分派），
    /// 包装层仅加 panic 边界，不改变 runtime 归属。
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// 阶段（LinkPhase）
    phase: AtomicU8,
    /// 本次订阅的历史边界（subscribe_ok.snapshot_offset；end ≤ 该值的帧为历史段）
    live_snapshot: AtomicU64,
    /// 服务端驻留最旧字节位置（subscribe_ok.min_offset）
    min_offset: AtomicU64,
    /// 已接收流游标（= 缓存尾；重订阅 from_offset 依据）
    cursor: AtomicU64,
    /// ack 水位：max(缓存游标 / 前端渲染游标)——桌面端按此释放背压
    acked: AtomicU64,
    /// 累计待 ack 字节（节流）
    pending_ack_bytes: AtomicU64,
    last_ack_at: AtomicU64,
    mode: AtomicU8,
    stopped: AtomicBool,
    /// 段2 订阅态（前端是否正在消费本链路的输出）——与管理器共享同一 `Arc`：
    /// 链路被重建时沿用同一份订阅态，页面存活期间的终端不会因链路重建而断流
    frontend_subscribed: Arc<AtomicBool>,
    /// 段2 已渲染水位（前端 `terminal_ack_rendered` 推进）：段2 未渲染窗口 =
    /// `cursor - frontend_rendered`。与段1 的 `acked` 分开记账——段1 锚定到
    /// Rust 缓存（收帧即视为缓存已消化，桌面端不必为渲染速度停摆），段2 锚定
    /// 到前端渲染进度（WebView 跟不上就停推，字节留在缓存）
    frontend_rendered: AtomicU64,
    /// 段2 背压暂停态（滞回）：见 `seg2_paused_after`
    seg2_paused: AtomicBool,
    /// 会话不存在连续重试计数
    session_missing: AtomicU32,
    /// 本次连接是否已收到 `subscribe_ok`：握手窗口（auth → subscribe_ok）内
    /// `live_snapshot` 仍是旧值（新链路为 0），此时到达的二进制帧实际是重播
    /// 历史段。若误判为实时帧推送前端，会与其后 `terminal_get_history` 的结果
    /// 重叠/错位（前端按字节游标裁剪只能兜住连续场景）。置位后才允许推送实时帧
    subscribe_ack: AtomicBool,
    /// 本次订阅是否已收到桌面端 `resync`（spec §4.7 截断重同步）：服务端已把
    /// 本订阅者重锚到 `min_offset` 并重播，此后**不存在历史边界**（不会再发
    /// history_end）→ 所有帧都是需要直达前端的恢复负载（见 should_emit_live_frame）。
    /// 收到 `subscribe_ok`（新连接/重订阅）时复位
    resynced: AtomicBool,
    cache: Mutex<SessionCache>,
    // ==================== 链路调试统计（终端字节对账） ====================
    /// 累计收帧数 / 收字节数（WS 二进制消息解析后）
    frames_received: AtomicU64,
    bytes_received: AtomicU64,
    /// 实时段帧（end > snapshot，经事件推送前端）/ 历史段帧（只入缓存）
    frames_live_emitted: AtomicU64,
    bytes_live_emitted: AtomicU64,
    frames_cache_only: AtomicU64,
    bytes_cache_only: AtomicU64,
    /// TB 解析残渣字节（帧边界损坏/未知版本截断，本环节丢失）
    parse_residue_bytes: AtomicU64,
    /// 缓存 LRU 累计淘汰字节（16MB 上限，淘汰段前端经 get_history 无法回补）
    cache_evicted_bytes: AtomicU64,
    /// 收帧统计打点时刻与当时游标（节流 + 无新数据不打）
    last_stats_ms: AtomicU64,
    last_stats_cursor: AtomicU64,
    /// 缺口告警上次打点时刻（节流）
    last_gap_log_ms: AtomicU64,
}

impl TerminalLink {
    fn new(
        session_id: String,
        app: AppHandle,
        write_tx: mpsc::Sender<Outbound>,
        frontend_subscribed: Arc<AtomicBool>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            app,
            write_tx,
            handle: Mutex::new(None),
            phase: AtomicU8::new(LinkPhase::Idle.as_u8()),
            live_snapshot: AtomicU64::new(0),
            min_offset: AtomicU64::new(0),
            cursor: AtomicU64::new(0),
            acked: AtomicU64::new(0),
            pending_ack_bytes: AtomicU64::new(0),
            last_ack_at: AtomicU64::new(0),
            mode: AtomicU8::new(LinkMode::Realtime.as_u8()),
            stopped: AtomicBool::new(true),
            frontend_subscribed,
            frontend_rendered: AtomicU64::new(0),
            seg2_paused: AtomicBool::new(false),
            session_missing: AtomicU32::new(0),
            subscribe_ack: AtomicBool::new(false),
            resynced: AtomicBool::new(false),
            cache: Mutex::new(SessionCache::new()),
            frames_received: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            frames_live_emitted: AtomicU64::new(0),
            bytes_live_emitted: AtomicU64::new(0),
            frames_cache_only: AtomicU64::new(0),
            bytes_cache_only: AtomicU64::new(0),
            parse_residue_bytes: AtomicU64::new(0),
            cache_evicted_bytes: AtomicU64::new(0),
            last_stats_ms: AtomicU64::new(0),
            last_stats_cursor: AtomicU64::new(0),
            last_gap_log_ms: AtomicU64::new(0),
        })
    }

    fn emit_state(&self, detail: &str) {
        let phase = LinkPhase::from_u8(self.phase.load(Ordering::SeqCst));
        let mode = if self.mode.load(Ordering::SeqCst) == LinkMode::Batch.as_u8() {
            "batch"
        } else {
            "realtime"
        };
        // 前端状态机唯一事件源：emit 失败必须留痕，否则 UI 静默停在旧状态无任何线索
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_STATE,
            serde_json::json!({
                "session_id": self.session_id,
                "phase": phase.as_api_str(),
                "cursor": self.cursor.load(Ordering::SeqCst),
                "snapshot_offset": self.live_snapshot.load(Ordering::SeqCst),
                "min_offset": self.min_offset.load(Ordering::SeqCst),
                "mode": mode,
                "detail": detail,
            }),
        ) {
            tracing::warn!(
                session_id = %self.session_id,
                detail = %detail,
                error = %e,
                "terminal state event emit failed"
            );
        }
    }

    /// 推送一帧到前端（`terminal-frame` 事件）。
    ///
    /// 实时帧直推与段2 背压恢复补推（`seg2_drain`）共用同一入口：两条路径的帧
    /// 语义完全一致（字节区间 + base64 负载），合并出口避免补推路径漏掉统计与
    /// emit 失败留痕
    fn emit_frame(&self, start: u64, end: u64, data: &[u8]) {
        self.frames_live_emitted.fetch_add(1, Ordering::SeqCst);
        self.bytes_live_emitted.fetch_add(data.len() as u64, Ordering::SeqCst);
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_FRAME,
            serde_json::json!({
                "session_id": self.session_id,
                "start_offset": start,
                "end_offset": end,
                "data_base64": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    data,
                ),
            }),
        ) {
            tracing::warn!(
                session_id = %self.session_id,
                start_offset = start,
                end_offset = end,
                error = %e,
                "terminal frame event emit failed"
            );
        }
    }

    /// 段2 未渲染窗口：前端已渲染游标之后的字节数（背压判定输入）
    fn seg2_unrendered_bytes(&self) -> u64 {
        self.cursor
            .load(Ordering::SeqCst)
            .saturating_sub(self.frontend_rendered.load(Ordering::SeqCst))
    }

    /// 缓存收帧 + 推进游标 + （段2 门控）事件推送 + ack 记账
    fn ingest_frame(&self, frame: ParsedFrame) {
        let frame_bytes = frame.end - frame.start;
        self.frames_received.fetch_add(1, Ordering::SeqCst);
        self.bytes_received.fetch_add(frame_bytes, Ordering::SeqCst);

        // 段2 背压水位滞回：仅段2 已订阅时评估——未订阅时前端渲染水位是陈旧的
        // （页面关闭期间无人推进），评估只会得到假窗口；重进页面由
        // page_subscribe 重置基线与暂停态。用本帧落盘后的预期游标评估，避免
        // 「先克隆入缓存再回读」
        let subscribed = self.frontend_subscribed.load(Ordering::SeqCst);
        if subscribed {
            let prospective_cursor = frame.end.max(self.cursor.load(Ordering::SeqCst));
            let unrendered = prospective_cursor.saturating_sub(self.frontend_rendered.load(Ordering::SeqCst));
            let was_paused = self.seg2_paused.load(Ordering::SeqCst);
            let now_paused = seg2_paused_after(unrendered, was_paused);
            if now_paused != was_paused {
                self.seg2_paused.store(now_paused, Ordering::SeqCst);
                // 链路调试（段2 背压对账）：暂停 = 停推，字节留缓存；恢复由
                // seg2_drain 一次性补推，两处日志与前端 ack 游标可对齐验证
                tracing::debug!(
                    session_id = %self.session_id,
                    unrendered_bytes = unrendered,
                    high_water_bytes = SEG2_HIGH_WATER_BYTES,
                    low_water_bytes = SEG2_LOW_WATER_BYTES,
                    paused = now_paused,
                    "seg2 push backpressure state changed"
                );
            }
        }

        let emit = should_emit_live_frame(
            frame.end,
            self.live_snapshot.load(Ordering::SeqCst),
            self.subscribe_ack.load(Ordering::SeqCst),
            subscribed,
            self.seg2_paused.load(Ordering::SeqCst),
            self.resynced.load(Ordering::SeqCst),
        );
        if emit {
            self.emit_frame(frame.start, frame.end, &frame.data);
        } else {
            // 历史段 / 握手窗口（subscribe_ok 未到）/ 段2 未订阅（页面未打开）/
            // 段2 背压暂停：只入缓存不推送——前两者经 terminal_get_history 一次性
            // 供给；后两者分别由「重进页面的历史拼接」与「恢复时的 seg2_drain 补推」
            // 供给，字节零丢失
            self.frames_cache_only.fetch_add(1, Ordering::SeqCst);
            self.bytes_cache_only.fetch_add(frame_bytes, Ordering::SeqCst);
        }

        let evicted = {
            let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            cache.push(frame.start, frame.data)
        };
        if evicted > 0 {
            self.cache_evicted_bytes.fetch_add(evicted, Ordering::SeqCst);
        }
        self.cursor.store(frame.end.max(self.cursor.load(Ordering::SeqCst)), Ordering::SeqCst);
        // 段1 水位：缓存游标推进即视为已消费（背压锚点到 Rust 缓存），随时可回发 ack
        self.acked.store(self.cursor.load(Ordering::SeqCst), Ordering::SeqCst);
        self.pending_ack_bytes.fetch_add(frame_bytes, Ordering::SeqCst);
    }

    /// 段2 背压恢复：窗口回落到低位水 → 解除暂停并**一次性补推**滞留区间。
    ///
    /// 为什么必须补推：暂停期间字节只进缓存，消费端（前端）看不见——它既没有
    /// 触发「帧首越过游标」的缺口帧，也不会主动重新拉取历史。若此后没有新输出，
    /// 前端将永久停在陈旧画面（与段1 的「数据留内核管道、读线程恢复即自然续读」
    /// 不同，缓存对消费端不可见，必须由生产端主动投递）。
    ///
    /// 补推按缓存的连续段切分（跨洞不合并），段间洞由前端既有「帧首越过游标 =
    /// 缺口」自愈路径处理
    fn seg2_drain(&self) {
        if !self.frontend_subscribed.load(Ordering::SeqCst) {
            return;
        }
        let was_paused = self.seg2_paused.load(Ordering::SeqCst);
        if !was_paused {
            return;
        }
        let unrendered = self.seg2_unrendered_bytes();
        if seg2_paused_after(unrendered, was_paused) {
            return;
        }
        self.seg2_paused.store(false, Ordering::SeqCst);
        let from = self.frontend_rendered.load(Ordering::SeqCst);
        let runs = {
            let cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            cache.contiguous_runs(from)
        };
        let mut pushed = 0u64;
        for (start, data) in runs {
            let end = start + data.len() as u64;
            pushed += data.len() as u64;
            self.emit_frame(start, end, &data);
        }
        tracing::debug!(
            session_id = %self.session_id,
            from_offset = from,
            unrendered_bytes = unrendered,
            drained_bytes = pushed,
            "seg2 backpressure released, buffered range re-pushed"
        );
    }

    /// 段2 消费者换代（进入终端页）：重置渲染基线与暂停态。
    ///
    /// 新消费者进入后会自行 `terminal_get_history` 拼接历史（其游标是该端的私
    /// 有状态），因此基线取当前缓存游标即可：既不产生假暂停（旧水位远低于游标），
    /// 也不会漏推——[基线, tail) 由新消费者自己的历史拼接覆盖
    fn seg2_reset_for_new_consumer(&self) {
        self.frontend_rendered
            .store(self.cursor.load(Ordering::SeqCst), Ordering::SeqCst);
        self.seg2_paused.store(false, Ordering::SeqCst);
    }

    /// 段2 渲染水位推进（前端 `terminal_ack_rendered`）
    ///
    /// 上界钳到收帧游标：前端不可能渲染未收到的字节，越界值（陈旧/错乱 ack）
    /// 会把窗口算小 → 背压永不触发且补推起点错位
    fn seg2_mark_rendered(&self, offset: u64) {
        let clamped = offset.min(self.cursor.load(Ordering::SeqCst));
        let cur = self.frontend_rendered.load(Ordering::SeqCst);
        if clamped > cur {
            self.frontend_rendered.store(clamped, Ordering::SeqCst);
        }
        self.seg2_drain();
    }

    /// 重同步落地（spec §4.7 / M3）：桌面端已把本订阅者重锚到 `min_offset`
    ///
    /// 与「间接自愈」路径（帧首越过游标 → forceReplay → getHistory → minOffset
    /// 越过游标 → 清屏）相比，本路径少一次 HTTP/命令往返，且不会在自愈冷却期
    /// 内把残缺字节写进终端。动作：
    /// 1. 缓存重锚：丢弃驻留字节，head/tail = min_offset（重播流与旧缓存之间
    ///    必然有洞，留着只会污染 `has_gap`/`contiguous_runs`）
    /// 2. 水位重锚：cursor/acked/frontend_rendered 全部置到 min_offset（前端已
    ///    清屏重锚），pending_ack 归零——随后立即回发一次 ack 让桌面端窗口归零
    ///    解锁（否则它会以「窗口 = min_offset − 旧 ack」继续驻留）
    /// 3. 置 `resynced`：重播帧直达前端（不再等 history_end，它不会到来）
    /// 4. 发事件 `terminal-resync`：前端清屏 + 重锚 + 一次性提示
    fn apply_resync(&self, min_offset: u64, snapshot_offset: u64) {
        tracing::warn!(
            session_id = %self.session_id,
            min_offset,
            snapshot_offset,
            cursor = self.cursor.load(Ordering::SeqCst),
            "terminal resync received, cache and cursors re-anchored"
        );
        {
            let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            cache.reset_to(min_offset);
        }
        self.cursor.store(min_offset, Ordering::SeqCst);
        self.acked.store(min_offset, Ordering::SeqCst);
        self.pending_ack_bytes.store(0, Ordering::SeqCst);
        self.min_offset.store(min_offset, Ordering::SeqCst);
        self.live_snapshot.store(snapshot_offset, Ordering::SeqCst);
        self.frontend_rendered.store(min_offset, Ordering::SeqCst);
        self.seg2_paused.store(false, Ordering::SeqCst);
        self.subscribe_ack.store(true, Ordering::SeqCst);
        self.resynced.store(true, Ordering::SeqCst);
        self.phase.store(LinkPhase::Live.as_u8(), Ordering::SeqCst);
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_RESYNC,
            serde_json::json!({
                "session_id": self.session_id,
                "min_offset": min_offset,
                "snapshot_offset": snapshot_offset,
            }),
        ) {
            tracing::warn!(
                session_id = %self.session_id,
                error = %e,
                "terminal resync event emit failed"
            );
        }
        self.emit_state("resync");
    }

    /// 缺口告警（节流）：帧首越过收帧游标 = 流字节缺口。min_offset 越过游标的
    /// 截断场景（服务端历史淘汰）也满足该条件，字段一并携带供判别
    fn log_offset_gap(&self, cursor_before: u64, frame_start: u64) {
        let now = now_millis();
        let last = self.last_gap_log_ms.load(Ordering::SeqCst);
        if now.saturating_sub(last) < GAP_LOG_INTERVAL_MS {
            return;
        }
        if self
            .last_gap_log_ms
            .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        tracing::warn!(
            session_id = %self.session_id,
            cursor_before,
            frame_start,
            missing_bytes = frame_start - cursor_before,
            min_offset = self.min_offset.load(Ordering::SeqCst),
            "terminal stream offset gap detected"
        );
    }

    /// 收帧统计周期打点（5s 且游标有推进才打）：与桌面端产出/转发统计对齐，
    /// 比对累计字节可定位丢字节环节
    fn maybe_log_ingest_stats(&self) {
        let now = now_millis();
        let last = self.last_stats_ms.load(Ordering::SeqCst);
        let cursor = self.cursor.load(Ordering::SeqCst);
        if now.saturating_sub(last) < INGEST_STATS_INTERVAL_MS
            || (last != 0 && cursor == self.last_stats_cursor.load(Ordering::SeqCst))
        {
            return;
        }
        if self
            .last_stats_ms
            .compare_exchange(last, now, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        self.last_stats_cursor.store(cursor, Ordering::SeqCst);
        let cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        tracing::debug!(
            session_id = %self.session_id,
            cursor,
            acked = self.acked.load(Ordering::SeqCst),
            pending_ack_bytes = self.pending_ack_bytes.load(Ordering::SeqCst),
            frames_received = self.frames_received.load(Ordering::SeqCst),
            bytes_received = self.bytes_received.load(Ordering::SeqCst),
            frames_live = self.frames_live_emitted.load(Ordering::SeqCst),
            bytes_live_emitted = self.bytes_live_emitted.load(Ordering::SeqCst),
            frames_cached = self.frames_cache_only.load(Ordering::SeqCst),
            bytes_cached = self.bytes_cache_only.load(Ordering::SeqCst),
            cache_head = cache.head,
            cache_tail = cache.tail,
            cache_bytes = cache.bytes,
            cache_entries = cache.entries.len(),
            cache_evicted_bytes = self.cache_evicted_bytes.load(Ordering::SeqCst),
            parse_residue_bytes = self.parse_residue_bytes.load(Ordering::SeqCst),
            frontend_subscribed = self.frontend_subscribed.load(Ordering::SeqCst),
            "terminal link ingest stats (periodic)"
        );
    }

    /// 查询历史（缓存优先）：`(min, snapshot, history_bytes, data_base64, has_gap)`
    ///
    /// `has_gap` = 返回区间内存在已知字节洞（快照跨洞拼接）：消费端必须清屏后
    /// 重播，不能把拼接产物当作连续流失真上屏
    fn cached_history(&self, from: u64) -> Option<(u64, u64, u64, String, bool)> {
        let cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if cache.is_empty() {
            return None;
        }
        let (head, tail, bytes, data) = cache.snapshot(from);
        let has_gap = cache.has_gap(from);
        Some((
            head,
            tail,
            bytes,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data),
            has_gap,
        ))
    }

    /// 发送出站帧（命令侧调用）。通道关闭 = IO 任务已退出（链接死亡），此时
    /// 命令随之丢弃必须留痕——重连恢复由状态机/重新订阅负责，不在此处重试
    async fn send_out(&self, out: Outbound) {
        if let Err(e) = self.write_tx.send(out).await {
            tracing::warn!(
                session_id = %self.session_id,
                out = ?e.0,
                "terminal link io task exited, outbound command dropped"
            );
        }
    }
}

// ==================== Manager（单例） ====================

/// 终端链路管理器：session_id → TerminalLink
pub struct TerminalLinkManager {
    links: Mutex<HashMap<String, Arc<TerminalLink>>>,
    /// 段2（前端 ↔ Rust）订阅态：session_id → 共享开关。
    ///
    /// 为什么独立于 `links` 存放：段2 的生命周期是「终端页进出」，与链路的
    /// 「会话存续」正交——链路会因会话停止/断开被 `unsubscribe` 停掉并在恢复时
    /// 新建实例，而页面可能一直开着。订阅态挂在管理器上、以 `Arc` 共享给链路
    /// 实例，链路重建后沿用同一份开关，页面存活期间不会静默断流。
    ///
    /// 入口处（页面挂载早于链路建立）也必须能记录订阅意愿：`page_subscribe`
    /// 只写开关、不依赖链路存在，链路建立时读取当前值即可。
    consumers: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

static MANAGER: OnceLock<Arc<TerminalLinkManager>> = OnceLock::new();

pub fn terminal_link_manager() -> Arc<TerminalLinkManager> {
    MANAGER
        .get_or_init(|| {
            Arc::new(TerminalLinkManager {
                links: Mutex::new(HashMap::new()),
                consumers: Mutex::new(HashMap::new()),
            })
        })
        .clone()
}

impl TerminalLinkManager {
    /// 取会话的段2 订阅开关（不存在则创建，初值 false）。
    ///
    /// 同一会话始终返回同一 `Arc`：链路重建时 `subscribe` 复用该开关，
    /// 页面在链路重建前后订阅态保持一致
    fn consumer_flag(&self, session_id: &str) -> Arc<AtomicBool> {
        let mut consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        consumers
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    /// 段2 订阅（进入终端页）：开启实时帧事件推送，并重置段2 背压基线。
    ///
    /// 幂等；链路尚未建立也生效（订阅意愿先于链路记录，链路建立后即生效——
    /// 新链路以初始基线启动，与新消费者首次历史拼接一致）
    pub fn page_subscribe(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        let was = self.consumer_flag(session_id).swap(true, Ordering::SeqCst);
        // 新消费者换代：渲染基线与暂停态必须一并重置——沿用上一代的低水位会
        // 在与缓存游标的差值上算出巨大假窗口 → 一进页面就暂停推送（终端空屏）
        if let Some(link) = self.get(session_id) {
            link.seg2_reset_for_new_consumer();
        }
        if !was {
            // 链路调试（订阅边界）：段2 状态迁移是「前端是否收帧」的唯一开关，
            // 与页面进出一一对应；缺此日志时「页面开着却没内容」无法与
            // 「段1 未订阅」区分
            tracing::debug!(session_id = %session_id, "terminal page subscribe (frontend consumer attached)");
        }
    }

    /// 段2 取消订阅（退出终端页）：停止实时帧事件推送，收帧与缓存继续
    /// （段1 不受影响——桌面端输出照常入 Rust 缓存，重进页面经历史拼接回补）
    pub fn page_unsubscribe(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        let was = self.consumer_flag(session_id).swap(false, Ordering::SeqCst);
        if was {
            tracing::debug!(session_id = %session_id, "terminal page unsubscribe (frontend consumer detached, cache keeps filling)");
        }
    }

    /// 查询段2 订阅态（诊断/测试用）
    pub fn is_page_subscribed(&self, session_id: &str) -> bool {
        let consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        consumers
            .get(session_id)
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// 段1 订阅会话（会话 WS 连接成功时触发；幂等——已存在且未停止则忽略）
    ///
    /// 订阅 = 建立每会话 WS 连接（auth → subscribe），流与重连全由 IO 任务管理；
    /// 意外断开自动重连重订阅（保留游标），手动取消经 `unsubscribe` 关连接
    pub fn subscribe(&self, app: AppHandle, session_id: String) {
        if session_id.is_empty() {
            return;
        }
        // 段2 开关与链路共享同一 Arc（重建链路沿用页面订阅态）。在取 `links`
        // 锁之前取：避免 `links` → `consumers` 嵌套加锁（另一路径
        // `page_subscribe` 是 consumers → links 顺序，锁序不一致即有死锁风险）
        let frontend_subscribed = self.consumer_flag(&session_id);
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = links.get(&session_id) {
            if !existing.stopped.load(Ordering::SeqCst) {
                // 幂等：链路已在运行。此时**必须补发一次当前状态**——前端
                // `subscribed` 信念可能为假（页面/Store 重建、状态事件丢失、
                // 重复 running 广播等），而前端只能靠 `terminal-state` 事件收敛。
                // 静默 return 会让它永久停在未订阅：输入被 subscribed 门控拒绝、
                // 订阅重试循环空转（历史 bug 现场：运行中输入无反应）
                existing.emit_state("resubscribed");
                return;
            }
        }
        let (write_tx, write_rx) = mpsc::channel::<Outbound>(256);
        let link = TerminalLink::new(session_id.clone(), app, write_tx, frontend_subscribed);
        link.stopped.store(false, Ordering::SeqCst);
        link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
        link.emit_state("subscribing");
        let handle = spawn_with_error_boundary("terminal_link_io", link_io(link.clone(), write_rx));
        *link.handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        links.insert(session_id, link);
    }

    /// 取消订阅（会话停止 / 手动断开）：关连接不再重连，保留缓存供重开恢复
    pub fn unsubscribe(&self, session_id: &str) {
        let link = {
            let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            links.get(session_id).cloned()
        };
        if let Some(link) = link {
            link.stopped.store(true, Ordering::SeqCst);
            link.phase.store(LinkPhase::Idle.as_u8(), Ordering::SeqCst);
            link.emit_state("unsubscribed");
            // 唤醒 IO 任务优雅退出
            let tx = link.write_tx.clone();
            let session_id = session_id.to_string();
            spawn_with_error_boundary("terminal_unsubscribe_close", async move {
                // send 失败 = IO 任务已先行退出（连接已断/已停止）：常规退路径，debug 留痕即可
                if tx.send(Outbound::Close).await.is_err() {
                    tracing::debug!(session_id = %session_id, "io task already exited, close signal skipped");
                }
            });
        }
    }

    /// 全部取消（设备手动断开 / 连接关闭）
    pub fn unsubscribe_all(&self) {
        let ids: Vec<String> = {
            let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            links.keys().cloned().collect()
        };
        for id in ids {
            self.unsubscribe(&id);
        }
    }

    /// 会话删除：清链路 + 缓存 + 段2 订阅态（会话已不存在，页面订阅意愿一并作废）
    pub fn remove(&self, session_id: &str) {
        self.unsubscribe(session_id);
        {
            let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            links.remove(session_id);
        }
        let mut consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        consumers.remove(session_id);
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<TerminalLink>> {
        let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        links.get(session_id).cloned()
    }
}

// ==================== IO 任务（连接 / 收发 / 重连） ====================

/// 单次连接会话（成功 → 流结束即退出返回；意外断开 → 外层按原因重连）
async fn connect_once(link: &Arc<TerminalLink>, write_rx: &mut mpsc::Receiver<Outbound>) -> Result<(), LinkExit> {
    let conn = crate::state::get_connection_manager();
    let target = conn
        .get_target()
        .await
        .ok_or(LinkExit::Io)?; // 目标缺失：按 Io 重连（目标恢复后自动续）
    let url = format!(
        "ws://{}:{}{}/{}",
        target.address,
        target.port,
        crate::system::constants::connection::WS_TERMINAL_SESSION_PATH,
        link.session_id
    );
    let token = crate::state::get_global_token();

    link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %link.session_id, error = %e, "terminal ws connect failed");
            LinkExit::Io
        })?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // 每次连接重建握手闸门：subscribe_ok 之前到达的帧只入缓存（重播历史段）
    link.subscribe_ack.store(false, Ordering::SeqCst);

    // 首消息认证（JWT；连接是会话绑定态的）
    let auth = format!(r#"{{"type":"auth","token":"{}"}}"#, token);
    ws_tx.send(WsMsg::Text(auth)).await.map_err(|_| LinkExit::Io)?;
    link.phase.store(LinkPhase::Auth.as_u8(), Ordering::SeqCst);
    link.emit_state("auth");

    // ack 回发辅助（节流：64KB 阈值 + 250ms 空闲兜底）
    async fn maybe_ack(
        link: &Arc<TerminalLink>,
        ws_sink: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            WsMsg,
        >,
    ) {
        let acked = link.acked.load(Ordering::SeqCst);
        if acked == 0 {
            return;
        }
        let now = now_millis();
        let pending = link.pending_ack_bytes.swap(0, Ordering::SeqCst);
        let last = link.last_ack_at.load(Ordering::SeqCst);
        if should_send_ack(pending, last, now) {
            let frame = build_ack_frame(&link.session_id, acked);
            link.last_ack_at.store(now, Ordering::SeqCst);
            // 链路调试（背压对账）：ack 回发推进桌面端 unacked 释放——与桌面端
            // on_ack 日志对照验证反馈环；ack 已节流（64KB/250ms），无风暴风险
            tracing::debug!(
                session_id = %link.session_id,
                acked,
                pending_bytes = pending,
                "terminal ack frame sent (release desktop unacked accounting)"
            );
            // ack 回发失败 = 连接已坏：水位信息本次丢失，重连订阅后由游标重同步
            if let Err(e) = futures_util::SinkExt::send(ws_sink, WsMsg::Binary(frame)).await {
                tracing::warn!(
                    session_id = %link.session_id,
                    acked,
                    error = %e,
                    "terminal ack frame send failed, wait for reconnect"
                );
            }
        } else {
            // 未达阈值也未到空闲兜底：下次收帧 / 空闲定时器再判（节流 pending 保留）
            link.pending_ack_bytes.fetch_add(pending, Ordering::SeqCst);
        }
    }

    // ack 空闲兜底定时器：见 should_send_ack 注释——末批（<64KB）积压 ack 若只靠
    // 收帧事件触发，上游被背压暂停后永远不会回发，桌面端 PTY 读永久停摆。
    // 半空闲窗口轮询，保证积压 ack 最迟 ~1.5×ACK_MAX_IDLE_MS 内回发；
    // MissedTickBehavior::Delay 防止长暂停（如 ws send 阻塞）后补发风暴
    let mut ack_idle_tick = tokio::time::interval(Duration::from_millis(ACK_IDLE_TICK_MS));
    ack_idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            out = write_rx.recv() => {
                let Some(out) = out else { return Ok(()); };
                match out {
                    Outbound::Close => {
                        tracing::debug!(session_id = %link.session_id, "terminal link closed by command");
                        return Ok(());
                    }
                    Outbound::Subscribe { from_offset } => {
                        // 链路调试：订阅锚点（重连续传关键参数，控制帧低频）
                        tracing::debug!(
                            session_id = %link.session_id,
                            from_offset,
                            "terminal subscribe frame sent"
                        );
                        let msg = if from_offset > 0 {
                            format!(r#"{{"type":"subscribe","from_offset":{}}}"#, from_offset)
                        } else {
                            r#"{"type":"subscribe"}"#.to_string()
                        };
                        if ws_tx.send(WsMsg::Text(msg)).await.is_err() {
                            return Err(LinkExit::Io);
                        }
                    }
                    Outbound::Input { data, special_key } => {
                        let b64 = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            data.as_bytes(),
                        );
                        let msg = match special_key {
                            Some(k) => format!(r#"{{"type":"input","data":"{}","special_key":"{}"}}"#, b64, k),
                            None => format!(r#"{{"type":"input","data":"{}"}}"#, b64),
                        };
                        if ws_tx.send(WsMsg::Text(msg)).await.is_err() {
                            return Err(LinkExit::Io);
                        }
                    }
                    Outbound::Ack { offset } => {
                        // 段2 水位：前端渲染游标推进（上界钳到收帧游标），窗口回落
                        // 到低位水即解除暂停并一次性补推缓存里滞留的区间
                        link.seg2_mark_rendered(offset);
                        // 段1 ack 顺带补发：段1 水位锚定 Rust 缓存游标（收帧时已
                        // 推进），前端 ack 只是「消费端仍有进展」的天然触发点
                        maybe_ack(link, &mut ws_tx).await;
                    }
                    Outbound::SetMode { mode } => {
                        link.mode.store(mode.as_u8(), Ordering::SeqCst);
                        let msg = format!(r#"{{"type":"mode","mode":"{}"}}"#, mode.as_str());
                        if ws_tx.send(WsMsg::Text(msg)).await.is_err() {
                            return Err(LinkExit::Io);
                        }
                    }
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg.map_err(|e| {
                    tracing::debug!(session_id = %link.session_id, error = %e, "terminal ws stream error");
                    LinkExit::Io
                })?;
                match msg {
                    WsMsg::Text(text) => {
                        handle_control_text(link, &text, &mut ws_tx).await?;
                    }
                    WsMsg::Binary(bin) => {
                        let frames = parse_tb_frames(&bin);
                        // 解析残渣对账：帧边界损坏/未知版本截断时尾部字节既不进缓存
                        // 也不推事件（本环节丢字节），必须留痕
                        let parsed_bytes: usize = frames
                            .iter()
                            .map(|f| TB_FRAME_HEADER_LEN + f.data.len())
                            .sum();
                        if parsed_bytes != bin.len() {
                            link.parse_residue_bytes
                                .fetch_add((bin.len() - parsed_bytes) as u64, Ordering::SeqCst);
                            tracing::warn!(
                                session_id = %link.session_id,
                                message_bytes = bin.len(),
                                parsed_bytes,
                                "tb frame parse residue bytes dropped at parse stage"
                            );
                        }
                        // 缺口检测：帧首越过收帧游标 = 流字节缺口（每条消息只记首处，
                        // 告警节流，防真实缺口期间风暴）
                        let mut gap: Option<(u64, u64)> = None;
                        for frame in &frames {
                            let cur = link.cursor.load(Ordering::SeqCst);
                            if cur > 0 && frame.start > cur {
                                gap = Some((cur, frame.start));
                                break;
                            }
                        }
                        if let Some((cursor_before, frame_start)) = gap {
                            link.log_offset_gap(cursor_before, frame_start);
                        }
                        for frame in frames {
                            link.ingest_frame(frame);
                        }
                        link.maybe_log_ingest_stats();
                        maybe_ack(link, &mut ws_tx).await;
                    }
                    WsMsg::Ping(p) => {
                        // pong 回发失败 = 连接已坏，由流错误路径收敛重连（keepalive 常规细节，debug 即可）
                        if let Err(e) = ws_tx.send(WsMsg::Pong(p)).await {
                            tracing::debug!(session_id = %link.session_id, error = %e, "terminal pong send failed");
                        }
                    }
                    WsMsg::Pong(_) => {}
                    WsMsg::Close(_) => break,
                    _ => {}
                }
            }
            _ = ack_idle_tick.tick() => {
                // 空闲窗口到：回发积压 ack（pending=0 时 should_send_ack 内部短路，
                // 不会产生空 ack 风暴）
                maybe_ack(link, &mut ws_tx).await;
            }
        }
    }
    // 正常断流（服务端关闭）→ 视作意外断开回退重连（除非 stopped）
    if link.stopped.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(LinkExit::Io)
    }
}

/// 重连后桌面端传播模式重置 realtime 的再同步：本端仍标记 batch 时返回
/// 需补发的 mode 控制帧（否则 None——桌面端默认 realtime 与本端一致）
fn batch_resync_message(mode: u8) -> Option<String> {
    if mode == LinkMode::Batch.as_u8() {
        Some(r#"{"type":"mode","mode":"batch"}"#.to_string())
    } else {
        None
    }
}

/// 处理 JSON 控制帧（auth_ok / subscribe_ok / history_end / session_stopped / error）
async fn handle_control_text(
    link: &Arc<TerminalLink>,
    text: &str,
    ws_tx: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        WsMsg,
    >,
) -> Result<(), LinkExit> {
    let msg: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    match msg.get("type").and_then(|v| v.as_str()) {
        Some("auth_ok") => {            // 认证成功：发出订阅（from_offset = 已接收游标，重连续传不重发已缓存区）
            let from = link.cursor.load(Ordering::SeqCst);
            link.send_out(Outbound::Subscribe { from_offset: from }).await;
        }
        Some("subscribe_ok") => {
            let snapshot = msg.get("snapshot_offset").and_then(|v| v.as_u64()).unwrap_or(0);
            let min = msg.get("min_offset").and_then(|v| v.as_u64()).unwrap_or(0);
            // 链路调试（字节对账）：快照锚点与桌面端 subscribe_ok 发送侧日志对照
            tracing::debug!(
                session_id = %link.session_id,
                snapshot_offset = snapshot,
                min_offset = min,
                "subscribe_ok received, replaying history segment into cache"
            );
            link.live_snapshot.store(snapshot, Ordering::SeqCst);
            link.min_offset.store(min, Ordering::SeqCst);
            // 新一轮订阅 = 正常历史/实时分段：清掉上一次重同步的「全部帧直达前端」态
            link.resynced.store(false, Ordering::SeqCst);
            // 闸门置位前必须先落 live_snapshot：重播历史段（end ≤ snapshot）
            // 仍只入缓存，只有真正的实时帧（end > snapshot）才允许推送
            link.subscribe_ack.store(true, Ordering::SeqCst);
            link.phase.store(LinkPhase::History.as_u8(), Ordering::SeqCst);
            link.session_missing.store(0, Ordering::SeqCst);
            link.emit_state("subscribed");
        }
        Some("history_end") => {
            tracing::debug!(
                session_id = %link.session_id,
                cursor = link.cursor.load(Ordering::SeqCst),
                "history_end received, entering live phase"
            );
            link.phase.store(LinkPhase::Live.as_u8(), Ordering::SeqCst);
            // 重连后桌面端重订阅会把传播模式重置为 realtime（handle_session_subscribe
            // mode.store(MODE_REALTIME)）：若本端已标记 batch（页面退出）则补发 mode 帧
            // 恢复——否则 batch 语义在重连后静默失效，空转接收全量实时流量直到下次进页面
            if let Some(msg) = batch_resync_message(link.mode.load(Ordering::SeqCst)) {
                if ws_tx.send(WsMsg::Text(msg)).await.is_err() {
                    return Err(LinkExit::Io);
                }
            }
            link.emit_state("live");
        }
        Some("resync") => {
            // 截断重同步（spec §4.7）：桌面端已重锚并从 min_offset 重播
            let min = msg.get("min_offset").and_then(|v| v.as_u64()).unwrap_or(0);
            let snapshot = msg.get("snapshot_offset").and_then(|v| v.as_u64()).unwrap_or(0);
            link.apply_resync(min, snapshot);
            // 立即回发 ack（水位 = 重锚点）：桌面端窗口 = next(min) − acked(min) = 0
            // → 该订阅者立刻解除驻留，重播不必等 250ms 空闲兜底窗口
            if ws_tx
                .send(WsMsg::Binary(build_ack_frame(&link.session_id, min)))
                .await
                .is_err()
            {
                return Err(LinkExit::Io);
            }
            link.last_ack_at.store(now_millis(), Ordering::SeqCst);
        }
        Some("session_stopped") => {
            tracing::info!(session_id = %link.session_id, "terminal session stopped");
            link.stopped.store(true, Ordering::SeqCst);
            link.phase.store(LinkPhase::Idle.as_u8(), Ordering::SeqCst);
            link.emit_state("stopped");
            return Ok(()); // 连接由服务端关闭，正常退出
        }
        Some("error") => {
            let code = msg.get("code").and_then(|v| v.as_str()).unwrap_or("UNKNOWN").to_string();
            let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            tracing::warn!(session_id = %link.session_id, code = %code, message = %message, "terminal ws server error");
            if code == "SESSION_NOT_FOUND" {
                let strikes = link.session_missing.fetch_add(1, Ordering::SeqCst) + 1;
                if strikes >= MAX_SESSION_MISSING_STRIKES {
                    tracing::warn!(
                        session_id = %link.session_id,
                        strikes,
                        "session missing after {} attempts, stopping terminal link",
                        MAX_SESSION_MISSING_STRIKES
                    );
                    link.stopped.store(true, Ordering::SeqCst);
                    link.phase.store(LinkPhase::Idle.as_u8(), Ordering::SeqCst);
                    link.emit_state("session_missing");
                    return Err(LinkExit::SessionMissing);
                }
                // 有限重试：服务端 error 后保持连接，等客户端重订阅——重连由连接
                // 层兜底（Io 退出）；此处直接订阅重试
                link.emit_state("retry");
                if let Err(e) = futures_util::SinkExt::send(
                    ws_tx,
                    WsMsg::Binary(build_ack_frame(&link.session_id, link.acked.load(Ordering::SeqCst))),
                )
                .await
                {
                    tracing::warn!(
                        session_id = %link.session_id,
                        error = %e,
                        "terminal ack frame send failed, wait for reconnect"
                    );
                }
                link.send_out(Outbound::Subscribe { from_offset: link.cursor.load(Ordering::SeqCst) })
                    .await;
            }
        }
        _ => {
            tracing::debug!(session_id = %link.session_id, type = ?msg.get("type"), "unknown control frame");
        }
    }
    Ok(())
}

/// IO 任务主循环：连接 → 断 → 退避重连（意外）/ 停止（手动 / 会话缺失）
async fn link_io(link: Arc<TerminalLink>, mut write_rx: mpsc::Receiver<Outbound>) {
    let mut attempt: u32 = 0;
    loop {
        if link.stopped.load(Ordering::SeqCst) {
            return;
        }
        match connect_once(&link, &mut write_rx).await {
            Ok(()) => return, // 正常退出
            Err(LinkExit::SessionMissing) => {
                if link.stopped.load(Ordering::SeqCst) {
                    return;
                }
                // 会话缺失已达上限等外部恢复：不自动重连，等待 frontend 重新 subscribe
                return;
            }
            Err(LinkExit::Io) => {
                if link.stopped.load(Ordering::SeqCst) {
                    return;
                }
                link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
                link.emit_state("reconnecting");
                let delay_ms =
                    (RECONNECT_BASE_MS * 2u64.pow(attempt.min(4))).min(RECONNECT_MAX_MS);
                attempt = (attempt + 1).min(16);
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
// ==================== Tauri 命令面（前端触发入口） ====================

/// 段1 订阅会话（会话 WS 连接成功后触发；幂等）
///
/// 订阅由 Rust 管理：连接、认证、订阅、缓存、意外断开自动重连重订阅
///（保留字节游标）；前端只负责在正确的时机触发/取消。
/// 与段2（`terminal_page_subscribe`）独立：本命令决定「是否连桌面端收帧」，
/// 段2 决定「是否把帧推给前端」——两者生命周期互不影响
#[tauri::command]
pub async fn terminal_subscribe(app: tauri::AppHandle, session_id: String) -> Result<(), String> {
    let manager = terminal_link_manager();
    manager.subscribe(app, session_id);
    Ok(())
}

/// 段1 取消订阅（会话停止 / 手动断开）：关连接不再重连；缓存保留供重开恢复
#[tauri::command]
pub async fn terminal_unsubscribe(session_id: String) -> Result<(), String> {
    terminal_link_manager().unsubscribe(&session_id);
    Ok(())
}

/// 段2 订阅（进入终端页）：开启 `terminal-frame` 事件推送。
///
/// 幂等；不依赖链路存在——链路未建立时先记录订阅意愿，链路建立后即生效
/// （页面挂载早于会话订阅完成的场景不会丢流）
#[tauri::command]
pub async fn terminal_page_subscribe(session_id: String) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("session_id is empty".to_string());
    }
    terminal_link_manager().page_subscribe(&session_id);
    Ok(())
}

/// 段2 取消订阅（退出终端页）：停止事件推送，段1 收帧与缓存照常
/// （重进页面经 `terminal_get_history` 回补页面关闭期间的输出）
#[tauri::command]
pub async fn terminal_page_unsubscribe(session_id: String) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("session_id is empty".to_string());
    }
    terminal_link_manager().page_unsubscribe(&session_id);
    Ok(())
}

/// 全部取消订阅（设备手动断开 / 连接关闭时由前端调用）
#[tauri::command]
pub async fn terminal_unsubscribe_all() -> Result<(), String> {
    terminal_link_manager().unsubscribe_all();
    Ok(())
}

/// 会话删除：清理链路 + 缓存
#[tauri::command]
pub async fn terminal_remove(session_id: String) -> Result<(), String> {
    terminal_link_manager().remove(&session_id);
    Ok(())
}

/// 发送终端输入（前端 → Rust → WS input 帧 → 桌面端 PTY）
#[tauri::command]
pub async fn terminal_send_input(session_id: String, data: String, special_key: Option<String>) -> Result<(), String> {
    let manager = terminal_link_manager();
    let link = manager.get(&session_id).ok_or_else(|| "terminal link not subscribed".to_string())?;
    link.send_out(Outbound::Input { data, special_key }).await;
    Ok(())
}

/// 切换实时传播模式（双速）：realtime = 进终端页读即传；batch = 退出终端页
/// 但会话未停（桌面端累计满 batch_bytes 才转发，参数桌面端可配置）
#[tauri::command]
pub async fn terminal_set_mode(session_id: String, mode: String) -> Result<(), String> {
    let manager = terminal_link_manager();
    let link = manager.get(&session_id).ok_or_else(|| "terminal link not subscribed".to_string())?;
    let mode = match mode.as_str() {
        "batch" => LinkMode::Batch,
        _ => LinkMode::Realtime,
    };
    link.send_out(Outbound::SetMode { mode }).await;
    Ok(())
}

/// 渲染背压 ack：提升 ack 水位（前端 onWriteParsed 后按渲染游标调用）；
/// Rust 侧节流回发 v3 ACK 帧，桌面端据此释放未 ack 记账恢复 PTY 读取
#[tauri::command]
pub async fn terminal_ack_rendered(session_id: String, offset: u64) -> Result<(), String> {
    let manager = terminal_link_manager();
    let link = manager.get(&session_id).ok_or_else(|| "terminal link not subscribed".to_string())?;
    link.send_out(Outbound::Ack { offset }).await;
    Ok(())
}

/// 一次性历史（拼接用）：缓存优先；缓存头被淘汰（或缓存为空）时回退桌面
/// HTTP `GET /api/sessions/{id}/history?from=...` 拉取（kind=desktop 走既有
/// JWT/信封校验，见 commands/http_proxy）
#[tauri::command]
pub async fn terminal_get_history(
    app: tauri::AppHandle,
    session_id: String,
    from: u64,
) -> Result<serde_json::Value, String> {
    let manager = terminal_link_manager();
    let link = manager.get(&session_id).ok_or_else(|| "terminal link not subscribed".to_string())?;

    // 缓存命中：直接返回（真源 = Rust 缓存）。camelCase 键对齐前端
    // TerminalHistoryResult 接口——Tauri invoke 只对请求参数做 camelCase
    // 转换，返回值原样传递，键名必须在 Rust 侧与前端契约一致
    if let Some((min, snapshot, history_bytes, data_b64, has_gap)) = link.cached_history(from) {
        // 链路调试（字节对账）：缓存命中路径——payload_bytes 为 base64 长度
        //（略大于原始字节），与 dataBase64 一同供前端拼接对账
        tracing::debug!(
            session_id = %session_id,
            from_offset = from,
            min_offset = min,
            snapshot_offset = snapshot,
            history_bytes,
            payload_b64_bytes = data_b64.len(),
            has_gap,
            "terminal history served from rust cache"
        );
        return Ok(serde_json::json!({
            "from": from,
            "minOffset": min,
            "snapshotOffset": snapshot,
            "historyBytes": history_bytes,
            "dataBase64": data_b64,
            "gapDetected": has_gap,
        }));
    }

    // 缓存未命中（空/头被淘汰）：一次性 HTTP 拉取历史
    tracing::debug!(
        session_id = %session_id,
        from_offset = from,
        "terminal history cache miss, falling back to desktop http"
    );
    // 桌面端返回统一 ApiResponse 信封 {code, message, data:{min_offset, snapshot_offset,
    // history_bytes, data_base64}}——此处剥信封、code!=0 视为错误、转 camelCase
    let conn = crate::state::get_connection_manager();
    let target = conn
        .get_target()
        .await
        .ok_or_else(|| "no target device, history unavailable".to_string())?;
    let url = format!(
        "http://{}:{}/api/sessions/{}/history?from={}",
        target.address, target.port, session_id, from
    );
    let request = crate::commands::http_proxy::HttpProxyRequest {
        request_id: format!("terminal-history-{session_id}"),
        method: "GET".to_string(),
        url,
        headers: std::collections::HashMap::new(),
        body: None,
        timeout_ms: Some(30_000),
        kind: Some("desktop".to_string()),
    };
    let response = crate::commands::http_proxy::execute_proxy(request, Some(&app)).await.map_err(|e| {
        tracing::warn!(session_id = %session_id, error = %e, "terminal history http fetch failed");
        e.to_string()
    })?;
    if response.status != 200 {
        return Err(format!("history fetch failed: status {}", response.status));
    }
    let parsed: serde_json::Value = serde_json::from_str(&response.body_text)
        .map_err(|e| format!("history response parse failed: {e}"))?;
    let code = parsed.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
    if code != 0 {
        let message = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("unknown error");
        return Err(format!("history fetch failed: code {code} {message}"));
    }
    let data = parsed.get("data").cloned().unwrap_or(parsed);
    // 桌面端 `SessionHistoryData` 带 `#[serde(rename_all = "camelCase")]`：线上
    // 键名是 minOffset/snapshotOffset/historyBytes/dataBase64。此前读 snake_case
    // 四键全 miss，静默落到默认值（min_offset→from、history_bytes→0），LRU 淘汰
    // 后 HTTP 回退拼不出历史且截断信号丢失。
    let min_offset = data.get("minOffset").and_then(|v| v.as_u64()).unwrap_or(from);
    let snapshot_offset = data
        .get("snapshotOffset")
        .and_then(|v| v.as_u64())
        .unwrap_or(from);
    let history_bytes = data
        .get("historyBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let data_base64 = data
        .get("dataBase64")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // 链路调试（字节对账）：HTTP 回取路径——min_offset > from 说明桌面端历史
    // 头也被淘汰（前端需清屏重播），与桌面端 history served 日志对照
    tracing::debug!(
        session_id = %session_id,
        from_offset = from,
        min_offset,
        snapshot_offset,
        history_bytes,
        payload_b64_bytes = data_base64.len(),
        "terminal history fetched via desktop http"
    );
    Ok(serde_json::json!({
        "from": from,
        "minOffset": min_offset,
        "snapshotOffset": snapshot_offset,
        "historyBytes": history_bytes,
        "dataBase64": data_base64,
        // 桌面端历史队列按 chunk 连续驻留，不存在本地 WS 链路的丢帧洞
        "gapDetected": false,
    }))
}

/// 查询链路状态（前端轮询/诊断）
#[tauri::command]
pub async fn terminal_get_state(session_id: String) -> Result<serde_json::Value, String> {
    let manager = terminal_link_manager();
    let Some(link) = manager.get(&session_id) else {
        return Ok(serde_json::json!({ "session_id": session_id, "phase": "idle" }));
    };
    let phase = LinkPhase::from_u8(link.phase.load(Ordering::SeqCst));
    let mode = if link.mode.load(Ordering::SeqCst) == LinkMode::Batch.as_u8() { "batch" } else { "realtime" };
    // camelCase 键对齐前端 TerminalLinkState 接口（invoke 返回值不做键名转换）
    Ok(serde_json::json!({
        "sessionId": session_id,
        "phase": phase.as_api_str(),
        "cursor": link.cursor.load(Ordering::SeqCst),
        "snapshotOffset": link.live_snapshot.load(Ordering::SeqCst),
        "minOffset": link.min_offset.load(Ordering::SeqCst),
        "acked": link.acked.load(Ordering::SeqCst),
        "mode": mode,
        "stopped": link.stopped.load(Ordering::SeqCst),
        "historyBytes": link.cache.lock().map(|c| c.bytes).unwrap_or(0),
        // 段2 诊断：订阅态 / 前端渲染水位 / 未渲染窗口 / 背压暂停态——页面
        // 「开着却无内容」时据此区分段1 未订阅、段2 未订阅、段2 背压暂停
        "frontendSubscribed": link.frontend_subscribed.load(Ordering::SeqCst),
        "frontendRendered": link.frontend_rendered.load(Ordering::SeqCst),
        "seg2UnrenderedBytes": link.seg2_unrendered_bytes(),
        "seg2Paused": link.seg2_paused.load(Ordering::SeqCst),
    }))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 TB v3 帧字节（magic "TB" + version + flags + start_offset(8 LE) + len(4 LE) + data）
    fn v3_frame(start: u64, data: &[u8], flags: u8) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&TB_MAGIC);
        out.push(TB_VERSION_V3);
        out.push(flags);
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn batch_resync_message_only_when_batch() {
        assert_eq!(batch_resync_message(LinkMode::Realtime.as_u8()), None);
        let msg = batch_resync_message(LinkMode::Batch.as_u8()).expect("batch link must resync");
        assert_eq!(msg, r#"{"type":"mode","mode":"batch"}"#);
    }

    #[test]
    fn parse_tb_frames_v3_single() {
        let bytes = v3_frame(100, b"hello", 0);
        let frames = parse_tb_frames(&bytes);
        assert_eq!(frames.len(), 1);
        assert_eq!((frames[0].start, frames[0].end), (100, 105));
        assert_eq!(frames[0].data, b"hello");
    }

    #[test]
    fn parse_tb_frames_v3_multiple_concatenated() {
        let mut bytes = v3_frame(100, b"hello", 0);
        bytes.extend_from_slice(&v3_frame(105, b" world", 0));
        let frames = parse_tb_frames(&bytes);
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].start, frames[0].end), (100, 105));
        assert_eq!((frames[1].start, frames[1].end), (105, 111));
        assert_eq!(frames[1].data, b" world");
    }

    #[test]
    fn parse_tb_frames_truncated_header_stops_without_panic() {
        let mut bytes = v3_frame(100, b"hello", 0);
        bytes.truncate(bytes.len() - 1); // 帧体截断：len 越界 → 停止解析
        assert!(parse_tb_frames(&bytes).is_empty());
        // 头都不足 16 字节：直接停止
        assert!(parse_tb_frames(b"TB").is_empty());
        // 魔数不符：不解析
        assert!(parse_tb_frames(&[0x00, 0x01, 0x02, 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_empty());
    }

    #[test]
    fn parse_tb_frames_unknown_version_stops() {
        // magic 对 + version=9（未知）→ 停止解析（后续帧不解析）
        let mut bytes = vec![0x54, 0x42, 9, 0];
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(0x61);
        assert!(parse_tb_frames(&bytes).is_empty());
    }

    #[test]
    fn build_ack_frame_format() {
        let frame = build_ack_frame("sess-1", 123456);
        assert_eq!(&frame[0..2], b"TB");
        assert_eq!(frame[2], TB_VERSION_V3);
        assert_eq!(frame[3], TB_FLAG_ACK);
        assert_eq!(
            u64::from_le_bytes(frame[4..12].try_into().unwrap()),
            123456
        );
        let payload_len = usize::try_from(u32::from_le_bytes(frame[12..16].try_into().unwrap())).unwrap();
        assert_eq!(payload_len, 6);
        assert_eq!(&frame[16..16 + payload_len], b"sess-1");
    }

    /// 达阈值即回发：与空闲时长无关（节流上沿）
    #[test]
    fn should_send_ack_when_pending_reaches_threshold() {
        assert!(should_send_ack(ACK_BYTES_THRESHOLD, 1_000, 1_000));
        assert!(should_send_ack(ACK_BYTES_THRESHOLD + 1, 1_000, 1_001));
    }

    /// 回归护栏（背压死锁）：末批不足阈值的积压 ack，空闲兜底窗口到期必须回发。
    /// 旧实现仅在收帧时求值该规则，上游被背压暂停（不再收帧）后积压永不回发，
    /// 桌面端 unacked 停在低位水之上 → PTY 读永久暂停（滑动失效 + 输入无回显）
    #[test]
    fn should_send_ack_flushes_stranded_pending_after_idle_window() {
        let last = 10_000;
        // 空闲窗口未到：保留待发（节流语义不退化）
        assert!(!should_send_ack(1, last, last + ACK_MAX_IDLE_MS - 1));
        // 空闲窗口到期：即使只有 1 字节积压也必须回发，否则死锁
        assert!(should_send_ack(1, last, last + ACK_MAX_IDLE_MS));
        assert!(should_send_ack(1024, last, last + ACK_MAX_IDLE_MS + 5));
    }

    /// pending 为 0 一律不回发：空闲定时器周期调用不得产生空 ack 风暴
    #[test]
    fn should_send_ack_never_sends_without_pending_bytes() {
        assert!(!should_send_ack(0, 1_000, 1_000));
        assert!(!should_send_ack(0, 1_000, 1_000 + ACK_MAX_IDLE_MS * 100));
        assert!(!should_send_ack(0, 0, 1_000_000));
    }

    /// 首次回发（尚无 ack 基准）仍需攒满阈值：避免握手后立即产生零散 ack
    #[test]
    fn should_send_ack_first_send_waits_for_threshold() {
        assert!(!should_send_ack(1024, 0, 1_000_000));
        assert!(should_send_ack(ACK_BYTES_THRESHOLD, 0, 1_000_000));
    }

    /// pending == 0 时即使距上次回发很久也不发（last_ack_at 非零路径同样短路）
    #[test]
    fn should_send_ack_zero_pending_short_circuits_before_idle_rule() {
        assert!(!should_send_ack(0, 500, 500 + ACK_MAX_IDLE_MS * 10));
        // 对照：同样时刻只要有一个字节积压就回发
        assert!(should_send_ack(1, 500, 500 + ACK_MAX_IDLE_MS * 10));
    }

    /// 「任何非零积压都不会滞留」性质：阈值以下的每种 pending 都在空闲窗口内
    /// 翻转为回发（若规则被改成「同时要求阈值与空闲」等永不可达组合，此处必红）
    #[test]
    fn should_send_ack_never_strands_sub_threshold_pending() {
        for pending in [1u64, 2, 512, 4096, ACK_BYTES_THRESHOLD - 1] {
            let last = 1_000;
            assert!(
                !should_send_ack(pending, last, last + ACK_MAX_IDLE_MS - 1),
                "空闲窗口未到不应回发: pending={pending}"
            );
            assert!(
                should_send_ack(pending, last, last + ACK_MAX_IDLE_MS),
                "空闲窗口到期必须回发: pending={pending}"
            );
        }
    }

    /// 轮询间隔必须落在空闲窗口内（否则积压 ack 的送达被推迟到窗口之外）
    #[test]
    fn ack_idle_tick_is_within_idle_window() {
        assert!(ACK_IDLE_TICK_MS > 0);
        assert!(ACK_IDLE_TICK_MS <= ACK_MAX_IDLE_MS);
        assert!(
            ACK_IDLE_TICK_MS + ACK_MAX_IDLE_MS <= ACK_MAX_IDLE_MS * 2,
            "积压 ack 最迟应在 1.5×空闲窗口内回发"
        );
    }

    // ==================== 段2 门控与背压 ====================

    /// 段2 推送门控：四个条件全真才推；任一为假都必须只入缓存（丢帧即终端缺内容）
    #[test]
    fn should_emit_live_frame_requires_all_four_gates() {
        // 全真：实时段 + 握手完成 + 段2 已订阅 + 段2 未越水位
        assert!(should_emit_live_frame(200, 100, true, true, false, false));
        // 历史段（end ≤ snapshot）：由 terminal_get_history 一次性供给
        assert!(!should_emit_live_frame(100, 100, true, true, false, false));
        // 握手窗口（subscribe_ok 未到）：重播段不得当实时帧推送
        assert!(!should_emit_live_frame(200, 100, false, true, false, false));
        // 段2 未订阅（终端页未打开）
        assert!(!should_emit_live_frame(200, 100, true, false, false, false));
        // 段2 背压暂停：字节留缓存，恢复时补推
        assert!(!should_emit_live_frame(200, 100, true, true, true, false));
    }

    /// 重同步后（spec §4.7）：历史边界不再适用——重播段本身就是恢复负载，
    /// 即使 end ≤ snapshot 也必须直达前端（否则恢复内容退回一次 get_history 往返）
    #[test]
    fn should_emit_live_frame_resynced_bypasses_snapshot_gate() {
        // 重播帧（end == snapshot）：resynced 下必须推送
        assert!(should_emit_live_frame(100, 100, true, true, false, true));
        // 重播起点帧（end == min_offset < snapshot）
        assert!(should_emit_live_frame(50, 100, true, true, false, true));
        // 其余三个门控仍然生效（resynced 不是万能放行）
        assert!(!should_emit_live_frame(100, 100, false, true, false, true), "握手未完成");
        assert!(!should_emit_live_frame(100, 100, true, false, false, true), "段2 未订阅");
        assert!(!should_emit_live_frame(100, 100, true, true, true, true), "段2 暂停");
    }

    /// 重同步重锚：缓存整段丢弃并把 head/tail 锚定到重锚点，且不登记缺口
    /// （重播首帧恰从重锚点起 → 消费端按重锚游标无缺口接收）
    #[test]
    fn session_cache_reset_to_reanchors_without_gap() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"aaaa");
        push_data(&mut cache, 4, b"bbbb");
        assert_eq!(cache.tail, 8);

        cache.reset_to(4096);
        assert!(cache.is_empty(), "重锚必须丢弃全部驻留字节");
        assert_eq!(cache.head, 4096);
        assert_eq!(cache.tail, 4096);
        assert!(!cache.has_gap(4096), "重锚后不得残留旧缺口");

        // 重播流从重锚点连续到达：不产生缺口登记
        push_data(&mut cache, 4096, b"cccc");
        assert_eq!(cache.tail, 4100);
        assert!(!cache.has_gap(4096), "从重锚点起的连续重播不得被判为缺口");
        assert_eq!(cache.snapshot(4096).3, b"cccc");
    }

    /// 段2 水位滞回：未暂停超高位水才停推；已暂停降到低位水才恢复；
    /// 区间内保持现状（单阈值会在临界点抖振，每次抖振都伴随一次缓存补推）
    #[test]
    fn seg2_paused_after_has_hysteresis_between_watermarks() {
        let high = SEG2_HIGH_WATER_BYTES;
        let low = SEG2_LOW_WATER_BYTES;
        assert!(low < high, "低位水必须低于高位水，否则无滞回区间");

        // 未暂停：低位水区间内不触发（阈值是严格大于）
        assert!(!seg2_paused_after(0, false));
        assert!(!seg2_paused_after(high, false));
        assert!(!seg2_paused_after(low, false));
        // 未暂停：越高位水 → 暂停
        assert!(seg2_paused_after(high + 1, false));

        // 已暂停：仍高于低位水 → 保持暂停（滞回区间内不回弹）
        assert!(seg2_paused_after(high + 1, true));
        assert!(seg2_paused_after(low + 1, true));
        // 已暂停：降到低位水（含）以内 → 恢复
        assert!(!seg2_paused_after(low, true));
        assert!(!seg2_paused_after(0, true));
    }

    /// 连续段切分：段内连续、段间有洞必须切开——跨洞合成一帧会让消费端按
    /// 帧头区间推导的负载与真实负载错位（转义序列接在半途）
    #[test]
    fn session_cache_contiguous_runs_splits_on_gap() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"abcde"); // [0,5)
        push_data(&mut cache, 5, b"fghij"); // [5,10) 连续 → 合并
        push_data(&mut cache, 13, b"klmno"); // 洞 [10,13) → 新段

        let runs = cache.contiguous_runs(0);
        assert_eq!(runs.len(), 2, "跨洞必须切分为两段: {runs:?}");
        assert_eq!(runs[0].0, 0);
        assert_eq!(runs[0].1, b"abcdefghij");
        assert_eq!(runs[1].0, 13);
        assert_eq!(runs[1].1, b"klmno");
    }

    /// 连续段切分：起点落在片段中间 → 首段从 from 起截取；from 越过 tail → 空
    #[test]
    fn session_cache_contiguous_runs_slices_from_offset() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"abcdefgh");

        let runs = cache.contiguous_runs(3);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0, 3);
        assert_eq!(runs[0].1, b"defgh");

        assert!(cache.contiguous_runs(8).is_empty());
        assert!(cache.contiguous_runs(999).is_empty());
    }

    /// 连续段切分：起点早于驻留头（被 LRU 淘汰）→ 从 head 起（消费端会按
    /// min_offset 判截断，不能把已淘汰区间当成可供给数据）
    #[test]
    fn session_cache_contiguous_runs_clamps_to_head() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 5;
        push_data(&mut cache, 0, b"abcde"); // [0,5)
        push_data(&mut cache, 5, b"fghij"); // 淘汰首块 → head=5
        assert_eq!(cache.head, 5);

        let runs = cache.contiguous_runs(0);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].0, 5, "起点必须钳到驻留头");
        assert_eq!(runs[0].1, b"fghij");
    }

    /// 段2 订阅态挂在管理器上（独立于链路）：链路未建立也记录订阅意愿，
    /// 同一会话始终返回同一份开关（链路重建后沿用）
    #[test]
    fn manager_page_subscription_is_session_scoped_and_persistent() {
        let manager = TerminalLinkManager {
            links: Mutex::new(HashMap::new()),
            consumers: Mutex::new(HashMap::new()),
        };
        // 初始未订阅（含未记录过的会话）
        assert!(!manager.is_page_subscribed("s1"));

        manager.page_subscribe("s1");
        assert!(manager.is_page_subscribed("s1"));
        let flag = manager.consumer_flag("s1");
        assert!(flag.load(Ordering::SeqCst));

        // 同一会话多次 get-or-create 返回同一 Arc（链路重建后沿用订阅态）
        assert!(Arc::ptr_eq(&flag, &manager.consumer_flag("s1")));

        // 会话隔离
        assert!(!manager.is_page_subscribed("s2"));

        manager.page_unsubscribe("s1");
        assert!(!manager.is_page_subscribed("s1"));
        assert!(
            !flag.load(Ordering::SeqCst),
            "取消订阅必须作用在同一份开关上（链路持有的 Arc 同步可见）"
        );

        // 会话删除：订阅态一并作废
        manager.page_subscribe("s1");
        manager.remove("s1");
        assert!(!manager.is_page_subscribed("s1"));
    }

    fn push_data(cache: &mut SessionCache, start: u64, data: &[u8]) {
        cache.push(start, data.to_vec());
    }

    #[test]
    fn session_cache_push_advances_tail_and_bounds() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 64;
        push_data(&mut cache, 0, b"abcdef");
        push_data(&mut cache, 6, b"ghij");
        assert_eq!(cache.head, 0);
        assert_eq!(cache.tail, 10);
        assert_eq!(cache.bytes, 10);
    }

    #[test]
    fn session_cache_snapshot_partial_slice_from_mid_entry() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 64;
        push_data(&mut cache, 0, b"abcdef");
        push_data(&mut cache, 6, b"ghij");
        // from=3 落在首块中段：半块 slice → "def" + 后续块 "ghij"
        let (head, tail, bytes, data) = cache.snapshot(3);
        assert_eq!((head, tail, bytes), (0, 10, 10));
        assert_eq!(data, b"defghij");
    }

    #[test]
    fn session_cache_lru_evicts_oldest_head() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 16;
        push_data(&mut cache, 0, b"aaaaaaaab"); // 9 字节
        push_data(&mut cache, 9, b"bbbbbbbbc"); // 9 字节 → 18 > 16 淘汰前块
        assert_eq!(cache.head, 9);
        assert_eq!(cache.bytes, 9);
        assert_eq!(cache.entries.len(), 1);
        // snapshot(0)：from 被提升到 head，min=head 供消费端判截断
        let (head, _, _, data) = cache.snapshot(0);
        assert_eq!(head, 9);
        assert_eq!(data, b"bbbbbbbbc");
    }

    #[test]
    fn session_cache_snapshot_from_beyond_tail_empty() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"abc");
        let (head, tail, _, data) = cache.snapshot(10);
        assert_eq!((head, tail), (0, 3));
        assert!(data.is_empty());
    }

    #[test]
    fn session_cache_single_huge_frame_evicts_itself() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 4;
        // b"toolargepayload" 共 15 字节，远超上限：整帧淘汰，queue 清空
        push_data(&mut cache, 0, b"toolargepayload");
        assert!(cache.entries.is_empty());
        assert_eq!(cache.head, 15);
        assert_eq!(cache.tail, 15);
    }

    // ==================== 字节洞登记（快照跨洞必须上报） ====================

    #[test]
    fn session_cache_records_gap_and_reports_it() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 1024;
        push_data(&mut cache, 0, b"abcde"); // [0,5)
        push_data(&mut cache, 8, b"fghij"); // 洞 [5,8)
        assert_eq!(cache.tail, 13);
        assert!(cache.has_gap(0), "跨洞区间必须上报（否则静默错位渲染）");
        assert!(cache.has_gap(5), "起点落在洞首仍跨洞");
    }

    #[test]
    fn session_cache_reports_no_gap_when_contiguous() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"abcde");
        push_data(&mut cache, 5, b"fghij"); // 连续：不登记洞
        assert!(!cache.has_gap(0));
        assert!(!cache.has_gap(5));
    }

    #[test]
    fn session_cache_first_frame_offset_is_not_a_gap() {
        let mut cache = SessionCache::new();
        // 首帧偏移非 0（订阅快照起点）：无前序帧，不构成洞
        push_data(&mut cache, 100, b"abc");
        assert!(!cache.has_gap(0));
        assert!(!cache.has_gap(100));
    }

    #[test]
    fn session_cache_gap_after_from_is_not_reported() {
        let mut cache = SessionCache::new();
        push_data(&mut cache, 0, b"abcde");
        push_data(&mut cache, 8, b"fghij");
        // 起点在洞尾之后（[9,13) 连续）：不上报
        assert!(!cache.has_gap(9));
        // 起点在洞首之前：区间仍跨洞
        assert!(cache.has_gap(4));
    }

    #[test]
    fn session_cache_drops_gap_when_fully_evicted() {
        let mut cache = SessionCache::new();
        cache.max_bytes = 6;
        push_data(&mut cache, 0, b"abcde"); // [0,5)
        push_data(&mut cache, 8, b"fghij"); // 洞 [5,8)；淘汰首块 → head=5
        assert_eq!(cache.head, 5);
        assert!(cache.has_gap(0), "洞 [5,8) 仍在驻留区间内");
        // 新帧把洞整体挤出驻留区：head 推进到 13，洞不再可能被快照命中
        push_data(&mut cache, 13, b"klmno");
        assert_eq!(cache.head, 13);
        assert!(!cache.has_gap(0), "洞已整体淘汰，不再上报");
    }
}
