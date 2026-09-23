//! 对等网络接收侧引擎适配器（issue 10 编排下沉后的宿主残面，票 06 收敛）。
//!
//! 与发送侧（[`super::peer_engine_transfer`]）同构但零侵入：接收方向的任务复用
//! [`PeerTransferDto`] 形状，快照推 `peer:receive` 总线 topic（仅总线：桌面
//! Tauri 前端事件桥随宿主命令面退役，票 06），任务列表/历史/弹窗编排由
//! file-transfer 插件经总线快照驱动。
//!
//! - 事件消费：接收节点装配时的引擎事件通道——`OfferPending` 登记询问
//!   （status=pending + oneshot 回执入表）、`Progress` 节流入账、`Terminal`
//!   结算终态；通道关闭（节点停止）时把在途接收如实落 failed；
//! - 原语面：[`respond_peer_transfer`]（host-peer `respond-transfer` 应答回流）、
//!   收发两方向的暂停/恢复会话控制（wire Pause/Resume 帧），以及 host-peer `close`
//!   接收侧分支的 [`cancel_peer_receiving`]（pending 即拒、running 取消拉取/接收会话）；
//! - 策略面：接收策略（ask/always_accept/always_deny）+ 询问超时 + 落点目录
//!   + 并发上限（host-peer `set-receive-policy` / `set-download-dir` 原语与
//!   发送载荷脉冲入口），变更经 `update_transfer_config`
//!   热生效（在途会话保持建立时快照，下一条连接生效）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use bedcode_peer_net::transfer::batch::validate_ask_timeout_secs;
use bedcode_peer_net::transfer::{FileMeta, DEFAULT_ASK_TIMEOUT_SECS};
use bedcode_peer_net::{NodeId, ReceivePolicy, SharedDirHandler, TerminalState, TransferConfig, TransferEvent};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::peer_engine_transfer::{PeerTransferDto, PeerTransferFileDto};

// ==================== 常量 ====================

/// 进度事件最小发射间隔（与发送侧转发层同值：低于人眼感知阈值）
const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);
/// 接收侧内存终态封顶（会话内追溯用；活跃任务不受影响）
const RECEIVE_TERMINAL_CAP: usize = 100;

// ==================== 数据模型 ====================

/// 接收策略模式（wire/持久化字符串；前端分段控件直用）
pub const POLICY_ASK: &str = "ask";
pub const POLICY_ALWAYS_ACCEPT: &str = "always_accept";
pub const POLICY_ALWAYS_DENY: &str = "always_deny";

/// 接收侧引擎配置（节点生命周期内的内存态；业务设置真源在 file-transfer
/// 插件私有库，经 set-receive-policy / set-download-dir 原语推送闸门）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PeerTransferSettings {
    /// `ask` | `always_accept` | `always_deny`
    policy_mode: String,
    /// ask 模式询问窗口（秒；crate 校验 10..=600）
    ask_timeout_secs: u64,
    /// 桌面端落点覆盖（None = 缺省 Downloads\BedCode\；移动端恒 None）
    download_dir: Option<String>,
    /// 发送方向并发上限（1..=8；插件设置真源，随发送载荷脉冲推送闸门）
    pub(crate) concurrency: u8,
}

impl Default for PeerTransferSettings {
    fn default() -> Self {
        Self {
            policy_mode: POLICY_ASK.to_string(),
            ask_timeout_secs: DEFAULT_ASK_TIMEOUT_SECS,
            download_dir: None,
            concurrency: DEFAULT_CONCURRENCY,
        }
    }
}

/// 发送方向并发默认值（spec §7：默认 3）
pub const DEFAULT_CONCURRENCY: u8 = 3;

/// 校验并发上限（1..=8，spec §7 上限初拟 8）
pub fn validate_concurrency(n: u8) -> std::result::Result<(), String> {
    if (1..=8).contains(&n) {
        Ok(())
    } else {
        Err(format!("concurrency must be 1..=8, got {n}"))
    }
}

impl PeerTransferSettings {
    fn build_policy(&self) -> ReceivePolicy {
        match self.policy_mode.as_str() {
            POLICY_ALWAYS_ACCEPT => ReceivePolicy::AlwaysAccept,
            POLICY_ALWAYS_DENY => ReceivePolicy::AlwaysDeny,
            _ => ReceivePolicy::Ask {
                timeout: std::time::Duration::from_secs(self.ask_timeout_secs),
            },
        }
    }
}

// ==================== 状态容器 ====================

#[derive(Default)]
struct ReceiveInner {
    /// 惰性加载的设置缓存（None = 未读盘）
    settings: Option<PeerTransferSettings>,
    /// 全量接收任务（活跃 + 会话内终态；最新在前）
    tasks: Vec<PeerTransferDto>,
    /// 进度节流基线
    last_emit: Option<tokio::time::Instant>,
}

