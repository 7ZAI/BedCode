//! # 终端链路状态机（票 12，自宿主 `terminal_link.rs` 等价迁移）
//!
//! 每会话一条链路（session_id → LinkState）；驱动源三类：
//! 1. **命令**（prewarm / subscribe / unsubscribe / send-input / ack-rendered，前端触发）
//! 2. **连接事实**（ws:open / ws:close / ws:reconnect-scheduled，宿主传输面）
//! 3. **下行帧**（ws:message 二进制信封：控制帧 JSON + 输出帧裸字节）
//!
//! ## 生命周期（进/出终端页）
//!
//! - 进页前 `prewarm`：只建连（宿主代发 auth），**不发 subscribe 帧**——桌面端每次
//!   subscribe 都从环窗口起点整段重放，预热时页面通道尚未登记，走 subscribe 预热
//!   等于白传一遍（长会话可达数 MB）。
//! - 进页 `subscribe`（fresh subscribe 语义）：发 subscribe 帧 → 桌面端回 `subscribed`
//!   → 整段重放环窗口。**重放前是否清屏**由本状态机按「页面是否已收到过该会话输出」
//!   （`LinkManager.page_content`）判定并发 `terminal-resync{reason:"replay"}`——重订阅、
//!   断线重连、会话重启三条路径都会重放，不先清屏就是两代内容叠加（重复显示）。
//! - 离开终端页 `unsubscribe`：显式 close（同时取消宿主 auto-reconnect）。
//!
//! ## 与退役前实现的差异（行为等价性见票 12 §6.3/§9）
//!
//! - 重连退避由宿主 auto-reconnect 承担（`ReconnectManager` 全局单一事实源）：
//!   断开 → 宿主退避重建 → `ws:open`（新句柄 + `reconnectedFrom` 旧句柄）→
//!   本状态机重新订阅（prewarm 未订阅过的链路不重订阅——无订阅可恢复）；
//!   `session_missing` 重试复用同一循环（主动 close → 宿主退避 → 重连重订阅）。
//! - ack 空闲兜底（250ms 轮询）退役：前端 onWriteParsed/rAF 持续推进 ack，
//!   64KB 阈值节流保留。
//! - 「页面订阅门控」（段2 frontend_subscribed）由宿主窄转发层承载：
//!   `terminal_stream_forward_output` 对未订阅页面返回 Err，字节就地丢弃
//!   （与原门控丢弃语义一致，重进页面经重订阅回放补齐）。

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use bedcode_plugin_api_mobile::host::ws::WS_FRAME_KIND_TEXT;
use bedcode_plugin_api_mobile::host::{HostEvents, HostLog, HostTerminalStream, HostWs};
use bedcode_plugin_api_mobile::wasm_host::WasmHost;

use crate::protocol::{
    build_ack_frame, build_subscribe_frame, build_input_text_frame, classify_server_error,
    plan_input_frames, should_send_ack, InputFrame, ServerErrorClass, MAX_SESSION_MISSING_STRIKES,
};
use crate::{read_primary_target, PLUGIN_ID};

// ==================== 事件名（前端 listen；payload 形状与退役前逐字段一致） ====================

/// 状态变更事件（phase / detail / retry_in_ms）
pub(crate) static EVENT_TERMINAL_STATE: LazyLock<String> =
    LazyLock::new(|| format!("plugin:{PLUGIN_ID}:terminal-state"));
/// 重锚事件（重放世代切换 / 环淘汰：前端清屏 + 本地计数归零 + 一次性提示）
pub(crate) static EVENT_TERMINAL_RESYNC: LazyLock<String> =
    LazyLock::new(|| format!("plugin:{PLUGIN_ID}:terminal-resync"));

/// 自动重连 config（宿主退避参数；数值对齐退役前 `DEFAULT_INITIAL_DELAY_MS` /
/// `DEFAULT_MAX_DELAY_MS`，下限钳制在宿主侧强制——1s 下限是自愈风暴教训）
const AUTO_RECONNECT_CONFIG: &str = r#"{"baseMs":1000,"maxMs":30000}"#;

// ==================== 阶段 ====================

