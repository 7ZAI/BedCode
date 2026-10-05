//! WS 终端流端点（websocket 业务下沉票 04）
//!
//! 客户端经 `/ws/plugin/com.bedcode.terminal-session/terminal` 直连（manifest
//! `contributes.wsEndpoints` 声明，`auth: jwt` 由宿主校验）。**终端协议完全归
//! 本插件**：输入直写 PTY、输出经 `ring-fetch` 按**每连接独立游标**拉取并以
//! 二进制帧下发、ACK/截断重同步/停止帧全部插件定义；宿主只转原始帧
//! （text/binary），不解析终端帧、不读 session id、不维护终端订阅表（spec §3.3）。
//!
//! ## 帧协议（插件定义，宿主不透明）
//!
//! 客户端 → 插件（text JSON）：
//! ```json
//! {"type":"subscribe","sessionId":"...","mode":"live"|"poll"}   // mode 缺省 live
//! {"type":"unsubscribe"}
//! {"type":"ack","offset":N}        // 流控信号：客户端确认收到 N 之前全部输出
//! {"type":"resync","offset":N}     // 客户端已清屏，从 N 继续（环淘汰后重锚）
//! {"type":"input","data":"..."}    // UTF-8 文本输入（无控制字符；控制字符走 binary）
//! {"type":"poll"}                  // 主动拉取（客户端驱动 drain 的触发）
//! ```
//! 客户端 → 插件（binary）：原始输入字节（可含控制字符，Ctrl-C 等）
//! 插件 → 客户端（binary）：输出字节（ring-fetch 原始数据，不 JSON 化）
//! 插件 → 客户端（text JSON）：
//! ```json
//! {"type":"subscribed","sessionId":"...","mode":"..."}
//! {"type":"unsubscribed"}
//! {"type":"ring_resync","offset":N}              // 环已淘汰：N 之前数据不可恢复
//! {"type":"session_stopped","sessionId":"...","reason":"stopped|killed|error","exitCode":N?}
//! {"type":"error","message":"..."}
//! ```
//!
//! ## 输出泵（有界 drain，不在宿主回调内形成长链）
//!
//! 任何入站帧（subscribe/ack/resync/input/binary/poll）之后对订阅连接做一次
//! **有界 drain**：逐次 `ring-fetch`（单次上限 16 KiB）直到追平或周期预算用尽
//! （8 次 ≈ 128 KiB）；输出以二进制帧即时下发。慢客户端发送失败只停本人
//! （debug 留痕，fail-visible 计数），不阻塞其他连接与 PTY 产出（pull 模型：
//! 宿主环绝不回传背压，spec §7.4）。drain 的触发源有三个：
//!
//! 1. 客户端入站帧（输入 / ack / poll——**ack 自时钟**：每收 64 KiB 或最迟
//!    ~375 ms 一帧，构成本路径的主驱动）；
//! 2. 宿主限频唤醒 `<owner>::pty:output`（50 ms 限频，`drain_session`）——
//!    修掉「空闲后首个字节只能等 1 s tick」的延迟档；
//! 3. 1 s 调度 tick 兜底（`drain_all_on_tick`）。
//!
//! ## 背压（客户端交付水位窗口，与插件前端拉取路径同口径）
//!
//! 移动端按**渲染水位**回发 `ack`（`onWriteParsed` → `terminal_ack_rendered`），
//! 插件据此记账：未确认积压达上沿（`output::HIGH_WATER_BYTES`）即**抑制**本轮
//! drain，数据留在宿主环内等 ack 回落（迟滞下沿 `LOW_WATER_BYTES` 才恢复），
//! 与插件自己前端 `session.output.pull` 的双水位迟滞**同一对常量、同一裁决
//! 函数**（`output::decide_pull_gate`）——两条消费路径的背压口径必须一致。
//!
//! ack 回发的是**本地累计字节**（`subscribed` / `ring_resync` 时归零），不是环
//! 绝对偏移，故 [`Watermark`] 持 `base` 做换算（`acked = base + ack_local`），
//! `base` 只在两个时刻重锚，与移动端归零点严格一一对应（见 `reanchor` /
//! `reset` 文档）。
//!
//! **背压不扩大缓冲，只换语义**：抑制的作用是「别把数据推进客户端邮箱」——
//! 邮箱（actix mailbox，64 槽）与数据帧共用，控制帧（`ring_resync` /
//! `session_stopped`）满时同样发不出去（`send_text` 走同一条 `try_send`）。
//! 邮箱被数据灌满 ⇒ 重锚信号静默丢失 ⇒ 客户端永远学不到缺口。所以抑制的
//! 真实收益是**给控制帧留出邮箱空间** + 不刷错误日志；抗丢字节的缓冲深度由环
//! 容量决定（`launch::SESSION_PTY_RING_BYTES`），不由窗口决定。
//!
//! ## 尾帧与停止帧
//!
//! `pty:exit` 事件在宿主环摘除**之后**发布（host-pty 单一发布者不变量），
//! 故退出时点的尾帧 = 退出前最后一次成功 drain 已下发的字节；exit 处理仍做
//! 一次尽力 `ring-fetch`（窗口竞态下已摘除 → debug 跳过），随后向该会话的
//! 全部订阅连接下发 `session_stopped` 停止帧（尾帧在前、停止帧在后）。
//!
//! ## 连接生命周期
//!
//! 订阅态只存内存（进程级静态表，wasm 同实例串行）；`client-disconnect` /
//! 插件停用 / `unsubscribe` 都摘除对应状态——无宿主订阅表、无后台任务
//! （全部 drain 同步执行于帧回调与 tick，不 spawn）。`pty:exit` 后连接保留
//! （客户端可重订阅重启后的同 id 会话），但订阅置空、游标归零。

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::ws::{ws_event_topic, WS_CLIENT_DISCONNECT};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostEvents, HostLog, HostPty, HostWebsocket};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;
#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

// 背压裁决与阈值直接复用插件前端拉取路径的**同一纯函数 / 同一对常量**
// （`output` 域）：两条消费路径若各持一套窗口，解释不了「桌面前端不卡而手机卡」。
use crate::output::{decide_pull_gate, PullGate, HIGH_WATER_BYTES, LOW_WATER_BYTES};

/// 端点路径（manifest `contributes.wsEndpoints` 声明，宿主注入命名空间段）
pub const ENDPOINT_PATH: &str = "terminal";

/// 单次 ring-fetch 上限（对齐宿主 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` = 16 KiB；
/// 宿主侧仍会钳位）
const MAX_FETCH_BYTES: u32 = 16 * 1024;
/// 单轮 drain 的最大 fetch 次数（≈128 KiB 预算：每帧回调不无限拉取，
/// 慢消费的积压靠下一轮 drain 续拉）
const MAX_FETCHES_PER_CYCLE: u32 = 8;

/// 连续抑制轮数上限（抗记账错位死锁）：达上限仍等不到 ack 回落 → 强制放行一轮
///
/// 正常背压下每轮抑制都会被下一次 ack 推进（未确认跌破下沿即恢复），所以
/// 「连续 N 轮抑制且水位不动」只可能是记账错位（`base` 换算错 / 客户端 ack
/// 停滞 / 客户端基准归零与插件重锚失配）。此时宁可过量下发
/// ≤128 KiB（邮箱有 64 槽兜底）也不能变成**永久静默**——背压机制的失败模式
/// 必须是「多发」而不是「不发」。
const PARK_MAX_CYCLES: u32 = 4;

