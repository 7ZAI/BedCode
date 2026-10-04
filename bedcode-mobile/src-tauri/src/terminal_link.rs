//! Terminal Link — 会话级终端流（Rust 后端持有，插件端点新协议）
//!
//! ## 协议（桌面插件 `ws_terminal.rs` 为事实源，spec §3.3）
//!
//! 客户端 → 插件（text JSON）：
//! ```json
//! {"type":"auth","token":"<jwt>"}                       // 握手（不变）
//! {"type":"subscribe","sessionId":"<id>","mode":"live"} // 订阅：fresh subscribe 即回放环窗口
//! {"type":"ack","offset":<本地已渲染字节数>}              // 流控信号（阈值/空闲兜底节流）
//! {"type":"input","data":"<UTF-8 文本，无控制字符>"}      // 可打印输入
//! {"type":"poll"}                                       // 追加拉取（批量态客户端驱动）
//! ```
//! 客户端 → 插件（binary）：原始输入字节（控制字符/特殊键，`KeyCombo::to_pty_bytes`）。
//! 插件 → 客户端（binary）：**输出裸字节**（无 16B 帧头、无 per-frame offset）。
//! 插件 → 客户端（text JSON）：
//! ```json
//! {"type":"subscribed","sessionId":"...","mode":"..."}
//! {"type":"ring_resync","offset":N}   // 环淘汰：N 之前数据不可恢复 → 清屏 + 基准重置
//! {"type":"session_stopped","sessionId":"...","reason":"...","exitCode":N?}
//! {"type":"error","message":"..."}
//! ```
//!
//! ## 生命周期
//!
//! - **订阅**（进入终端页 / 会话页预加载）：建连 + 认证 + 订阅。fresh subscribe
//!   后插件**回放环窗口**（历史与实时同一条流），无 `history_end` 边界——
//!   收 `subscribed` 即进入 live（前端门控信号，8s 超时兜底在前端）。
//! - **退订**（离开终端页 / 会话停止 / 手动断开）：**关闭连接**（实现择一，契约
//!   测试锁定「close」；插件 `client-disconnect` 清理订阅态）——不得后台常拉。
//! - **重连**（意外断开）：指数退避保留；重连后**重新订阅**（无续传语义；环淘汰
//!   由 `ring_resync` 如实告知）。
//! - **重订阅**（已 live 时再次 subscribe / 重连恢复 / 停止后重建同 id 会话）：
//!   收 `subscribed` 后发 `terminal-resync` 事件 → 前端清屏 + 本地计数基准重置
//!   （重播与已在屏内容不重叠）。
//!
//! ## 本地计数与重锚
//!
//! - 输出字节本地累计（`cursor`），**仅用于 ack 水位回发**（`{"type":"ack","offset"}`
//!   的 offset = 前端已渲染字节数，经 `terminal_ack_rendered` 推进），不再作为
//!   绝对偏移发给桌面。桌面插件侧 ack 只作 drain 触发、忽略 offset 值。
//! - `ring_resync` 是**唯一**重锚信号：清屏 + 基准重置（本端无字节缓存——
//!   历史由订阅回放提供，环淘汰直接告知）。
//! - `subscribed` 之前的二进制帧一律丢弃（fresh subscribe 前的旧流残留；重播会
//!   覆盖全部内容，转发只会造成前端重复渲染）。
//! - 慢客户端：插件发送失败只停本人（游标不前进，下轮续拉）；本端不假设
//!   「未收到即丢失」——页面未订阅期间的输出由桌面环窗口保留，重订阅回放补齐。
//!
//! ## 输入
//!
//! - 可打印文本（输入栏命令等）：`{"type":"input","data":"<UTF-8>"}` text 帧。
//! - 控制字符 / 特殊键（Enter/Tab/Esc/Del/Ctrl+C 等）：`KeyCombo::to_pty_bytes()`
//!   （`enums/special_key.rs`）→ **binary 帧**原始字节，直写 PTY。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::Message as WsMsg;

use crate::connection::heartbeat::{HeartbeatConfig, HeartbeatManager};
use crate::connection::reconnect::{ReconnectConfig, ReconnectManager};
use crate::enums::special_key::KeyCombo;
use crate::system::constants::reconnect::{DEFAULT_INITIAL_DELAY_MS, DEFAULT_MAX_DELAY_MS};
use crate::system::error_boundary::spawn_with_error_boundary;

// ==================== 终端链路事件出口（trait；票 07 集成测试注入 mock） ====================

/// 链路 → 前端事件发射出口抽象
///
/// 生产实现包 `tauri::AppHandle`（`Emitter::emit`）；`TerminalLink` 经此 trait
/// 发射 `terminal-state` / `terminal-resync` 事件。集成测试无法构造真实
/// `AppHandle`（`tauri::test::mock_app` 是 `MockRuntime`，与终端命令的 Wry
/// 泛型不匹配——票 05 Comments 已计划此行），故把发射面收窄为单方法 trait，
/// 测试注入记录替身驱动协议闭环。
pub trait TerminalEventSink: Send + Sync + 'static {
    /// 发射前端事件（event 名 + 序列化载荷；调用方决定失败留痕）
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
}

impl TerminalEventSink for AppHandle {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }
}

// ==================== 常量 ====================

/// 链路活性探测：Ping 周期（秒）与静默判死阈值（秒）
///
/// 与事件通道 `HeartbeatManager` 的默认值同档（30s / 90s）。静默阈值取 3×
/// Ping 周期：静默 shell 不产生出站帧，判活只能靠入站活动，阈值必须宽到能
/// 容下「终端长时间无人操作」而不误杀。
///
/// **重连退避已收敛到 `connection/reconnect.rs` 的 `ReconnectManager`**
/// （2026-10-04 替换）。本文件此前自建 `RECONNECT_BASE_MS = 500` /
/// `RECONNECT_MAX_MS = 8000` 的手写退避，两处问题：① `500` **低于**全局下限
/// `MIN_RECONNECT_DELAY_MS = 1000`（2026-09-29 那次 616 次/98 秒自愈风暴正是
/// 「无退避下限」形态）；② 无 jitter，多会话同步重连构成惊群。现由 `link_io`
/// 驱动 `ReconnectManager`，与设备级事件通道共用同一份指数退避 + 抖动 + 下限钳制。
///
/// **节奏变化**（收敛的已知代价）：500→1000→…→8000 封顶 变为
/// 1000→2000→…→30000 封顶。首轮更保守、封顶更长。
const TERMINAL_HEARTBEAT_INTERVAL_SECS: u64 = 30;
const TERMINAL_HEARTBEAT_TIMEOUT_SECS: u64 = 90;
/// 判活轮询周期（毫秒）——只查时间戳，不发帧，代价可忽略
const TERMINAL_HEARTBEAT_PROBE_TICK_MS: u64 = 5_000;

/// 会话不存在（启动中/已停止）连续重试上限，超出后停止等待外部恢复
const MAX_SESSION_MISSING_STRIKES: u32 = 3;

/// 流控 ack 回发节流：累计待 ack 字节达阈值即回发（桌面插件 ack 触发 drain）
const ACK_BYTES_THRESHOLD: u64 = 64 * 1024;
/// ack 空闲兜底：距上次回发超此时长仍推进则强制回发
const ACK_MAX_IDLE_MS: u64 = 250;
/// ack 空闲兜底轮询间隔：连接循环用该周期调用 should_send_ack，使「无新帧到达」
/// 时积压的 ack 仍能被回发。必须 ≤ ACK_MAX_IDLE_MS（见 should_send_ack 注释）
const ACK_IDLE_TICK_MS: u64 = ACK_MAX_IDLE_MS / 2;

/// 收帧统计打点间隔（有新数据才打；不打逐帧日志，防输出风暴期日志淹没链路）
const INGEST_STATS_INTERVAL_MS: u64 = 5000;

// ==================== 事件名（前端 listen） ====================

/// 状态变更（phase / reconnect / stopped / session_missing / retry / resync）
pub(crate) const EVENT_TERMINAL_STATE: &str = "terminal-state";
/// 重锚（`ring_resync` 或重订阅回包）：前端据此清屏 + 本地计数归零 + 一次性提示
pub(crate) const EVENT_TERMINAL_RESYNC: &str = "terminal-resync";

// ==================== 状态定义 ====================

/// 连接/订阅阶段（新协议：无独立 history 阶段——收 `subscribed` 即 live）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPhase {
    Idle,
    Connecting,
    Auth,
    Live,
}

impl LinkPhase {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => LinkPhase::Connecting,
            2 => LinkPhase::Auth,
            3 => LinkPhase::Live,
            _ => LinkPhase::Idle,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            LinkPhase::Idle => 0,
            LinkPhase::Connecting => 1,
            LinkPhase::Auth => 2,
            LinkPhase::Live => 3,
        }
    }

    fn as_api_str(self) -> &'static str {
        match self {
            LinkPhase::Idle => "idle",
            LinkPhase::Connecting => "connecting",
            LinkPhase::Auth => "auth",
            LinkPhase::Live => "live",
        }
    }
}

/// 订阅模式（subscribe 帧的 `mode` 字段；移动端当前只使用 live——页面进出
/// 关闭/重建连接，批量态 poll 由协议保留）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMode {
    Live,
    Poll,
}

impl LinkMode {
    fn as_str(self) -> &'static str {
        match self {
            LinkMode::Live => "live",
            LinkMode::Poll => "poll",
        }
    }
}

/// 连接终止原因（区分意外断开与否，决定重连策略）
enum LinkExit {
    /// 意外断开 / 连接失败：退避重连
    Io,
    /// 会话不存在（启动中/已停止）超限：停止等待外部恢复
    SessionMissing,
}

/// 服务端 error 帧分类（`{"type":"error","message":...}`）
enum ServerErrorClass {
    /// 会话不存在（启动竞态 / 已停止）：退避重试，有限次数后停止
    SessionMissing,
    /// 其它错误：留痕 + 状态事件，连接保持
    Other,
}