/// Tauri 托管的接收侧状态容器
///
/// - `inner`：设置缓存 + 任务列表（std Mutex 短临界区，磁盘 IO 一律锁外）；
/// - `pending`：等待宿主应答的询问回执（batch_id → 引擎 oneshot）；
/// - `runtime`：节点运行时的复合处理器句柄与其配置快照——热更新以快照为
///   基底只替换策略/落点，保留 chunk_size 与落位钩子（移动端 MediaStore）。
#[derive(Default)]
pub struct PeerReceiveState {
    inner: Mutex<ReceiveInner>,
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    runtime: tokio::sync::Mutex<Option<(Arc<SharedDirHandler>, TransferConfig)>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn short_fingerprint(node_id: &str) -> String {
    node_id.get(..8).unwrap_or(node_id).to_string()
}

fn is_terminal(task: &PeerTransferDto) -> bool {
    !matches!(task.status.as_str(), "pending" | "running" | "paused")
}

// ==================== 引擎设置（节点生命周期内存态） ====================

/// 惰性加载本次节点生命周期的引擎配置。业务设置由 file-transfer 插件
/// 私有库持有，宿主不再读取旧 transfer_settings.json。
pub(crate) async fn ensure_settings_loaded(app: &AppHandle) -> PeerTransferSettings {
    let state = app.state::<PeerReceiveState>();
    let mut inner = state.inner.lock().expect("peer receive lock poisoned");
    if let Some(settings) = inner.settings.clone() {
        return settings;
    }
    let settings = PeerTransferSettings::default();
    inner.settings = Some(settings.clone());
    settings
}

/// 生效落点解析：覆盖值优先，否则桌面缺省 Downloads\BedCode\
fn effective_download_dir(app: &AppHandle, settings: &PeerTransferSettings) -> crate::Result<PathBuf> {
    if let Some(dir) = settings
        .download_dir
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        return Ok(PathBuf::from(dir));
    }
    super::resolve_download_dir(app)
}

// ==================== 节点装配接缝（peer_net 调用）====================

/// 装配期登记：处理器句柄 + 配置快照（热更新基底）。start_locked 以缺省
/// Ask/60s 构造；用户设置由 file-transfer 插件激活后经 set-receive-policy /
/// set-download-dir 原语推送纠正
pub(crate) async fn register_handler(app: &AppHandle, handler: Arc<SharedDirHandler>, mut config: TransferConfig) {
    let settings = ensure_settings_loaded(app).await;
    if let Ok(dir) = effective_download_dir(app, &settings) {
        config.download_dir = dir;
    }
    config.policy = settings.build_policy();
    handler.update_transfer_config(config.clone());
    let state = app.state::<PeerReceiveState>();
    *state.runtime.lock().await = Some((handler, config));
}

/// 节点停止时摘除句柄（此后设置命令仅改内存态，下次启动重新装配）
pub(crate) async fn clear_handler(app: &AppHandle) {
    let state = app.state::<PeerReceiveState>();
    *state.runtime.lock().await = None;
}

/// 处理器与配置快照句柄（issue 11 远端拉取用：配置为落点/策略热更新基底）
pub(crate) async fn handler_and_config(app: &AppHandle) -> Option<(Arc<SharedDirHandler>, TransferConfig)> {
    let snapshot = {
        let state = app.state::<PeerReceiveState>();
        let guard = state.runtime.lock().await;
        guard.clone()
    };
    snapshot
}

/// 引擎事件消费主循环（替换 issue 07 的日志占位消费者）
pub(crate) async fn drive_receive_events(app: AppHandle, mut rx: mpsc::Receiver<TransferEvent>) {
    while let Some(event) = rx.recv().await {
        match event {
            TransferEvent::OfferPending {
                remote,
                batch_id,
                files,
                total_size,
                reply,
            } => {
                register_offer(&app, remote, batch_id, files, total_size, reply).await;
            }
            TransferEvent::Progress {
                batch_id,
                transferred,
                total,
                rate_bps,
                ..
            } => {
                update_progress(&app, &batch_id, transferred, total, rate_bps);
                throttle_publish(&app);
            }
            TransferEvent::Terminal { batch_id, state, .. } => {
                settle_terminal(&app, &batch_id, state);
            }
            // 数据供方暂停/恢复：本端接收任务状态同步（对端门控推流）
            TransferEvent::Paused { batch_id, .. } => {
                set_receive_pause_status(&app, &batch_id, true).await;
            }
            TransferEvent::Resumed { batch_id, .. } => {
                set_receive_pause_status(&app, &batch_id, false).await;
            }
            // 服务侧拉取事件走独立 serve 通道，由 peer-net 事件适配器消费，
            // 本接收通道理论上收不到；防御性忽略
            TransferEvent::PullServed { .. } => {}
        }
    }
    // 通道关闭 = 节点停止：在途接收如实落终态；pending 回执弃置即自动拒
    fail_active_transfers(&app, "peer-net node stopped");
}

/// 对端展示名解析：发现缓存广播名优先，短指纹兜底（拨入方与本机同网互见，
/// 正常有记录；缺失场景与首连确认桥一致按指纹兜底）
///
/// 必须 async：本函数只在 Tokio 运行时内被调用（pull 队列 / 事件消费循环，
/// 均经 tokio::spawn），不可 block_on——嵌套 runtime 会 panic
/// （"Cannot start a runtime from within a runtime"）。
async fn resolve_peer_name(app: &AppHandle, remote: &NodeId) -> String {
    let fallback = || short_fingerprint(remote.as_str());
    match super::runtime_snapshot(app).await {
        Some((_, cache)) => cache
            .get(remote)
            .map(|record| record.device_name)
            .unwrap_or_else(fallback),
        None => fallback(),
    }
}