/// 连接/订阅阶段（新协议：无独立 history 阶段——收 `subscribed` 即 live）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkPhase {
    Idle,
    Connecting,
    Auth,
    Live,
}

impl LinkPhase {
    fn as_api_str(self) -> &'static str {
        match self {
            LinkPhase::Idle => "idle",
            LinkPhase::Connecting => "connecting",
            LinkPhase::Auth => "auth",
            LinkPhase::Live => "live",
        }
    }
}

// ==================== 单会话链路状态 ====================

/// 会话级终端链路（每会话一个实例；管理器持有）
pub(crate) struct LinkState {
    session_id: String,
    /// WS 连接句柄（None = 无连接：未建 / 断开等宿主重连 / 已停止）
    handle: Option<String>,
    /// 最近一次连接的句柄（宿主重连成功事件 `reconnectedFrom` 的匹配键）
    last_handle: Option<String>,
    phase: LinkPhase,
    /// 本链路是否已停止（不再重连；重建由 subscribe 负责）
    stopped: bool,
    /// 已收到 `subscribed`：门控——此前到达的输出帧一律丢弃（fresh subscribe
    /// 前的旧流残留，重播会覆盖全部内容）
    subscribe_ack: bool,
    /// 本地已收字节（统计 + ack 记账；subscribed/ring_resync 后归零）
    cursor: u64,
    /// 前端已渲染字节（ack-rendered 命令推进；ack 帧 offset 值）
    frontend_rendered: u64,
    /// 累计待 ack 字节（64KB 阈值节流）
    pending_ack: u64,
    /// 会话不存在连续重试计数
    session_missing: u32,
    /// 本次 `subscribed` 回包后需发 terminal-resync（重订阅/重连/停止后重建）：
    /// 前端可能已有在屏内容，重播前必须清屏 + 本地计数基准重置
    pending_resync: bool,
    /// 断开等待宿主重连中（close 事实已到、open 尚未到）
    waiting_reconnect: bool,
    /// 只建连未订阅（`prewarm`）：恢复路径不发 subscribe 帧（无订阅可恢复，
    /// 且预热期的重放注定被丢弃——页面通道尚未登记）；真正的订阅由页面挂载
    /// 后的 `subscribe` 发出（它会把本标志清掉）
    prewarmed: bool,
}

impl LinkState {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            handle: None,
            last_handle: None,
            phase: LinkPhase::Connecting,
            stopped: false,
            subscribe_ack: false,
            cursor: 0,
            frontend_rendered: 0,
            pending_ack: 0,
            session_missing: 0,
            pending_resync: false,
            waiting_reconnect: false,
            prewarmed: false,
        }
    }

    /// 重置订阅基准（fresh subscribe 前清零：旧流残留不得计入新基准）
    fn reset_subscription_base(&mut self) {
        self.subscribe_ack = false;
        self.cursor = 0;
        self.frontend_rendered = 0;
        self.pending_ack = 0;
    }
}

// ==================== 全局管理器 ====================

/// 建连（宿主代发 auth）——`subscribe` / `prewarm` 共用，避免两条建连路径漂移。
///
/// 目标 = 主连接桌面（host-connection 原语）；`jwtAuth` = 宿主代发首消息认证
/// （token 不落插件，C4）；`autoReconnect` = 宿主退避重建（本插件无时钟）。
/// 失败（桌面未连 / 无目标）上抛给调用方。
fn connect_link(h: &WasmHost) -> anyhow::Result<String> {
    let target = read_primary_target()?;
    let config = serde_json::json!({
        "url": format!("ws://{}:{}{}", target.address, target.port, crate::TERMINAL_WS_PATH),
        "jwtAuth": true,
        "autoReconnect": serde_json::from_str::<serde_json::Value>(AUTO_RECONNECT_CONFIG).expect("static json"),
    });
    h.ws_connect(config.to_string().as_str())
        .map_err(|e| anyhow::anyhow!("terminal ws connect failed: {e}"))
}

