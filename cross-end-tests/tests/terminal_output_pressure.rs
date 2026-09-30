//! 场景 5：终端输出压力下的**字节连续性**与**背压闭环**（跨端真实链路）
//!
//! 场景 4（`terminal_ws_flow`）证的是「单条 marker 能到达」，是**功能存在性**。
//! 本文件证的是链路在**压力 / 客户端卡死**下丢不丢字节、丢了说不说——即
//! `ws_terminal` 背压窗口（`Watermark`）+ 环淘汰重锚（`ring_resync`）的跨端契约。
//!
//! ## 为什么必须跨端（两端各自的 mock 都证明不了）
//!
//! 背压记账的 ack 是**客户端本地累计字节**，插件侧要经 `base` 换算成环绝对
//! 偏移；重锚要让**双方**基准同时归零。任一端单测都只能锁一半公式：桌面侧
//! mock 客户端会「假装 ack」，移动侧 mock 桌面会「假装发帧」，两侧的错误在
//! 真实互连前都不会暴露。这里跑的是移动端真实 `TerminalLinkManager`（含 ack
//! 节流与 250ms 空闲兜底）→ 桌面真实插件 → 真实 bash PTY。
//!
//! ## 行为契约
//!
//! | 契约 | 行为 | 场景 |
//! |---|---|---|
//! | C-101 | 客户端正常确认（按渲染水位 ack）时，输出**零缺口**：`SEQ_<n>` 序列严格 +1 递增，且收齐 | 阶段 A（量级 512 KiB ≪ 4 MiB 环容量 ⇒ 环不可能淘汰 ⇒ 任何缺口都是链路自身丢的） |
//! | C-102 | 客户端**停止确认**（模拟 WebView 卡死）且产出超过环容量时，缺口必须**伴随 `terminal-resync` 信号**（fail-visible） | 阶段 B（8 MiB 洪水，环必淘汰） |
//! | C-103 | 收到的字节流始终是**真实产出的连续前缀**（序号只增不减、绝不重复或倒退）——重锚后从新基准续拉，不倒带 | A + B 共用 |
//! | C-104 | 背压确实在起作用：客户端停 ack 后上游**不再无止境推送**（水位被记账，且有恢复手段） | 阶段 B（停 ack 后观察到的帧增长受限） |
//!
//! 阶段 A 的「零缺口」是**构造性保证**而非计时赌注：产出总量 < 环容量 ⇒
//! `PtyRing` 不可能淘汰任何字节 ⇒ 缺口只可能来自链路上某一跳的静默丢弃。

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bedcode_mobile_lib::auth::http::{AuthHttpClient, DeviceAuthContext};
use bedcode_mobile_lib::session::http::SessionHttpClient;
use bedcode_mobile_lib::terminal_link::{terminal_ack_rendered, terminal_link_manager};

use common::desktop_ctx;
use common::mobile_ctx;

/// 轮询预算（阶段 B 要等一次真实环淘汰 + 重锚往返，比功能场景宽）
const PRESSURE_TIMEOUT: Duration = Duration::from_secs(30);

/// 阶段 A 产出量级：512 KiB（远小于 4 MiB 环 ⇒ 构造性零淘汰）
const SEQ_LINES: u32 = 40_000;

/// 序号字段宽度（定宽零填充）
///
/// **定宽不是为了好看，是解析鲁棒性的硬要求**：重锚会把两个区域拼接在一位数字中间
/// （实测 `SEQ_0002` + `146\r\n`），变长序号下拼接产物与真序号**无法区分**（都能读成
/// 一个合法数字）；定宽后拼接产物的位数必然不对，可被识破。
const SEQ_DIGITS: usize = 8;

/// 产出命令：`for i in $(seq from to); do printf 'SEQ_%08d\n' $i; done`
fn seq_command(from: u32, to: u32) -> String {
    format!("for i in $(seq {from} {to}); do printf 'SEQ_%0{SEQ_DIGITS}d\\n' $i; done\n")
}