/// 远端拉取任务预登记（issue 11）：pull 会话发起前插入 running 行，使后续
/// Progress/Terminal 事件与按批取消入口命中既有任务表（免协商无 pending 阶段）
pub(crate) async fn register_remote_pull(
    app: &AppHandle,
    remote: NodeId,
    batch_id: String,
    rel_path: String,
    total_size: u64,
) {
    let peer_name = resolve_peer_name(app, &remote).await;
    let now = now_ms();
    let dto = PeerTransferDto {
        batch_id: batch_id.clone(),
        node_id: remote.to_string(),
        peer_name,
        direction: "receive".to_string(),
        status: "running".to_string(),
        files: vec![PeerTransferFileDto {
            path: rel_path,
            size: total_size,
        }],
        total_bytes: total_size,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        inner.tasks.insert(0, dto);
    }
    publish(app);
    tracing::info!(batch_id = %batch_id, remote = %remote, "remote pull task registered");
}

/// 会话发起前的拨号等失败落终态：pull 队列中连接未能建立时任务行不得悬挂 running
pub(crate) fn fail_task(app: &AppHandle, batch_id: &str, detail: String) {
    {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.batch_id == batch_id && !is_terminal(t))
        {
            task.status = "failed".to_string();
            task.detail = Some(detail);
            task.updated_at_ms = now_ms();
        }
    }
    publish(app);
}

async fn register_offer(
    app: &AppHandle,
    remote: NodeId,
    batch_id: String,
    files: Vec<FileMeta>,
    total_size: u64,
    reply: tokio::sync::oneshot::Sender<bool>,
) {
    let peer_name = resolve_peer_name(app, &remote).await;
    let now = now_ms();
    let dto = PeerTransferDto {
        batch_id: batch_id.clone(),
        node_id: remote.to_string(),
        peer_name,
        direction: "receive".to_string(),
        status: "pending".to_string(),
        files: files
            .into_iter()
            .map(|f| PeerTransferFileDto {
                path: f.path,
                size: f.size,
            })
            .collect(),
        total_bytes: total_size,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    {
        let state = app.state::<PeerReceiveState>();
        state
            .pending
            .lock()
            .expect("peer receive pending lock poisoned")
            .insert(batch_id.clone(), reply);
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        inner.tasks.insert(0, dto);
    }
    publish(app);
    tracing::info!(
        batch_id = %batch_id,
        remote = %remote,
        "incoming peer transfer offer pending user reply"
    );
}

/// 纯函数：暂停/恢复状态迁移判定（running/pending → paused；paused → running）。
/// 返回目标状态；状态无需改变（已处于目标态/终态）时返回 None。
/// 恢复必须命中 paused——否则对端 Resume 帧到达后任务卡在 paused。
fn receive_pause_target(status: &str, paused: bool) -> Option<&'static str> {
    if paused {
        matches!(status, "running" | "pending").then_some("paused")
    } else if status == "paused" {
        Some("running")
    } else {
        None
    }
}

/// 纯函数：传输是否已完成（字节已满且总量已知；total==0 视为未知不可判）
fn receive_transfer_complete(total: u64, transferred: u64) -> bool {
    total > 0 && transferred >= total
}

/// 纯函数：暂停中的任务是否实为已完成（UI 滞后场景：状态 paused 但字节已满），
/// 用于 settle_terminal 结算 completed 而非 kept for resume
fn receive_full_completed(status: &str, transferred: u64, total: u64, terminal_completed: bool) -> bool {
    status == "paused" && terminal_completed && receive_transfer_complete(total, transferred)
}

/// 纯函数：进度入账（paused 不覆盖为 running——残留 Progress 事件会把按钮
/// 从「恢复」弹回「暂停」；字节/速率照常更新供恢复后进度衔接）
fn apply_receive_progress(task: &mut PeerTransferDto, transferred: u64, total: u64, rate_bps: f64, ts: u64) {
    if task.status != "paused" {
        task.status = "running".to_string();
    }
    task.transferred_bytes = transferred;
    if total > 0 {
        task.total_bytes = total;
    }
    task.rate_bps = rate_bps;
    task.updated_at_ms = ts;
}

/// 进度入账（pending → running：AlwaysAccept 策略无询问阶段直接进数据面）
///
/// `total` 为引擎批内总大小真源：远端拉取任务预登记时大小未知（pull spec
/// size 恒 0），首个 Progress 即补正 total_bytes——否则 totalBytes 恒 0，
/// 前端进度条永远停在 0%。
fn update_progress(app: &AppHandle, batch_id: &str, transferred: u64, total: u64, rate_bps: f64) {
    let state = app.state::<PeerReceiveState>();
    let mut inner = state.inner.lock().expect("peer receive lock poisoned");
    if let Some(task) = inner
        .tasks
        .iter_mut()
        .find(|t| t.batch_id == batch_id && !is_terminal(t))
    {
        apply_receive_progress(task, transferred, total, rate_bps, now_ms());
    }
}