/// 全局链路管理器（WASM 单线程环境；lib.rs 经 `LINK_MANAGER` 访问）
pub(crate) static LINK_MANAGER: LazyLock<Mutex<LinkManager>> = LazyLock::new(|| {
    Mutex::new(LinkManager {
        links: HashMap::new(),
        page_content: HashSet::new(),
    })
});

/// 终端链路管理器
#[derive(Default)]
pub(crate) struct LinkManager {
    links: HashMap<String, LinkState>,
    /// 已向页面转发过输出的会话 = **页面在屏内容事实**。
    ///
    /// 用途：`subscribed` = 桌面端即将（或正在）从环窗口起点**整段重放**；若页面
    /// 已有上一代内容（重订阅 / 断线重连 / 会话重启），必须随之发 `terminal-resync`
    /// 让前端先清屏，否则两代内容叠加 = 重复显示。
    ///
    /// 键是 session id（跨链路实例保留：会话重启后页面仍显示旧终屏内容，新链路
    /// 同样需要清屏）。页面离开后不清理：多一次空清屏（前端 `hasRenderedContent`
    /// 会跳过）比漏清（重放叠加渲染）安全得多；会话删除时才随链路一并清理。
    page_content: HashSet<String>,
}

impl LinkManager {
    // ==================== 命令面（前端触发） ====================

    /// 订阅会话（fresh subscribe 语义）：
    /// - 链路运行中 → 重发 subscribe 帧（桌面端游标归零、**整段重播环窗口**）
    /// - 已停止 / 不存在 → 建连（宿主代发 auth）→ 发 subscribe 帧
    ///
    /// 每次调用都是一次完整重放（桌面端 fresh subscribe 语义），所以前端**一页
    /// 一代只应调用一次**：重复订阅 = 重复传输 + 多一次清屏重绘。重放前的清屏由
    /// `subscribed` 处理按「页面是否已有内容」判定并发 `terminal-resync`。
    ///
    /// 连接失败（桌面未连 / 无目标）→ Err 上抛，由前端既有订阅重试路径
    /// （3s 定时）驱动重试；成功路径的确认经状态事件异步到达。
    pub(crate) fn subscribe(h: &WasmHost, session_id: &str) -> anyhow::Result<()> {
        if session_id.is_empty() {
            return Ok(());
        }
        let frame = build_subscribe_frame(session_id);
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        if let Some(link) = mgr.links.get_mut(session_id) {
            if !link.stopped {
                if let Some(handle) = link.handle.clone() {
                    // 链路已在运行：fresh subscribe（桌面端重播环窗口）；
                    // 端口标志清掉——从此刻起断线重连需恢复订阅
                    link.prewarmed = false;
                    h.ws_send_text(&handle, &frame)
                        .map_err(|e| anyhow::anyhow!("resubscribe send failed: {}", e.message))?;
                    h.log_debug(&format!("terminal fresh subscribe frame sent (session {session_id})"));
                    return Ok(());
                }
            }
            // 已停止或断开等待中：不复用旧条目（重建）；断开中的重连会话由
            // 显式 close 取消（重建即新连接）
            if let Some(old) = link.handle.take() {
                let _ = h.ws_close(&old, "{}");
            }
            mgr.links.remove(session_id);
        }

        let handle = connect_link(h)?;
        let mut link = LinkState::new(session_id.to_string());
        link.handle = Some(handle.clone());
        link.last_handle = Some(handle.clone());
        // 认证由宿主代发（连接建立即认证帧在途）；本端直接进入订阅等待。
        // 订阅失败经 error 帧显性表达——会话不存在 → 主动断开借宿主退避重试
        h.ws_send_text(&handle, &frame)
            .map_err(|e| anyhow::anyhow!("subscribe send failed: {}", e.message))?;
        mgr.links.insert(session_id.to_string(), link);
        emit_state(h, session_id, LinkPhase::Auth, 0, "subscribed_sent");
        Ok(())
    }