/// 从已收到的输出字节里抽出序号序列（按到达顺序）
///
/// **三条鲁棒性要求**（每条都对应一个实测坑）：
/// 1. **在行内定位 `SEQ_`，而不是要求整行以 `SEQ_` 开头**——shell 提示符不以换行结尾，
///    会把下一条命令的首个输出行粘在提示符行尾（实测 `...$ for i in ...done\r` 紧跟
///    `SEQ_00020001`），整行匹配会把真序号一起丢掉（曾误报「缺 1 行」假故障）。
/// 2. **定宽校验**（见 [`SEQ_DIGITS`]）——识破跳帧 / 重锚拼接产生的半截序号。
/// 3. **只取定宽内的数字**——拼接会把序号拉长，位数校验就是第二道闸。
///
/// 拼接后的流对**帧边界**天然免疫（跨帧切断的数字在拼接后已复原）；用
/// `from_utf8_lossy`：本场景输出全是 ASCII，跳帧切断的多字节序列只影响末尾噪声。
fn seq_numbers(bytes: &[u8]) -> Vec<u32> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("SEQ_") {
        let start = cursor + rel + 4;
        // 取 SEQ_DIGITS + 1 位：定宽本身要成立，第 9 位存在与否用于识破「被拼接拉长」
        let digits: String = text[start..].chars().take(SEQ_DIGITS + 1).collect();
        let head = digits.get(..SEQ_DIGITS).unwrap_or("");
        if head.len() == SEQ_DIGITS && head.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(n) = head.parse::<u32>() {
                out.push(n);
            }
        }
        cursor = start;
    }
    out
}

/// 序号序列的「跳号点」（`nums[i] + 1 != nums[i+1]` 的 i 列表）
fn gaps(nums: &[u32]) -> Vec<usize> {
    nums.windows(2)
        .enumerate()
        .filter(|(_, w)| w[1] != w[0] + 1)
        .map(|(i, _)| i)
        .collect()
}

/// **环淘汰导致**的重锚次数（`offset > 0` 的 `terminal-resync`）
///
/// 与 [`mobile_ctx::EventRecorder::resync_count`] 的区别：后者把**首次订阅**的那次
/// 重锚也算进去——移动端 `subscribe()` 对全新链路也无条件置 `pending_resync`（无法
/// 区分「首次订阅」与「同 id 会话重启后重建」，保守清屏），其 `offset = 0`；而环
/// 淘汰重锚的 `offset` 必 > 0（`truncated` 要求 `min_offset > 0`，故 `next_offset > 0`）。
/// 「是否丢字节」的判据只能看后者。
/// 断言序号序列的**完整性**：每个不连续点都必须由一次重锚解释
///
/// 契约分两层，对应不同的缺陷：
/// - **不连续点**（`w[1] != w[0] + 1`，含前向跳变与倒退）只能出现在**重锚边界**上：
///   重锚 = 客户端已清屏 + 从环的新驻留起点续拉，**新起点在环上的位置是任意的**
///   （实测：重锚前后可从更靠后的位置续拉），所以跨重锚的前向跳变与倒退都是合法的；
/// - **每个不连续点都要有一次重锚信号解释**（fail-visible：缺口不得静默）。所以判据是
///   「不连续点数 ≤ 重锚次数 + 1」——`+1` 容忍「订阅时环已淘汰过一次」的首序号偏大。
///
/// 零淘汰阶段（阶段 A / C-2 / C-3 / C-4）另有更紧的判据：既要求零不连续点、也要求
/// 零重锚，两者互为佐证。
fn assert_contiguous_or_explained(label: &str, nums: &[u32], resync_count: usize) {
    let discontinuities = nums
        .windows(2)
        .filter(|w| w[1] != w[0] + 1)
        .count();
    assert!(
        discontinuities <= resync_count + 1,
        "{label}: 不连续点 {discontinuities} 处 > 重锚 {resync_count} 次 + 1 —— \
         存在未被重锚信号告知的静默字节丢失（重锚是唯一的缺口告知手段）"
    );
}