/// 终态结算：状态映射与发送侧 apply_terminal 同构。接收侧无 epoch 概念：
/// 同批重发由发送端在新连接上发起、引擎按落盘断点续传，本端每条连接恰好
/// 一次 Terminal。
/// 终态结算：状态映射 → 历史封顶 → 即时推送。任务已 paused（用户暂停拉取/推送）
/// 时跳过结算——行保留，待用户恢复或取消（与发送侧 apply_terminal 同语义）。
fn settle_terminal(app: &AppHandle, batch_id: &str, terminal: TerminalState) {
    let (status, detail, reject_reason) = match &terminal {
        TerminalState::Completed => ("completed".to_string(), None, None),
        TerminalState::Rejected { reason } => ("rejected".to_string(), None, Some(reason.as_str().to_string())),
        TerminalState::Cancelled { by_peer } => (
            "cancelled".to_string(),
            // 机器可读原因码（前端 i18n 映射；兼容映射旧本地化文本），
            // 禁止把人类文案直接落 wire
            Some(if *by_peer {
                "cancelled-by-sender".to_string()
            } else {
                "cancelled-by-self".to_string()
            }),
            None,
        ),
        TerminalState::Failed { detail } => ("failed".to_string(), Some(detail.clone()), None),
    };
    {
        let state = app.state::<PeerReceiveState>();
        state
            .pending
            .lock()
            .expect("peer receive pending lock poisoned")
            .remove(batch_id);
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.batch_id == batch_id && !is_terminal(t))
        {
            // 用户暂停分支：会话终态只是中断确认——不落终态、不覆盖进度展示。
            // 例外：Completed 且字节已满（UI 滞后时点暂停的已完成任务）——
            // 结算 completed 而不是卡在 paused（真机现象：已传完仍可点暂停）
            let full_completed = receive_full_completed(
                &task.status,
                task.transferred_bytes,
                task.total_bytes,
                matches!(terminal, TerminalState::Completed),
            );
            if task.status == "paused" && !full_completed {
                tracing::info!(batch_id = %batch_id, state = ?terminal, "receive session ended while paused (kept for resume)");
                return;
            }
            task.status = status;
            // 完成结算：最后一条 Progress 可能略低于总量，归整为满额
            if task.status == "completed" {
                task.transferred_bytes = task.total_bytes;
            }
            task.detail = detail;
            task.reject_reason = reject_reason;
            task.updated_at_ms = now_ms();
        }
        evict_terminal_cap_locked(&mut inner.tasks);
    }
    tracing::info!(batch_id = %batch_id, state = ?terminal, "peer receive session ended");
    publish(app);
}

/// 接收任务暂停/恢复状态同步（对端 Pause/Resume 帧到达，或本端命令置位）
pub(crate) async fn set_receive_pause_status(app: &AppHandle, batch_id: &str, paused: bool) {
    {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.batch_id == batch_id && !is_terminal(t))
        {
            // 暂停：running/pending → paused；恢复：paused → running。
            // 恢复必须命中 paused——否则对端 Resume 帧到达后任务卡在 paused
            // （真机现象：移动端恢复后桌面端任务状态不再同步）
            if let Some(target) = receive_pause_target(&task.status, paused) {
                task.status = target.to_string();
                task.rate_bps = 0.0;
                task.updated_at_ms = now_ms();
            }
        }
    }
    publish(app);
}

/// 节点停止收尾：全部活跃接收落 failed 并推送（pending 回执随表清空弃置）
fn fail_active_transfers(app: &AppHandle, detail: &str) {
    let had_active = {
        let state = app.state::<PeerReceiveState>();
        state
            .pending
            .lock()
            .expect("peer receive pending lock poisoned")
            .clear();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        let mut changed = false;
        for task in inner.tasks.iter_mut().filter(|t| !is_terminal(t)) {
            task.status = "failed".to_string();
            task.detail = Some(detail.to_string());
            task.updated_at_ms = now_ms();
            changed = true;
        }
        changed
    };
    if had_active {
        tracing::warn!("peer-net stopped with active receiving transfers, marked failed");
        publish(app);
    }
}

/// 终态封顶：最新在前保序保留前 CAP 条终态，活跃任务不受影响
fn evict_terminal_cap_locked(tasks: &mut Vec<PeerTransferDto>) {
    let mut kept = 0usize;
    tasks.retain(|t| {
        if !is_terminal(t) {
            return true;
        }
        kept += 1;
        kept <= RECEIVE_TERMINAL_CAP
    });
}

// ==================== 发布 ====================

fn snapshot(app: &AppHandle) -> Vec<PeerTransferDto> {
    let state = app.state::<PeerReceiveState>();
    let inner = state.inner.lock().expect("peer receive lock poisoned");
    inner.tasks.clone()
}

/// 全量列表推送（与发送侧 peer-transfer-changed 平行的独立通道；仅总线
/// 单路，票 06——桌面 Tauri 前端事件桥随宿主命令面退役）
fn publish(app: &AppHandle) {
    let payload = serde_json::to_value(snapshot(app)).unwrap_or_default();
    {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        inner.last_emit = Some(tokio::time::Instant::now());
    }
    super::publish_bus_only("peer-receive-changed", payload);
}

