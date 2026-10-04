//! 场景 6：重锚 / 停止控制信号与数据块的**投递顺序契约**（跨端真实链路）
//!
//! 场景 5（`terminal_output_pressure`）证「缺口必须伴随重锚信号」与「字节流单调
//! 不倒退」；本场景证**信号本身的时机与顺序**——前端渲染正确性依赖的三条顺序
//! 契约，任一条被重构破坏都会造成「字节没少但屏上错位 / 缺头」：
//!
//! | 契约 | 含义 | 阶段 |
//! |---|---|---|
//! | C-301 | 环淘汰重锚帧必须先于**它后面的数据块**被消费者观察到：重锚时刻已记录
//!   的字节数必须 < 重锚 offset（重锚 = 客户端落后、从环新驻留起点续拉；若续拉块
//!   先于信号到达，客户端会把它渲染到旧屏上，随后被清屏清掉 → 重锚头部 ≤16 KiB
//!   视觉缺失） | 阶段 1 |
//! | C-302 | `session_stopped` 停止帧必须先于任何后续输出——尾帧与停止帧的先后由
//!   桌面插件保证（`ws_terminal.rs` 结构锁 S3），本端消费侧不得把停止信号提前到
//!   尾帧之前（提前 = 前端 `sessionStopped` 置位后尾帧被 `deliverRawBytes` 丢弃） | 阶段 2 |
//! | C-303 | IO 意外断开 → 自动重连成功后必须发 `terminal-resync`（offset=0：清屏 +
//!   本地基准重置）再回放——重播与断线前在屏内容重叠时不清屏 = 行式输出整屏重复 /
//!   绝对定位错位（2026-10-04 审查修复 F-1：判断原放 `connect_once` 顶部，被
//!   `link_io` 先重置 phase 而成为永远不触发的死代码） | 阶段 3 |
//!
//! ## 观测面：合并时间线（[`Timeline`]）
//!
//! 状态事件（`terminal-state`）/ 重锚信号（`terminal-resync`）/ 输出数据块（页面级
//! Channel 的 `Raw` 载荷）按移动端 Rust 侧**真实调用顺序**落进同一条共享 Vec——
//! 三个出口都从同一个 IO 任务同步追加（先 `emit` 后 `channel.send`），追加序 =
//! 调用序。这是前端依赖的投递顺序契约的**可测一半**；另一半（WebView 是否按 IPC
//! 发送序交付事件与 Channel 载荷）无头环境无法验证——前端 `writeCoalescer` 的 rAF
//! 合并额外提供 ~1 帧缓冲（`onClear` 的 `dispose()` 会吞掉尚未写入 xterm 的
//! in-flight 块），使该平台假设的失败窗口进一步收窄（详见 2026-10-04 链路审查）。

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::{terminal_link_manager, TerminalEventSink};
use tauri::ipc::{Channel, InvokeResponseBody};

use common::desktop_ctx;
use common::mobile_ctx;

/// 单场景等待预算（与场景 5 同宽：阶段 1 要等一次真实环淘汰 + 重锚往返）
const TIMEOUT: Duration = Duration::from_secs(30);

/// 阶段 1 洪水量级：500_000 行 ≈ 7.5 MiB ≫ 4 MiB 环容量 ⇒ 客户端停 ack 下环必淘汰
const FLOOD_LINES: u32 = 500_000;

// ==================== 合并时间线（事件 + 数据块按调用序落一条 Vec） ====================

/// 时间线条目：状态事件 / 重锚信号 / 输出数据块
#[derive(Debug, Clone, PartialEq)]
enum TimelineEntry {
    /// `terminal-state`（detail 是诊断子类：subscribed / reconnecting / stopped …）
    State { detail: String },
    /// `terminal-resync`（offset：环偏移；重连清屏 = 0，环淘汰 = >0）
    Resync { offset: u64 },
    /// 一个输出数据块（len = 本块字节数）
    Bytes { len: usize },
}

#[derive(Default)]
struct TimelineInner {
    entries: Vec<TimelineEntry>,
    /// 已记录的全部输出字节（与 entries 同步追加，供字节量对账 / marker 检索）
    bytes: Vec<u8>,
}