/// 出站帧（IO 任务与命令侧交互通道）
#[derive(Debug)]
enum Outbound {
    /// fresh subscribe（命令侧重订阅 / 连接建立后首次订阅共用）
    Subscribe,
    /// 优雅关闭（不再重连）
    Close,
    /// 可打印文本输入 → `{"type":"input","data":"<UTF-8>"}` text 帧
    TextInput { data: String },
    /// 控制字符/特殊键输入 → binary 帧原始字节（KeyCombo::to_pty_bytes）
    BinaryInput { bytes: Vec<u8> },
    /// 前端渲染水位推进 → ack 帧回发（节流见 should_send_ack）
    Ack { offset: u64 },
}

// ==================== 帧构造（纯函数，可单测） ====================

/// 订阅帧 `{"type":"subscribe","sessionId":"<id>","mode":"live"|"poll"}`
fn build_subscribe_frame(session_id: &str, mode: LinkMode) -> String {
    serde_json::json!({
        "type": "subscribe",
        "sessionId": session_id,
        "mode": mode.as_str(),
    })
    .to_string()
}

/// 流控 ack 帧 `{"type":"ack","offset":N}`（offset = 本地已渲染字节数）
fn build_ack_frame(offset: u64) -> String {
    serde_json::json!({ "type": "ack", "offset": offset }).to_string()
}

/// 追加拉取帧 `{"type":"poll"}`（批量态客户端驱动）。
///
/// 协议保留能力：移动端当前生命周期策略（进入订阅 / 离开关闭）不使用批量态，
/// 无生产调用——帧形状由单测锁定，供未来按需接线
#[allow(dead_code)]
fn build_poll_frame() -> String {
    r#"{"type":"poll"}"#.to_string()
}

/// 可打印文本输入帧 `{"type":"input","data":"<UTF-8 文本>"}`
fn build_input_text_frame(data: &str) -> String {
    serde_json::json!({ "type": "input", "data": data }).to_string()
}

/// 特殊键 → PTY 原始字节（`KeyCombo::parse` + `to_pty_bytes`）
fn special_key_to_pty_bytes(name: &str) -> Option<Vec<u8>> {
    let combo = KeyCombo::parse(name)?;
    combo.to_pty_bytes()
}

/// 输入投递计划：把「文本 + 特殊键」这一对入参展开成**有序**的出站帧序列。
///
/// 纯函数（可单测），语义即本命令的对外契约（spec §3.3「两者可并存，帧序即写入序」）：
/// - 有文本 → 追加 `TextInput`；文本在前
/// - 有特殊键 → 追加 `BinaryInput`；键字节在后（「先打字，再回车」）
/// - 两者皆空 → 空计划（无帧投递）
///
/// **历史缺陷（2026-10-02 修复）**：实现曾写成 `if 有键 … else if 有文本` 的互斥分支，
/// 于是输入栏「命令 + Enter」这条唯一生产路径（前端恒传 `specialKey: "enter"`）只发得出
/// 一个裸回车，命令文本被静默丢弃——真机表现是「输入没反应」，且前端无任何报错可查。
/// 契约与实现不一致的判据：函数自身的文档注释写着「两者可并存」。
///
/// 特殊键**先校验后投递**：不支持的键名返回 Err 且一帧都不发，避免「文本已写进 PTY、
/// 回车没发」的半截输入（PTY 侧不可回滚）。
fn plan_input_frames(data: &str, special_key: Option<&str>) -> Result<Vec<Outbound>, String> {
    let key_bytes = match special_key.filter(|k| !k.is_empty()) {
        Some(key) => Some(special_key_to_pty_bytes(key).ok_or_else(|| format!("unsupported special key: {key}"))?),
        None => None,
    };
    let mut frames = Vec::with_capacity(2);
    if !data.is_empty() {
        frames.push(Outbound::TextInput { data: data.to_string() });
    }
    if let Some(bytes) = key_bytes {
        frames.push(Outbound::BinaryInput { bytes });
    }
    Ok(frames)
}

/// ack 回发节流判定（64KB 阈值 + 250ms 空闲兜底）。
///
/// `pending == 0` 一律不发：pending 在收帧时累计（见 `ingest_output`），为 0 即
/// 表示自上次回发后没有新收到的字节，重发不推进插件侧记账。空闲规则必须在
/// 连接循环的空闲定时器周期求值：末批不足 64KB 且距上次回发 <250ms 时保留
/// 待发，若此后上游暂停（不再有帧到达），pending 永远等不到下一次触发。
/// 定时器周期调用必须显式排除 pending=0，否则每 250ms 回发一次空 ack。
fn should_send_ack(pending: u64, last_ack_at: u64, now: u64) -> bool {
    if pending == 0 {
        return false;
    }
    pending >= ACK_BYTES_THRESHOLD || (last_ack_at != 0 && now.saturating_sub(last_ack_at) >= ACK_MAX_IDLE_MS)
}

/// 服务端 error 帧分类（纯函数）：`会话不存在`（启动竞态）→ SessionMissing；
/// 其余（含插件内错误/宿主错误）→ Other
fn classify_server_error(message: &str) -> ServerErrorClass {
    if message.contains("会话不存在") {
        ServerErrorClass::SessionMissing
    } else {
        ServerErrorClass::Other
    }
}

// ==================== Terminal Link（每会话） ====================

/// 会话级终端链路（每会话一个实例；管理器持有）
pub struct TerminalLink {
    session_id: String,
    app: Arc<dyn TerminalEventSink>,
    /// 出站通道（命令侧 → IO 任务）
    write_tx: mpsc::Sender<Outbound>,
    /// IO 任务句柄（manual stop 时 abort）
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// 阶段（LinkPhase）
    phase: AtomicU8,
    /// 本链路是否已停止（不再重连；重建由 subscribe 负责）
    stopped: AtomicBool,
    /// 已收到 `subscribed`：门控——此前到达的二进制帧一律丢弃（fresh subscribe
    /// 前的旧流残留，重播会覆盖全部内容）
    subscribe_ack: AtomicBool,
    /// 本地已收字节（统计 + ack 记账；subscribed/ring_resync 后归零）
    cursor: AtomicU64,
    /// 前端已渲染字节（`terminal_ack_rendered` 推进；ack 帧 offset 值）
    frontend_rendered: AtomicU64,
    /// 累计待 ack 字节（节流）
    pending_ack_bytes: AtomicU64,
    last_ack_at: AtomicU64,
    /// 会话不存在连续重试计数
    session_missing: AtomicU32,
    /// 本次 `subscribed` 回包后需发 `terminal-resync`（重订阅/重连/停止后重建）：
    /// 前端可能已有在屏内容，重播前必须清屏 + 本地计数基准重置
    pending_resync: AtomicBool,
    /// 段2 订阅态（前端是否正在消费本链路输出）——与管理器共享同一 `Arc`：
    /// 链路重建时沿用同一份，页面存活期间的终端不会因链路重建而断流
    frontend_subscribed: Arc<AtomicBool>,
    /// 段2 推送通道槽（页面级 Tauri Channel，输出帧唯一出口）——与管理器共享
    /// 同一 `Arc`；`None` = 页面未订阅/已卸载（输出丢弃，重进经重订阅回放补齐）；
    /// 发送失败即「消费端已离去」，就地清空避免后续每次补投都失败刷日志
    seg2_channel: Arc<Mutex<Option<Channel<InvokeResponseBody>>>>,
    // ==================== 链路调试统计（终端字节对账） ====================
    frames_received: AtomicU64,
    bytes_received: AtomicU64,
    frames_forwarded: AtomicU64,
    bytes_forwarded: AtomicU64,
    frames_dropped: AtomicU64,
    bytes_dropped: AtomicU64,
    last_stats_ms: AtomicU64,
    last_stats_cursor: AtomicU64,
}