// ==================== 交付水位（纯逻辑，native 单测） ====================

/// 单连接 drain 裁决（比 [`PullGate`] 多一态：区分「ack 驱动的放行」与
/// 「抗死锁强制的放行」——两者的可观测性要求完全不同，后者是记账错位告警）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// 放行（未确认在窗口内）
    Allow,
    /// 抗死锁强制放行（连续抑制超上限；**非 0 即记账错位信号**）
    AllowForced,
    /// 抑制：未确认积压达上沿（或驻留中未降到下沿）
    Throttle { unacked: u64 },
}

/// 单连接交付水位：插件环绝对偏移上的 `pushed` / `acked` 两水位 + 驻留态
///
/// **为什么必须换算**：移动端 `ack` 的 offset 是「自最近一次基准归零起的本地
/// 累计字节」（`terminal_link.rs` 在 `subscribed` / `ring_resync` 两处归零），
/// 与环绝对偏移不同源。`base` 记「客户端本地计数归零那一刻的环游标」，
/// `acked = base + ack_local`。两个重锚时刻与客户端的归零点严格一一对应：
///
/// | 插件动作 | 客户端动作 | `base` |
/// |---|---|---|
/// | `subscribe` → `conn.cursor = 0` | 收 `subscribed` → 本地计数归零 | [`Watermark::reset`] |
/// | 发 `ring_resync` → 游标跳新基准 | 收 `ring_resync` → 本地计数归零 | [`Watermark::reanchor`] |
///
/// 换算偏差的两个方向安全性不同（这是 `acked` 取单调 max 的理由）：
/// - `base` 偏**大** → `acked` 偏大 → 未确认偏小 → 少抑制（多发，安全）；
/// - `base` 偏**小** → 多抑制 → 由 `PARK_MAX_CYCLES` 逃生阀兜底（不会永久停）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Watermark {
    /// 客户端本地计数归零时的环游标（ack 换算基准）
    pub base: u64,
    /// 已推送水位（最近一次成功 fetch 的 `next_offset`，单调不回退）
    pub pushed: u64,
    /// 已确认水位（`base + ack_local`，单调不回退：乱序 / 重放的旧 ack 不生效）
    pub acked: u64,
    /// 驻留态（双水位迟滞的中间态）
    pub parked: bool,
    /// 驻留进入次数（**仅状态翻转**累计）
    pub park_count: u64,
    /// 驻留退出次数
    pub unpark_count: u64,
    /// 被抑制的 drain 轮数（诊断：客户端侧「上游没再推」的观测量）
    pub throttled_cycles: u64,
    /// 抗死锁强制放行次数（**非 0 即记账错位的信号**，须告警）
    pub forced_drains: u64,
    /// 连续抑制轮数（逃生阀计数）
    park_cycles: u32,
}

impl Watermark {
    /// 全新订阅（`subscribe`）：水位全部归零
    ///
    /// 客户端本地计数归零 + 游标置 0，旧订阅的 `pushed`/`acked` 已无意义
    /// （会话重启后环偏移也从 0 起——沿用旧水位会把新环误判为「已推送远超
    /// 已确认」而永久驻留）。
    pub fn reset(&mut self) {
        *self = Watermark::default();
    }

    /// 环淘汰重锚（插件自己发出 `ring_resync`）：基准跳到新游标
    ///
    /// 同一环内偏移单调，故 `pushed` / `acked` 保留（仍是绝对偏移）；只换
    /// `base` 并解除驻留——重锚是客户端唯一的自愈动作，抑制它等于把客户端
    /// 锁死在「永远拿不到新数据也永远不 ack」的死锁里（`decide_pull_gate`
    /// 的第一条同向保险：`from_offset < acked` 一律放行）。
    pub fn reanchor(&mut self, next_offset: u64) {
        self.base = next_offset;
        self.parked = false;
        self.park_cycles = 0;
    }

    /// 记录推送水位（单调 max）
    pub fn note_pushed(&mut self, next_offset: u64) {
        self.pushed = self.pushed.max(next_offset);
    }

    /// 记录客户端确认水位：`ack_local` 是**本地累计字节**（非环偏移）
    pub fn note_acked_local(&mut self, ack_local: u64) {
        self.acked = self.acked.max(self.base.saturating_add(ack_local));
    }

    /// 未确认积压（`pushed - acked`，饱和）
    pub fn unacked(&self) -> u64 {
        self.pushed.saturating_sub(self.acked)
    }

    /// 裁决本轮 drain 是否放行（双水位迟滞 + 抗死锁逃生），并推进驻留态与计数
    ///
    /// 裁决本身委托 [`output::decide_pull_gate`]（与插件前端拉取路径同一纯
    /// 函数）；本方法只负责「驻留态翻转计数 + 连续抑制逃生」。
    ///
    /// **逃生时不清 `parked`**：强制放行只是「本轮破例放行」，抑制状态要延续到
    /// ack 真的回落为止。早期实现在这里清 `parked` + 返回 `PullGate::Allow`，
    /// 后果是：① 强放行那一轮被误报为「解除驻留」（掩盖了错位信号）；
    /// ② 下一轮重新计一次 `park_count`（一次驻留被数成 N 次）。
    pub fn gate(&mut self, from_offset: u64) -> Gate {
        match decide_pull_gate(
            self.pushed,
            self.acked,
            from_offset,
            self.parked,
            HIGH_WATER_BYTES,
            LOW_WATER_BYTES,
        ) {
            PullGate::Allow => {
                if self.parked {
                    self.parked = false;
                    self.unpark_count += 1;
                }
                self.park_cycles = 0;
                Gate::Allow
            }
            PullGate::Throttle { unacked } => {
                self.park_cycles = self.park_cycles.saturating_add(1);
                if self.park_cycles > PARK_MAX_CYCLES {
                    // 逃生：连续抑制超上限仍无水位回落 → 本轮破例放行。
                    // 清连续计数但不解除驻留（保证「每 PARK_MAX_CYCLES 轮至少
                    // 放行一次」的前进性，且不虚增 park_count）。
                    self.park_cycles = 0;
                    self.forced_drains += 1;
                    return Gate::AllowForced;
                }
                if !self.parked {
                    // 仅翻转计数：驻留期每轮抑制都置真，不能把「一次驻留」
                    // 数成 N 次（与 output 域同口径）
                    self.parked = true;
                    self.park_count += 1;
                }
                self.throttled_cycles += 1;
                Gate::Throttle { unacked }
            }
        }
    }
}

/// 单连接水位诊断行（`session.output.watermarks` 报告的 `ws` 段）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsWatermarkRow {
    pub endpoint_id: String,
    pub client_id: String,
    pub session_id: String,
    pub base: u64,
    pub pushed: u64,
    pub acked: u64,
    pub unacked: u64,
    pub parked: bool,
    pub park_count: u64,
    pub unpark_count: u64,
    pub throttled_cycles: u64,
    pub forced_drains: u64,
}