    /// 预热链路（进终端页前只预热**传输面**）：建连（宿主代发 auth），
    /// **不发 subscribe 帧**。
    ///
    /// 为什么不能直接用 `subscribe` 预热：桌面端每次 subscribe 都从环窗口起点
    /// 整段重放，而预热时页面通道尚未登记——那笔重放注定被丢弃（白传一遍，
    /// 长会话可达数 MB）。页面挂载后的 `subscribe` 复用这条已认证连接，
    /// 全程只发生一次重放。
    ///
    /// 幂等：链路已在运行（有句柄且未停止）直接返回；已停止/断开的旧条目按
    /// `subscribe` 同款重建。失败（桌面未连 / 无目标）→ Err 上抛给调用方留痕，
    /// 不阻塞页面跳转。
    pub(crate) fn prewarm(h: &WasmHost, session_id: &str) -> anyhow::Result<()> {
        if session_id.is_empty() {
            return Ok(());
        }
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        if let Some(link) = mgr.links.get_mut(session_id) {
            if !link.stopped && link.handle.is_some() {
                return Ok(());
            }
            if let Some(old) = link.handle.take() {
                let _ = h.ws_close(&old, "{}");
            }
            mgr.links.remove(session_id);
        }
        let handle = connect_link(h)?;
        let mut link = LinkState::new(session_id.to_string());
        link.handle = Some(handle.clone());
        link.last_handle = Some(handle);
        link.prewarmed = true;
        mgr.links.insert(session_id.to_string(), link);
        // phase=connecting + detail=prewarmed：前端只会看到「未订阅但已建连」，
        // 真正的 subscribed 由页面挂载后的 subscribe 回包提供
        emit_state(h, session_id, LinkPhase::Connecting, 0, "prewarmed");
        Ok(())
    }

    /// 取消订阅（离开终端页 / 会话停止 / 手动断开）：关连接不再重连。
    /// 输出在断开期间由桌面环窗口保留，重进时重订阅回放补齐
    pub(crate) fn unsubscribe(h: &WasmHost, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        if let Some(link) = mgr.links.get_mut(session_id) {
            link.stopped = true;
            link.phase = LinkPhase::Idle;
            link.waiting_reconnect = false;
            if let Some(handle) = link.handle.take() {
                // 显式 close = 宿主取消 auto-reconnect（重连会话随句柄取消）
                let _ = h.ws_close(&handle, "{}");
            }
            emit_state(h, session_id, LinkPhase::Idle, 0, "unsubscribed");
        }
    }

    /// 全部取消（设备手动断开 / 连接关闭）
    pub(crate) fn unsubscribe_all(h: &WasmHost) {
        let ids: Vec<String> = {
            LINK_MANAGER
                .lock()
                .expect("link manager lock")
                .links
                .keys()
                .cloned()
                .collect()
        };
        for id in ids {
            Self::unsubscribe(h, &id);
        }
    }

    /// 会话删除：清链路（订阅态与推送通道在宿主 gateway 由命令层同步清理）
    pub(crate) fn remove(h: &WasmHost, session_id: &str) {
        Self::unsubscribe(h, session_id);
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        mgr.links.remove(session_id);
        // 会话已删除：在屏内容事实随之失效（同 id 重建的会话必须从头重放）
        mgr.page_content.remove(session_id);
    }