/// 断言序号序列**严格递增**（不得重复或倒退），违例时打印现场
///
/// 帧边界对账是判定位置的关键：帧内错位 = 环 / 插件拼装问题，帧边界错位 = 传输 /
/// 转发问题。保留给「本阶段不允许任何跳变」的场景（零淘汰）。
fn assert_strictly_increasing(label: &str, nums: &[u32], raw: &[u8], spans: &[(usize, usize)]) {
    let Some(bad) = nums.windows(2).position(|w| w[1] <= w[0]) else {
        return;
    };
    let lo = bad.saturating_sub(3);
    let hi = (bad + 4).min(nums.len());
    // 把「变小后的那个序号」在原始字节里定位（取**最后**一次出现，避开重播的早期副本）
    let needle = format!("SEQ_{}", nums[bad + 1]);
    let byte_pos = raw
        .windows(needle.len())
        .rposition(|w| w == needle.as_bytes())
        .unwrap_or(0);
    let from = byte_pos.saturating_sub(120);
    let to = (byte_pos + 120).min(raw.len());
    let context = String::from_utf8_lossy(&raw[from..to]).replace('\n', "\\n");
    // 帧边界对账：违例字节落在哪一帧 / 帧内偏移；并打印相邻三帧的长度
    let frame_idx = spans
        .iter()
        .position(|(start, len)| byte_pos >= *start && byte_pos < *start + *len);
    let in_frame = frame_idx.map(|i| byte_pos - spans[i].0);
    let neighbours: Vec<(usize, usize)> = frame_idx
        .map(|i| spans[i.saturating_sub(2)..(i + 3).min(spans.len())].to_vec())
        .unwrap_or_default();
    // 全量落盘供离线取证（流可能很大，只在违例时写）
    let dump = std::env::temp_dir().join(format!(
        "crossend_violation_{}.bin",
        label.replace([' ', '（', '）'], "_")
    ));
    let _ = std::fs::write(&dump, raw);
    panic!(
        "{label}: 序号必须严格递增（不得重复或倒退）\n  \
         违例：{:?} → {:?}\n  \
         序号窗口：{:?}\n  \
         字节偏移≈{}，原始上下文：{:?}\n  \
         帧对账：第 {:?} 帧 / 帧内 {:?}，相邻帧 (offset,len)={:?}\n  \
         全量流已落盘：{}",
        nums[bad],
        nums[bad + 1],
        &nums[lo..hi],
        byte_pos,
        context,
        frame_idx,
        in_frame,
        neighbours,
        dump.display()
    );
}

fn ring_resyncs(recorder: &mobile_ctx::EventRecorder) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|(event, payload)| {
            event == "terminal-resync"
                && payload
                    .get("offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    > 0
        })
        .count()
}

// ==================== 桌面终端预览消费者（角色 B） ====================
//
// 逐条镜像 `wasm-apps/terminal-session/src/components/terminal/TerminalPreview.vue`
// 的数据面契约（不是「差不多」而是**同一契约**，否则本阶段测的是另一个东西）：
//
// - `session.output.pull {sessionId, fromOffset}` → `null`（追平）|
//   `{data, nextOffset, truncated, throttled, unacked}`；
// - `throttled` → **不推游标**，退避（本测试直接结束本轮）；
// - `truncated` → 清屏 + 游标/已确认水位重锚到 `minOffset = next - bytes`；
// - ack 上报的是**交付水位**（入队即账），阈值 64 KiB（对齐前端
//   `ACK_BYTES_THRESHOLD`）+ 250 ms 空闲兜底。
//
// 为什么必须是「另一个消费者」而不是第二条 WS 连接：两个消费者的帧面根本不同
// ——一个走插件命令面（JSON + 游标拉取），一个走 WS 二进制帧 + 每连接游标。
// 用第二条 WS 连接去「模拟桌面端」会把最关键的东西（两个独立背压窗口共处
// 一个输出环）测成同一条路径的自证。

/// 单轮 pull 的最大批数（对齐前端 `OUTPUT_PULL_MAX_BATCHES`）
const DESKTOP_PULL_MAX_BATCHES: usize = 8;
/// 桌面端 pull 周期（对齐前端快档 `OUTPUT_PULL_INTERVAL_MS`）
const DESKTOP_PULL_INTERVAL_MS: u64 = 50;
/// 桌面端 ack 阈值（对齐前端 `ACK_BYTES_THRESHOLD`）
const DESKTOP_ACK_BYTES: u64 = 64 * 1024;
/// 桌面端 ack 空闲兜底（对齐 `ACK_MAX_IDLE_MS`）
const DESKTOP_ACK_IDLE_MS: u64 = 250;

/// 桌面端消费者的**活状态**（只被循环任务自己持有）
struct DesktopPreview {
    /// 输出游标（ring-fetch 基准）
    cursor: u64,
    /// 已交付水位（ack 上报的就是它）
    delivered: u64,
    /// 待 ack 字节
    pending_ack: u64,
    /// 已收到的原始字节（当前屏内容；truncated 时清空）
    bytes: Vec<u8>,
    /// 帧边界（每次 `data` 段记一条；truncated 时清空）
    frame_spans: Vec<(usize, usize)>,
    /// 清屏重锚次数（`truncated`）
    truncations: u64,
    /// 背压抑制次数（`throttled`）
    throttles: u64,
    /// 上次 ack 时刻（ms）
    last_ack_ms: u128,
    /// 致命错误（命令面报错不得静默：记下来由断言报）
    error: Option<String>,
}