/// 进度节流推送：距上次发射不足间隔则跳过（终态路径不经此函数即时推送）
fn throttle_publish(app: &AppHandle) {
    let due = {
        let state = app.state::<PeerReceiveState>();
        let inner = state.inner.lock().expect("peer receive lock poisoned");
        match inner.last_emit {
            Some(last) if last.elapsed() < PROGRESS_EMIT_INTERVAL => false,
            _ => true,
        }
    };
    if due {
        publish(app);
    }
}

// ==================== 传输控制原语面 ====================

/// 应答传输询问（接受全部 / 拒绝全部；host-peer `respond-transfer` 原语）。
/// 返回是否成功送达回执——false 表示批已超时被引擎自动拒或 ID 未知。
///
/// 接受路径乐观置 running（首个 Progress 到达前快照即呈现进行中）；拒绝路径
/// 乐观置 rejected/user-rejected，引擎随后的 Terminal 为幂等覆写。
pub async fn respond_peer_transfer(app: AppHandle, batch_id: String, accepted: bool) -> crate::Result<bool> {
    let state = app.state::<PeerReceiveState>();
    let reply = state
        .pending
        .lock()
        .expect("peer receive pending lock poisoned")
        .remove(&batch_id);
    let Some(reply) = reply else {
        tracing::warn!(batch_id = %batch_id, "respond for unknown/expired receive offer");
        return Ok(false);
    };
    let delivered = reply.send(accepted).is_ok();
    {
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.batch_id == batch_id && t.status == "pending")
        {
            if accepted {
                task.status = "running".to_string();
            } else {
                task.status = "rejected".to_string();
                task.reject_reason = Some("user-rejected".to_string());
            }
            task.updated_at_ms = now_ms();
        }
    }
    tracing::info!(batch_id = %batch_id, accepted, delivered, "peer transfer offer answered");
    publish(&app);
    Ok(delivered)
}

/// 取消接收任务（host-peer `close` 原语的接收侧分支）：pending 视同用户拒绝
/// 并回执 false；running 先查远端拉取会话（本端是拉取发起方），未命中再回退
/// 服务端会话表（push 接收批）。返回是否命中。
pub async fn cancel_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerReceiveState>();
    if dismiss_pending_offer(&state, &batch_id) {
        publish(&app);
        tracing::info!(batch_id = %batch_id, "pending receive dismissed as rejected");
        return Ok(true);
    }
    let hit = match super::peer_engine_remote::cancel_pull(&app, &batch_id) {
        true => true,
        false => match state.runtime.lock().await.as_ref() {
            Some((handler, _)) => handler.cancel_transfer(&batch_id),
            None => false,
        },
    };
    tracing::debug!(batch_id = %batch_id, hit, "cancel receiving requested");
    Ok(hit)
}

/// 待决询问破除：pending 视同用户拒绝——移除回执通道并回执 `false`（送达失败 =
/// 引擎侧询问已先行结算，本地仍按拒绝落态），任务落 rejected。命中返回 true。
fn dismiss_pending_offer(state: &PeerReceiveState, batch_id: &str) -> bool {
    let reply = state
        .pending
        .lock()
        .expect("peer receive pending lock poisoned")
        .remove(batch_id);
    match reply {
        None => false,
        Some(reply) => {
            // 回执送达失败 = 引擎侧询问已先行结算（超时/对端撤销），本地仍按拒绝落态
            if reply.send(false).is_err() {
                tracing::debug!(batch_id = %batch_id, "receive offer already settled by engine");
            }
            {
                let mut inner = state.inner.lock().expect("peer receive lock poisoned");
                if let Some(task) = inner
                    .tasks
                    .iter_mut()
                    .find(|t| t.batch_id == batch_id && t.status == "pending")
                {
                    task.status = "rejected".to_string();
                    task.reject_reason = Some("user-rejected".to_string());
                    task.updated_at_ms = now_ms();
                }
            }
            true
        }
    }
}

/// 暂停接收任务：两条路径都经 wire Pause 帧请求对端数据供方门控推流
/// （连接保持、断点不丢），任务置 paused 不落终态：
///
/// - 拉取会话（本端是拉取发起方，对端 serve 供流）：经 pull 暂停句柄下发；
/// - push 接收批（对端是发送方）：经接收会话暂停句柄下发。
///
/// 双端对同一传输任务对称：本端收/本端供两个视角都能本地暂停。返回是否命中。
pub async fn pause_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    // 顺序关键：先把任务落 paused 再下发 wire 命令。引擎终态事件（对端中断/
    // 会话失败）可能在命令生效前后到达，若任务仍是 running，settle_terminal
    // 会把它结算成 failed 并归档历史——真机现象「点暂停，任务直接变历史」。
    // 已完成校验：字节已满的任务不允许暂停（UI 可能滞后显示未完成，实际已
    // 传完落盘），直接结算 completed，避免误标 paused 卡住后续恢复
    let already_complete = {
        let state = app.state::<PeerReceiveState>();
        let inner = state.inner.lock().expect("peer receive lock poisoned");
        inner.tasks.iter().any(|t| {
            t.batch_id == batch_id && !is_terminal(t) && receive_transfer_complete(t.total_bytes, t.transferred_bytes)
        })
    };
    if already_complete {
        settle_terminal(&app, &batch_id, TerminalState::Completed);
        tracing::debug!(batch_id = %batch_id, "pause skipped: receive transfer already complete");
        return Ok(true);
    }
    set_receive_pause_status(&app, &batch_id, true).await;
    let mut hit = super::peer_engine_remote::pause_pull(&app, &batch_id).await;
    if !hit {
        hit = pause_receive_session(&app, &batch_id, true).await;
    }
    if !hit {
        // 无活动会话（已终态/会话未建立）：还原状态，避免呈现假暂停
        set_receive_pause_status(&app, &batch_id, false).await;
    }
    tracing::debug!(batch_id = %batch_id, hit, "pause receiving requested");
    Ok(hit)
}