    /// 发送终端输入（文本 + 特殊键共存，帧序即写入序；投递失败上抛——
    /// 多帧输入下「文本已写进 PTY、回车没发」必须让前端看得见）
    pub(crate) fn send_input(h: &WasmHost, session_id: &str, data: &str, special_key: Option<&str>) -> anyhow::Result<()> {
        let frames = plan_input_frames(data, special_key).map_err(anyhow::Error::msg)?;
        let handle = {
            let mgr = LINK_MANAGER.lock().expect("link manager lock");
            let link = mgr
                .links
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("terminal link not subscribed"))?;
            if !link.subscribe_ack {
                return Err(anyhow::anyhow!("terminal link not subscribed"));
            }
            link.handle.clone().ok_or_else(|| anyhow::anyhow!("terminal link not subscribed"))?
        };
        for frame in frames {
            match frame {
                InputFrame::Text { data } => h
                    .ws_send_text(&handle, &build_input_text_frame(&data))
                    .map_err(|e| anyhow::anyhow!("input text send failed: {}", e.message))?,
                InputFrame::Binary { bytes } => h
                    .ws_send_binary(&handle, &bytes)
                    .map_err(|e| anyhow::anyhow!("input binary send failed: {}", e.message))?,
            }
        }
        Ok(())
    }

    /// 渲染背压 ack：前端本地已渲染字节数推进；插件按 64KB 阈值节流回发
    /// `{"type":"ack","offset":N}`（桌面插件据此触发 drain）。ack 是尽力
    /// 投递的渲染进度确认，失败留痕不报错（丢失由重连恢复补上）
    pub(crate) fn ack_rendered(h: &WasmHost, session_id: &str, offset: u64) {
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        let Some(link) = mgr.links.get_mut(session_id) else {
            return;
        };
        link.frontend_rendered = offset;
        if !should_send_ack(link.pending_ack) {
            return;
        }
        link.pending_ack = 0;
        if let Some(handle) = link.handle.clone() {
            if let Err(e) = h.ws_send_text(&handle, &build_ack_frame(offset)) {
                h.log_warn(&format!("terminal ack frame send failed (session {session_id}): {}", e.message));
            }
        }
    }

    /// 链路状态快照（前端轮询/对账；camelCase 键对齐前端 TerminalLinkState）
    pub(crate) fn get_state(session_id: &str) -> serde_json::Value {
        let mgr = LINK_MANAGER.lock().expect("link manager lock");
        match mgr.links.get(session_id) {
            Some(link) => serde_json::json!({
                "sessionId": session_id,
                "phase": link.phase.as_api_str(),
                "cursor": link.cursor,
                "acked": link.frontend_rendered,
                "stopped": link.stopped,
                "subscribed": link.subscribe_ack,
            }),
            // 无链路 = idle（对齐退役前 terminal_get_state 的缺失分支）
            None => serde_json::json!({ "sessionId": session_id, "phase": "idle" }),
        }
    }

    // ==================== 连接事实（宿主传输面事件） ====================

    /// `ws:open`：重连成功（payload 带 `reconnectedFrom` 旧句柄）→ 恢复该
    /// 会话的订阅；无主的新句柄（孤儿连接，如插件停用期间的遗留重连）→
    /// 显式关闭止损（激活后无人订阅，留着白占连接配额）。首次 connect 的
    /// open（无 `reconnectedFrom`）由 subscribe 同步路径处理，此处忽略。
    pub(crate) fn on_ws_open(h: &WasmHost, handle: &str, reconnected_from: Option<&str>) {
        let from = match reconnected_from {
            Some(f) => f,
            None => return,
        };
        let resumed = {
            let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
            let target = mgr
                .links
                .iter_mut()
                .find(|(_, l)| l.last_handle.as_deref() == Some(from) && l.waiting_reconnect);
            match target {
                Some((_sid, link)) => {
                    // 重连恢复：新句柄接管 + 订阅基准重置（重播前门控关闭）
                    link.handle = Some(handle.to_string());
                    link.last_handle = Some(handle.to_string());
                    link.waiting_reconnect = false;
                    link.phase = LinkPhase::Auth;
                    link.reset_subscription_base();
                    // 预热链路（尚未订阅过）没有订阅可恢复：只接管句柄、不发
                    // subscribe 帧——预热期的重放同样注定被丢弃（页面通道未
                    // 登记），真正的重放入口只有页面挂载后的 subscribe
                    let need_subscribe = !link.prewarmed;
                    if !need_subscribe {
                        link.phase = LinkPhase::Connecting;
                    }
                    Some((link.session_id.clone(), need_subscribe))
                }
                None => None,
            }
        };
        match resumed {
            Some((session_id, true)) => {
                // 重新订阅（无续传语义；环淘汰由 ring_resync 如实告知）。
                // 重放前的清屏由 `subscribed` 按「页面是否已有内容」判定并发 resync
                if let Err(e) = h.ws_send_text(handle, &build_subscribe_frame(&session_id)) {
                    h.log_warn(&format!("resubscribe after reconnect failed (session {session_id}): {}", e.message));
                    return;
                }
                emit_state(h, &session_id, LinkPhase::Auth, 0, "subscribed_sent");
            }
            // 预热链路（无订阅可恢复）：句柄接管已在上面完成，无需再发帧、
            // 也不打扰前端（页面尚未挂载）
            Some((_session_id, false)) => {}
            // 孤儿连接：显式关闭（close 即取消其重连会话）
            None => {
                h.log_warn(&format!("closing orphan reconnected ws handle {handle}"));
                let _ = h.ws_close(handle, "{}");
            }
        }
    }

    /// `ws:close`：断开事实 → 清句柄。显式停止的链路保持 stopped（不复活）；
    /// 活跃链路进入「等待宿主重连」并转发 reconnecting 状态（重连由宿主
    /// auto-reconnect 接管，插件无时钟）
    pub(crate) fn on_ws_close(h: &WasmHost, handle: &str) {
        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
        let target = mgr.links.iter_mut().find(|(_, l)| l.handle.as_deref() == Some(handle));
        let Some((_sid, link)) = target else {
            return;
        };
        let session_id = link.session_id.clone();
        link.handle = None;
        link.last_handle = Some(handle.to_string());
        link.subscribe_ack = false;
        if link.stopped {
            return; // 显式停止路径的状态事件已发（stopped/unsubscribed），不重复
        }
        link.waiting_reconnect = true;
        link.phase = LinkPhase::Connecting;
        drop(mgr);
        emit_state(h, &session_id, LinkPhase::Connecting, 0, "reconnecting");
    }

    /// `ws:reconnect-scheduled`：宿主退避排期 → 透传前端倒计时（payload 与
    /// 退役前 `reconnect_scheduled` detail 一致：`retry_in_ms` 字段）
    pub(crate) fn on_reconnect_scheduled(h: &WasmHost, handle: &str, retry_in_ms: u64) {
        let session_id = {
            let mgr = LINK_MANAGER.lock().expect("link manager lock");
            mgr.links
                .iter()
                .find(|(_, l)| l.last_handle.as_deref() == Some(handle) && l.waiting_reconnect)
                .map(|(sid, _)| sid.clone())
        };
        if let Some(session_id) = session_id {
            emit_state_with_retry(h, &session_id, LinkPhase::Connecting, 0, "reconnect_scheduled", retry_in_ms);
        }
    }

    // ==================== 下行帧 ====================

    /// ws 下行帧分派：text = 控制协议（JSON 状态机）；binary = 输出裸字节
    /// （门控 → 窄转发）。句柄反查会话；未命中 = 旧连接残帧，丢弃
    pub(crate) fn on_ws_frame(h: &WasmHost, handle: &str, kind: u8, payload: &[u8]) {
        let session_id = {
            let mgr = LINK_MANAGER.lock().expect("link manager lock");
            mgr.links
                .iter()
                .find(|(_, l)| l.handle.as_deref() == Some(handle))
                .map(|(sid, _)| sid.clone())
        };
        let Some(session_id) = session_id else {
            return;
        };
        if kind == WS_FRAME_KIND_TEXT {
            let text = String::from_utf8_lossy(payload);
            handle_control_text(h, &session_id, &text);
        } else {
            ingest_output(h, &session_id, payload);
        }
    }

    /// 停用清理：取走全部活动句柄（deactivate 逐一 close 取消重连）
    pub(crate) fn drain_handles(&mut self) -> Vec<String> {
        let mut handles = Vec::new();
        for (_sid, link) in self.links.iter_mut() {
            link.stopped = true;
            link.phase = LinkPhase::Idle;
            if let Some(h) = link.handle.take() {
                handles.push(h);
            }
        }
        self.links.clear();
        handles
    }
}