/// 合并时间线：`TerminalEventSink`（事件）与页面级 Channel（数据）共用同一份
/// `Arc<Mutex<...>>`，追加序 = 移动端 Rust 侧真实调用序。
#[derive(Clone, Default)]
struct Timeline {
    inner: Arc<Mutex<TimelineInner>>,
}

impl Timeline {
    fn snapshot(&self) -> Vec<TimelineEntry> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).entries.clone()
    }

    fn bytes(&self) -> Vec<u8> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).bytes.clone()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes()).to_string()
    }

    /// 索引 `index` 的条目**之前**已记录的输出字节总量
    fn bytes_before(&self, index: usize) -> u64 {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.entries[..index]
            .iter()
            .map(|e| match e {
                TimelineEntry::Bytes { len } => *len as u64,
                _ => 0,
            })
            .sum()
    }

    /// 环淘汰重锚条目（offset > 0）`(条目索引, offset)`
    ///
    /// 与 `EventRecorder::resync_count` 同理：重连清屏的重锚 offset = 0，不算淘汰。
    fn ring_resync_entries(&self) -> Vec<(usize, u64)> {
        self.snapshot()
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                TimelineEntry::Resync { offset } if offset > 0 => Some((i, offset)),
                _ => None,
            })
            .collect()
    }
}

impl TerminalEventSink for Timeline {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match event {
            "terminal-state" => {
                let detail = payload.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string();
                inner.entries.push(TimelineEntry::State { detail });
            }
            "terminal-resync" => {
                let offset = payload.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
                inner.entries.push(TimelineEntry::Resync { offset });
            }
            other => tracing::warn!("unexpected event in timeline: {other}"),
        }
        Ok(())
    }
}

/// 造一个把输出裸字节追加进时间线的页面级 Channel（等价
/// `mobile_ctx::output_channel`，但数据块与事件共时间线）
fn timeline_channel(timeline: &Timeline) -> Channel<InvokeResponseBody> {
    let sink = timeline.clone();
    Channel::new(move |body| {
        let mut inner = sink.inner.lock().unwrap_or_else(|p| p.into_inner());
        match body {
            InvokeResponseBody::Raw(bytes) => {
                inner.entries.push(TimelineEntry::Bytes { len: bytes.len() });
                inner.bytes.extend_from_slice(&bytes);
            }
            other => {
                // 兜底形态：显性留痕而不是静默丢弃（终端输出恒为 Raw）
                inner.entries.push(TimelineEntry::Bytes { len: 0 });
                inner.bytes.extend_from_slice(format!("[non-raw:{other:?}]").as_bytes());
            }
        }
        Ok(())
    })
}