/// 环淘汰重锚控制帧载荷：`{type, offset, gapFrom}`
///
/// - `offset`：新基准（客户端据此重锚，本地计数归零）；
/// - `gapFrom`：**缺口起点**（= 被淘汰前的请求游标）。既有移动端实现只读
///   `type` / `offset`，对本字段无感（增量字段，老端忽略未知字段，AGENTS §9）；
///   它让「重锚前到底丢了多少」在诊断面可见——重锚后 `offset` 与客户端
///   本地计数已不同源，缺了这个字段缺口范围就永久不可知。
pub fn ring_resync_payload(gap_from: u64, next_offset: u64) -> serde_json::Value {
    serde_json::json!({
        "type": "ring_resync",
        "offset": next_offset,
        "gapFrom": gap_from,
    })
}

/// 订阅模式：live = 每帧触发 drain + tick 兜底；poll = 仅在客户端 poll/ack 时拉
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalMode {
    Live,
    Poll,
}

/// 单条连接的终端订阅态（进程级；client_id 为注册表键 = 对端地址串）
#[derive(Debug, Clone)]
struct TerminalConnection {
    client_id: String,
    endpoint_id: String,
    /// 订阅的会话（None = 未订阅）
    session_id: Option<String>,
    /// 会话登记域解析出的 PTY 句柄（订阅时解析，会话重启后重订阅再解析）
    pty_id: Option<String>,
    /// 每连接独立输出游标（ring-fetch 的 from_offset 基准）
    cursor: u64,
    /// 交付水位（背压记账；`Watermark` 是 Copy，drain 期间在局部副本上推进，
    /// 轮末 / 早退时整体回写——**不跨宿主调用持锁**）
    watermark: Watermark,
    mode: TerminalMode,
}

#[cfg(target_arch = "wasm32")]
static CONNECTIONS: Mutex<Vec<TerminalConnection>> = Mutex::new(Vec::new());

// ==================== 连接状态 ====================

/// 以锁内可变引用执行操作（不存在 → `None`；锁损坏显性上抛）。
///
/// wasm 单线程（同实例串行）：锁只作进程级静态表的互斥纪律，回调内不再嵌套
/// 取锁（发送等宿主调用不重入本表），闭包执行期间不会产生重入死锁。
#[cfg(target_arch = "wasm32")]
fn with_conn<R>(client_id: &str, f: impl FnOnce(&mut TerminalConnection) -> R) -> Option<R> {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let conn = table.iter_mut().find(|c| c.client_id == client_id)?;
    Some(f(conn))
}

/// 连接接入时登记空状态（client-connect 事件驱动；幂等）
#[cfg(target_arch = "wasm32")]
pub fn on_client_connect(client_id: &str, endpoint_id: &str) {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !table.iter().any(|c| c.client_id == client_id) {
        table.push(TerminalConnection {
            client_id: client_id.to_string(),
            endpoint_id: endpoint_id.to_string(),
            session_id: None,
            pty_id: None,
            cursor: 0,
            watermark: Watermark::default(),
            mode: TerminalMode::Live,
        });
    }
}

/// 连接断开：摘除该连接的订阅态（client-disconnect 事件驱动；幂等）
#[cfg(target_arch = "wasm32")]
pub fn on_client_disconnect(client_id: &str) {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let before = table.len();
    table.retain(|c| c.client_id != client_id);
    if table.len() != before {
        tracing_dbg_disconnect(client_id);
    }
}

/// 插件停用：清空全部订阅态（不残留连接级任务/状态）
#[cfg(target_arch = "wasm32")]
pub fn purge_all() {
    let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !table.is_empty() {
        WasmHost.log_info(&format!(
            "ws terminal: purged {} connection(s)",
            table.len()
        ));
        table.clear();
    }
}

/// 调试留痕（断开清理）
#[cfg(target_arch = "wasm32")]
fn tracing_dbg_disconnect(client_id: &str) {
    WasmHost.log_debug(&format!(
        "ws terminal: connection state dropped (client_id={client_id})"
    ));
}

// ==================== 入站帧处理（events-ws 回调） ====================

/// 帧 JSON 的词表（小写 `type` 字段；未知/畸形 → 显性 error 帧）
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalFrame {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    offset: Option<u64>,
    #[serde(default)]
    data: Option<String>,
}

/// 文本帧分派（订阅/退订/ack/resync/input/poll）——纯状态机，可 native 单测
///
/// 返回给调用方一个「是否需要 drain」标记（任何推进输出的帧之后都要 drain）。
#[cfg(target_arch = "wasm32")]
fn handle_text_frame(
    conn: &mut TerminalConnection,
    frame: &serde_json::Value,
) -> Result<bool, String> {
    let parsed: TerminalFrame = serde_json::from_value(frame.clone())
        .map_err(|e| format!("invalid terminal frame: {e}"))?;
    match parsed.frame_type.as_str() {
        "subscribe" => {
            let session_id = parsed
                .session_id
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "subscribe: missing sessionId".to_string())?;
            let record = crate::session::record_via_host(&session_id)?
                .ok_or_else(|| format!("会话不存在：{session_id}"))?;
            let pty_id = record
                .pty_id
                .ok_or_else(|| format!("会话缺少 PTY 句柄：{session_id}"))?;
            let mode = match parsed.mode.as_deref() {
                Some("poll") => TerminalMode::Poll,
                _ => TerminalMode::Live,
            };
            // 回帧**先发后提交**：subscribed 发不出去（满邮箱）→ 连接留在未订阅态
            // （session_id 未设、水位未清）——客户端从未收到 subscribed，其本地
            // ack 基线未归零，若此时提交订阅态后续输出帧会对不上基线。
            // 发送成功才落状态（新订阅从头拉：历史 = 环驻留窗口内的字节；
            // 水位全归零：客户端收到 subscribed 时也把本地计数归零）
            let reply = serde_json::json!({ "type": "subscribed", "sessionId": session_id, "mode": if mode == TerminalMode::Live { "live" } else { "poll" } });
            send_text(&conn.endpoint_id, &conn.client_id, &reply.to_string())?;
            conn.session_id = Some(session_id);
            conn.pty_id = Some(pty_id);
            conn.cursor = 0;
            conn.watermark.reset();
            conn.mode = mode;
            Ok(true)
        }
        "unsubscribe" => {
            // 回帧先发后提交（与 subscribe 同理由）：unsubscribed 发不出去 →
            // 连接保留订阅态（客户端未收到确认、仍按订阅中自处），避免
            // 单侧清状态后客户端对不上基线
            send_text(
                &conn.endpoint_id,
                &conn.client_id,
                &serde_json::json!({ "type": "unsubscribed" }).to_string(),
            )?;
            conn.session_id = None;
            conn.pty_id = None;
            conn.cursor = 0;
            Ok(false)
        }
        // 流控信号：客户端按**渲染水位**确认已消费的本地累计字节。
        //
        // offset 是**本地计数**（客户端在 `subscribed` / `ring_resync` 时归零），
        // 不是环绝对偏移——`Watermark::note_acked_local` 持 `base` 做换算。
        // ack 不推进游标（游标只由 ring-fetch 推进），只做两件事：
        // ① 推进已确认水位（让未确认窗口回落，解除抑制）；
        // ② 本身作为 drain 触发源（自时钟：每 64 KiB / 最迟 ~375 ms 一帧）。
        "ack" => {
            let offset = parsed
                .offset
                .ok_or_else(|| "ack: missing offset".to_string())?;
            conn.watermark.note_acked_local(offset);
            Ok(true)
        }
        // 客户端已清屏重锚：游标置为客户端给的新基准（ring_resync 后的续拉点）
        "resync" => {
            let offset = parsed
                .offset
                .ok_or_else(|| "resync: missing offset".to_string())?;
            conn.cursor = offset;
            conn.watermark.reanchor(offset);
            Ok(true)
        }
        "input" => {
            let data = parsed
                .data
                .ok_or_else(|| "input: missing data".to_string())?;
            let pty_id = conn
                .pty_id
                .clone()
                .ok_or_else(|| "input: 未订阅会话".to_string())?;
            WasmHost
                .pty_write(&pty_id, data.as_bytes())
                .map_err(|e| format!("host pty write failed: {}", e.message))?;
            // 输入后立即 drain（回声与随输入产生的输出即时返回）
            Ok(true)
        }
        "poll" => Ok(true),
        other => Err(format!("unknown terminal frame type: {other}")),
    }
}