// ==================== 控制帧状态机（自 handle_control_text 等价迁移） ====================

/// 处理 JSON 控制帧（subscribed / ring_resync / session_stopped / error / unknown）
fn handle_control_text(h: &WasmHost, session_id: &str, text: &str) {
    let msg: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    match msg.get("type").and_then(|v| v.as_str()) {
        // 订阅回包：门控置位——此后输出帧才是本订阅的流（回放 + 实时）。
        // 本地基准归零。重订阅（重连/停止后重建/已 live 再 subscribe）时
        // 发 terminal-resync 让前端清屏 + 归零
        Some("subscribed") => {
            {
                let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
                let Some(link) = mgr.links.get_mut(session_id) else {
                    return;
                };
                link.subscribe_ack = true;
                link.phase = LinkPhase::Live;
                link.session_missing = 0;
                link.cursor = 0;
                link.frontend_rendered = 0;
                link.pending_ack = 0;
                link.prewarmed = false;
            }
            // 重放世代判定：页面已收到过该会话的输出 ⇒ 桌面端这轮从环窗口起点
            // 的重放会与在屏内容叠加渲染（重订阅/重连/会话重启）⇒ 必须先清屏。
            // 判据用「页面内容事实」而不是订阅相位：两条重放路径（重订阅竞态、
            // 重连）都不保证订阅时相位已是 live。
            let page_has_content = LINK_MANAGER
                .lock()
                .expect("link manager lock")
                .page_content
                .contains(session_id);
            emit_state(h, session_id, LinkPhase::Live, 0, "subscribed");
            if page_has_content {
                emit_resync(h, session_id, 0, "replay");
            }
        }
        // 环淘汰重锚（唯一截断信号）：清屏 + 本地计数基准重置。
        // 本端不发送 resync 帧（协议保留能力）
        Some("ring_resync") => {
            let offset = msg.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            {
                let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
                if let Some(link) = mgr.links.get_mut(session_id) {
                    link.cursor = 0;
                    link.frontend_rendered = 0;
                    link.pending_ack = 0;
                }
            }
            h.log_warn(&format!(
                "terminal resync (session {session_id}): ring_offset={offset}, clear screen and re-anchor"
            ));
            emit_resync(h, session_id, offset, "truncated");
        }
        // 停止帧：尾帧已先于本帧按序到达并被消费（帧序保证）→ 本端关闭连接
        //（显式 close = 取消宿主自动重连，不再复活）
        Some("session_stopped") => {
            let reason = msg.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let exit_code = msg.get("exitCode").and_then(|v| v.as_i64());
            h.log_info(&format!(
                "terminal session stopped (session {session_id}, reason={reason}, exit_code={exit_code:?})"
            ));
            let handle = {
                let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
                let Some(link) = mgr.links.get_mut(session_id) else {
                    return;
                };
                link.stopped = true;
                link.phase = LinkPhase::Idle;
                link.handle.take()
            };
            if let Some(handle) = handle {
                let _ = h.ws_close(&handle, "{}");
            }
            emit_state(h, session_id, LinkPhase::Idle, 0, "stopped");
        }
        Some("error") => {
            let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            h.log_warn(&format!("terminal ws server error (session {session_id}): {message}"));
            match classify_server_error(&message) {
                ServerErrorClass::SessionMissing => {
                    let (strikes, handle) = {
                        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
                        let Some(link) = mgr.links.get_mut(session_id) else {
                            return;
                        };
                        link.session_missing += 1;
                        (link.session_missing, link.handle.clone())
                    };
                    if strikes >= MAX_SESSION_MISSING_STRIKES {
                        // 会话缺失达上限：停止等待外部恢复（close 取消重连；
                        // 恢复由前端重新 subscribe 驱动）
                        h.log_warn(&format!(
                            "session missing after {MAX_SESSION_MISSING_STRIKES} attempts, stopping terminal link (session {session_id})"
                        ));
                        let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
                        if let Some(link) = mgr.links.get_mut(session_id) {
                            link.stopped = true;
                            link.phase = LinkPhase::Idle;
                            link.handle = None;
                        }
                        drop(mgr);
                        if let Some(handle) = handle {
                            let _ = h.ws_close(&handle, "{}");
                        }
                        emit_state(h, session_id, LinkPhase::Idle, 0, "session_missing");
                    } else {
                        // 会话启动竞态：主动断开借宿主退避循环重试（新连接 +
                        // 重新订阅，节流语义与退役前 TCP 重建一致），状态事件
                        // 提示前端进入重试
                        emit_state(h, session_id, LinkPhase::Connecting, 0, "retry");
                        if let Some(handle) = handle {
                            // close 触发宿主 auto-reconnect（waiting_reconnect
                            // 状态由 ws:close 事件路径置位）
                            let _ = h.ws_close(&handle, "{}");
                        }
                    }
                }
                ServerErrorClass::Other => {
                    // 非致命错误：留痕 + 状态事件，连接保持（后续帧可能恢复）
                    emit_state(h, session_id, LinkPhase::Auth, 0, "error");
                }
            }
        }
        _ => {
            h.log_debug(&format!("unknown terminal control frame (session {session_id})"));
        }
    }
}