/// 桌面端消费者的**对外快照**（测试线程读；`acking` 是反向控制位）
#[derive(Default)]
struct DesktopSnapshot {
    cursor: u64,
    bytes: Vec<u8>,
    /// 帧边界（与 `bytes` 同步累加，供字节级取证）
    frame_spans: Vec<(usize, usize)>,
    truncations: u64,
    throttles: u64,
    error: Option<String>,
}

impl DesktopPreview {
    /// 拉一轮（最多 N 批）
    async fn pull_once(&mut self, session_id: &str) {
        for _ in 0..DESKTOP_PULL_MAX_BATCHES {
            let res = match desktop_ctx::try_plugin_command(
                desktop_ctx::SESSION_PLUGIN_ID,
                "session.output.pull",
                serde_json::json!({ "sessionId": session_id, "fromOffset": self.cursor }),
            )
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    self.error = Some(format!("session.output.pull failed: {e}"));
                    return;
                }
            };
            if res.is_null() {
                return; // 游标已追平产出端
            }
            if res["throttled"].as_bool().unwrap_or(false) {
                self.throttles += 1;
                return; // 退避：不推游标
            }
            let next = res["nextOffset"].as_u64().unwrap_or(self.cursor);
            let data = res["data"].as_array().cloned().unwrap_or_default();
            let n = data.len() as u64;
            if res["truncated"].as_bool().unwrap_or(false) {
                // 清屏 + 基准重锚到环驻留起点（`minOffset = next - bytes`）
                self.bytes.clear();
                self.frame_spans.clear();
                self.delivered = next.saturating_sub(n);
                self.pending_ack = 0;
                self.truncations += 1;
            }
            self.cursor = next;
            if n > 0 {
                self.frame_spans.push((self.bytes.len(), n as usize));
                self.bytes.extend(data.iter().map(|v| v.as_u64().unwrap_or(0) as u8));
                self.delivered = next;
                self.pending_ack += n;
            }
            if n == 0 {
                return;
            }
        }
    }

    /// 按 64 KiB 阈值 / 250 ms 空闲兜底回 ack（前端同一口径）
    async fn maybe_ack(&mut self, session_id: &str, acking: bool) {
        if !acking || self.pending_ack == 0 {
            return;
        }
        let now = now_millis();
        let idle_due =
            self.last_ack_ms != 0 && now.saturating_sub(self.last_ack_ms) >= u128::from(DESKTOP_ACK_IDLE_MS);
        let due = self.pending_ack >= DESKTOP_ACK_BYTES || idle_due;
        if !due {
            return;
        }
        self.pending_ack = 0;
        self.last_ack_ms = now;
        let offset = self.delivered;
        if let Err(e) = desktop_ctx::try_plugin_command(
            desktop_ctx::SESSION_PLUGIN_ID,
            "session.output.ack",
            serde_json::json!({ "sessionId": session_id, "offset": offset }),
        )
        .await
        {
            self.error = Some(format!("session.output.ack failed: {e}"));
        }
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

/// 启动桌面端预览消费者循环
///
/// **活状态只归循环任务所有**（`DesktopPreview` 是任务局部变量），每轮把快照写回
/// 共享槽；测试线程只读快照 + 翻 `acking` 位。避开 `std::sync::MutexGuard` 跨
/// `.await`（那样 future 不是 `Send`，`tokio::spawn` 编译不过）。
fn spawn_desktop_preview(
    session_id: String,
) -> (
    Arc<Mutex<DesktopSnapshot>>,
    Arc<std::sync::atomic::AtomicBool>,
    tokio::task::JoinHandle<()>,
) {
    let snapshot = Arc::new(Mutex::new(DesktopSnapshot::default()));
    let acking = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let handle = {
        let snapshot = snapshot.clone();
        let acking = acking.clone();
        tokio::spawn(async move {
            let mut live = DesktopPreview {
                cursor: 0,
                delivered: 0,
                pending_ack: 0,
                bytes: Vec::new(),
                frame_spans: Vec::new(),
                truncations: 0,
                throttles: 0,
                last_ack_ms: 0,
                error: None,
            };
            let mut ticker = tokio::time::interval(Duration::from_millis(DESKTOP_PULL_INTERVAL_MS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let acking_now = acking.load(std::sync::atomic::Ordering::SeqCst);
                live.pull_once(&session_id).await;
                live.maybe_ack(&session_id, acking_now).await;
                let mut slot = snapshot.lock().unwrap_or_else(|p| p.into_inner());
                slot.cursor = live.cursor;
                slot.bytes = live.bytes.clone();
                slot.frame_spans = live.frame_spans.clone();
                slot.truncations = live.truncations;
                slot.throttles = live.throttles;
                slot.error = live.error.clone();
                if live.error.is_some() {
                    return; // 错误已快照，不再空转
                }
            }
        })
    };
    (snapshot, acking, handle)
}

/// 轮询直到桌面端序号收齐（或超时）；返回桌面端看到的序号序列
async fn wait_desktop_markers(snapshot: &Arc<Mutex<DesktopSnapshot>>, target: u32, what: &str) -> Vec<u32> {
    let deadline = Instant::now() + PRESSURE_TIMEOUT;
    loop {
        let markers = {
            let guard = snapshot.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(err) = &guard.error {
                panic!("{what}: 桌面端消费者报错：{err}");
            }
            seq_numbers(&guard.bytes)
        };
        if markers.iter().any(|n| *n >= target) {
            return markers;
        }
        if Instant::now() >= deadline {
            panic!("{what}: 超时，桌面端仅收到 {} 个序号（目标 {target}）", markers.len());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminal_output_stays_continuous_or_reports_gap_explicitly() {
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

    // ==================== 认证（与场景 4 同一条真实配对链） ====================
    let auth = AuthHttpClient::new();
    let ctx = DeviceAuthContext {
        device_id: "cross-end-pressure-device",
        device_name: "CrossEnd Pressure",
        fingerprint: "cross-end-pressure-fp",
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
    let config_id = desktop_ctx::seed_shell_config("cross-end-pressure").await;

    // ==================== 阶段 A：正常确认 ⇒ 零缺口（C-101 / C-103） ====================
    let session_a = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 A 启动会话");

    let recorder_a = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs_a = Arc::new(mobile_ctx::OutputRecorder::default());
    terminal_link_manager().page_subscribe(&session_a, mobile_ctx::output_channel(outputs_a.clone()));
    terminal_link_manager().subscribe(recorder_a.clone(), session_a.clone());
    mobile_ctx::wait_event(&recorder_a, "阶段 A 链路 live", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;

    // 客户端按**渲染水位**回 ack（等价生产前端 `onWriteParsed` → ackRendered）：
    // 每收到一段就确认到累计水位，不制造假缺口也不掩盖背压。
    let ack_session = session_a.clone();
    let ack_outputs = outputs_a.clone();
    let ack_task = tokio::spawn(async move {
        let mut acked = 0u64;
        loop {
            let total = ack_outputs.bytes().len() as u64;
            if total > acked {
                acked = total;
                let _ = terminal_ack_rendered(ack_session.clone(), acked).await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    sessions
        .send_input(&base, &session_a, &seq_command(1, SEQ_LINES), None)
        .await
        .expect("阶段 A 写入产出命令");

    // 收齐 C-104 的等价条件：末序号到达即视为产出已完整送达
    let deadline = Instant::now() + PRESSURE_TIMEOUT;
    loop {
        let nums = seq_numbers(&outputs_a.bytes());
        if nums.iter().any(|n| *n >= SEQ_LINES) {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "阶段 A 超时：仅收到 {} 个序号（末序号={:?}，目标 {SEQ_LINES}）",
                nums.len(),
                nums.last()
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    ack_task.abort();

    let nums_a = seq_numbers(&outputs_a.bytes());
    let gaps_a = gaps(&nums_a);
    assert!(
        gaps_a.is_empty(),
        "C-101 阶段 A 出现序号缺口（产出 {} 行 < 环容量，环不可能淘汰 ⇒ 链路自身丢字节）：\
         缺口位置={gaps_a:?}，示例={:?}",
        SEQ_LINES,
        nums_a
            .iter()
            .enumerate()
            .filter(|(i, _)| gaps_a.contains(i))
            .map(|(i, n)| (n, nums_a.get(i + 1)))
            .take(5)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ring_resyncs(&recorder_a),
        0,
        "C-101 零缺口 ⇒ 不得出现**环淘汰**重锚（重锚会清屏；无缺口却清屏同样是缺陷）"
    );
    assert!(
        nums_a.iter().any(|n| *n >= SEQ_LINES),
        "C-104 阶段 A 必须收齐末序号（收到 {} 个）",
        nums_a.len()
    );

    // ==================== 阶段 B：停 ack + 超环洪水 ⇒ 缺口必显式（C-102 / C-103 / C-104） ====================
    let session_b = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 B 启动会话");

    let recorder_b = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs_b = Arc::new(mobile_ctx::OutputRecorder::default());
    terminal_link_manager().page_subscribe(&session_b, mobile_ctx::output_channel(outputs_b.clone()));
    terminal_link_manager().subscribe(recorder_b.clone(), session_b.clone());
    mobile_ctx::wait_event(&recorder_b, "阶段 B 链路 live", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;

    // 客户端**永不再 ack**（模拟 WebView 冻结 / 渲染管线停摆）：背压窗口累积到
    // 上沿后上游转为驻留；随后超环产出必然淘汰 ⇒ 必须观察到重锚信号。
    // 本阶段刻意**不**回 ack，正是为了逼出「缺口显式」。
    //
    // 阶段 B：5.5 MiB 洪水（远超 4 MiB 环）
    sessions
        .send_input(&base, &session_b, &seq_command(1, 500_000), None)
        .await
        .expect("阶段 B 写入洪水命令");

    let deadline = Instant::now() + PRESSURE_TIMEOUT;
    loop {
        if ring_resyncs(&recorder_b) > 0 {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "C-102 超时：客户端停 ack + 8 MiB 洪水下未观察到任何 terminal-resync ——\
                 缺口被静默吞掉（收到 {} 字节 / {} 帧）",
                outputs_b.bytes().len(),
                outputs_b.frame_count()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // C-103：即使发生重锚，序号序列也必须**只增不减、不重复**（从新基准续拉，
    // 不倒带、不重播已消费区间）。
    let nums_b = seq_numbers(&outputs_b.bytes());
    assert!(
        !nums_b.is_empty(),
        "C-103 阶段 B 必须收到真实 PTY 字节（收到 0 字节说明重锚后链路没续上）"
    );
    let resync_b = ring_resyncs(&recorder_b);
    assert_contiguous_or_explained("C-103 阶段 B", &nums_b, resync_b);
    let gaps_b = gaps(&nums_b);
    // C-102 的正向表述：缺口数不超过重锚次数 + 1（首序号 > 1 说明订阅时环已
    // 淘汰过一次，那一次的重锚发生在 subscribe 回包之前，计入 +1 的容差）。
    assert!(
        gaps_b.len() <= resync_b + 1,
        "C-102 缺口数 {} 超过重锚次数 {} + 1 ⇒ 存在未被信号告知的静默丢字节：缺口={gaps_b:?}，resync={resync_b}",
        gaps_b.len(),
        resync_b
    );

    // ==================== 阶段 C：桌面端 + 移动端**同看一个会话** ====================
    //
    // 这是 pull 模型最容易被改坏、却最难单端发现的不变式：**一个输出环、两个
    // 独立游标、两套独立背压窗口**。任何「共用一个窗口表」或「共用一个游标」
    // 的重构在这里都会立刻暴露（一个慢端拖死另一个 / 一个端重播已消费区间）。
    //
    // 两个角色走**不同的真实帧面**：
    // - 角色 A（移动端）= 真实 `TerminalLinkManager`（WS 二进制帧 + 每连接游标 +
    //   `ack` 帧背压）；
    // - 角色 B（桌面端）= 真实插件命令面 `session.output.pull` / `session.output.ack`
    //   （会话级游标 + `pump` 窗口）。
    let session_c = sessions
        .start_session(&base, &config_id, Some(120), Some(40))
        .await
        .expect("阶段 C 启动会话");

    let recorder_c = Arc::new(mobile_ctx::EventRecorder::default());
    let outputs_c = Arc::new(mobile_ctx::OutputRecorder::default());
    terminal_link_manager().page_subscribe(&session_c, mobile_ctx::output_channel(outputs_c.clone()));
    terminal_link_manager().subscribe(recorder_c.clone(), session_c.clone());
    mobile_ctx::wait_event(&recorder_c, "阶段 C 移动端 live", |events| {
        events
            .iter()
            .any(|(e, p)| e == "terminal-state" && p.get("detail").and_then(|v| v.as_str()) == Some("subscribed"))
    })
    .await;

    // 移动端 ack 任务（可控停摆：阶段 C-3 要模拟手机端渲染管线卡死）
    let mobile_ack = {
        let sid = session_c.clone();
        let outs = outputs_c.clone();
        tokio::spawn(async move {
            let mut acked = 0u64;
            loop {
                let total = outs.bytes().len() as u64;
                if total > acked {
                    acked = total;
                    let _ = terminal_ack_rendered(sid.clone(), acked).await;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };
    let (desktop_c, desktop_acking, desktop_task) = spawn_desktop_preview(session_c.clone());

    // ---- C-2 双端都正常确认 ⇒ 同一段真实输出、两端各自零缺口 ----
    const C_LINES: u32 = 20_000;
    sessions
        .send_input(&base, &session_c, &seq_command(1, C_LINES), None)
        .await
        .expect("阶段 C-2 写入产出命令");

    let desk_markers = wait_desktop_markers(&desktop_c, C_LINES, "C-2 桌面端").await;
    mobile_ctx::wait_until("C-2 移动端收齐", || {
        seq_numbers(&outputs_c.bytes()).iter().any(|n| *n >= C_LINES)
    })
    .await;
    let mob_markers = seq_numbers(&outputs_c.bytes());

    // 先钉「无重锚」再钉「无缺口」：truncated 会清屏并重建序号序列，
    // 先断言缺口会给出「看起来像丢字节」的错误信息，掩盖真因。
    assert_eq!(
        desktop_c.lock().unwrap().truncations,
        0,
        "C-201 桌面端不得清屏重锚（环容量 {} MiB > 本阶段产出）",
        desktop_ctx::SESSION_PLUGIN_ID
    );
    assert_eq!(
        ring_resyncs(&recorder_c),
        0,
        "C-201 移动端不得出现**环淘汰**重锚"
    );
    assert!(
        gaps(&desk_markers).is_empty(),
        "C-201 桌面端出现序号缺口（两端共环，缺口只能来自链路静默丢弃）：{:?}",
        gaps(&desk_markers)
    );
    assert!(
        gaps(&mob_markers).is_empty(),
        "C-201 移动端出现序号缺口：{:?}",
        gaps(&mob_markers)
    );
    assert_eq!(
        desk_markers, mob_markers,
        "C-202 同一输出环的两个消费者必须看到**同一段字节流**（序号序列逐项相等）"
    );
    assert_eq!(
        desk_markers, mob_markers,
        "C-202 同一输出环的两个消费者必须看到**同一段字节流**（序号序列逐项相等）"
    );

    // ---- C-3 移动端停 ack（手机端渲染管线卡死）⇒ 桌面端仍零缺口收齐 ----
    mobile_ack.abort();
    const C_LINES_2: u32 = 40_000;
    sessions
        .send_input(&base, &session_c, &seq_command(C_LINES + 1, C_LINES_2), None)
        .await
        .expect("阶段 C-3 写入产出命令");
    let desk_markers_2 = wait_desktop_markers(&desktop_c, C_LINES_2, "C-3 桌面端").await;
    let gaps_c3 = gaps(&desk_markers_2);
    let (desk_trunc_c3, desk_throttle_c3) = {
        let guard = desktop_c.lock().unwrap_or_else(|p| p.into_inner());
        (guard.truncations, guard.throttles)
    };
    assert!(
        gaps_c3.is_empty(),
        "C-203 移动端停 ack 期间，桌面端仍必须零缺口（背压窗口互不串扰）：\
         缺口={:?}，示例={:?}；桌面端 truncations={desk_trunc_c3} throttles={desk_throttle_c3}，\
         移动端环淘汰重锚={}，桌面端 throttles={desk_throttle_c3}",
        gaps_c3,
        gaps_c3
            .iter()
            .take(3)
            .map(|i| (desk_markers_2[*i], desk_markers_2.get(i + 1)))
            .collect::<Vec<_>>(),
        ring_resyncs(&recorder_c)
    );

    // ---- C-4 反向：桌面端停 ack ⇒ 移动端仍零缺口收齐 ----
    desktop_acking.store(false, std::sync::atomic::Ordering::SeqCst);
    const C_LINES_3: u32 = 60_000;
    sessions
        .send_input(&base, &session_c, &seq_command(C_LINES_2 + 1, C_LINES_3), None)
        .await
        .expect("阶段 C-4 写入产出命令");
    let mob_ack_back = {
        let sid = session_c.clone();
        let outs = outputs_c.clone();
        tokio::spawn(async move {
            let mut acked = 0u64;
            loop {
                let total = outs.bytes().len() as u64;
                if total > acked {
                    acked = total;
                    let _ = terminal_ack_rendered(sid.clone(), acked).await;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };
    mobile_ctx::wait_until("C-4 移动端收齐", || {
        seq_numbers(&outputs_c.bytes()).iter().any(|n| *n >= C_LINES_3)
    })
    .await;
    let mob_markers_3 = seq_numbers(&outputs_c.bytes());
    assert!(
        gaps(&mob_markers_3).is_empty(),
        "C-204 桌面端停 ack 期间，移动端仍必须零缺口（反向不串扰）：{:?}",
        gaps(&mob_markers_3)
    );
    mob_ack_back.abort();

    // ---- C-5 超环洪水 ⇒ 共享环淘汰对**两端各自**显式 ----
    //
    // 共享环的淘汰由**最慢的游标**决定；两端都在拉的前提下，谁被淘汰谁就会在
    // 下一轮 fetch 拿到 `truncated` / `ring_resync`。若只通知到其中一端，另一端
    // 就会在不知情的情况下静默错屏——这正是本阶段最需要钉死的不变式。
    //
    // 已知不对称（记录在此，不在本阶段断言）：命令面无逃生阀 ⇒ 桌面端 ack
    // 管道停摆会把该会话的桌面预览锁在静默（生产上 ack 由交付水位驱动、不存在
    // 停摆）。若将来要给命令面也加逃生阀，需同步评估「多发一轮」对 TUI 重绘的
    // 视觉影响。
    //
    // 桌面端必须**恢复 ack**（C-4 把它关了）：命令面无逃生阀 ⇒ 桌面端停 ack =
    // 永久停拉，也就永远观察不到淘汰。移动端保持停 ack（C-3 后未恢复）：窗口
    // 驻留 → 滴灌变慢 → 环在其身后被淘汰 → 它也会拿到 `ring_resync`。两端各自
    // 被淘汰、各自被告知，缺一不可。
    // C-205 超环洪水 ⇒ 落后消费者必被**显式**告知
    //
    // 共享环的淘汰由**最慢的游标**决定，所以“谁被淘汰”只取决于谁落后：快消费者紧跟
    // 产出端，它的游标永不越过 `min_offset` ⇒ **不该**收到重锚（若它也收到，说明
    // 快端被慢端拖累了，那是缺陷而不是特性）。因此本阶段断言两条：
    //   C-205 慢端（移动端，驻留中）被显式告知（缺口不静默）；
    //   C-207 快端（桌面端，持续拉取）**零淘汰、零缺口**（慢端不惩罚快端）。
    //
    // 桌面端必须恢复 ack（C-4 把它关了）：命令面拉取路径没有抗死锁逃生阀（未确认达
    // 上沿就无限 `throttled` 等 ack，这是前端的自愈契约），桌面端停 ack = 永久停拉，
    // 既看不到淘汰、也测不了「快端不受影响」。移动端保持停 ack（C-3 后未恢复）：
    // 窗口驻留 → 滴灌变慢 → 环在其身后被淘汰 → 它拿到 `ring_resync`。
    //
    // 已知不对称（记录在此，不在本阶段断言）：命令面无逃生阀 ⇒ 桌面端 ack 管道停摆
    // 会把该会话的桌面预览锁在静默（生产上 ack 由交付水位驱动，不存在停摆）。若将来
    // 要给命令面也加逃生阀，需同步评估「多发一轮」对 TUI 重绘的视觉影响。
    desktop_acking.store(true, std::sync::atomic::Ordering::SeqCst);
    sessions
        .send_input(&base, &session_c, &seq_command(C_LINES_3 + 1, 560_000), None)
        .await
        .expect("阶段 C-5 写入洪水命令");
    let deadline = Instant::now() + PRESSURE_TIMEOUT;
    loop {
        if ring_resyncs(&recorder_c) > 0 {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "C-205 超时：超环洪水 + 移动端停 ack 下未观察到**环淘汰**重锚\
                 （收到 {} 字节 / {} 帧）——缺口被静默吞掉",
                outputs_c.bytes().len(),
                outputs_c.frame_count()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // 两端各自的序号序列仍然只增不减（重锚后从新基准续拉，不倒带）
    let (raw_desk, desk_truncations, desk_spans) = {
        let guard = desktop_c.lock().unwrap_or_else(|p| p.into_inner());
        (guard.bytes.clone(), guard.truncations, guard.frame_spans.clone())
    };
    let desk_after = seq_numbers(&raw_desk);
    assert_eq!(
        desk_truncations, 0,
        "C-207 共享环不得惩罚快消费者：移动端停 ack + 超环洪水期间，桌面端零淘汰"
    );
    assert_strictly_increasing("C-206 桌面端（洪水后）", &desk_after, &raw_desk, &desk_spans);
    desktop_task.abort();

    // ==================== 收尾 ====================
    // 先停真会话（与场景 4 同口径：终止帧 / 摘除都跑真实路径），再拆环境
    for sid in [&session_a, &session_b, &session_c] {
        sessions.stop_session(&base, sid).await.expect("收尾：停止会话");
    }
    terminal_link_manager().remove(&session_a);
    terminal_link_manager().remove(&session_b);
    terminal_link_manager().remove(&session_c);
    mobile_ctx::clear_identity();
    desktop_ctx::stop_server(handle, server_task).await;
    desktop_ctx::cleanup_temp_dirs();
}