/// 二进制帧 = 原始输入字节（可含控制字符）→ 直写 PTY（特殊键由插件 keys.rs
/// 翻译，但客户端也可直接发送原始控制字节；这里不做二次解释）
#[cfg(target_arch = "wasm32")]
fn handle_binary_frame(conn: &mut TerminalConnection, payload: &[u8]) -> Result<bool, String> {
    let pty_id = conn
        .pty_id
        .clone()
        .ok_or_else(|| "binary input: 未订阅会话".to_string())?;
    WasmHost
        .pty_write(&pty_id, payload)
        .map_err(|e| format!("host pty write failed: {}", e.message))?;
    Ok(true)
}

/// events-ws 服务端域回调（声明端点 `terminal` 的入站帧）——宿主只转原始帧
#[cfg(target_arch = "wasm32")]
pub fn on_client_message(
    endpoint_id: &str,
    client_id: &str,
    kind: &str,
    payload: &[u8],
) -> anyhow::Result<()> {
    // 未登记连接（如 auth 事件先于 client-connect 投递的竞态）→ 惰性登记
    if with_conn(client_id, |_| ()).is_none() {
        on_client_connect(client_id, endpoint_id);
    }
    let need_drain = with_conn(client_id, |conn| {
        let result = match kind {
            "text" => {
                let text = String::from_utf8_lossy(payload);
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(frame) => handle_text_frame(conn, &frame),
                    Err(e) => Err(format!("terminal frame 非 JSON: {e}")),
                }
            }
            "binary" => handle_binary_frame(conn, payload),
            other => Err(format!("unknown frame kind: {other}")),
        };
        match result {
            Ok(need) => need,
            Err(e) => {
                let _ = send_text(
                    endpoint_id,
                    client_id,
                    &serde_json::json!({ "type": "error", "message": e }).to_string(),
                );
                false
            }
        }
    })
    .unwrap_or(false);
    if need_drain {
        drain_for(endpoint_id, client_id).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

// ==================== 输出泵（有界 drain） ====================

/// 对指定连接做一轮有界 drain：背压裁决 → 逐次 ring-fetch 直到追平 / 预算用尽 /
/// 环淘汰（truncated → 先发 `ring_resync` 再照发本块驻留数据）
#[cfg(target_arch = "wasm32")]
fn drain_for(endpoint_id: &str, client_id: &str) -> Result<(), String> {
    // 水位在局部副本上推进，轮末 / 早退时整体回写（不跨宿主调用持连接表锁）
    let (session_id, pty_id, mut cursor, mut watermark) = match with_conn(client_id, |c| {
        (
            c.session_id.clone(),
            c.pty_id.clone(),
            c.cursor,
            c.watermark,
        )
    }) {
        Some((Some(session_id), Some(pty_id), cursor, watermark)) => {
            (session_id, pty_id, cursor, watermark)
        }
        _ => return Ok(()), // 未订阅 / 无句柄：无事可拉
    };
    let mut fetches = 0;
    loop {
        if fetches >= MAX_FETCHES_PER_CYCLE {
            // 本周期预算用尽：下一帧/tick/通知续拉（不无限占用宿主调用链）
            break;
        }
        // 背压：未确认积压达上沿 → 本轮不拉不推（数据留在宿主环内等 ack 回落）。
        // 每轮重裁而不是只在轮首裁一次：轮内多次 fetch 会推高 `pushed`。
        // 驻留 / 解除只在**状态翻转**那一轮打日志（驻留期每轮都打会刷屏）。
        let was_parked = watermark.parked;
        match watermark.gate(cursor) {
            Gate::Allow => {
                if was_parked {
                    WasmHost.log_debug(&format!(
                        "ws terminal: 输出背压解除驻留（client_id={client_id}, session_id={session_id}, unacked={}, low={LOW_WATER_BYTES}）",
                        watermark.unacked()
                    ));
                }
            }
            Gate::AllowForced => {
                // 抗死锁强放行：正常背压下不可能发生（ack 会逐格推进），
                // 发生即意味着 base 换算 / 客户端基准归零与插件重锚失配。
                // **必须 warn**（不是 debug）：这是「背压机制本身出错」的唯一信号。
                WasmHost.log_warn(&format!(
                    "ws terminal: 背压逃生强制放行（client_id={client_id}, session_id={session_id}, 连续 {PARK_MAX_CYCLES} 轮抑制仍无 ack 回落，base={}——基准换算错位?）",
                    watermark.base
                ));
            }
            Gate::Throttle { unacked } => {
                if !was_parked {
                    WasmHost.log_debug(&format!(
                        "ws terminal: 输出背压驻留（client_id={client_id}, session_id={session_id}, unacked={unacked}, high={HIGH_WATER_BYTES}）"
                    ));
                }
                break;
            }
        }
        fetches += 1;
        match WasmHost.pty_ring_fetch(&pty_id, cursor, MAX_FETCH_BYTES) {
            Ok(Some(fetched)) => {
                if fetched.truncated {
                    // 环已淘汰游标之前的字节：显式重锚协议（客户端清屏续拉）。
                    //
                    // **必须先把重锚帧发出去**：客户端的本地计数基准只在重锚时
                    // 归零，发不出去就等于双方基准失配（后续 ack 全是错值）。
                    // 发不出去（邮箱满）时**不持久化本轮任何状态**直接返回：
                    // 截断只可能发生在本轮第一次 fetch（`min_offset` 单调，
                    // 游标一旦 ≥ min_offset 就不再截断），所以此刻没有「已成功
                    // 下发的前缀」需要保留，游标停在旧值 → 下轮重新截断 → 重锚
                    // 帧重发。宁可多拉一轮，不可丢了重锚信号。
                    if let Err(e) = send_text(
                        endpoint_id,
                        client_id,
                        &ring_resync_payload(cursor, fetched.next_offset).to_string(),
                    ) {
                        WasmHost.log_warn(&format!(
                            "ws terminal: ring_resync 发送失败，游标不前进待下轮重试（client_id={client_id}）: {e}"
                        ));
                        return Ok(());
                    }
                    // 本块取到的 [环驻留起点, next_offset) 是**有效连续字节**：
                    // 截断只说明「请求游标之前」有缺口。早前实现此处直接 break
                    // 把本块丢掉而游标已跳过 → 每次重锚都留一个客户端无从知晓
                    // 的 ≤16 KiB 洞（且落点常在转义序列中段 → 脏屏）。
                    watermark.reanchor(fetched.next_offset);
                    cursor = fetched.next_offset;
                }
                if !fetched.data.is_empty() {
                    if let Err(e) =
                        WasmHost.ws_send_binary_to_client(endpoint_id, client_id, &fetched.data)
                    {
                        // 慢客户端/队列满：只停本人（fail-visible debug 留痕），
                        // 不阻塞 PTY 产出与其他连接；游标不动，下轮续拉
                        WasmHost.log_debug(&format!(
                            "ws terminal: send to slow client failed (client_id={client_id}): {}",
                            e.message
                        ));
                        with_conn(client_id, |c| {
                            c.cursor = cursor;
                            c.watermark = watermark;
                        });
                        return Ok(());
                    }
                }
                cursor = fetched.next_offset;
                watermark.note_pushed(fetched.next_offset);
                // 无新数据（fetch 返回空块但未追平）→ 停止本轮，防空转
                if fetched.data.is_empty() {
                    break;
                }
            }
            Ok(None) => break, // 追平（游标 == 产出端）
            Err(e) => {
                let msg = e.to_string();
                // 退出竞态窗口：会话/PTY 已摘除（宿主 `not_found` 固定文案
                // `pty handle not found:` 或属主校验 `not owner:`）。按稳定前缀
                // 分类（不匹配泛化 `not found` 子串——宿主措辞若改，预期中的
                // 退出后竞态会静默重分类为硬 Err 并上抛调用方，session_stopped
                // 尾帧路径整体降级）。宿主错误面是裸串（无 kind/code 可借），
                // 匹配这里锁死的前缀是当前边界下最稳的分类。
                if msg.contains("pty handle not found:") || msg.contains("not owner:") {
                    // 会话/PTY 已摘除（exit 竞态窗口）：drain 到此为止
                    return Ok(());
                }
                return Err(format!("pty ring fetch failed: {msg}"));
            }
        }
    }
    with_conn(client_id, |c| {
        c.cursor = cursor;
        c.watermark = watermark;
    });
    Ok(())
}

/// 宿主限频唤醒驱动：某会话的环有新字节 → 该会话全部 live 订阅连接各拉一轮
///
/// 触发源是宿主 `<owner>::pty:output`（同句柄 ≥50 ms 一条，`host_api/pty_output.rs`）。
/// 此前该通知只唤醒插件**自己的前端**（`output::on_pty_output` → 前端事件），
/// 移动端 WS 路径拿不到 → 空闲期首个字节只能等 1 s 调度 tick。接上之后
/// 「无输入触发的输出」（`sleep 3; echo`、后台构建日志、`tail -f`）的感知延迟
/// 从最坏 1 s 降到 ~50 ms。
#[cfg(target_arch = "wasm32")]
pub fn drain_session(session_id: &str) {
    let targets: Vec<(String, String)> = {
        let table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .filter(|c| c.session_id.as_deref() == Some(session_id) && c.mode == TerminalMode::Live)
            .map(|c| (c.endpoint_id.clone(), c.client_id.clone()))
            .collect()
    };
    run_drains(targets, "output notify");
}

/// 调度 tick 兜底 drain：所有 live 模式订阅连接拉一轮新输出
/// （挂既有 1s scheduler tick，输出延迟 ≤ tick 档位）
#[cfg(target_arch = "wasm32")]
pub fn drain_all_on_tick() {
    let targets: Vec<(String, String)> = {
        let table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .filter(|c| c.session_id.is_some() && c.mode == TerminalMode::Live)
            .map(|c| (c.endpoint_id.clone(), c.client_id.clone()))
            .collect()
    };
    run_drains(targets, "tick");
}

/// 逐连接跑一轮 drain（单连接失败不中断其他连接）
#[cfg(target_arch = "wasm32")]
fn run_drains(targets: Vec<(String, String)>, trigger: &str) {
    for (endpoint_id, client_id) in targets {
        if let Err(e) = drain_for(&endpoint_id, &client_id) {
            WasmHost.log_warn(&format!(
                "ws terminal {trigger} drain failed (client_id={client_id}): {e}"
            ));
        }
    }
}

/// 水位诊断行快照（`session.output.watermarks` 的 `ws` 段）
///
/// **只列已订阅连接**：未订阅连接没有交付语义，列出来只会让诊断噪声压过真实行。
/// 纯读：不建条目、不改状态。
#[cfg(target_arch = "wasm32")]
pub fn watermark_rows() -> Vec<WsWatermarkRow> {
    let table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    table
        .iter()
        .filter_map(|c| {
            let session_id = c.session_id.clone()?;
            Some(WsWatermarkRow {
                endpoint_id: c.endpoint_id.clone(),
                client_id: c.client_id.clone(),
                session_id,
                base: c.watermark.base,
                pushed: c.watermark.pushed,
                acked: c.watermark.acked,
                unacked: c.watermark.unacked(),
                parked: c.watermark.parked,
                park_count: c.watermark.park_count,
                unpark_count: c.watermark.unpark_count,
                throttled_cycles: c.watermark.throttled_cycles,
                forced_drains: c.watermark.forced_drains,
            })
        })
        .collect()
}

// ==================== PTY 退出 → 尾帧 + 停止帧 ====================

/// 会话终止（`<owner>::pty:exit` 驱动，lib.rs 在 session::on_pty_exit 后调用）：
/// 尽力尾帧 fetch（环可能已摘除）→ 向该会话全部订阅连接下发 `session_stopped`
/// 停止帧（tail 在前、停止帧在后）；订阅置空、游标归零（客户端可重订阅）
#[cfg(target_arch = "wasm32")]
pub fn on_session_terminated(session_id: &str, reason: &str, exit_code: Option<i32>) {
    // 锁内只拷贝状态、锁外做宿主调用（drain_for 同款规矩）：
    // pty_ring_fetch / ws_send_* 是宿主调用，持 CONNECTIONS 锁执行会在
    // 单线程运行时下被宿主重入（send 触发 client-disconnect）卡死整张表
    let mut snapshots: Vec<(String, String, String, String, u64)> = {
        let mut table = CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter_mut()
            .filter(|c| c.session_id.as_deref() == Some(session_id))
            .map(|c| {
                let snapshot = (
                    c.endpoint_id.clone(),
                    c.client_id.clone(),
                    c.pty_id.clone().unwrap_or_default(),
                    session_id.to_string(),
                    c.cursor,
                );
                c.session_id = None;
                c.pty_id = None;
                c.cursor = 0;
                c.watermark.reset();
                snapshot
            })
            .collect()
    };
    // 锁外：尽力尾帧（环可能在最后一次 fetch 后被摘除 → 跳过）
    for (endpoint_id, client_id, pty_id, sid, cursor) in &mut snapshots {
        if pty_id.is_empty() {
            continue;
        }
        match WasmHost.pty_ring_fetch(pty_id, *cursor, MAX_FETCH_BYTES) {
            Ok(Some(fetched)) => {
                // 退出路径的**截断必须上报**：慢客户端在退出时点已被
                // 淘汰过，尾帧就是从环中段开始的；不补 `ring_resync`
                // 客户端就会把「半截屏 + session_stopped」当成完整
                // 终态（这是尾帧路径独有的静默缺口——drain 路径有
                // 截断必重锚，退出路径早前漏了）。
                if fetched.truncated {
                    if let Err(e) = send_text(
                        endpoint_id,
                        client_id,
                        &ring_resync_payload(*cursor, fetched.next_offset).to_string(),
                    ) {
                        WasmHost.log_warn(&format!(
                            "ws terminal: 退出尾帧 ring_resync 发送失败（client_id={client_id}）: {e}"
                        ));
                    }
                    *cursor = fetched.next_offset;
                }
                if !fetched.data.is_empty() {
                    if let Err(e) =
                        WasmHost.ws_send_binary_to_client(endpoint_id, client_id, &fetched.data)
                    {
                        WasmHost.log_warn(&format!(
                            "ws terminal: 退出尾帧下发失败（client_id={client_id}, bytes={}）: {}",
                            fetched.data.len(),
                            e.message
                        ));
                    }
                    *cursor = fetched.next_offset;
                }
            }
            Ok(None) => {}
            Err(e) => {
                WasmHost.log_debug(&format!(
                    "ws terminal: 退出尾帧 fetch 失败（跳过，client_id={client_id}）: {}",
                    e.message
                ));
            }
        }
    }
    // 锁外：停止帧（tail 在前、停止帧在后）
    for (endpoint_id, client_id, _, sid, _) in snapshots {
        let mut payload = serde_json::json!({
            "type": "session_stopped",
            "sessionId": sid,
            "reason": reason,
        });
        if let Some(code) = exit_code {
            payload["exitCode"] = serde_json::json!(code);
        }
        // 停止帧发不出去 = 客户端永远等不到终态（会一直转圈）——必须留痕
        if let Err(e) = send_text(&endpoint_id, &client_id, &payload.to_string()) {
            WasmHost.log_warn(&format!(
                "ws terminal: session_stopped 帧发送失败（session_id={sid}, client_id={client_id}）: {e}"
            ));
            continue;
        }
        WasmHost.log_info(&format!(
            "ws terminal: session stopped frame sent (session_id={sid}, client_id={client_id}, reason={reason})"
        ));
    }
}

// ==================== 发送工具 ====================

#[cfg(target_arch = "wasm32")]
fn send_text(endpoint_id: &str, client_id: &str, text: &str) -> Result<(), String> {
    WasmHost
        .ws_send_text_to_client(endpoint_id, client_id, text)
        .map_err(|e| format!("ws terminal: send-text-to-client failed: {}", e.message))
}

// ==================== native：wasm 专属路径为空实现 ====================

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_message(
    _endpoint_id: &str,
    _client_id: &str,
    _kind: &str,
    _payload: &[u8],
) -> anyhow::Result<()> {
    anyhow::bail!("ws terminal unavailable outside wasm runtime")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_connect(_client_id: &str, _endpoint_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_client_disconnect(_client_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn purge_all() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn drain_all_on_tick() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn drain_session(_session_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn watermark_rows() -> Vec<WsWatermarkRow> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_session_terminated(_session_id: &str, _reason: &str, _exit_code: Option<i32>) {}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- 测试工具：按花括号配平提取函数体（结构锁用） ----------

    /// 从 `signature` 起按花括号配平提取函数体文本
    ///
    /// 为什么不用「取到下一个 `#[cfg]`」这类切片：函数顺序一变就假红。
    /// 配平提取只认签名与花括号，函数搬家不影响断言。
    fn fn_body(src: &str, signature: &str) -> String {
        let start = src
            .find(signature)
            .unwrap_or_else(|| panic!("source must contain {signature}"));
        let bytes = src.as_bytes();
        let mut depth = 0usize;
        let mut in_str = false;
        let mut i = start;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return src[start..=i].to_string();
                    }
                }
                // 跳过字符串字面量里的花括号（错误文案里带 `{client_id}`）
                b'"' if !in_str => in_str = true,
                b'"' if in_str => in_str = false,
                _ => {}
            }
            i += 1;
        }
        panic!("unbalanced braces after {signature}");
    }

    fn source() -> String {
        std::fs::read_to_string(format!("{}/src/ws_terminal.rs", env!("CARGO_MANIFEST_DIR")))
            .expect("read ws_terminal.rs")
    }

    // ==================== 交付水位（背压状态机） ====================

    /// W1 ack 换算：客户端回发的是**本地累计字节**，`acked = base + ack_local`
    #[test]
    fn ack_local_offset_is_translated_through_base() {
        let mut wm = Watermark::default();
        wm.note_pushed(10_000);
        wm.note_acked_local(4_000);
        assert_eq!(wm.acked, 4_000, "base=0 时换算是恒等映射");
        assert_eq!(wm.unacked(), 6_000);
    }

    /// W2 水位单调：乱序 / 重放的旧 ack 不让已确认水位回退
    #[test]
    fn acked_watermark_is_monotonic() {
        let mut wm = Watermark::default();
        wm.note_pushed(1_000);
        wm.note_acked_local(800);
        wm.note_acked_local(200); // 重放的旧 ack：忽略
        wm.note_pushed(500); // 推送水位同样单调
        assert_eq!((wm.pushed, wm.acked, wm.unacked()), (1_000, 800, 200));
    }

    /// W3 重锚换基准：`ring_resync` 后客户端本地计数归零，ack 必须按新 base 解释
    ///
    /// 反例（不换算的后果）：重锚到 100_000 后客户端回发 4_000，若直接当绝对
    /// 偏移读，已确认水位会比真实进度小 96_000 → 永久误判为「未确认巨大」。
    #[test]
    fn reanchor_moves_ack_base_and_clears_parked() {
        let mut wm = Watermark::default();
        wm.note_pushed(200_000);
        wm.note_acked_local(4_000);
        wm.gate(200_000); // 未确认远超上沿 → 驻留
        assert!(wm.parked, "前置条件：已驻留");

        wm.reanchor(100_000);
        assert_eq!(wm.base, 100_000);
        assert!(!wm.parked, "重锚是客户端唯一的自愈动作，必须同时解除驻留");
        // 客户端从新基准起算：本地 4_000 = 绝对 104_000
        wm.note_acked_local(4_000);
        assert_eq!(wm.acked, 104_000);
        assert_eq!(wm.unacked(), 96_000, "已推送 200_000 - 已确认 104_000");
    }

    /// W4 全新订阅归零：旧订阅的驻留态不得遗留给新订阅（否则新环永久静默）
    #[test]
    fn reset_clears_all_state_for_a_fresh_subscription() {
        let mut wm = Watermark::default();
        wm.note_pushed(500_000);
        wm.note_acked_local(0);
        wm.gate(500_000);
        assert!(wm.parked, "前置条件：旧订阅已驻留");

        wm.reset();
        assert_eq!(wm, Watermark::default());
        assert_eq!(
            wm.gate(0),
            Gate::Allow,
            "会话重启后环偏移从 0 起，旧水位会把新环误判为「已推送远超已确认」"
        );
    }

    /// W4 背压生效：未确认积压达上沿 → 抑制本轮 drain
    #[test]
    fn gate_throttles_when_unacked_reaches_high_water() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES);
        assert_eq!(
            wm.gate(HIGH_WATER_BYTES),
            Gate::Throttle {
                unacked: HIGH_WATER_BYTES
            }
        );
        assert!(wm.parked);
        assert_eq!(wm.park_count, 1);
        assert_eq!(wm.throttled_cycles, 1);
    }

    /// W6 驻留计数只在翻转时累计：驻留期 N 轮抑制 → parkCount 仍为 1
    #[test]
    fn park_count_only_counts_state_flips() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES);
        for _ in 0..3 {
            assert!(matches!(wm.gate(HIGH_WATER_BYTES), Gate::Throttle { .. }));
        }
        assert_eq!(wm.park_count, 1, "一次驻留不得被数成 3 次震荡");
        assert_eq!(wm.throttled_cycles, 3, "抑制轮数照实累计");
    }

    /// W7 迟滞 + ack 推进 → 解除驻留（消费驱动的恢复）
    #[test]
    fn ack_advance_unparks() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES);
        wm.gate(HIGH_WATER_BYTES);
        assert!(wm.parked);

        // 客户端确认到下沿以下（未确认 = 32 KiB < LOW）
        wm.note_acked_local(HIGH_WATER_BYTES - 32 * 1024);
        assert_eq!(wm.unacked(), 32 * 1024);
        assert_eq!(wm.gate(HIGH_WATER_BYTES), Gate::Allow);
        assert!(!wm.parked);
        assert_eq!(wm.unpark_count, 1);
    }

    /// W8 迟滞带内不解除：驻留中未确认落在 [LOW, HIGH) 仍抑制（防震荡）
    ///
    /// 游标取客户端真实位置（= 已推送端）：若传 0 就会命中「游标落后于 acked
    /// 一律放行」那条规则，测不到迟滞带。
    #[test]
    fn hysteresis_band_keeps_parked() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES);
        wm.gate(HIGH_WATER_BYTES);
        wm.note_acked_local(HIGH_WATER_BYTES - LOW_WATER_BYTES); // 未确认 = LOW
        assert_eq!(wm.unacked(), LOW_WATER_BYTES);
        assert!(
            matches!(wm.gate(HIGH_WATER_BYTES), Gate::Throttle { .. }),
            "未确认恰在下沿 = 仍在带内，不得反复进出驻留"
        );
        assert!(wm.parked, "带内保持驻留");
    }

    /// W9 重锚 / 游标回退不可抑制（死锁保险）：`from_offset < acked` 一律放行
    ///
    /// 反例：若游标落后于已确认水位也被抑制，客户端永远拿不到数据 → 永远不
    /// ack → 永远解不开驻留（自锁）。
    #[test]
    fn cursor_behind_acked_is_always_allowed() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES * 4);
        wm.note_acked_local(4_000);
        wm.gate(HIGH_WATER_BYTES * 4);
        assert!(wm.parked, "前置条件：已驻留");
        assert_eq!(wm.gate(0), Gate::Allow, "游标落后于已确认水位时必须放行");
        assert!(!wm.parked);
    }

    /// W10 抗死锁逃生：连续抑制超上限 → 强制放行；且**周期性**重复（前进性）
    ///
    /// 背压机制的失败模式必须是「多发」而不是「不发」：基准换算错位 / 客户端
    /// ack 停滞都不允许把链路锁成永久静默。
    #[test]
    fn forced_drain_escape_hatch_preserves_forward_progress() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES * 4);
        // 客户端永不 ack（ack 停滞）：连续抑制
        for _ in 0..PARK_MAX_CYCLES {
            assert!(matches!(wm.gate(0), Gate::Throttle { .. }));
        }
        assert_eq!(wm.forced_drains, 0, "上限内不得提前逃生");
        assert_eq!(
            wm.gate(0),
            Gate::AllowForced,
            "连续 {PARK_MAX_CYCLES} 轮抑制后必须强制放行一轮（三态独立可观测）"
        );
        assert_eq!(wm.forced_drains, 1);
        // 逃生**不解除驻留**：抑制要延续到 ack 真的回落为止（否则会把「一次
        // 驻留」数成多次，且掩盖「这是强放行不是 ack 驱动的恢复」）
        assert!(wm.parked, "强放行只破例一轮，驻留态延续");
        assert_eq!(wm.park_count, 1, "一次驻留不得因逃生被数成多次");
        assert_eq!(wm.unpark_count, 0, "强放行不是 ack 驱动的解除驻留");
        // 前进性：再抑制满一轮又强制放行（不是「只救一次」）
        for _ in 0..PARK_MAX_CYCLES {
            assert!(matches!(wm.gate(0), Gate::Throttle { .. }));
        }
        assert_eq!(wm.gate(0), Gate::AllowForced);
        assert_eq!(wm.forced_drains, 2);
        assert_eq!(wm.park_count, 1);
    }

    /// W10b 逃生之后 ack 真的推进 → 回到正常放行（强放行不是永久态）
    #[test]
    fn ack_after_forced_drain_resumes_normal_release() {
        let mut wm = Watermark::default();
        wm.note_pushed(HIGH_WATER_BYTES * 4);
        for _ in 0..=PARK_MAX_CYCLES {
            let _ = wm.gate(0);
        }
        assert!(
            wm.parked && wm.forced_drains >= 1,
            "前置条件：已强放行且仍驻留"
        );

        // 客户端终于追上来（未确认降到下沿以下）
        wm.note_acked_local(HIGH_WATER_BYTES * 4 - LOW_WATER_BYTES / 2);
        assert_eq!(wm.gate(HIGH_WATER_BYTES * 4), Gate::Allow);
        assert!(!wm.parked, "ack 推进后应真正解除驻留");
        assert_eq!(wm.unpark_count, 1);
    }

    // ==================== 重锚控制帧 ====================

    /// P1 重锚帧形状：`type` / `offset` 与移动端解析器逐字一致（老端忽略新增字段）
    #[test]
    fn ring_resync_payload_shape_is_client_compatible() {
        let payload = ring_resync_payload(1_000, 20_000);
        assert_eq!(payload["type"], "ring_resync");
        assert_eq!(payload["offset"], 20_000, "新基准 = 客户端重锚点");
        assert_eq!(
            payload["gapFrom"], 1_000,
            "缺口起点：重锚后 offset 与本地计数不同源，缺口范围靠它才可查"
        );
    }

    // ==================== 结构锁（三处修复不可静默回退） ====================

    /// S1 `drain_for` 的截断分支：**先发重锚帧 → 再发本块数据**，中间不得 break
    ///
    /// 锁住的具体缺陷：早前实现把游标跳到 `next_offset` 后直接 `break`，
    /// 本块取到的驻留字节从未下发而游标已跳过 → 每次重锚留一个客户端无从知晓
    /// 的 ≤16 KiB 洞。行为路径是 wasm-only（native 跑不到），故钉源码结构。
    #[test]
    fn drain_sends_reanchor_frame_before_and_data_after() {
        let body = fn_body(&source(), "fn drain_for(");
        let resync = body
            .find("ring_resync_payload(")
            .unwrap_or_else(|| panic!("drain_for 必须用 ring_resync_payload 发重锚帧"));
        let data_send = body
            .find("ws_send_binary_to_client(")
            .unwrap_or_else(|| panic!("drain_for 必须下发本块数据"));
        assert!(
            resync < data_send,
            "重锚帧必须先于数据下发（客户端靠它先清屏再收新基准的字节）"
        );
        let between = &body[resync..data_send];
        assert!(
            !between.contains("break;"),
            "重锚与补发数据之间不得 break：截断块里的驻留字节是有效连续字节，\
             跳过它就是客户端无从知晓的字节洞（实测缺陷）"
        );
    }

    /// S2 `drain_for` 环淘汰时先发重锚帧，发不出去就**不推进游标**（保重试）
    #[test]
    fn failed_reanchor_frame_does_not_advance_cursor() {
        let body = fn_body(&source(), "fn drain_for(");
        let resync = body.find("ring_resync_payload(").expect("重锚帧");
        let reanchor = body
            .find("watermark.reanchor(")
            .unwrap_or_else(|| panic!("重锚成功后必须推进基准"));
        let persist = body
            .find("c.watermark = watermark")
            .unwrap_or_else(|| panic!("轮末必须回写水位"));
        assert!(
            resync < reanchor && reanchor < persist,
            "重锚帧发送成功 → 推进基准 → 轮末回写；顺序颠倒会让客户端基准失配"
        );
        // 发送失败分支必须早退且不回写（游标停在旧值 → 下轮重新截断 → 重锚重发）
        let fail_branch = &body[resync..reanchor];
        assert!(
            fail_branch.contains("ring_resync 发送失败") && fail_branch.contains("return Ok(())"),
            "重锚帧发不出去必须显性留痕并早退，不得静默推进游标"
        );
    }

    /// S3 退出路径必须上报截断（尾帧缺口不得静默）
    #[test]
    fn exit_path_reports_truncation_before_stopped_frame() {
        let body = fn_body(&source(), "pub fn on_session_terminated(");
        let fetch = body.find("pty_ring_fetch(").expect("尾帧 fetch");
        let resync = body.find("ring_resync_payload(").unwrap_or_else(|| {
            panic!("退出路径必须检查 truncated 并发重锚帧（实测缺陷：尾帧缺口静默）")
        });
        let tail_send = body.find("ws_send_binary_to_client(").expect("尾帧下发");
        assert!(
            fetch < resync && resync < tail_send,
            "顺序：fetch → 重锚帧 → 尾帧 → 停止帧"
        );
        // 停止帧发不出去必须留痕（客户端会一直等终态）
        assert!(
            body.contains("session_stopped 帧发送失败"),
            "停止帧发送失败必须留痕，不得静默丢弃（客户端会永远等不到终态）"
        );
    }

    /// S4 背压接线：ack 真参与记账 + 三个 drain 触发源齐备
    #[test]
    fn ack_advances_watermark_instead_of_being_discarded() {
        let body = fn_body(&source(), "fn handle_text_frame(");
        assert!(
            body.contains("conn.watermark.note_acked_local(offset)"),
            "ack offset 必须推进已确认水位（背压记账），不得再丢弃"
        );
        assert!(
            !body.contains("let _offset ="),
            "旧的「解析后丢弃 ack offset」写法不得复活"
        );
        // 三个重锚/触发点
        assert!(
            body.contains("conn.watermark.reset()"),
            "subscribe 必须归零水位"
        );
        assert!(
            body.contains("conn.watermark.reanchor(offset)"),
            "resync 帧必须换基准"
        );
    }

    /// S5 宿主限频唤醒必须驱动 WS drain（否则移动端空闲期只能等 1s tick）
    #[test]
    fn lib_routes_output_notify_to_ws_drain() {
        let src = std::fs::read_to_string(format!("{}/src/lib.rs", env!("CARGO_MANIFEST_DIR")))
            .expect("read lib.rs");
        let route = fn_body(&src, "fn on_message(");
        assert!(
            route.contains("output::on_pty_output(&msg.payload)"),
            "限频唤醒必须仍转成前端事件"
        );
        assert!(
            route.contains("ws_terminal::drain_session("),
            "同一份限频唤醒必须驱动 WS 连接 drain（实测延迟档：空闲后首个字节最坏 1s）"
        );
    }

    /// S6 会话 PTY 必须声明环容量（省略 → 退回宿主默认 256 KiB）
    #[test]
    fn spawn_declares_session_ring_bytes() {
        let src = std::fs::read_to_string(format!("{}/src/launch.rs", env!("CARGO_MANIFEST_DIR")))
            .expect("read launch.rs");
        let body = fn_body(&src, "pub fn spawn_session(");
        assert!(
            body.contains(".ring_bytes(SESSION_PTY_RING_BYTES)"),
            "spawn 必须声明环容量（环是拉取模型下唯一有界缓冲，省略 = 256 KiB 浅环）"
        );
        // 声明值不得小于背压窗口的两倍（否则抑制无意义）——运行时复核编译期断言
        assert!(
            crate::launch::SESSION_PTY_RING_BYTES >= HIGH_WATER_BYTES * 2,
            "背压窗口必须 ≤ 环容量 / 2"
        );
    }

    // ==================== 既有契约（保持） ====================

    fn frame(r#type: &str) -> serde_json::Value {
        serde_json::json!({ "type": r#type })
    }

    /// 文本帧解析：subscribe 缺 sessionId / 未知类型 / 非法 JSON → 显性错误
    #[test]
    fn text_frame_parsing_rejects_bad_shapes() {
        // handle_text_frame 是 wasm 专用（走 WasmHost）；native 侧解析器
        // TerminalFrame 与词表分支由 serde 形状锁覆盖：
        let parsed: Result<TerminalFrame, _> = serde_json::from_value(frame("subscribe"));
        assert!(parsed.is_ok(), "subscribe 帧形状可解析");
        let parsed: Result<TerminalFrame, _> =
            serde_json::from_value(serde_json::json!({"type": "ack"}));
        assert!(parsed.is_ok());
        let parsed: Result<TerminalFrame, _> = serde_json::from_value(serde_json::json!({}));
        assert!(parsed.is_err(), "缺 type 字段拒绝");
    }

    /// 模式缺省 live / 显式 poll（帧解析形状锁）
    #[test]
    fn mode_defaults_to_live() {
        let parsed: TerminalFrame = serde_json::from_value(serde_json::json!({
            "type": "subscribe", "sessionId": "s1"
        }))
        .unwrap();
        assert!(parsed.mode.is_none(), "缺省无 mode 字段");
        let parsed: TerminalFrame = serde_json::from_value(serde_json::json!({
            "type": "subscribe", "sessionId": "s1", "mode": "poll"
        }))
        .unwrap();
        assert_eq!(parsed.mode.as_deref(), Some("poll"));
    }

    /// 结构锁一：终端协议生产路径**不经宿主会话/终端业务类型**——本文件实现段
    /// 不得出现 `Message::` / `SyncEvent::` / `SessionControlAction` / `hostBroadcastSessionId`
    #[test]
    fn ws_terminal_has_no_host_business_types() {
        let root = env!("CARGO_MANIFEST_DIR");
        let src = std::fs::read_to_string(format!("{root}/src/ws_terminal.rs"))
            .expect("read ws_terminal.rs");
        let implementation = src.split("#[cfg(test)]").next().unwrap_or(&src);
        let mut violations: Vec<String> = Vec::new();
        for (idx, raw) in implementation.lines().enumerate() {
            let line = raw.trim_start();
            if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
                continue;
            }
            for marker in [
                "Message::",
                "SyncEvent::",
                "SessionControlAction",
                "hostBroadcastSessionId",
                "WatchMode",
                "SessionStopped",
            ] {
                if line.contains(marker) {
                    violations.push(format!("{}:{}: {}", "ws_terminal.rs", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            violations.is_empty(),
            "ws_terminal 实现段不得出现宿主业务类型（票 04）：\n{}",
            violations.join("\n")
        );
    }
}