/// 收输出字节：计数 + ack 记账 + 门控转发。
///
/// 门控 = `subscribe_ack`（已收 subscribed）；门控未通过的字节丢弃（fresh
/// subscribe 前的旧流残留，重播覆盖）。页面未订阅由宿主窄转发层 Err 表达，
/// 字节同样丢弃——重进页面经重订阅回放补齐
fn ingest_output(h: &WasmHost, session_id: &str, data: &[u8]) {
    let len = data.len() as u64;
    let mut mgr = LINK_MANAGER.lock().expect("link manager lock");
    let forwarded = {
        let Some(link) = mgr.links.get_mut(session_id) else {
            return;
        };
        if !link.subscribe_ack {
            return; // fresh subscribe 前的旧流残留
        }
        link.cursor += len;
        link.pending_ack += len;
        h.terminal_stream_forward_output(session_id, data).is_ok()
    };
    if forwarded {
        // 页面真的收到了字节 = 在屏内容事实（`subscribed` 时据此决定重放前清屏）
        mgr.page_content.insert(session_id.to_string());
    }
    // 转发失败（页面未订阅 / 通道已释放）：字节丢弃（无缓存），游标照记——
    // 重进页面时 fresh subscribe 重播环窗口补齐，与退役前门控丢弃语义一致。
    // 不计入 page_content：页面本就没内容，重放无需清屏
}