/// 轮询等待时间线满足谓词（超时 panic 并打印现场）
async fn wait_timeline(timeline: &Timeline, what: &str, mut pred: impl FnMut(&[TimelineEntry]) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if pred(&timeline.snapshot()) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {what}; timeline={:#?}", timeline.snapshot());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resync_and_stop_signals_precede_their_data_and_reconnect_reanchors() {
    if tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init()
        .is_err()
    {
        tracing::debug!("tracing subscriber already initialized");
    }

    desktop_ctx::init_app_context().await;
    let (port, handle, server_task) = desktop_ctx::start_server().await;
    mobile_ctx::set_target(port).await;

    let base = mobile_ctx::base_url(port);
    let address = format!("127.0.0.1:{port}");

    // ==================== 认证（与场景 4/5 同一条真实配对链） ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "cross-end-resync-ordering-device",
        device_name: "CrossEnd ResyncOrdering",
        fingerprint: "cross-end-resync-ordering-fp",
        uid_hash: None,
    };
    let pairing = auth
        .request_pairing(&base, ctx.device_id, ctx.device_name, ctx.fingerprint)
        .await
        .expect("配对码签发");
    let token = auth
        .verify_pairing_code(&base, ctx, &pairing.pairing_code, &address)
        .await
        .expect("配对码换 token")
        .token;
    mobile_ctx::remember_token(&token);

    let sessions = SessionHttpClient::new();
    let config_id = desktop_ctx::seed_shell_config("cross-end-resync-ordering").await;

    // ==================== 阶段 1：环淘汰重锚必须先于其后续数据块（C-301） ====================
    let session_r = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 1 启动会话");

    let timeline_r = Timeline::default();
    terminal_link_manager().page_subscribe(&session_r, timeline_channel(&timeline_r));
    terminal_link_manager().subscribe(Arc::new(timeline_r.clone()), session_r.clone());
    wait_timeline(&timeline_r, "阶段 1 链路 live", |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::State { detail } if detail == "subscribed"))
    })
    .await;

    // 客户端**永不再 ack**（模拟 WebView 冻结）：背压上沿后上游驻留，随后超环产出
    // 必然淘汰 ⇒ 观察重锚信号（与场景 5 阶段 B 同口径，刻意不 ack 逼出缺口显式）。
    sessions
        .send_input(
            &base,
            &session_r,
            &format!("for i in $(seq 1 {FLOOD_LINES}); do printf 'SEQ_%08d\\n' $i; done\n"),
            None,
        )
        .await
        .expect("阶段 1 写入洪水命令");

    wait_timeline(&timeline_r, "阶段 1 环淘汰重锚", |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::Resync { offset } if *offset > 0))
    })
    .await;

    // C-301：对**每一个**环淘汰重锚——信号被记录时已到的字节必须 < 该重锚的 offset。
    // 语义：重锚 = 客户端落后（已收字节 P < 环驻留起点 min_offset ≤ offset），续拉块
    // 从 offset 起。若续拉块先于信号被转发（重构把数据提前），已收字节会 ≥ offset，
    // 前端把续拉块渲染到旧屏上再被清屏清掉 → 重锚头部视觉缺失（且永不重发）。
    let resyncs_r = timeline_r.ring_resync_entries();
    assert!(
        !resyncs_r.is_empty(),
        "C-301 前置：停 ack + 超环洪水必须产生环淘汰重锚（收到 {} 字节）",
        timeline_r.bytes().len()
    );
    for (idx, offset) in &resyncs_r {
        let before = timeline_r.bytes_before(*idx);
        assert!(
            before < *offset,
            "C-301 重锚信号必须先于其后续数据块：索引 {idx} 的重锚 offset={offset}，\
             但该时刻已记录字节 {before} ≥ offset —— 续拉块先于信号到达，\
             客户端会把它渲染到旧屏再被清屏清掉（重锚头部 ≤16 KiB 视觉缺失）"
        );
    }
    // 续拉确认：流在重锚之后继续越过锚点（不是收到重锚就停摆）
    let last_anchor = resyncs_r.last().expect("已断言非空").1;
    mobile_ctx::wait_until("阶段 1 续拉越过锚点", || {
        timeline_r.bytes().len() as u64 > last_anchor + 1024
    })
    .await;
    tracing::info!("阶段 1 通过：{} 次环淘汰重锚，每次均先于其后续数据块", resyncs_r.len());

    // ==================== 阶段 2：session_stopped 必须先于尾帧之后（C-302） ====================
    let session_s = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 2 启动会话");

    let timeline_s = Timeline::default();
    terminal_link_manager().page_subscribe(&session_s, timeline_channel(&timeline_s));
    terminal_link_manager().subscribe(Arc::new(timeline_s.clone()), session_s.clone());
    wait_timeline(&timeline_s, "阶段 2 链路 live", |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::State { detail } if detail == "subscribed"))
    })
    .await;

    // shell 退出 → PTY EOF → 桌面插件尾帧 fetch + session_stopped 停止帧（尾帧在前）
    sessions
        .send_input(&base, &session_s, "exit\n", None)
        .await
        .expect("阶段 2 写入退出命令");

    wait_timeline(&timeline_s, "阶段 2 停止信号", |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::State { detail } if detail == "stopped"))
    })
    .await;

    // C-302：停止信号之后不得再有输出字节。尾帧必须先于 session_stopped 到达并已
    // 转发——若停止信号被提前（重构把控制帧处理挪到数据帧之前），前端 sessionStopped
    // 置位后尾帧会被 deliverRawBytes 丢弃，终端最后几行输出永久缺失。
    let entries_s = timeline_s.snapshot();
    let stop_idx = entries_s
        .iter()
        .position(|e| matches!(e, TimelineEntry::State { detail } if detail == "stopped"))
        .expect("已等到底停止信号");
    let bytes_after_stop = entries_s[stop_idx + 1..]
        .iter()
        .filter(|e| matches!(e, TimelineEntry::Bytes { .. }))
        .count();
    assert_eq!(
        bytes_after_stop, 0,
        "C-302 停止信号后不得再有输出字节（尾帧必须先于 session_stopped 到达）：\
         停止信号索引 {stop_idx}，其后数据块 {bytes_after_stop} 个；时间线={entries_s:?}"
    );
    assert!(
        timeline_s.bytes().len() > 0,
        "C-302 前置：停止前必须收到过真实输出（shell 启动 + 提示符）"
    );
    tracing::info!("阶段 2 通过：停止信号前尾帧已全部送达，停止后零数据块");

    // ==================== 阶段 3：IO 断开 → 重连 → 重锚清屏信号（C-303，F-1 回归锁） ====================
    let session_t = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 3 启动会话");

    let timeline_t = Timeline::default();
    terminal_link_manager().page_subscribe(&session_t, timeline_channel(&timeline_t));
    terminal_link_manager().subscribe(Arc::new(timeline_t.clone()), session_t.clone());
    wait_timeline(&timeline_t, "阶段 3 链路 live", |entries| {
        entries
            .iter()
            .any(|e| matches!(e, TimelineEntry::State { detail } if detail == "subscribed"))
    })
    .await;

    // 断线前先产生一点在屏内容（重播将与它重叠，必须靠重锚清屏）
    sessions
        .send_input(&base, &session_t, "echo RECONNECT_MARKER\n", None)
        .await
        .expect("阶段 3 写入断线前输出");
    mobile_ctx::wait_until("阶段 3 断线前 marker 送达", || {
        timeline_t.text().contains("RECONNECT_MARKER")
    })
    .await;

    // 服务端强制断连（等价网络中断的服务端侧动作，不伪造数据：断开后帧真的不再流动）
    let dropped = desktop_ctx::disconnect_plugin_ws_endpoint_clients(
        desktop_ctx::SESSION_PLUGIN_ID,
        "terminal",
        4000,
        "test-drop",
    )
    .await;
    assert!(dropped >= 1, "阶段 3 前置：必须至少断开一条 WS 连接，实际 {dropped} 条");

    // 移动端感知断开 → 退避重连 → 重新订阅（fresh subscribe 回放环窗口）
    wait_timeline(&timeline_t, "阶段 3 重连完成", |entries| {
        let Some(rc) = entries
            .iter()
            .position(|e| matches!(e, TimelineEntry::State { detail } if detail == "reconnecting"))
        else {
            return false;
        };
        // reconnecting 之后又回到 subscribed
        entries[rc + 1..]
            .iter()
            .any(|e| matches!(e, TimelineEntry::State { detail } if detail == "subscribed"))
    })
    .await;

    // C-303 核心：重连重订阅后**必须**有 offset=0 的重锚信号（清屏 + 本地基准重置）
    // 且位于 reconnecting 之后——回放与断线前在屏内容重叠，不清屏即整屏重复/错位。
    let entries_t = timeline_t.snapshot();
    let rc_idx = entries_t
        .iter()
        .position(|e| matches!(e, TimelineEntry::State { detail } if detail == "reconnecting"))
        .expect("已等到 reconnecting");
    let replay_clear = entries_t[rc_idx..]
        .iter()
        .any(|e| matches!(e, TimelineEntry::Resync { offset } if *offset == 0));
    assert!(
        replay_clear,
        "C-303 重连恢复后未发 offset=0 的重锚信号：回放将直接叠在断线前的旧屏内容上\
         （F-1 回归锁；修复前判断放 connect_once 顶部、被 link_io 先重置 phase 而永不触发）。\
         时间线={entries_t:?}"
    );

    // 重连后链路续上：再发一条命令，必须收到
    sessions
        .send_input(&base, &session_t, "echo AFTER_RECONNECT\n", None)
        .await
        .expect("阶段 3 写入重连后输出");
    mobile_ctx::wait_until("阶段 3 重连后 marker 送达", || {
        timeline_t.text().contains("AFTER_RECONNECT")
    })
    .await;
    tracing::info!("阶段 3 通过：IO 断开重连后发出清屏重锚，链路续上");

    // ==================== 收尾 ====================
    // 阶段 1/2 会话先停（阶段 3 的端点级断连已把它们也断过一次），再拆环境
    for sid in [&session_r, &session_s, &session_t] {
        let _ = sessions.stop_session(&base, sid).await;
    }
    terminal_link_manager().remove(&session_r);
    terminal_link_manager().remove(&session_s);
    terminal_link_manager().remove(&session_t);
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