impl TerminalLink {
    fn new(
        session_id: String,
        app: Arc<dyn TerminalEventSink>,
        write_tx: mpsc::Sender<Outbound>,
        frontend_subscribed: Arc<AtomicBool>,
        seg2_channel: Arc<Mutex<Option<Channel<InvokeResponseBody>>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            app,
            write_tx,
            handle: Mutex::new(None),
            phase: AtomicU8::new(LinkPhase::Idle.as_u8()),
            stopped: AtomicBool::new(true),
            subscribe_ack: AtomicBool::new(false),
            cursor: AtomicU64::new(0),
            frontend_rendered: AtomicU64::new(0),
            pending_ack_bytes: AtomicU64::new(0),
            last_ack_at: AtomicU64::new(0),
            session_missing: AtomicU32::new(0),
            pending_resync: AtomicBool::new(false),
            frontend_subscribed,
            seg2_channel,
            frames_received: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            frames_forwarded: AtomicU64::new(0),
            bytes_forwarded: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            bytes_dropped: AtomicU64::new(0),
            last_stats_ms: AtomicU64::new(0),
            last_stats_cursor: AtomicU64::new(0),
        })
    }

    fn emit_state(&self, detail: &str) {
        // 前端状态机唯一事件源：emit 失败必须留痕，否则 UI 静默停在旧状态无任何线索
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_STATE,
            serde_json::json!({
                "session_id": self.session_id,
                "phase": LinkPhase::from_u8(self.phase.load(Ordering::SeqCst)).as_api_str(),
                "cursor": self.cursor.load(Ordering::SeqCst),
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

    /// 重连退避排期事件：携带下一次重连的等待毫秒数
    ///
    /// 与 `emit_state("reconnecting")` 配对：前者报「已进入退避」，后者报
    /// 「等多久再试」。前端据此显示倒计时而非干等（退避封顶 30s，无提示时
    /// 用户无法区分「在重连」与「已死」）。字段为增量，老端忽略即可。
    fn emit_reconnect_scheduled(&self, delay: std::time::Duration) {
        let retry_in_ms = delay.as_millis().min(u128::from(u64::MAX)) as u64;
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_STATE,
            serde_json::json!({
                "session_id": self.session_id,
                "phase": LinkPhase::from_u8(self.phase.load(Ordering::SeqCst)).as_api_str(),
                "cursor": self.cursor.load(Ordering::SeqCst),
                "detail": "reconnect_scheduled",
                "retry_in_ms": retry_in_ms,
            }),
        ) {
            tracing::warn!(
                session_id = %self.session_id,
                retry_in_ms,
                error = %e,
                "terminal reconnect_scheduled event emit failed"
            );
        }
    }

    /// 重锚事件（`ring_resync` 或重订阅回包）：前端清屏 + 本地计数归零 +
    /// 一次性提示。`offset` 为插件环偏移（诊断用；ring_resync 时有效），
    /// 本地计数基准一律归零（本地计数仅用于 ack 水位，与插件绝对偏移解耦）
    fn emit_resync(&self, offset: u64) {
        tracing::warn!(
            session_id = %self.session_id,
            ring_offset = offset,
            "terminal resync: clear screen and re-anchor local byte counter"
        );
        if let Err(e) = self.app.emit(
            EVENT_TERMINAL_RESYNC,
            serde_json::json!({
                "session_id": self.session_id,
                "offset": offset,
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

    /// 推送一段输出字节到前端（页面级 Channel，**裸字节**——无 TB v3 帧头）。
    /// 通道缺失或发送失败（页面已卸载 / WebView 侧已释放）时留痕并就地清空
    /// 引用——字节丢弃（无缓存；重进页面经重订阅回放补齐）
    fn forward_output(&self, data: &[u8]) {
        self.frames_forwarded.fetch_add(1, Ordering::SeqCst);
        self.bytes_forwarded.fetch_add(data.len() as u64, Ordering::SeqCst);
        let mut slot = self.seg2_channel.lock().unwrap_or_else(|p| p.into_inner());
        let Some(channel) = slot.as_ref() else {
            return;
        };
        if let Err(e) = channel.send(InvokeResponseBody::Raw(data.to_vec())) {
            tracing::warn!(
                session_id = %self.session_id,
                bytes = data.len(),
                error = %e,
                "terminal output channel send failed, detaching consumer channel"
            );
            *slot = None;
        }
    }

    /// 收输出字节：计数 + ack 记账 + （门控通过时）推前端。
    ///
    /// 门控 = `subscribe_ack`（已收 subscribed）**且** 段2 已订阅（页面在前台）。
    /// 未通过门控的字节丢弃并计数：subscribed 前是 fresh subscribe 前的旧流
    /// 残留（重播覆盖）；页面未订阅期间的输出由桌面环窗口保留（重订阅回放补齐）
    fn ingest_output(&self, data: &[u8]) {
        let len = data.len() as u64;
        self.frames_received.fetch_add(1, Ordering::SeqCst);
        self.bytes_received.fetch_add(len, Ordering::SeqCst);
        if self.subscribe_ack.load(Ordering::SeqCst) && self.frontend_subscribed.load(Ordering::SeqCst) {
            self.cursor.fetch_add(len, Ordering::SeqCst);
            self.pending_ack_bytes.fetch_add(len, Ordering::SeqCst);
            self.forward_output(data);
        } else {
            self.frames_dropped.fetch_add(1, Ordering::SeqCst);
            self.bytes_dropped.fetch_add(len, Ordering::SeqCst);
        }
    }

    /// 收帧统计周期打点（5s 且游标有推进才打）：与桌面插件产出对账，
    /// 比对 forwarded/dropped 可定位本端丢字节环节
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
        tracing::debug!(
            session_id = %self.session_id,
            cursor,
            frames_received = self.frames_received.load(Ordering::SeqCst),
            bytes_received = self.bytes_received.load(Ordering::SeqCst),
            frames_forwarded = self.frames_forwarded.load(Ordering::SeqCst),
            bytes_forwarded = self.bytes_forwarded.load(Ordering::SeqCst),
            frames_dropped = self.frames_dropped.load(Ordering::SeqCst),
            bytes_dropped = self.bytes_dropped.load(Ordering::SeqCst),
            frontend_rendered = self.frontend_rendered.load(Ordering::SeqCst),
            pending_ack_bytes = self.pending_ack_bytes.load(Ordering::SeqCst),
            "terminal link ingest stats (periodic)"
        );
    }

    /// 发送出站帧（命令侧调用）。通道关闭 = IO 任务已退出（链接死亡），此时
    /// 命令随之丢弃必须留痕——重连恢复由状态机/重新订阅负责，不在此处重试
    ///
    /// **失败必须上抛**：输入面多帧化（文本帧 + 特殊键帧）后，调用方需要知道
    /// 帧没投递出去；单帧时代「丢弃 + 留痕」尚可接受，多帧时代静默丢弃会让
    /// 「命令文本已写入 PTY、回车没发」被当成成功返回（PTY 侧不可回滚）。
    async fn send_out(&self, out: Outbound) -> Result<(), String> {
        self.write_tx.send(out).await.map_err(|e| {
            tracing::warn!(
                session_id = %self.session_id,
                out = ?e.0,
                "terminal link io task exited, outbound command dropped"
            );
            format!("terminal link closed (session {session_id})", session_id = self.session_id)
        })
    }
}

// ==================== Manager（单例） ====================

/// 终端链路管理器：session_id → TerminalLink
pub struct TerminalLinkManager {
    links: Mutex<HashMap<String, Arc<TerminalLink>>>,
    /// 段2（前端 ↔ Rust）订阅态：session_id → 共享开关。
    ///
    /// 为什么独立于 `links` 存放：段2 的生命周期是「终端页进出」，与链路的
    /// 「会话存续」正交——链路会因会话停止被停掉并在恢复时新建实例，而页面
    /// 可能一直开着。订阅态挂在管理器上、以 `Arc` 共享给链路实例，链路重建后
    /// 沿用同一份开关，页面存活期间不会静默断流。
    consumers: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// 段2 推送通道槽（页面级）：与 `consumers` 同款共享方式，`page_subscribe`
    /// 写入、`page_unsubscribe`/会话删除清空
    page_channels: Mutex<HashMap<String, Arc<Mutex<Option<Channel<InvokeResponseBody>>>>>>,
}

static MANAGER: OnceLock<Arc<TerminalLinkManager>> = OnceLock::new();

pub fn terminal_link_manager() -> Arc<TerminalLinkManager> {
    MANAGER
        .get_or_init(|| {
            Arc::new(TerminalLinkManager {
                links: Mutex::new(HashMap::new()),
                consumers: Mutex::new(HashMap::new()),
                page_channels: Mutex::new(HashMap::new()),
            })
        })
        .clone()
}

impl TerminalLinkManager {
    /// 取会话的段2 订阅开关（不存在则创建，初值 false）
    fn consumer_flag(&self, session_id: &str) -> Arc<AtomicBool> {
        let mut consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        consumers
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    /// 取会话的段2 推送通道槽（不存在则创建，初值 `None`）
    fn consumer_channel(&self, session_id: &str) -> Arc<Mutex<Option<Channel<InvokeResponseBody>>>> {
        let mut channels = self.page_channels.lock().unwrap_or_else(|p| p.into_inner());
        channels
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// 段2 订阅（进入终端页）：登记前端推送通道 + 开启输出推送。
    /// 幂等；链路尚未建立也生效（通道与订阅意愿先于链路记录）。
    pub fn page_subscribe(&self, session_id: &str, channel: Channel<InvokeResponseBody>) {
        if session_id.is_empty() {
            return;
        }
        *self
            .consumer_channel(session_id)
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(channel);
        let was = self.consumer_flag(session_id).swap(true, Ordering::SeqCst);
        if !was {
            tracing::debug!(session_id = %session_id, "terminal page subscribe (frontend consumer attached)");
        }
    }

    /// 段2 取消订阅（退出终端页）：清空推送通道 + 停止推送。
    /// 输出在页面关闭期间不缓存（环窗口由重订阅回放补齐）
    pub fn page_unsubscribe(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        *self
            .consumer_channel(session_id)
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        let was = self.consumer_flag(session_id).swap(false, Ordering::SeqCst);
        if was {
            tracing::debug!(session_id = %session_id, "terminal page unsubscribe (frontend consumer detached)");
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

    /// 订阅会话（进入终端页 / 预加载时触发；**fresh subscribe 语义**）。
    ///
    /// - 链路未建立（首订）：新建链路 → 连接 → 认证 → 订阅（回放环窗口）。
    /// - 链路已在运行：发送 fresh subscribe 帧（插件游标归零重播）——若此前
    ///   已进入 live（重订阅 / 页面预加载后进入），`subscribed` 回包后发
    ///   `terminal-resync` 让前端清屏 + 基准重置，防重播与已在屏内容重叠。
    /// - 链路已停止（会话停止/手动关闭）：重建链路，同样标记重锚。
    pub fn subscribe(&self, app: Arc<dyn TerminalEventSink>, session_id: String) {
        if session_id.is_empty() {
            return;
        }
        // 段2 开关与通道先于 `links` 锁取：锁序与 page_subscribe 一致，避免
        // links → consumers 嵌套加锁死锁
        let frontend_subscribed = self.consumer_flag(&session_id);
        let seg2_channel = self.consumer_channel(&session_id);
        let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = links.get(&session_id) {
            if !existing.stopped.load(Ordering::SeqCst) {
                // 链路已在运行：fresh subscribe（重播环窗口）
                if existing.phase.load(Ordering::SeqCst) == LinkPhase::Live.as_u8() {
                    existing.pending_resync.store(true, Ordering::SeqCst);
                }
                let tx = existing.write_tx.clone();
                let sid = session_id.clone();
                spawn_with_error_boundary("terminal_resubscribe", async move {
                    if tx.send(Outbound::Subscribe).await.is_err() {
                        tracing::debug!(session_id = %sid, "io task exited before resubscribe, ignore");
                    }
                });
                return;
            }
        }
        let (write_tx, write_rx) = mpsc::channel::<Outbound>(256);
        let link = TerminalLink::new(session_id.clone(), app, write_tx, frontend_subscribed, seg2_channel);
        link.stopped.store(false, Ordering::SeqCst);
        link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
        // 停止后重建（同 id 会话重启）：前端 xterm 可能仍有上一轮内容 →
        // subscribed 回包后清屏重播（旧内容不与新流重叠）
        link.pending_resync.store(true, Ordering::SeqCst);
        link.emit_state("subscribing");
        let handle = spawn_with_error_boundary("terminal_link_io", link_io(link.clone(), write_rx));
        *link.handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        links.insert(session_id, link);
    }

    /// 取消订阅（离开终端页 / 会话停止 / 手动断开）：关连接不再重连。
    /// 输出在断开期间由桌面环窗口保留，重进时重订阅回放补齐
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

    /// 会话删除：清链路 + 段2 订阅态与推送通道
    pub fn remove(&self, session_id: &str) {
        self.unsubscribe(session_id);
        {
            let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            links.remove(session_id);
        }
        let mut consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        consumers.remove(session_id);
        let mut channels = self.page_channels.lock().unwrap_or_else(|p| p.into_inner());
        channels.remove(session_id);
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<TerminalLink>> {
        let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
        links.get(session_id).cloned()
    }
}

// ==================== IO 任务（连接 / 收发 / 重连） ====================

/// 单次连接会话（成功 → 流结束即退出返回；意外断开 → 外层按原因重连）
async fn connect_once(link: &Arc<TerminalLink>, write_rx: &mut mpsc::Receiver<Outbound>, policy: &Arc<ReconnectManager>) -> Result<(), LinkExit> {
    // 「上一段连接曾进入 live → 重连恢复需发 terminal-resync」的判断**不在这里**：
    // link_io 的 Err(Io) 分支在睡眠前已把 phase 置回 Connecting（见下），本函数被
    // 调用时 phase 恒为 Connecting，此处判 Live 是永远不触发的死代码。真正落点
    // 见 link_io 的 Err(LinkExit::Io) 分支（置 Connecting 之前补 pending_resync）。
    let conn = crate::state::get_connection_manager();
    let target = conn.get_target().await.ok_or(LinkExit::Io)?; // 目标缺失：按 Io 重连（目标恢复后自动续）
    let url = format!(
        "ws://{}:{}{}",
        target.address,
        target.port,
        crate::system::constants::connection::WS_PLUGIN_TERMINAL_PATH
    );
    let token = crate::state::get_global_token();

    link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(&url).await.map_err(|e| {
        tracing::warn!(session_id = %link.session_id, error = %e, "terminal ws connect failed");
        LinkExit::Io
    })?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // 每次连接重建握手闸门：subscribed 之前的二进制帧丢弃（旧流残留）
    link.subscribe_ack.store(false, Ordering::SeqCst);
    link.cursor.store(0, Ordering::SeqCst);
    link.pending_ack_bytes.store(0, Ordering::SeqCst);

    // 首消息认证（JWT；连接是会话绑定态的）
    let auth = format!(r#"{{"type":"auth","token":"{}"}}"#, token);
    if ws_tx.send(WsMsg::Text(auth)).await.is_err() {
        return Err(LinkExit::Io);
    }
    link.phase.store(LinkPhase::Auth.as_u8(), Ordering::SeqCst);
    link.emit_state("auth");

    // 认证后立即订阅（插件认证由宿主校验、无 auth_ok；订阅失败经 error 帧显性
    // 表达——会话不存在 → 退避重试）。fresh subscribe = 回放环窗口
    if ws_tx
        .send(WsMsg::Text(build_subscribe_frame(&link.session_id, LinkMode::Live)))
        .await
        .is_err()
    {
        return Err(LinkExit::Io);
    }
    link.emit_state("subscribed_sent");

    // ack 回发辅助（节流：64KB 阈值 + 250ms 空闲兜底；offset = 前端已渲染字节）
    async fn maybe_ack(
        link: &Arc<TerminalLink>,
        ws_sink: &mut futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
            WsMsg,
        >,
    ) {
        let now = now_millis();
        let pending = link.pending_ack_bytes.swap(0, Ordering::SeqCst);
        let last = link.last_ack_at.load(Ordering::SeqCst);
        if should_send_ack(pending, last, now) {
            let offset = link.frontend_rendered.load(Ordering::SeqCst);
            link.last_ack_at.store(now, Ordering::SeqCst);
            tracing::debug!(
                session_id = %link.session_id,
                offset,
                pending_bytes = pending,
                "terminal ack frame sent (flow control, offset = rendered bytes)"
            );
            if let Err(e) = futures_util::SinkExt::send(ws_sink, WsMsg::Text(build_ack_frame(offset))).await {
                tracing::warn!(
                    session_id = %link.session_id,
                    offset,
                    error = %e,
                    "terminal ack frame send failed, wait for reconnect"
                );
            }
        } else {
            // 未达阈值也未到空闲兜底：下次收帧 / 空闲定时器再判（节流 pending 保留）
            link.pending_ack_bytes.fetch_add(pending, Ordering::SeqCst);
        }
    }

    // ack 空闲兜底定时器：见 should_send_ack 注释——末批（<64KB）积压 ack 若只
    // 靠收帧事件触发，上游被暂停后永远不会回发。半空闲窗口轮询，保证积压 ack
    // 最迟 ~1.5×ACK_MAX_IDLE_MS 内回发；MissedTickBehavior::Delay 防补发风暴
    let mut ack_idle_tick = tokio::time::interval(Duration::from_millis(ACK_IDLE_TICK_MS));
    ack_idle_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // ==================== 链路活性检测（2026-10-04 新增） ====================
    //
    // 此前本链路**完全没有心跳**，死连接只能靠 `ws_rx.next()` 返回 Err 或收到
    // Close 来发现。TCP 半开时（对端进程已死、中间 NAT 仍维持连接）`next()`
    // 永久挂起，用户看到的是一个「活着但永远不出字」的终端。`ack_idle_tick`
    // 分支救不了：它只负责回发 ack，且 `ws_tx.send` 在半开连接上照样成功
    // （只写本地缓冲区）。
    //
    // 复用事件通道的 `HeartbeatManager`（与 `ws_client` 同一套判据）：
    // - `mark_connected()` 在握手成功后记下基准（**必须在订阅前**，否则静默
    //   订阅期间的首个窗口无基准可依）
    // - 周期性发 `WsMsg::Ping`（RFC 6455 标准帧，tungstenite 对端自动回 Pong）
    // - **任意入站帧**（业务/控制/Pong/Ping）都调 `on_activity()`：静默 shell
    //   不产生 Pong，只认 Pong 会把「终端空闲」误判成「连接已死」
    // - 超过 timeout 没有任何入站活动 → 判死，落到既有 Err(Io) 重连路径
    //
    // 注意：ack / subscribe / input 等**出站**活动不计入（半开时出站照样成功）。
    let heartbeat = HeartbeatManager::new(HeartbeatConfig::new(
        TERMINAL_HEARTBEAT_INTERVAL_SECS,
        TERMINAL_HEARTBEAT_TIMEOUT_SECS,
    ));
    heartbeat.mark_connected().await;
    let mut ping_tick = tokio::time::interval(Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS));
    ping_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_tick.tick().await; // 跳过立即触发的那一拍

    loop {
        tokio::select! {
            out = write_rx.recv() => {
                let Some(out) = out else { return Ok(()); };
                match out {
                    Outbound::Close => {
                        tracing::debug!(session_id = %link.session_id, "terminal link closed by command");
                        return Ok(());
                    }
                    Outbound::Subscribe => {
                        tracing::debug!(session_id = %link.session_id, "terminal fresh subscribe frame sent");
                        if ws_tx
                            .send(WsMsg::Text(build_subscribe_frame(&link.session_id, LinkMode::Live)))
                            .await
                            .is_err()
                        {
                            return Err(LinkExit::Io);
                        }
                    }
                    Outbound::TextInput { data } => {
                        let msg = build_input_text_frame(&data);
                        if ws_tx.send(WsMsg::Text(msg)).await.is_err() {
                            return Err(LinkExit::Io);
                        }
                    }
                    Outbound::BinaryInput { bytes } => {
                        if ws_tx.send(WsMsg::Binary(bytes)).await.is_err() {
                            return Err(LinkExit::Io);
                        }
                    }
                    Outbound::Ack { offset } => {
                        // 前端渲染水位推进（本地计数，仅用于 ack offset）
                        link.frontend_rendered.store(offset, Ordering::SeqCst);
                        maybe_ack(link, &mut ws_tx).await;
                    }
                }
            }
            msg = ws_rx.next() => {
                let Some(msg) = msg else { break; };
                let msg = msg.map_err(|e| {
                    tracing::debug!(session_id = %link.session_id, error = %e, "terminal ws stream error");
                    LinkExit::Io
                })?;
                // 任意入站帧 = 链路仍活（静默 shell 不发 Pong，只认 Pong 会误判死）
                heartbeat.on_activity().await;
                match msg {
                    WsMsg::Text(text) => {
                        handle_control_text(link, &text, policy).await?;
                    }
                    WsMsg::Binary(bin) => {
                        link.ingest_output(&bin);
                        link.maybe_log_ingest_stats();
                        maybe_ack(link, &mut ws_tx).await;
                    }
                    WsMsg::Ping(p) => {
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
                maybe_ack(link, &mut ws_tx).await;
            }
            _ = ping_tick.tick() => {
                // 探活：发标准 Ping 帧（对端 tungstenite 自动回 Pong）
                if ws_tx.send(WsMsg::Ping(Vec::new())).await.is_err() {
                    tracing::debug!(session_id = %link.session_id, "terminal ping send failed");
                    return Err(LinkExit::Io);
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(TERMINAL_HEARTBEAT_PROBE_TICK_MS)) => {
                // 半开探测：TCP 半开时 ping 的 send() 仍然成功（只写本地缓冲区），
                // 必须靠「多久没有任何入站活动」判死，而不是靠 send 报错
                if heartbeat.is_connection_lost().await {
                    tracing::warn!(
                        session_id = %link.session_id,
                        timeout_secs = TERMINAL_HEARTBEAT_TIMEOUT_SECS,
                        "terminal link silent beyond timeout (half-open suspected), reconnecting"
                    );
                    return Err(LinkExit::Io);
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

/// 处理 JSON 控制帧（subscribed / ring_resync / session_stopped / error / unknown）
async fn handle_control_text(
    link: &Arc<TerminalLink>,
    text: &str,
    policy: &Arc<ReconnectManager>,
) -> Result<(), LinkExit> {
    let msg: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    match msg.get("type").and_then(|v| v.as_str()) {
        // 订阅回包：门控置位——此后二进制帧才是本订阅的流（回放 + 实时）。
        // 本地基准归零（本地计数仅用于 ack 水位）。重订阅（重连/停止后重建/
        // 已 live 再 subscribe）时发 terminal-resync 让前端清屏 + 归零
        Some("subscribed") => {
            tracing::debug!(
                session_id = %link.session_id,
                mode = msg.get("mode").and_then(|v| v.as_str()).unwrap_or(""),
                "terminal subscribed, replay window follows in-stream"
            );
            link.subscribe_ack.store(true, Ordering::SeqCst);
            link.phase.store(LinkPhase::Live.as_u8(), Ordering::SeqCst);
            link.session_missing.store(0, Ordering::SeqCst);
            link.cursor.store(0, Ordering::SeqCst);
            link.frontend_rendered.store(0, Ordering::SeqCst);
            link.pending_ack_bytes.store(0, Ordering::SeqCst);
            link.emit_state("subscribed");
            // 重连恢复（2026-10-04 OCR M-01）：链路稳定回到 live 即视为本轮重连
            // 成功。**此前从不调 on_success**——`link_io` 只在失败路径推进策略，
            // retry_count 在整个链路生命周期累积，指数序列爬到 30s 封顶后，之后
            // 每次断线（哪怕经过健康期）都从 30s 起退而非 1s。事件通道
            // （connection/manager.rs reconnect 成功分支）对同一策略调 on_success，
            // 本链路此前与它行为分叉。收到 subscribed = 订阅门控通过，是最接近
            // 「重连成功」的可观测时点（fresh subscribe 时 retry_count 为 0，
            // on_success 是幂等的空操作）。
            policy.on_success().await;
            if link.pending_resync.swap(false, Ordering::SeqCst) {
                // 重订阅/重连/停止后重建：前端可能已有在屏内容 → 清屏 + 基准重置
                link.emit_resync(0);
            }
        }
        // 环淘汰重锚（唯一重锚信号）：清屏 + 本地计数基准重置。插件已自锚
        // 游标并从新基准续拉；本端不发送 resync 帧（协议保留能力）
        Some("ring_resync") => {
            let offset = msg.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            link.cursor.store(0, Ordering::SeqCst);
            link.frontend_rendered.store(0, Ordering::SeqCst);
            link.pending_ack_bytes.store(0, Ordering::SeqCst);
            link.emit_resync(offset);
        }
        // 停止帧：尾帧已先于本帧按序到达并被消费（帧序保证）→ 本端关闭连接
        Some("session_stopped") => {
            let reason = msg.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let exit_code = msg.get("exitCode").and_then(|v| v.as_i64());
            tracing::info!(
                session_id = %link.session_id,
                reason,
                exit_code,
                "terminal session stopped"
            );
            link.stopped.store(true, Ordering::SeqCst);
            link.phase.store(LinkPhase::Idle.as_u8(), Ordering::SeqCst);
            link.emit_state("stopped");
            return Ok(()); // 连接由调用方关闭（本端不再重连）
        }
        Some("error") => {
            let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            tracing::warn!(session_id = %link.session_id, message = %message, "terminal ws server error");
            match classify_server_error(&message) {
                ServerErrorClass::SessionMissing => {
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
                    // 会话启动竞态：留痕 + 状态事件，按退避重连重订阅
                    link.emit_state("retry");
                    return Err(LinkExit::Io);
                }
                ServerErrorClass::Other => {
                    // 非致命错误：留痕 + 状态事件，连接保持（后续帧可能恢复）
                    link.emit_state("error");
                }
            }
        }
        _ => {
            tracing::debug!(session_id = %link.session_id, type = ?msg.get("type"), "unknown terminal control frame");
        }
    }
    Ok(())
}

/// IO 任务主循环：连接 → 断 → 退避重连（意外）/ 停止（手动 / 会话缺失）
/// 终端链路的重连退避策略：**与设备级事件通道共用 `ReconnectManager`**
///
/// 单一事实源（2026-10-04）：本文件此前自建 `RECONNECT_BASE_MS = 500` /
/// `RECONNECT_MAX_MS = 8000` 的手写退避表，① `500` **低于**全局下限
/// `MIN_RECONNECT_DELAY_MS = 1000`（2026-09-29 那次 616 次/98 秒自愈风暴正是
/// 「无退避下限」形态）；② 无 jitter，多会话同步重连构成惊群。
///
/// `max_retries = 0` = 无限重试：终端链路随 subscribe 生命周期存在与销毁，不做
/// 「N 次后交还用户」的裁决（交还与否由 `SessionMissing` 与 `stopped` 决定）。
///
/// 抽成独立函数是为了可测：直接内联在 `link_io` 里时退避序列无法被断言，
/// 「有没有人又手写了一张表」只能靠读代码——而这正是本次缺陷的成因。
fn terminal_reconnect_policy() -> Arc<ReconnectManager> {
    ReconnectManager::new(ReconnectConfig::new(0, DEFAULT_INITIAL_DELAY_MS, DEFAULT_MAX_DELAY_MS))
}

/// IO 任务主循环：连接 → 断 → 退避重连（意外）/ 停止（手动 / 会话缺失）
async fn link_io(link: Arc<TerminalLink>, mut write_rx: mpsc::Receiver<Outbound>) {
    // 退避策略：共享 `ReconnectManager`（指数退避 + 抖动 + 1s 下限钳制）
    let policy = terminal_reconnect_policy();
    loop {
        if link.stopped.load(Ordering::SeqCst) {
            return;
        }
        match connect_once(&link, &mut write_rx, &policy).await {
            Ok(()) => return, // 正常退出
            Err(LinkExit::SessionMissing) => {
                // 会话缺失已达上限等外部恢复：不自动重连，等待 frontend 重新 subscribe
                return;
            }
            Err(LinkExit::Io) => {
                if link.stopped.load(Ordering::SeqCst) {
                    return;
                }
                // 重连恢复：上一段连接曾进入 live → 重播与已在屏内容可能重叠，
                // 重订阅的 `subscribed` 回包后必须发 terminal-resync 让前端清屏 +
                // 基准重置。**必须在把 phase 置回 Connecting 之前判断**：否则下一次
                // connect_once 进入时 phase 恒为 Connecting，本标记永远不会置位
                // （2026-10-04 审查发现的死代码——原实现把同样判断放在
                // connect_once 顶部，被这里先重置 phase 而永远跳过，导致断线重连
                // 后回放直接叠在旧屏内容上，行式输出整屏重复）。
                if link.phase.load(Ordering::SeqCst) == LinkPhase::Live.as_u8() {
                    link.pending_resync.store(true, Ordering::SeqCst);
                }
                link.phase.store(LinkPhase::Connecting.as_u8(), Ordering::SeqCst);
                link.emit_state("reconnecting");
                // 策略推进一轮（无限重试下恒 Some；防御性保留熔断/轮数分支）
                if policy.start().await.is_none() {
                    tracing::warn!(
                        session_id = %link.session_id,
                        "terminal reconnect policy gave up"
                    );
                    return;
                }
                // `get_delay()` = 本轮退避 + ±10% 抖动（start 已置 current_delay）
                let delay = policy.get_delay().await;
                link.emit_reconnect_scheduled(delay);
                tokio::time::sleep(delay).await;
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

/// 订阅会话（进入终端页 / 预加载；fresh subscribe 语义——已运行链路重订阅回放）
#[tauri::command]
pub async fn terminal_subscribe(app: tauri::AppHandle, session_id: String) -> Result<(), String> {
    terminal_link_manager().subscribe(Arc::new(app), session_id);
    Ok(())
}

/// 取消订阅（离开终端页 / 会话停止 / 手动断开）：关连接不再重连
#[tauri::command]
pub async fn terminal_unsubscribe(session_id: String) -> Result<(), String> {
    terminal_link_manager().unsubscribe(&session_id);
    Ok(())
}

/// 段2 订阅（进入终端页）：登记页面通道并开启输出推送。
/// 幂等；不依赖链路存在——链路未建立时先记录订阅意愿与通道，链路建立后即生效
#[tauri::command]
pub async fn terminal_page_subscribe(session_id: String, channel: Channel<InvokeResponseBody>) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("session_id is empty".to_string());
    }
    terminal_link_manager().page_subscribe(&session_id, channel);
    Ok(())
}

/// 段2 取消订阅（退出终端页）：清空页面通道并停止推送
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

/// 发送终端输入（前端 → Rust → WS 帧 → 桌面 PTY）
///
/// 双形态（spec §3.3）：`special_key`（控制字符/特殊键）→ `KeyCombo::to_pty_bytes()`
/// → **binary 帧**原始字节；`data`（可打印 UTF-8 文本）→ `{"type":"input","data":...}`
/// text 帧。两者可并存（如「命令 + Enter」= text 帧 + binary `\r`），帧序即写入序。
///
/// 投递计划由纯函数 [`plan_input_frames`] 决定（顺序、校验时机都在那里单测）。
///
/// **投递失败上抛**：链路 IO 任务退出时帧会被丢弃，此时返回 Err（而非静默 Ok）——
/// 多帧输入下「文本已写进 PTY、回车没发」必须让前端看得见（PTY 侧不可回滚）。
#[tauri::command]
pub async fn terminal_send_input(session_id: String, data: String, special_key: Option<String>) -> Result<(), String> {
    let frames = plan_input_frames(&data, special_key.as_deref())?;
    let manager = terminal_link_manager();
    let link = manager
        .get(&session_id)
        .ok_or_else(|| "terminal link not subscribed".to_string())?;
    for frame in frames {
        link.send_out(frame).await?;
    }
    Ok(())
}

/// 渲染背压 ack：前端本地已渲染字节数推进（onWriteParsed 后按渲染游标调用）；
/// Rust 侧节流回发 `{"type":"ack","offset":N}`（桌面插件据此触发 drain）
#[tauri::command]
pub async fn terminal_ack_rendered(session_id: String, offset: u64) -> Result<(), String> {
    let manager = terminal_link_manager();
    let link = manager
        .get(&session_id)
        .ok_or_else(|| "terminal link not subscribed".to_string())?;
    link.send_out(Outbound::Ack { offset })
        .await
        // ack 是尽力投递的渲染进度确认（丢失由对端重发 / 重连恢复补上），
        // 失败已在 send_out 内留痕，不向渲染热路径报错
        .ok();
    Ok(())
}

/// 一次性历史（HTTP 直取；无本地缓存——历史由订阅回放提供，本命令保留为
/// 「环窗口外历史 / 无流会话」的显式拉取通道；前端暂未接线）
///
/// 桌面端返回统一 ApiResponse 信封 {code, message, data:{min_offset, snapshot_offset,
/// history_bytes, data_base64}}（camelCase 内层）——剥信封、code!=0 视为错误
#[tauri::command]
pub async fn terminal_get_history(
    app: tauri::AppHandle,
    session_id: String,
    from: u64,
) -> Result<serde_json::Value, String> {
    tracing::debug!(
        session_id = %session_id,
        from = from,
        "terminal history fetched via desktop http"
    );
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
    let response = crate::commands::http_proxy::execute_proxy(request, Some(&app))
        .await
        .map_err(|e| e.to_string())?;
    if response.status != 200 {
        return Err(format!("history fetch failed: status {}", response.status));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&response.body_text).map_err(|e| format!("history response parse failed: {e}"))?;
    let code = parsed.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
    if code != 0 {
        let message = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!("history fetch failed: code {code} {message}"));
    }
    let data = parsed.get("data").cloned().unwrap_or(parsed);
    let min_offset = data.get("minOffset").and_then(|v| v.as_u64()).unwrap_or(from);
    let snapshot_offset = data.get("snapshotOffset").and_then(|v| v.as_u64()).unwrap_or(from);
    let history_bytes = data.get("historyBytes").and_then(|v| v.as_u64()).unwrap_or(0);
    let data_base64 = data.get("dataBase64").and_then(|v| v.as_str()).unwrap_or("");
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
        return Ok(serde_json::json!({ "sessionId": session_id, "phase": "idle" }));
    };
    // camelCase 键对齐前端 TerminalLinkState 接口（invoke 返回值不做键名转换）
    Ok(serde_json::json!({
        "sessionId": session_id,
        "phase": LinkPhase::from_u8(link.phase.load(Ordering::SeqCst)).as_api_str(),
        "cursor": link.cursor.load(Ordering::SeqCst),
        "acked": link.frontend_rendered.load(Ordering::SeqCst),
        "stopped": link.stopped.load(Ordering::SeqCst),
        "subscribed": link.subscribe_ack.load(Ordering::SeqCst),
    }))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    // 仅测试需要（退避下限断言）：生产代码不引用，放模块级会变成 release 未用导入
    use crate::system::constants::reconnect::MIN_RECONNECT_DELAY_MS;

    // ==================== 重连退避收敛（防手写第二张表） ====================

    /// 行为契约（2026-10-04 审计 P0-1 同型缺陷的终端链路版本）：终端链路的退避
    /// 必须与设备级事件通道**同一来源**。旧实现自建 `500→…→8000` 手写表，
    /// 其中 500 低于全局下限 1000 —— 与事件通道那次「护栏挂在死代码上」是同一类
    /// 教训，只是这次护栏和被保护的对象都在跑，却各用各的数。
    ///
    /// 正例：序列逐轮等于 `ReconnectManager` 默认等比退避（1s/2s/4s/8s/16s）
    #[tokio::test]
    async fn terminal_backoff_matches_shared_reconnect_manager_sequence() {
        let policy = terminal_reconnect_policy();
        for expect_ms in [1000u64, 2000, 4000, 8000, 16000] {
            policy.start().await.expect("无限重试配置下不应耗尽");
            let delay = policy.get_delay().await;
            // 抖动是 0~+10% 的正偏移，故断言下界与「不超过 1.1×」上界
            assert!(
                delay.as_millis() as u64 >= expect_ms,
                "第 {} 轮退避 {}ms 低于等比基线 {}ms",
                policy.get_retry_count().await,
                delay.as_millis(),
                expect_ms
            );
            assert!(
                delay.as_millis() as u64 <= expect_ms + expect_ms / 10,
                "第 {} 轮退避 {}ms 超出 +10% 抖动上界",
                policy.get_retry_count().await,
                delay.as_millis()
            );
        }
    }

    /// 反例（最关键的一条）：单次等待不得低于全局下限 1s。旧实现在首轮就是
    /// 500ms —— 与 2026-09-29 那次 616 次/98 秒自愈风暴同型。
    #[tokio::test]
    async fn terminal_backoff_never_breaches_global_minimum_delay() {
        let policy = terminal_reconnect_policy();
        policy.start().await.unwrap();
        let delay = policy.get_delay().await;
        assert!(
            delay >= std::time::Duration::from_millis(MIN_RECONNECT_DELAY_MS),
            "首轮退避 {}ms 击穿了全局下限 {}ms",
            delay.as_millis(),
            MIN_RECONNECT_DELAY_MS
        );
    }

    /// 退避封顶必须生效：长时间断开后单轮等待不得无限增长
    #[tokio::test]
    async fn terminal_backoff_is_capped() {
        let policy = terminal_reconnect_policy();
        for _ in 0..10 {
            policy.start().await.unwrap();
        }
        let delay = policy.get_delay().await;
        assert!(
            delay <= std::time::Duration::from_millis(DEFAULT_MAX_DELAY_MS + DEFAULT_MAX_DELAY_MS / 10),
            "第 10 轮退避 {}ms 超出封顶 {}ms",
            delay.as_millis(),
            DEFAULT_MAX_DELAY_MS
        );
    }

    /// 终端链路保持既有语义：**无限**重试（随 subscribe 生命周期销毁，不做
    /// 「N 次后交还用户」的裁决）。若有人改成有限轮次，退避耗尽会让
    /// `link_io` 直接 return 而不再自愈——终端永久卡死。
    #[tokio::test]
    async fn terminal_reconnect_is_unlimited_by_design() {
        let policy = terminal_reconnect_policy();
        for _ in 0..20 {
            assert!(
                policy.start().await.is_some(),
                "第 {} 轮被截断：终端链路应无限重试",
                policy.get_retry_count().await + 1
            );
        }
        assert!(!policy.is_abandoned().await);
    }

    /// 反例（2026-10-04 OCR M-01 回归锁）：链路稳定回到 live 必须复位退避
    /// 序列。修复前 `link_io` 只在失败路径调 `policy.start()`，从不调
    /// `on_success()`——retry_count 在整个链路生命周期累积，指数序列爬到
    /// 封顶（30s）后，之后每次断线（哪怕刚经过健康期）都从 30s 起退，而非
    /// 从 1s 重来。事件通道（connection/manager.rs reconnect 成功分支）对同一
    /// 策略调 on_success，终端链路与它行为分叉（「单一事实源」名存实亡）。
    ///
    /// 正例：多轮失败（计数爬到高位）后收到 `subscribed` 门控帧 → 计数归零，
    /// 下一轮 `start()` 回到初始退避而非封顶值。
    #[tokio::test]
    async fn subscribed_resets_backoff_sequence_after_failures() {
        let (tx, _rx) = mpsc::channel::<Outbound>(8);
        let link = TerminalLink::new(
            "s1".to_string(),
            Arc::new(NullSink),
            tx,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Mutex::new(None)),
        );

        let policy = terminal_reconnect_policy();
        // 多轮失败：退避爬到封顶
        for _ in 0..10 {
            policy.start().await.unwrap();
        }
        let capped = policy.get_delay().await;
        let before = policy.get_retry_count().await;
        assert!(before >= 10, "前置：退避计数应先爬上来（实际 {before}）");

        // 链路恢复：收到 subscribed 门控帧（link_io 的 connect_once 收帧路径）
        let subscribed = handle_control_text(
            &link,
            r#"{"type":"subscribed","mode":"live"}"#,
            &policy,
        )
        .await;
        assert!(subscribed.is_ok(), "subscribed 帧处理不得报错（LinkExit 无 Debug，断言 is_ok）");

        // 反例断言：计数归零 → 下一次失败从初始退避重来，而不是沿用封顶值
        assert_eq!(
            policy.get_retry_count().await,
            0,
            "恢复 live 后退避计数必须归零（on_success 语义）"
        );
        let next = policy.start().await.unwrap();
        assert!(
            next < capped,
            "复位后首轮退避 {:?} 应远低于封顶前 {:?}——否则健康期后的断线仍 30s 起退",
            next,
            capped
        );
        assert!(
            next >= std::time::Duration::from_millis(MIN_RECONNECT_DELAY_MS),
            "复位后同样受 1s 下限保护（实际 {:?}",
            next
        );
    }

    // ==================== 链路活性检测（半开） ====================

    /// 判活基准必须回落到**建连时刻**：静默 shell 不发 Pong，若首个 Ping 之前
    /// 没有基准，这段窗口的死连接检测不到（同事件通道那次修复的同型缺陷）。
    ///
    /// timeout 用 1ms + 30ms 真实等待（Instant 无注入缝，与 heartbeat.rs 既有
    /// 超时用例同款取法）。注意 `HeartbeatConfig::new(secs, secs)` 收的是**秒**，
    /// 要毫秒级必须用结构体字面量。
    #[tokio::test]
    async fn liveness_detects_silence_after_mark_connected_without_any_pong() {
        let hb = HeartbeatManager::new(HeartbeatConfig {
            interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
            timeout: std::time::Duration::from_millis(1),
            max_timeouts: 3,
        });
        hb.mark_connected().await;
        assert!(!hb.is_connection_lost().await, "刚建连不应立即判死");
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(
            hb.is_connection_lost().await,
            "建连后无任何入站活动且已超时应判死（半开检测不得失效）"
        );
    }

    /// 正例：**任意**入站帧都刷新基准。终端的静默期只靠 Pong 会被误判成死链，
    /// 只有业务输出帧而无 Pong 时仍必须算「活着」。
    #[tokio::test]
    async fn any_inbound_activity_refreshes_the_liveness_baseline() {
        let hb = HeartbeatManager::new(HeartbeatConfig {
            interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
            timeout: std::time::Duration::from_millis(1),
            max_timeouts: 3,
        });
        hb.mark_connected().await;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(hb.is_connection_lost().await, "前置：静默超时应已判死");

        // 收到业务输出帧（非 Pong）
        hb.on_activity().await;
        assert!(!hb.is_connection_lost().await, "收到入站帧后应立即恢复为「活着」");
    }

    /// 边界：`mark_connected` 开启新一轮建连，清掉上一轮的基准与超时计数
    #[tokio::test]
    async fn mark_connected_resets_previous_round_state() {
        let hb = HeartbeatManager::new(HeartbeatConfig {
            interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
            timeout: std::time::Duration::from_millis(1),
            max_timeouts: 3,
        });
        hb.on_activity().await;
        hb.increment_timeout().await;
        hb.increment_timeout().await;
        hb.mark_connected().await;
        assert_eq!(hb.get_consecutive_timeouts().await, 0);
        assert!(!hb.is_connection_lost().await);
    }

    /// 未 mark_connected（心跳循环从未启动）→ 不擅自判死，判定权交还调用方
    #[tokio::test]
    async fn liveness_does_not_judge_before_mark_connected() {
        let hb = HeartbeatManager::new(HeartbeatConfig {
            interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
            timeout: std::time::Duration::from_millis(1),
            max_timeouts: 3,
        });
        assert!(!hb.is_connection_lost().await);
    }

    // ==================== 帧构造（新协议 wire 形状锁） ====================

    fn keys(v: &serde_json::Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    #[test]
    fn build_subscribe_frame_shape_and_mode() {
        let live = serde_json::from_str::<serde_json::Value>(&build_subscribe_frame("s1", LinkMode::Live)).unwrap();
        assert_eq!(keys(&live), ["mode", "sessionId", "type"]);
        assert_eq!(live["type"], "subscribe");
        assert_eq!(live["sessionId"], "s1");
        assert_eq!(live["mode"], "live");

        let poll = serde_json::from_str::<serde_json::Value>(&build_subscribe_frame("s2", LinkMode::Poll)).unwrap();
        assert_eq!(poll["mode"], "poll", "批量态 mode 帧形状（协议保留能力）");
    }

    #[test]
    fn build_ack_frame_shape() {
        let ack = serde_json::from_str::<serde_json::Value>(&build_ack_frame(4096)).unwrap();
        assert_eq!(keys(&ack), ["offset", "type"]);
        assert_eq!(ack["type"], "ack");
        assert_eq!(ack["offset"], 4096);
    }

    #[test]
    fn build_poll_frame_shape() {
        let poll = serde_json::from_str::<serde_json::Value>(&build_poll_frame()).unwrap();
        assert_eq!(keys(&poll), ["type"]);
        assert_eq!(poll["type"], "poll");
    }

    /// 文本输入帧：UTF-8 原文（非 base64）+ JSON 转义正确（含引号/控制字符）
    #[test]
    fn build_input_text_frame_escapes_and_keeps_utf8() {
        let f = build_input_text_frame("ls -la \"a\"\u{1f600}");
        let parsed: serde_json::Value = serde_json::from_str(&f).unwrap();
        assert_eq!(parsed["type"], "input");
        assert_eq!(parsed["data"], "ls -la \"a\"\u{1f600}");
        assert!(!f.contains("base64"), "可打印输入不得走 base64");
    }

    /// 输入双形态（特殊键 → binary 字节）：UI 提供的全部特殊键都能经
    /// KeyCombo::parse + to_pty_bytes 映射（Enter/Tab/Esc/Del/Ctrl+C/Z/L/方向键）
    #[test]
    fn special_key_to_pty_bytes_covers_ui_keys() {
        let cases: &[(&str, &[u8])] = &[
            ("enter", &[0x0d]),
            ("tab", &[0x09]),
            ("escape", &[0x1b]),
            ("delete", &[0x1b, b'[', b'3', b'~']),
            ("ctrl_c", &[0x03]),
            ("ctrl_z", &[0x1a]),
            ("ctrl_l", &[0x0c]),
            ("arrow_up", &[0x1b, b'[', b'A']),
            ("arrow_down", &[0x1b, b'[', b'B']),
            ("arrow_left", &[0x1b, b'[', b'D']),
            ("arrow_right", &[0x1b, b'[', b'C']),
        ];
        for (name, expect) in cases {
            assert_eq!(
                special_key_to_pty_bytes(name).as_deref(),
                Some(*expect),
                "special key {name} 必须映射到二进制 PTY 字节"
            );
        }
        assert_eq!(special_key_to_pty_bytes("no_such_key"), None);
        assert_eq!(special_key_to_pty_bytes(""), None);
    }

    // ==================== 输入投递计划（文本 + 特殊键 共存契约） ====================

    /// 抽出帧序列的可读形状（断言「投了几帧、什么顺序、什么载荷」，不绑死枚举变体名）
    fn plan_shape(data: &str, special_key: Option<&str>) -> Result<Vec<String>, String> {
        Ok(plan_input_frames(data, special_key)?
            .into_iter()
            .map(|f| match f {
                Outbound::TextInput { data } => format!("text:{data}"),
                Outbound::BinaryInput { bytes } => {
                    format!("bytes:{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
                }
                other => panic!("输入计划不得产生非输入帧：{other:?}"),
            })
            .collect())
    }

    /// C-IN-001 正例：纯文本（输入栏「发送」= 不带回车）
    #[test]
    fn plan_input_frames_text_only_sends_one_text_frame() {
        assert_eq!(
            plan_shape("ls -la", None).unwrap(),
            vec!["text:ls -la".to_string()],
            "纯文本必须且只发一帧 text"
        );
    }

    /// C-IN-002 反例（回归锁）：纯特殊键（快捷键 / 方向键 / Ctrl+C）→ 只发 binary
    #[test]
    fn plan_input_frames_key_only_sends_one_binary_frame() {
        assert_eq!(
            plan_shape("", Some("ctrl_c")).unwrap(),
            vec!["bytes:03".to_string()],
            "纯特殊键必须且只发一帧 binary"
        );
    }

    /// C-IN-003 正例（历史缺陷回归锁）：「命令 + Enter」两帧共存且**文本在前**
    ///
    /// 缺陷版实现是 `if 有键 … else if 有文本`，输入栏唯一的生产路径
    /// （前端恒传 specialKey="enter"）只发得出裸回车、命令文本被丢弃。
    #[test]
    fn plan_input_frames_keeps_text_before_special_key() {
        assert_eq!(
            plan_shape("echo HI", Some("enter")).unwrap(),
            vec!["text:echo HI".to_string(), "bytes:0d".to_string()],
            "命令 + Enter 必须先发文本帧再发回车帧（帧序即写入序）"
        );
    }

    /// C-IN-004 反例（变异探针）：把文本帧丢掉/换序，这条断言必须失败——
    /// 与 C-IN-003 组成同一契约的正反两面，禁止只留顺序断言
    #[test]
    fn plan_input_frames_order_is_observable_by_plan_length_and_payload() {
        let both = plan_shape("echo HI", Some("enter")).unwrap();
        let text_only = plan_shape("echo HI", None).unwrap();
        assert_eq!(both.len(), 2, "有键 + 有文本必须产生两帧");
        assert_eq!(
            both[0], text_only[0],
            "共存计划的第一帧必须与纯文本计划的第一帧一致（即文本未被丢弃）"
        );
    }

    /// C-IN-005 异常：不支持的键名 → Err，且**一帧都不投递**（半截输入不可回滚）
    #[test]
    fn plan_input_frames_rejects_unknown_key_without_partial_send() {
        let err = plan_shape("echo HI", Some("no_such_key")).unwrap_err();
        assert!(err.contains("no_such_key"), "错误信息必须点名非法键名，实际={err}");
    }

    /// C-IN-006 边界：空串键名视作「无特殊键」（前端可能传 Some("")）
    #[test]
    fn plan_input_frames_empty_key_name_is_treated_as_absent() {
        assert_eq!(
            plan_shape("echo HI", Some("")).unwrap(),
            vec!["text:echo HI".to_string()],
            "空键名不得产生 binary 帧"
        );
    }

    /// C-IN-007 边界：两者皆空 → 空计划（不发帧，但不报错）
    #[test]
    fn plan_input_frames_empty_both_sends_nothing() {
        assert!(plan_input_frames("", None).unwrap().is_empty(), "空输入不得产生任何帧");
    }

    // ==================== 投递失败上抛（多帧输入的半截输入护栏） ====================

    /// 测试替身：不发射任何前端事件（投递失败路径不经过发射面）
    struct NullSink;

    impl TerminalEventSink for NullSink {
        fn emit(&self, _event: &str, _payload: serde_json::Value) -> Result<(), String> {
            Ok(())
        }
    }

    /// 构造一条链路：返回 (link, 发送端持有者)；丢弃发送端即模拟 IO 任务退出
    fn link_with_channel(session_id: &str) -> (Arc<TerminalLink>, mpsc::Sender<Outbound>) {
        let (tx, rx) = mpsc::channel::<Outbound>(8);
        let link = TerminalLink::new(
            session_id.to_string(),
            Arc::new(NullSink),
            tx.clone(),
            Arc::new(AtomicBool::new(true)),
            Arc::new(Mutex::new(None)),
        );
        // IO 任务立刻退出：接收端被丢弃 → 后续 send 必失败
        drop(rx);
        (link, tx)
    }

    /// C-IN-008 正例：通道仍活着 → 投递成功
    #[tokio::test]
    async fn send_out_succeeds_while_io_task_alive() {
        let (tx, mut rx) = mpsc::channel::<Outbound>(8);
        let link = TerminalLink::new(
            "s1".to_string(),
            Arc::new(NullSink),
            tx,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Mutex::new(None)),
        );

        let res = link.send_out(Outbound::TextInput { data: "ls".to_string() }).await;

        assert!(res.is_ok(), "通道存活时投递必须成功，实际={res:?}");
        assert!(matches!(rx.try_recv(), Ok(Outbound::TextInput { .. })), "帧必须真的进了通道");
    }

    /// C-IN-009 异常：IO 任务退出（通道关闭）→ 必须返回 Err
    ///
    /// 单帧时代「丢弃 + 留痕」尚可接受；多帧输入（命令 + Enter）下静默丢弃会让
    /// 「文本已写进 PTY、回车没发」被当成成功返回（PTY 侧不可回滚）。
    #[tokio::test]
    async fn send_out_reports_error_after_io_task_exited() {
        let (link, _tx) = link_with_channel("s-dead");

        let res = link.send_out(Outbound::TextInput { data: "echo HI".to_string() }).await;

        let err = res.expect_err("通道关闭必须上抛 Err（不得静默丢弃）");
        assert!(err.contains("s-dead"), "错误信息必须点名会话（便于前端/日志定位），实际={err}");
    }

    // ==================== ack 节流（回归护栏：语义与旧 TB v3 版一致） ====================

    /// 达阈值即回发：与空闲时长无关（节流上沿）
    #[test]
    fn should_send_ack_when_pending_reaches_threshold() {
        assert!(should_send_ack(ACK_BYTES_THRESHOLD, 1_000, 1_000));
        assert!(should_send_ack(ACK_BYTES_THRESHOLD + 1, 1_000, 1_001));
    }

    /// 回归护栏（背压死锁）：末批不足阈值的积压 ack，空闲兜底窗口到期必须回发。
    /// 旧实现仅在收帧时求值该规则，上游被暂停（不再收帧）后积压永不回发
    #[test]
    fn should_send_ack_flushes_stranded_pending_after_idle_window() {
        let last = 10_000;
        assert!(!should_send_ack(1, last, last + ACK_MAX_IDLE_MS - 1));
        assert!(should_send_ack(1, last, last + ACK_MAX_IDLE_MS));
        assert!(should_send_ack(1024, last, last + ACK_MAX_IDLE_MS + 5));
    }

    /// pending 为 0 一律不回发：空闲定时器周期调用不得产生空 ack 风暴
    #[test]
    fn should_send_ack_never_sends_without_pending_bytes() {
        assert!(!should_send_ack(0, 1_000, 1_000));
        assert!(!should_send_ack(0, 1_000, 1_000 + ACK_MAX_IDLE_MS * 100));
    }

    /// 首次回发（尚无 ack 基准）仍需攒满阈值：避免握手后立即产生零散 ack
    #[test]
    fn should_send_ack_first_send_waits_for_threshold() {
        assert!(!should_send_ack(1024, 0, 1_000_000));
        assert!(should_send_ack(ACK_BYTES_THRESHOLD, 0, 1_000_000));
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
    }

    // ==================== error 帧分类（会话不存在 → 退避重试） ====================

    #[test]
    fn classify_server_error_session_missing_and_other() {
        // 桌面插件 subscribe 失败的消息含「会话不存在」字样（启动竞态/已停止）
        assert!(matches!(
            classify_server_error("会话不存在：s1"),
            ServerErrorClass::SessionMissing
        ));
        assert!(matches!(
            classify_server_error("subscribe: missing sessionId"),
            ServerErrorClass::Other
        ));
        assert!(matches!(
            classify_server_error("host pty write failed: x"),
            ServerErrorClass::Other
        ));
        assert!(matches!(classify_server_error(""), ServerErrorClass::Other));
    }

    // ==================== 阶段映射 ====================

    #[test]
    fn link_phase_round_trip_and_api_str() {
        assert_eq!(LinkPhase::from_u8(LinkPhase::Live.as_u8()), LinkPhase::Live);
        assert_eq!(LinkPhase::from_u8(LinkPhase::Auth.as_u8()), LinkPhase::Auth);
        assert_eq!(LinkPhase::from_u8(LinkPhase::Connecting.as_u8()), LinkPhase::Connecting);
        assert_eq!(LinkPhase::from_u8(LinkPhase::Idle.as_u8()), LinkPhase::Idle);
        assert_eq!(LinkPhase::from_u8(99), LinkPhase::Idle);
        assert_eq!(LinkPhase::Live.as_api_str(), "live");
        assert_eq!(LinkPhase::Idle.as_api_str(), "idle");
        // 新协议无独立 history 阶段：u8=3 即 live（旧版 4 已退役）
        assert_eq!(
            LinkPhase::from_u8(4),
            LinkPhase::Idle,
            "旧 TB v3 的 history 阶段号不得再被识别"
        );
    }

    // ==================== 段2 订阅态（管理器，链路独立） ====================

    /// 段2 订阅态与推送通道槽挂在管理器上（独立于链路）：链路未建立也记录订阅
    /// 意愿，同一会话始终返回同一份开关/通道（链路重建后沿用）；取消订阅与会话
    /// 删除必须一并清空通道，否则推送会持续往已卸载的页面发
    #[test]
    fn manager_page_subscription_is_session_scoped_and_persistent() {
        let manager = TerminalLinkManager {
            links: Mutex::new(HashMap::new()),
            consumers: Mutex::new(HashMap::new()),
            page_channels: Mutex::new(HashMap::new()),
        };
        let channel: Channel<InvokeResponseBody> = Channel::new(|_| Ok(()));

        assert!(!manager.is_page_subscribed("s1"));

        manager.page_subscribe("s1", channel.clone());
        assert!(manager.is_page_subscribed("s1"));
        let flag = manager.consumer_flag("s1");
        assert!(flag.load(Ordering::SeqCst));

        // 同一会话多次 get-or-create 返回同一 Arc（链路重建后沿用订阅态 + 通道槽）
        assert!(Arc::ptr_eq(&flag, &manager.consumer_flag("s1")));
        let slot = manager.consumer_channel("s1");
        assert!(slot.lock().unwrap().is_some(), "段2 订阅必须登记推送通道");
        assert!(Arc::ptr_eq(&slot, &manager.consumer_channel("s1")));

        // 会话隔离：另一会话既不订阅、也没有通道
        assert!(!manager.is_page_subscribed("s2"));
        assert!(manager.consumer_channel("s2").lock().unwrap().is_none());

        manager.page_unsubscribe("s1");
        assert!(!manager.is_page_subscribed("s1"));
        assert!(
            !flag.load(Ordering::SeqCst),
            "取消订阅必须作用在同一份开关上（链路持有的 Arc 同步可见）"
        );
        assert!(
            manager.consumer_channel("s1").lock().unwrap().is_none(),
            "退出页面必须清空推送通道"
        );

        // 会话删除：订阅态与通道一并作废
        manager.page_subscribe("s1", channel);
        manager.remove("s1");
        assert!(!manager.is_page_subscribed("s1"));
        assert!(manager.consumer_channel("s1").lock().unwrap().is_none());
    }

    // ==================== 结构锁 ====================

    /// 结构锁一：新协议**WS 协议实现段**不得出现旧信封 / 旧 TB v3 残留——
    /// `Message::`（信封）、TB 帧头常量、`from_offset` / `history_end` 等。
    /// `terminal_get_history` 是桌面 HTTP 响应映射（camelCase `minOffset` 等是
    /// 桌面 HTTP wire 字段，与 WS 帧协议无关），不在扫描范围
    #[test]
    fn terminal_link_has_no_legacy_protocol_residue() {
        let root = env!("CARGO_MANIFEST_DIR");
        let src = std::fs::read_to_string(format!("{root}/src/terminal_link.rs")).expect("read terminal_link.rs");
        // 截取 WS 协议实现段：到 `terminal_get_history` 为止（之后是 HTTP 历史代理）
        let implementation = src
            .split("pub async fn terminal_get_history")
            .next()
            .unwrap_or(&src)
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(&src);
        let mut violations: Vec<String> = Vec::new();
        for (idx, raw) in implementation.lines().enumerate() {
            let line = raw.trim_start();
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            for marker in [
                "Message::",
                "from_offset",
                "history_end",
                "TB_FRAME_HEADER_LEN",
                "snapshot_offset",
                "min_offset",
            ] {
                if line.contains(marker) {
                    violations.push(format!("terminal_link.rs:{}: {}", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "terminal_link WS 协议实现段不得出现旧协议残留（票 05）：\n{}",
            violations.join("\n")
        );
    }
}
