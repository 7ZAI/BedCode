//! Terminal Link — 会话级终端 WebSocket（Rust 后端持有）
//!
//! 用户需求 1/2/3 的移动端落点：
//! 1. 订阅由本模块（Rust）管理、前端触发：会话启动 → `terminal_subscribe`；
//!    停止/手动断开 → `terminal_unsubscribe`；意外断开 → 模块内自动退避重连 +
//!    按字节游标（from_offset）重订阅；手动断开 → 关连接不再重连。
//! 2. 终端数据真源 = 本模块的会话级字节缓存：WS 收帧 → 缓存 → 事件转发；
//!    渲染 backpressure ack 由本模块持字节水位回发（`terminal_ack_rendered`
//!    提升水位）；输入经 `terminal_send_input` → WS input 帧 → 桌面端 PTY。
//! 3. 字节连续（TB v3）：帧按 `[start_offset, end_offset)` 区间缓存与校验；
//!    历史 = 缓存区间（[head, snapshot)），一次性位移数据经
//!    `terminal_get_history` 提供（缓存优先，缓存头被淘汰时回退桌面 HTTP
//!    `GET /api/sessions/{id}/history` 一次性拉取）；实时帧（start ≥ snapshot）
//!    经 `terminal-frame` 事件推送——前端「拼完历史才消费实时」（契约见
//!    useTerminalBuffer 改造）。
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
const ACK_BYTES_THRESHOLD: u64 = 64 * 1024;
/// ack 空闲兜底：距上次回发超此时长仍推进则强制回发
const ACK_MAX_IDLE_MS: u64 = 250;

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
        }
    }

    /// 收帧入缓存（区间连续；防御性容忍间隙——重订阅后不一致由 snapshot 元数据纠正）。
    /// 返回本次 LRU 淘汰的最旧字节数（对账口径：淘汰字节无法再经缓存供给前端）
    fn push(&mut self, start: u64, data: Vec<u8>) -> u64 {
        let end = start + data.len() as u64;
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
        self.tail = end;
        evicted
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
    /// 会话不存在连续重试计数
    session_missing: AtomicU32,
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
    fn new(session_id: String, app: AppHandle, write_tx: mpsc::Sender<Outbound>) -> Arc<Self> {
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
            session_missing: AtomicU32::new(0),
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

    /// 缓存收帧 + 推进游标 + （live 段）事件推送 + ack 记账
    fn ingest_frame(&self, frame: ParsedFrame) {
        let frame_bytes = frame.end - frame.start;
        self.frames_received.fetch_add(1, Ordering::SeqCst);
        self.bytes_received.fetch_add(frame_bytes, Ordering::SeqCst);
        let live_snapshot = self.live_snapshot.load(Ordering::SeqCst);
        if frame.end > live_snapshot {
            self.frames_live_emitted.fetch_add(1, Ordering::SeqCst);
            self.bytes_live_emitted.fetch_add(frame_bytes, Ordering::SeqCst);
            // 实时帧：事件推送（历史段 [head, snapshot) 只入缓存不推送——前端
            // 经 terminal_get_history 一次性取；重连 WS 重播段同理静默入缓存）
            if let Err(e) = self.app.emit(
                EVENT_TERMINAL_FRAME,
                serde_json::json!({
                    "session_id": self.session_id,
                    "start_offset": frame.start,
                    "end_offset": frame.end,
                    "data_base64": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &frame.data,
                    ),
                }),
            ) {
                tracing::warn!(
                    session_id = %self.session_id,
                    start_offset = frame.start,
                    end_offset = frame.end,
                    error = %e,
                    "terminal frame event emit failed"
                );
            }
        } else {
            // 历史段：对账口径——这部分字节经 terminal_get_history 供给前端
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
        // 缓存游标推进即视为已消费（背压锚点到 Rust 缓存），随时可回发 ack
        self.acked.store(self.cursor.load(Ordering::SeqCst), Ordering::SeqCst);
        self.pending_ack_bytes.fetch_add(frame_bytes, Ordering::SeqCst);
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
            "terminal link ingest stats (periodic)"
        );
    }

    /// 查询历史（缓存优先）：`(min, snapshot, history_bytes, data_base64)`
    fn cached_history(&self, from: u64) -> Option<(u64, u64, u64, String)> {
        let cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        if cache.is_empty() {
            return None;
        }
        let (head, tail, bytes, data) = cache.snapshot(from);
        Some((
            head,
            tail,
            bytes,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data),
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
}

static MANAGER: OnceLock<Arc<TerminalLinkManager>> = OnceLock::new();

pub fn terminal_link_manager() -> Arc<TerminalLinkManager> {
    MANAGER
        .get_or_init(|| Arc::new(TerminalLinkManager { links: Mutex::new(HashMap::new()) }))
        .clone()
}

impl TerminalLinkManager {
    /// 订阅会话（会话启动时触发；幂等——已存在且未停止则忽略）
    ///
    /// 订阅 = 建立每会话 WS 连接（auth → subscribe），流与重连全由 IO 任务管理；
    /// 意外断开自动重连重订阅（保留游标），手动取消经 `unsubscribe` 关连接
    pub fn subscribe(&self, app: AppHandle, session_id: String) {
        if session_id.is_empty() {
            return;
        }
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = links.get(&session_id) {
            if !existing.stopped.load(Ordering::SeqCst) {
                return; // 已在运行
            }
        }
        let (write_tx, write_rx) = mpsc::channel::<Outbound>(256);
        let link = TerminalLink::new(session_id.clone(), app, write_tx);
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

    /// 会话删除：清链路 + 缓存
    pub fn remove(&self, session_id: &str) {
        self.unsubscribe(session_id);
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        links.remove(session_id);
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
        if pending >= ACK_BYTES_THRESHOLD || (last != 0 && now.saturating_sub(last) >= ACK_MAX_IDLE_MS) {
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
            // 未达阈值也未到空闲兜底：下次收帧时再判（节流 pending 保留）
            link.pending_ack_bytes.fetch_add(pending, Ordering::SeqCst);
        }
    }

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
                        // 前端渲染游标提升水位（可能高于缓存游标——渲染先行时）
                        let cur = link.acked.load(Ordering::SeqCst);
                        if offset > cur {
                            link.acked.store(offset, Ordering::SeqCst);
                        }
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
        Some("auth_ok") => {
            // 认证成功：发出订阅（from_offset = 已接收游标，重连续传不重发已缓存区）
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

/// 订阅会话（会话启动 / 恢复运行时触发；幂等）
///
/// 订阅由 Rust 管理：连接、认证、订阅、缓存、意外断开自动重连重订阅
///（保留字节游标）；前端只负责在正确的时机触发/取消
#[tauri::command]
pub async fn terminal_subscribe(app: tauri::AppHandle, session_id: String) -> Result<(), String> {
    let manager = terminal_link_manager();
    manager.subscribe(app, session_id);
    Ok(())
}

/// 取消订阅（会话停止 / 手动断开）：关连接不再重连；缓存保留供重开恢复
#[tauri::command]
pub async fn terminal_unsubscribe(session_id: String) -> Result<(), String> {
    terminal_link_manager().unsubscribe(&session_id);
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
    if let Some((min, snapshot, history_bytes, data_b64)) = link.cached_history(from) {
        // 链路调试（字节对账）：缓存命中路径——payload_bytes 为 base64 长度
        //（略大于原始字节），与 dataBase64 一同供前端拼接对账
        tracing::debug!(
            session_id = %session_id,
            from_offset = from,
            min_offset = min,
            snapshot_offset = snapshot,
            history_bytes,
            payload_b64_bytes = data_b64.len(),
            "terminal history served from rust cache"
        );
        return Ok(serde_json::json!({
            "from": from,
            "minOffset": min,
            "snapshotOffset": snapshot,
            "historyBytes": history_bytes,
            "dataBase64": data_b64,
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
}