/// 恢复暂停的接收任务：写 Resume 帧续流（对端数据供方解除门控）
pub async fn resume_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let mut hit = super::peer_engine_remote::resume_pull(&app, &batch_id).await;
    if !hit {
        hit = pause_receive_session(&app, &batch_id, false).await;
    }
    if hit {
        set_receive_pause_status(&app, &batch_id, false).await;
    }
    tracing::debug!(batch_id = %batch_id, hit, "resume receiving requested");
    Ok(hit)
}

/// 经接收会话暂停句柄下发暂停/恢复（push 接收批：本端写 Pause/Resume 帧
/// 请求对端发送会话门控推流）。
async fn pause_receive_session(app: &AppHandle, batch_id: &str, paused: bool) -> bool {
    let handler = {
        let state = app.state::<PeerReceiveState>();
        let guard = state.runtime.lock().await;
        guard.as_ref().map(|(handler, _)| Arc::clone(handler))
    };
    match handler {
        Some(handler) => handler.set_receive_paused(batch_id, paused).await,
        None => false,
    }
}

/// 恢复全部暂停的接收任务（逐条经 wire Resume 帧续流；无活动会话的保持
/// paused 等待取消/重试）。返回实际恢复数。由「全部继续」入口调用，使该
/// 按钮对发送/接收两方向的任务一致生效。
pub(crate) async fn resume_all_peer_receiving(app: &AppHandle) -> usize {
    let ids: Vec<String> = {
        let state = app.state::<PeerReceiveState>();
        let inner = state.inner.lock().expect("peer receive lock poisoned");
        inner
            .tasks
            .iter()
            .filter(|t| t.status == "paused")
            .map(|t| t.batch_id.clone())
            .collect()
    };
    let mut resumed = 0usize;
    for batch_id in ids {
        if resume_peer_receiving(app.clone(), batch_id).await.unwrap_or(false) {
            resumed += 1;
        }
    }
    resumed
}

// ==================== 设置闸门 ====================

// 设置真源在 file-transfer 插件私有库，宿主只持有本次节点生命周期内的引擎闸门
// 内存态；查询/写入命令面与旧 `transfer_settings.json` 读写随票 06 退役，
// 以下入口只由 host-peer 原语（`peer_net::*_for_plugin`）调用。

/// 更新接收策略与询问超时（校验后运行中节点热生效）
pub async fn set_peer_receive_policy(app: AppHandle, mode: String, timeout_secs: u64) -> crate::Result<()> {
    if !matches!(mode.as_str(), POLICY_ASK | POLICY_ALWAYS_ACCEPT | POLICY_ALWAYS_DENY) {
        return Err(crate::AppError::InvalidInput(format!(
            "set receive policy: unknown mode '{mode}'"
        )));
    }
    validate_ask_timeout_secs(timeout_secs)
        .map_err(|detail| crate::AppError::InvalidInput(format!("set receive policy: ask timeout {detail}")))?;
    let mut settings = ensure_settings_loaded(&app).await;
    settings.policy_mode = mode;
    settings.ask_timeout_secs = timeout_secs;
    apply_settings(&app, &settings).await;
    tracing::info!(
        mode = %settings.policy_mode,
        timeout = settings.ask_timeout_secs,
        "receive policy updated"
    );
    Ok(())
}

/// 桌面端：更换接收落点目录（None = 恢复缺省 Downloads\BedCode\）。
/// 目录即时创建；运行中节点热生效。
pub async fn set_peer_download_dir(app: AppHandle, path: Option<String>) -> crate::Result<()> {
    let mut settings = ensure_settings_loaded(&app).await;
    match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(dir) => {
            tokio::fs::create_dir_all(dir).await.map_err(|e| {
                crate::AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("create download dir '{dir}' failed: {e}"),
                ))
            })?;
            settings.download_dir = Some(dir.to_string());
        }
        None => settings.download_dir = None,
    }
    apply_settings(&app, &settings).await;
    tracing::info!(dir = ?settings.download_dir, "receive download dir updated (desktop)");
    Ok(())
}

/// 设置发送方向并发上限（仅影响后续排队调度，无需热更新运行时）
pub async fn set_peer_transfer_concurrency(app: AppHandle, concurrency: u8) -> crate::Result<()> {
    validate_concurrency(concurrency).map_err(crate::AppError::InvalidInput)?;
    let mut settings = ensure_settings_loaded(&app).await;
    if settings.concurrency == concurrency {
        return Ok(());
    }
    settings.concurrency = concurrency;
    apply_settings(&app, &settings).await;
    tracing::info!(concurrency, "transfer concurrency updated");
    Ok(())
}