// ==================== 状态事件发射 ====================

/// 状态变更事件（payload 形状与退役前 `terminal-state` 逐字段一致：
/// session_id / phase / cursor / detail；emit 失败由宿主留痕）
fn emit_state(h: &WasmHost, session_id: &str, phase: LinkPhase, cursor: u64, detail: &str) {
    h.emit_event(
        &EVENT_TERMINAL_STATE,
        &serde_json::json!({
            "session_id": session_id,
            "phase": phase.as_api_str(),
            "cursor": cursor,
            "detail": detail,
        }),
    );
}

/// 重连排期事件（带 `retry_in_ms` 倒计时，前端据此显示「等多久再试」）
fn emit_state_with_retry(
    h: &WasmHost,
    session_id: &str,
    phase: LinkPhase,
    cursor: u64,
    detail: &str,
    retry_in_ms: u64,
) {
    h.emit_event(
        &EVENT_TERMINAL_STATE,
        &serde_json::json!({
            "session_id": session_id,
            "phase": phase.as_api_str(),
            "cursor": cursor,
            "detail": detail,
            "retry_in_ms": retry_in_ms,
        }),
    );
}

/// 重锚事件（`ring_resync` 或重订阅回包）：前端清屏 + 本地计数归零 +
/// 一次性提示。`offset` 为桌面插件环偏移（诊断用），本地计数基准一律归零
/// 重锚事件（重放世代切换 / 环淘汰）：前端清屏 + 本地计数归零。
///
/// `reason`：`"truncated"`（环淘汰，数据有缺口）/ `"replay"`（重放世代切换，
/// 无数据丢失）——前端据此区分是否上报「历史截断」（重放重锚不是截断）。
/// `offset` 为桌面插件环偏移（诊断用），本地计数基准一律归零。
/// 旧客户端忽略未知字段 `reason`（默认按 truncated 处理，行为不回归）。
fn emit_resync(h: &WasmHost, session_id: &str, offset: u64, reason: &str) {
    h.emit_event(
        &EVENT_TERMINAL_RESYNC,
        &serde_json::json!({
            "session_id": session_id,
            "offset": offset,
            "reason": reason,
        }),
    );
    emit_state(h, session_id, LinkPhase::Live, 0, "resync");
}