/// 应用新设置：内存态替换 + 运行中处理器热更新（以装配快照为基底替换策略/落点）
async fn apply_settings(app: &AppHandle, settings: &PeerTransferSettings) {
    let state = app.state::<PeerReceiveState>();
    state.inner.lock().expect("peer receive lock poisoned").settings = Some(settings.clone());

    let mut runtime = state.runtime.lock().await;
    if let Some((handler, base)) = runtime.as_ref() {
        let mut config = base.clone();
        config.policy = settings.build_policy();
        if let Ok(dir) = effective_download_dir(app, settings) {
            config.download_dir = dir;
        }
        handler.update_transfer_config(config.clone());
        *runtime = Some((Arc::clone(handler), config));
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: &str, timeout: u64) -> PeerTransferSettings {
        PeerTransferSettings {
            policy_mode: mode.to_string(),
            ask_timeout_secs: timeout,
            download_dir: None,
            concurrency: DEFAULT_CONCURRENCY,
        }
    }

    #[test]
    fn settings_default_is_ask_with_default_window() {
        let settings = PeerTransferSettings::default();
        assert_eq!(settings.policy_mode, POLICY_ASK);
        assert_eq!(settings.ask_timeout_secs, DEFAULT_ASK_TIMEOUT_SECS);
        assert!(settings.download_dir.is_none());
        match settings.build_policy() {
            ReceivePolicy::Ask { timeout } => {
                assert_eq!(timeout, std::time::Duration::from_secs(DEFAULT_ASK_TIMEOUT_SECS))
            }
            other => panic!("default must build Ask, got {other:?}"),
        }
    }

    #[test]
    fn policy_modes_map_to_engine_variants() {
        assert!(matches!(
            settings(POLICY_ALWAYS_ACCEPT, 60).build_policy(),
            ReceivePolicy::AlwaysAccept
        ));
        assert!(matches!(
            settings(POLICY_ALWAYS_DENY, 60).build_policy(),
            ReceivePolicy::AlwaysDeny
        ));
        match settings(POLICY_ASK, 30).build_policy() {
            ReceivePolicy::Ask { timeout } => {
                assert_eq!(timeout, std::time::Duration::from_secs(30))
            }
            other => panic!("ask must build Ask, got {other:?}"),
        }
    }

    #[test]
    fn concurrency_bounds_are_one_to_eight() {
        assert!(validate_concurrency(1).is_ok(), "下限 1 必须放行");
        assert!(validate_concurrency(8).is_ok(), "上限 8 必须放行");
        assert!(validate_concurrency(0).is_err(), "0 并发会让发送队列饿死");
        assert!(validate_concurrency(9).is_err(), "越上界并发按 spec §7 拒绝");
        assert_eq!(DEFAULT_CONCURRENCY, 3, "默认并发跟随 spec §7");
    }

    #[test]
    fn terminal_cap_evicts_oldest_and_keeps_active() {
        let mut tasks: Vec<PeerTransferDto> = Vec::new();
        for i in (0..(RECEIVE_TERMINAL_CAP + 3)).rev() {
            tasks.push(PeerTransferDto {
                batch_id: format!("b-{i}"),
                node_id: "a".repeat(64),
                peer_name: "Peer".to_string(),
                direction: "receive".to_string(),
                status: "completed".to_string(),
                files: Vec::new(),
                total_bytes: 1,
                transferred_bytes: 1,
                rate_bps: 0.0,
                detail: None,
                reject_reason: None,
                created_at_ms: i as u64,
                updated_at_ms: i as u64,
            });
        }
        tasks.push(PeerTransferDto {
            batch_id: "b-active".to_string(),
            node_id: "a".repeat(64),
            peer_name: "Peer".to_string(),
            direction: "receive".to_string(),
            status: "running".to_string(),
            files: Vec::new(),
            total_bytes: 1,
            transferred_bytes: 0,
            rate_bps: 0.0,
            detail: None,
            reject_reason: None,
            created_at_ms: 9_999,
            updated_at_ms: 9_999,
        });

        evict_terminal_cap_locked(&mut tasks);

        let terminals = tasks.iter().filter(|t| is_terminal(t)).count();
        assert_eq!(terminals, RECEIVE_TERMINAL_CAP);
        assert!(!tasks.iter().any(|t| t.batch_id == "b-0"), "oldest evicted");
        assert!(tasks.iter().any(|t| t.batch_id == "b-active"), "active kept");
    }

    // ==================== 暂停/恢复状态迁移（Bug：暂停成功仍显暂停） ====================

    /// 接收侧暂停/恢复目标状态映射：running/pending → paused；paused → running
    #[test]
    fn receive_pause_target_maps_run_and_pending_to_paused() {
        assert_eq!(receive_pause_target("running", true), Some("paused"));
        assert_eq!(receive_pause_target("pending", true), Some("paused"));
        // 已暂停/终态不再变化
        assert_eq!(receive_pause_target("paused", true), None);
        assert_eq!(receive_pause_target("completed", true), None);
    }

    /// 恢复必须命中 paused——否则 Resume 帧到达后任务卡在 paused（真机现象）
    #[test]
    fn receive_pause_target_resume_hits_only_paused() {
        assert_eq!(receive_pause_target("paused", false), Some("running"));
        assert_eq!(receive_pause_target("running", false), None);
        assert_eq!(receive_pause_target("pending", false), None);
        assert_eq!(receive_pause_target("completed", false), None);
    }

    /// 已完成判定：总量已知且字节已满（total==0 不可判）
    #[test]
    fn receive_transfer_complete_requires_known_total_and_full_bytes() {
        assert!(receive_transfer_complete(100, 100));
        assert!(receive_transfer_complete(100, 120), "overshoot defensive");
        assert!(!receive_transfer_complete(100, 99));
        assert!(!receive_transfer_complete(0, 0), "unknown total");
        assert!(!receive_transfer_complete(0, 50), "unknown total with bytes");
    }

    /// full_completed：仅 paused+Completed+满字节 判定为已完成（否则 kept for resume）
    #[test]
    fn receive_full_completed_only_for_paused_completed_full() {
        assert!(receive_full_completed("paused", 100, 100, true));
        assert!(!receive_full_completed("paused", 99, 100, true), "not full");
        assert!(
            !receive_full_completed("paused", 100, 100, false),
            "non-completed terminal"
        );
        assert!(!receive_full_completed("running", 100, 100, true), "running not paused");
        assert!(!receive_full_completed("paused", 0, 0, true), "unknown total");
    }

    /// 进度入账：paused 保持暂停（不被打回 running）、字节照常更新
    #[test]
    fn apply_receive_progress_keeps_paused_but_updates_bytes() {
        let mut task = PeerTransferDto {
            batch_id: "b".to_string(),
            node_id: "a".repeat(64),
            peer_name: "Peer".to_string(),
            direction: "receive".to_string(),
            status: "paused".to_string(),
            files: Vec::new(),
            total_bytes: 100,
            transferred_bytes: 40,
            rate_bps: 0.0,
            detail: None,
            reject_reason: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        apply_receive_progress(&mut task, 55, 100, 3.5, 2);
        assert_eq!(task.status, "paused", "暂停期间不得被打回 running");
        assert_eq!(task.transferred_bytes, 55);
        assert_eq!(task.rate_bps, 3.5);
        assert_eq!(task.updated_at_ms, 2);
    }

    /// 进度入账：running 任务照常推进（total 首次补正）
    #[test]
    fn apply_receive_progress_advances_running_and_backfills_total() {
        let mut task = PeerTransferDto {
            batch_id: "b".to_string(),
            node_id: "a".repeat(64),
            peer_name: "Peer".to_string(),
            direction: "receive".to_string(),
            status: "running".to_string(),
            files: Vec::new(),
            total_bytes: 0,
            transferred_bytes: 0,
            rate_bps: 0.0,
            detail: None,
            reject_reason: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        apply_receive_progress(&mut task, 10, 200, 1.0, 2);
        assert_eq!(task.status, "running");
        assert_eq!(task.transferred_bytes, 10);
        assert_eq!(task.total_bytes, 200, "首个 Progress 补正总量");
    }
}

/// ③ 回归：接收侧取消链路（票 06 改名时曾断链）。
///
/// 覆盖 `cancel_peer_receiving`：pending 询问视同拒绝并回执 false；无 pending、
/// 无 runtime（pull 未命中）时返回 false。真实 mTLS loopback（push 取消走
/// `handler.cancel_transfer` / pull 走 `cancel_pull`）属后续真实 wasm 闭环，本
/// 模块先锁 dead-code 级契约：pending 分支是「证明取消接收不是纯 shell」的入口。
#[cfg(test)]
mod cancel_regression_tests {
    use super::*;

    #[tokio::test]
    async fn dismiss_pending_offer_dismisses_as_reject_and_replies_false() {
        let state = PeerReceiveState::default();
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.pending.lock().expect("lock").insert("b1".to_string(), tx);
        state.inner.lock().expect("lock").tasks.insert(
            0,
            PeerTransferDto {
                batch_id: "b1".to_string(),
                node_id: "a".repeat(64),
                peer_name: "Peer".to_string(),
                direction: "receive".to_string(),
                status: "pending".to_string(),
                files: Vec::new(),
                total_bytes: 1,
                transferred_bytes: 0,
                rate_bps: 0.0,
                detail: None,
                reject_reason: None,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        );

        assert!(dismiss_pending_offer(&state, "b1"), "pending offer 破除必须命中 true");
        assert_eq!(rx.await, Ok(false), "pending 询问必须回执 false（视同拒绝）");
        assert!(
            state.pending.lock().expect("lock").get("b1").is_none(),
            "pending 必须移除"
        );
        let inner = state.inner.lock().expect("lock");
        let task = inner.tasks.iter().find(|t| t.batch_id == "b1").expect("任务行保留");
        assert_eq!(task.status, "rejected", "pending 落态 rejected");
        assert_eq!(task.reject_reason.as_deref(), Some("user-rejected"));
    }

    #[test]
    fn dismiss_pending_offer_miss_returns_false_and_keeps_state() {
        let state = PeerReceiveState::default();
        assert!(
            !dismiss_pending_offer(&state, "unknown"),
            "无 pending → false（不是错误）"
        );
        assert!(state.pending.lock().expect("lock").is_empty());
    }
}
