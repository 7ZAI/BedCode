//! 对等网络发送侧（issue 09）：扇出发送编排 + 进度转发 + 终态历史。
//!
//! 领域模型（spec 决策 2）：「群发」仅是前端编排概念——宿主为每个接收方
//! 维护一条相互独立的传输批（独立 batch_id / 独立进度 / 独立策略闸门），
//! 任一接收方拒绝不影响其余。本模块职责：
//!
//! - 命令面：[`send_files_to_peer`]（收集源文件 → 拨号 → `send_batch` 后台
//!   会话）、[`cancel_peer_transfer`]、[`retry_peer_transfer`]（同批 ID 重发 =
//!   断点续传，断点真源在接收端落盘侧）、[`list_peer_transfers`] /
//!   [`clear_peer_transfer_history`]；
//! - 事件桥：任务列表变更全量推送 `peer-transfer-changed`（与 issue 08 的
//!   `peer-devices-changed` 同款范式）；引擎 Progress 按 chunk 高频发射，
//!   转发层做时间窗节流防 IPC 事件风暴；
//! - 历史：终态记录持久化 `transfer_history.json`（封顶滚动淘汰、原子写），
//!   升级/重启后仍可追溯（spec 用户故事 26/28）；活跃任务的源文件清单仅存
//!   内存——重启后的历史条目如实保留终态但不可重试（重试需重新发起）。
//!
//! 接收方向（direction=receive）的记录形态在此预留，接入属 issue 10。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bedcode_peer_net::{
    CancelToken, Connection, DiscoveredPeerRecord, OutgoingFile, PeerNetNode, TerminalState,
    TransferEvent,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::peer_receive::ensure_settings_loaded;
use tokio::sync::mpsc;

// ==================== 常量 ====================

/// 历史文件名（数据目录内，与身份/可信表同源）
const HISTORY_FILE: &str = "transfer_history.json";
/// 历史封顶条数：超出后按最旧优先滚动淘汰（spec 用户故事 26）
const HISTORY_CAP: usize = 200;
/// 进度事件最小发射间隔：引擎按 ≤64KiB chunk 发射 Progress，
/// 不加节流会在高速链路打爆 IPC；数值远低于人眼感知阈值
const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);
/// 单批文件数上限：目录递归收集的失控保护
const MAX_FILES_PER_BATCH: usize = 512;
/// 历史文件格式版本（未来字段演进时 fail-fast）
const HISTORY_FORMAT_VERSION: u32 = 1;

// ==================== 数据模型 ====================

/// 单条传输任务/历史记录（camelCase 直跨 IPC 与历史文件，两端前端同构消费）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerTransferDto {
    /// 批 ID（发送方生成；重试复用同 ID 以命中接收端断点）
    pub batch_id: String,
    /// 完整对端节点 ID（64 位小写 hex）
    pub node_id: String,
    /// 对端展示名（发起时从发现缓存解析；离线兜底短指纹）
    pub peer_name: String,
    /// 方向：`send` | `receive`（receive 由 issue 10 接入）
    pub direction: String,
    /// `running` | `completed` | `rejected` | `cancelled` | `failed`
    pub status: String,
    /// 批内文件清单（remote 相对路径 + 字节数）
    pub files: Vec<PeerTransferFileDto>,
    /// 批内总大小（字节）
    pub total_bytes: u64,
    /// 本批累计已传字节（含续传基线口径，跨重试会话单调推进）
    pub transferred_bytes: u64,
    /// 瞬时速率（B/s；引擎滑动窗口口径）
    pub rate_bps: f64,
    /// 失败/取消补充说明（引擎错误链原文，仅展示用途）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 拒绝原因（status=rejected 时存在；wire kebab-case）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    /// 发起时刻（Unix 毫秒）
    pub created_at_ms: u64,
    /// 最近状态变更时刻（Unix 毫秒）
    pub updated_at_ms: u64,
}

/// 批内单文件元数据
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerTransferFileDto {
    /// 发送方相对路径（`/` 分隔；接收端落位形状）
    pub path: String,
    /// 文件字节数
    pub size: u64,
}

// ==================== 状态容器 ====================

/// 内存任务条目：对外 DTO + 重试所需的源清单
struct SendTask {
    dto: PeerTransferDto,
    /// 发送源清单（仅内存存活；重启后历史条目 sources 为空即不可重试）
    sources: Vec<OutgoingFile>,
}

/// Tauri 托管的传输任务状态容器
///
/// - `inner` 单把 Mutex 串行化全部读写（临界区均为内存短操作），磁盘 IO 一律
///   锁外：首次惰性加载与终态落盘都经 spawn_blocking 移出异步上下文；
/// - `sessions` 登记活跃会话的取消令牌与代次：重试换发新令牌并递增代次，
///   陈旧会话迟到的事件按代次丢弃，防止覆盖新一轮状态。
#[derive(Default)]
pub struct PeerTransferState {
    inner: Mutex<PeerTransferInner>,
    sessions: Mutex<HashMap<String, (CancelToken, u64)>>,
}

#[derive(Default)]
struct PeerTransferInner {
    /// 历史是否已从磁盘加载（进程内一次）
    loaded: bool,
    /// 全量任务列表（活跃 + 历史；最新在前）
    tasks: Vec<SendTask>,
}

/// 历史文件磁盘形态
#[derive(Serialize, Deserialize)]
struct HistoryFile {
    version: u32,
    entries: Vec<PeerTransferDto>,
}

/// 跨实例串行化历史写盘（并发终态下避免 tmp+rename 竞态）
static HISTORY_SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_terminal_status(status: &str) -> bool {
    status != "running"
}

/// 重试资格：失败/取消/被拒可重试（completed 无意义，running 在途禁重复发起）
fn retryable(status: &str) -> bool {
    matches!(status, "failed" | "cancelled" | "rejected")
}

impl PeerTransferState {
    /// 登记新会话并返回代次（每批从 0 起；重试经 next_session_epoch 递增）
    fn register_session(&self, batch_id: &str) -> u64 {
        let mut guard = self.sessions.lock().expect("peer transfer sessions poisoned");
        let epoch = guard.get(batch_id).map(|(_, e)| *e + 1).unwrap_or(0);
        guard.insert(batch_id.to_string(), (CancelToken::new(), epoch));
        epoch
    }

    /// 当前会话令牌（取消入口/会话启动取用；不存在或已结算返回 None）
    fn cancel_token_of(&self, batch_id: &str) -> Option<CancelToken> {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|(token, _)| token.clone())
    }

    fn session_is_current(&self, batch_id: &str, epoch: u64) -> bool {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|(_, e)| *e == epoch)
            .unwrap_or(false)
    }

    fn unregister_session(&self, batch_id: &str) {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .remove(batch_id);
    }
}

// ==================== 命令面 ====================

/// 当前全量任务列表（活跃 + 历史，最新在前；未加载时先惰性读盘）
#[tauri::command]
pub async fn list_peer_transfers(app: AppHandle) -> crate::Result<Vec<PeerTransferDto>> {
    ensure_history_loaded(&app).await?;
    Ok(snapshot(&app))
}

/// 清空传输历史（终态记录全部移除；进行中任务不受影响）。返回清除条数。
#[tauri::command]
pub async fn clear_peer_transfer_history(app: AppHandle) -> crate::Result<usize> {
    ensure_history_loaded(&app).await?;
    let removed = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let before = inner.tasks.len();
        let removed_ids: HashSet<String> = inner
            .tasks
            .iter()
            .filter(|t| is_terminal_status(&t.dto.status))
            .map(|t| t.dto.batch_id.clone())
            .collect();
        inner.tasks.retain(|t| !removed_ids.contains(&t.dto.batch_id));
        before - inner.tasks.len()
    };
    if removed > 0 {
        persist_history(&app).await;
        publish(&app);
    }
    tracing::info!(count = removed, "peer transfer history cleared");
    Ok(removed)
}

/// 取消进行中的发送任务（幂等：已终态返回 false）
#[tauri::command]
pub async fn cancel_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerTransferState>();
    match state.cancel_token_of(&batch_id) {
        Some(token) => {
            tracing::info!(batch_id = %batch_id, "peer transfer cancel requested");
            token.cancel();
            Ok(true)
        }
        None => Ok(false),
    }
}

/// 向一个可信对端推送一批文件/文件夹（扇出 = 前端对本命令的多节点调用）
///
/// 目录递归展开为相对路径清单（保持目录形状，跨平台 `/` 分隔），同名目标
/// 自动去重编号。命令立即返回 running 态记录；进度经 `peer-transfer-changed`
/// 全量列表事件持续推送，终态落历史。
#[tauri::command]
pub async fn send_files_to_peer(
    app: AppHandle,
    node_id: String,
    paths: Vec<String>,
) -> crate::Result<PeerTransferDto> {
    let parsed = super::peer_net::parse_node_id(&node_id)?;
    if paths.is_empty() || paths.iter().all(|p| p.trim().is_empty()) {
        return Err(crate::AppError::InvalidInput(
            "send_files_to_peer: paths must not be empty".to_string(),
        ));
    }
    ensure_history_loaded(&app).await?;
    let Some((node, cache)) = super::peer_net::runtime_snapshot(&app).await else {
        return Err(crate::AppError::Internal(
            "peer transfer send failed: node not started".to_string(),
        ));
    };
    let record = cache.get(&parsed).ok_or_else(|| {
        crate::AppError::Internal(
            "peer transfer send failed: peer not in discovery cache (offline or unknown)"
                .to_string(),
        )
    })?;
    let peer_name = record.device_name.clone();

    // 目录递归是潜在阻塞 IO：移出异步上下文（上限保护在收集函数内）
    let trimmed = paths.into_iter().map(|p| p.trim().to_string()).collect::<Vec<_>>();
    let collected =
        tauri::async_runtime::spawn_blocking(move || collect_outgoing_files(&trimmed))
            .await
            .map_err(|e| crate::AppError::Internal(format!("join source collection failed: {e}")))??;

    let batch_id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    let dto = PeerTransferDto {
        batch_id: batch_id.clone(),
        node_id: parsed.to_string(),
        peer_name,
        direction: "send".to_string(),
        status: "running".to_string(),
        files: collected.files_dto(),
        total_bytes: collected.total_bytes,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: now,
        updated_at_ms: now,
    };

    let epoch = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        inner
            .tasks
            .insert(0, SendTask { dto: dto.clone(), sources: collected.sources.clone() });
        state.register_session(&batch_id)
    };

    let encrypt = ensure_settings_loaded(&app).await.encryption_enabled;
    publish(&app);
    drive_send_session(app.clone(), node, record, batch_id, collected.sources, epoch, encrypt);
    tracing::info!(
        batch_id = %dto.batch_id,
        node_id = %node_id,
        files = dto.files.len(),
        total = dto.total_bytes,
        "peer transfer send started"
    );
    Ok(dto)
}

/// 重试发送任务：复用同一批 ID 重新拨号发起——接收端按已写偏移续传
/// （断点真源在落盘侧，issue 06），已完成文件自动跳过
///
/// 仅 failed / cancelled / rejected 可重试；源清单在重启后丢失的历史条目
/// 返回明确错误（前端如实提示需重新发起）。
#[tauri::command]
pub async fn retry_peer_transfer(
    app: AppHandle,
    batch_id: String,
) -> crate::Result<PeerTransferDto> {
    ensure_history_loaded(&app).await?;

    // 锁内完成校验与重置（纯内存），锁外再做拨号前置快照
    let node_id_hex = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let task = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id)
            .ok_or_else(|| {
                crate::AppError::NotFound(format!("peer transfer task not found: {batch_id}"))
            })?;
        if task.dto.direction != "send" {
            return Err(crate::AppError::InvalidInput(format!(
                "peer transfer retry only supports send tasks: {batch_id}"
            )));
        }
        if !retryable(&task.dto.status) {
            return Err(crate::AppError::InvalidInput(format!(
                "peer transfer task not retryable in state '{}': {batch_id}",
                task.dto.status
            )));
        }
        if task.sources.is_empty() {
            return Err(crate::AppError::Internal(format!(
                "peer transfer retry sources unavailable (task created before restart): {batch_id}"
            )));
        }
        task.dto.status = "running".to_string();
        task.dto.transferred_bytes = 0;
        task.dto.rate_bps = 0.0;
        task.dto.detail = None;
        task.dto.reject_reason = None;
        task.dto.updated_at_ms = now_ms();
        task.dto.node_id.clone()
    };

    let parsed = super::peer_net::parse_node_id(&node_id_hex)?;
    let Some((node, cache)) = super::peer_net::runtime_snapshot(&app).await else {
        return Err(crate::AppError::Internal(
            "peer transfer retry failed: node not started".to_string(),
        ));
    };
    let record = cache.get(&parsed).ok_or_else(|| {
        crate::AppError::Internal(
            "peer transfer retry failed: peer not in discovery cache (offline or unknown)"
                .to_string(),
        )
    })?;

    let (dto, sources, epoch) = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let task = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id)
            .expect("validated above");
        let epoch = state.register_session(&batch_id);
        (task.dto.clone(), task.sources.clone(), epoch)
    };

    let encrypt = ensure_settings_loaded(&app).await.encryption_enabled;
    publish(&app);
    drive_send_session(app.clone(), node, record, batch_id, sources, epoch, encrypt);
    tracing::info!(batch_id = %dto.batch_id, "peer transfer retry started");
    Ok(dto)
}

// ==================== 会话驱动 ====================

/// 后台发送会话：拨号 → `send_batch` → 事件转发（节流）/ 终态结算
///
/// `epoch` 是陈旧会话防护：重试会换发新令牌并递增代次，旧会话迟到的事件
/// （如取消后立刻重试时旧 Cancelled 终态）一律丢弃，不覆盖新一轮状态。
fn drive_send_session(
    app: AppHandle,
    node: PeerNetNode,
    record: DiscoveredPeerRecord,
    batch_id: String,
    sources: Vec<OutgoingFile>,
    epoch: u64,
    encrypt: bool,
) {
    tauri::async_runtime::spawn(async move {
        let (events_tx, mut events_rx) = mpsc::channel::<TransferEvent>(256);

        // 引擎会话主体：send_batch 消费连接并在终态经通道上报；
        // 通道随本闭包返回而关闭，转发循环随之自然退出
        let session_batch_id = batch_id.clone();
        let session_app = app.clone();
        let session = tauri::async_runtime::spawn(async move {
            let conn: Connection = match dial_for_send(&node, &record).await {
                Ok(conn) => conn,
                Err(detail) => {
                    settle_failed(&session_app, &session_batch_id, &detail, epoch).await;
                    return;
                }
            };
            let cancel = session_app
                .state::<PeerTransferState>()
                .cancel_token_of(&session_batch_id)
                .unwrap_or_else(CancelToken::new);
            let _ = bedcode_peer_net::send_batch(
                conn,
                session_batch_id,
                sources,
                events_tx,
                cancel,
                encrypt,
            )
            .await;
        });

        // 事件转发循环：Progress 节流推送，Terminal 结算历史并即时推送
        let mut last_emit = tokio::time::Instant::now() - PROGRESS_EMIT_INTERVAL;
        while let Some(event) = events_rx.recv().await {
            match event {
                TransferEvent::OfferPending { .. } => {}
                TransferEvent::Progress { batch_id: bid, transferred, rate_bps, .. } => {
                    update_progress(&app, &bid, transferred, rate_bps);
                    if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                        last_emit = tokio::time::Instant::now();
                        publish(&app);
                    }
                }
                TransferEvent::Terminal { batch_id: bid, state, remote: _ } => {
                    apply_terminal(&app, &bid, state, epoch).await;
                    break;
                }
            }
        }
        session.abort();
    });
}

/// 发送专用拨号（每次会话独立握手）：可信对端静默放行；拒绝/不可达归一为
/// 失败明细（业务文案由前端按状态渲染）
async fn dial_for_send(
    node: &PeerNetNode,
    record: &DiscoveredPeerRecord,
) -> Result<Connection, String> {
    match node.dial(&record.to_static_peer_record()).await {
        Ok(conn) => Ok(conn),
        Err(bedcode_peer_net::PeerNetError::DialDeniedByPeer { .. }) => {
            Err("dial denied by peer (trust revoked or first-connect pending)".to_string())
        }
        Err(e) => Err(format!("dial unreachable: {e}")),
    }
}

/// 进度入账（存储始终最新值；发射节流由调用方控制）
fn update_progress(app: &AppHandle, batch_id: &str, transferred: u64, rate_bps: f64) {
    let state = app.state::<PeerTransferState>();
    let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
    if let Some(task) = inner
        .tasks
        .iter_mut()
        .find(|t| t.dto.batch_id == batch_id && t.dto.status == "running")
    {
        task.dto.transferred_bytes = transferred;
        task.dto.rate_bps = rate_bps;
        task.dto.updated_at_ms = now_ms();
    }
}

/// 终态结算：状态映射 → 历史落盘 → 即时推送（epoch 不匹配的陈旧会话丢弃）
async fn apply_terminal(app: &AppHandle, batch_id: &str, terminal: TerminalState, epoch: u64) {
    let state = app.state::<PeerTransferState>();
    if !state.session_is_current(batch_id, epoch) {
        tracing::debug!(batch_id = %batch_id, "stale transfer session event ignored");
        return;
    }
    state.unregister_session(batch_id);

    let (status, detail, reject_reason) = match &terminal {
        TerminalState::Completed => ("completed".to_string(), None, None),
        TerminalState::Rejected { reason } => (
            "rejected".to_string(),
            None,
            Some(reason.as_str().to_string()),
        ),
        TerminalState::Cancelled { by_peer } => (
            "cancelled".to_string(),
            Some(if *by_peer { "cancelled by receiver" } else { "cancelled by self" }.to_string()),
            None,
        ),
        TerminalState::Failed { detail } => ("failed".to_string(), Some(detail.clone()), None),
    };
    {
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id && t.dto.status == "running")
        {
            task.dto.status = status;
            task.dto.detail = detail;
            task.dto.reject_reason = reject_reason;
            task.dto.updated_at_ms = now_ms();
        }
        evict_history_cap_locked(&mut inner.tasks);
    }
    tracing::info!(batch_id = %batch_id, state = ?terminal, "peer transfer session ended");
    persist_history(app).await;
    publish(app);
}

/// 会话前置失败（拨号被拒/不可达等）：直接落 failed 终态
async fn settle_failed(app: &AppHandle, batch_id: &str, detail: &str, epoch: u64) {
    apply_terminal(
        app,
        batch_id,
        TerminalState::Failed { detail: detail.to_string() },
        epoch,
    )
    .await;
}

// ==================== 发布与持久化 ====================

/// 全量列表快照（锁内克隆，锁外使用）
fn snapshot(app: &AppHandle) -> Vec<PeerTransferDto> {
    let state = app.state::<PeerTransferState>();
    let inner = state.inner.lock().expect("peer transfer lock poisoned");
    inner.tasks.iter().map(|t| t.dto.clone()).collect()
}

/// 全量列表推送（锁外 emit；与 peer-devices-changed 同款范式）
fn publish(app: &AppHandle) {
    let payload = serde_json::to_value(snapshot(app)).unwrap_or_default();
    super::peer_net::emit_json(app, "peer-transfer-changed", payload);
}

/// 首次访问时惰性加载历史（并发首载可能重复读盘，内容一致幂等无害）
async fn ensure_history_loaded(app: &AppHandle) -> crate::Result<()> {
    let state = app.state::<PeerTransferState>();
    {
        let inner = state.inner.lock().expect("peer transfer lock poisoned");
        if inner.loaded {
            return Ok(());
        }
    }
    let dir = super::peer_net::app_data_dir(app)?;
    let entries = tauri::async_runtime::spawn_blocking(move || read_history_file(&dir))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join history load failed: {e}")))?
        .unwrap_or_default();
    let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
    if !inner.loaded {
        // 加载的历史条目 sources 为空（重启后不可重试，如实语义）
        inner.tasks.extend(entries.into_iter().map(|dto| SendTask { dto, sources: Vec::new() }));
        sort_tasks_newest_first(&mut inner.tasks);
        inner.loaded = true;
    }
    Ok(())
}

/// 终态快照落盘（tmp+rename 原子替换；写盘在 spawn_blocking，串行锁防竞态）
async fn persist_history(app: &AppHandle) {
    let state = app.state::<PeerTransferState>();
    let entries: Vec<PeerTransferDto> = {
        let inner = state.inner.lock().expect("peer transfer lock poisoned");
        inner
            .tasks
            .iter()
            .filter(|t| is_terminal_status(&t.dto.status))
            .take(HISTORY_CAP)
            .map(|t| t.dto.clone())
            .collect()
    };
    let dir = match super::peer_net::app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("persist history aborted: {e}");
            return;
        }
    };
    let _guard = HISTORY_SAVE_LOCK.lock().await;
    let result = tauri::async_runtime::spawn_blocking(move || write_history_file(&dir, &entries))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join history save failed: {e}")))
        .and_then(|r| r.map_err(crate::AppError::Io));
    if let Err(e) = result {
        tracing::error!("persist transfer history failed: {e}");
    }
}

/// 历史封顶淘汰：列表为最新在前，保序保留前 CAP 条终态记录，
/// 最旧条目（尾部）被滚动移除；活跃任务不受影响
fn evict_history_cap_locked(tasks: &mut Vec<SendTask>) {
    let mut kept = 0usize;
    tasks.retain(|t| {
        if !is_terminal_status(&t.dto.status) {
            return true;
        }
        kept += 1;
        kept <= HISTORY_CAP
    });
}

fn sort_tasks_newest_first(tasks: &mut [SendTask]) {
    tasks.sort_by(|a, b| b.dto.created_at_ms.cmp(&a.dto.created_at_ms));
}

fn read_history_file(dir: &Path) -> std::io::Result<Vec<PeerTransferDto>> {
    let raw = std::fs::read_to_string(dir.join(HISTORY_FILE))?;
    let file: HistoryFile = serde_json::from_str(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if file.version != HISTORY_FORMAT_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported transfer history version {}", file.version),
        ));
    }
    Ok(file.entries)
}

fn write_history_file(dir: &Path, entries: &[PeerTransferDto]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join(HISTORY_FILE);
    let tmp = dir.join(format!("{HISTORY_FILE}.tmp"));
    let body = HistoryFile {
        version: HISTORY_FORMAT_VERSION,
        entries: entries.to_vec(),
    };
    let bytes = serde_json::to_vec(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &target)
}

// ==================== 源文件收集 ====================

/// 收集结果：待发文件清单 + 各文件大小 + 总字节数
struct CollectedSources {
    sources: Vec<OutgoingFile>,
    sizes: Vec<u64>,
    total_bytes: u64,
}

impl CollectedSources {
    fn files_dto(&self) -> Vec<PeerTransferFileDto> {
        self.sources
            .iter()
            .zip(self.sizes.iter())
            .map(|(f, size)| PeerTransferFileDto {
                path: f.remote_path.clone(),
                size: *size,
            })
            .collect()
    }
}

/// 从用户选择路径构建待发清单：文件取名直推，目录递归展开保持相对形状
///
/// 同名 remote 目标以「名称 (2).ext」样式编号去重，避免接收端落位互踩；
/// 数量超上限或选不出任何可发文件均显式报错。
fn collect_outgoing_files(paths: &[String]) -> crate::Result<CollectedSources> {
    let mut sources: Vec<OutgoingFile> = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();

    for raw in paths {
        let path = PathBuf::from(raw.trim());
        let meta = std::fs::metadata(&path).map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "send source '{}' unreadable: {e}",
                path.display()
            ))
        })?;
        if meta.is_dir() {
            let root_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            walk_directory(&path, &root_name, &mut sources, &mut sizes, &mut used)?;
        } else if meta.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            push_outgoing(path, unique_remote_path(name, &mut used), meta.len(), &mut sources, &mut sizes);
        }
    }

    if sources.is_empty() {
        return Err(crate::AppError::InvalidInput(
            "no sendable files found in selection".to_string(),
        ));
    }
    if sources.len() > MAX_FILES_PER_BATCH {
        return Err(crate::AppError::InvalidInput(format!(
            "selection exceeds per-batch limit of {MAX_FILES_PER_BATCH} files"
        )));
    }
    let total_bytes = sizes.iter().sum();
    Ok(CollectedSources { sources, sizes, total_bytes })
}

/// 递归展开目录（排序保证确定性；空目录跳过，非普通文件忽略）
fn walk_directory(
    dir: &Path,
    display_prefix: &str,
    sources: &mut Vec<OutgoingFile>,
    sizes: &mut Vec<u64>,
    used: &mut HashSet<String>,
) -> crate::Result<()> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "read directory '{}' failed: {e}",
                dir.display()
            ))
        })?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if sources.len() >= MAX_FILES_PER_BATCH {
            return Ok(());
        }
        let path = entry.path();
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let child_display = format!("{display_prefix}/{name}");
        if meta.is_dir() {
            walk_directory(&path, &child_display, sources, sizes, used)?;
        } else if meta.is_file() {
            push_outgoing(
                path,
                unique_remote_path(child_display, used),
                meta.len(),
                sources,
                sizes,
            );
        }
    }
    Ok(())
}

fn push_outgoing(
    source: PathBuf,
    remote_path: String,
    size: u64,
    sources: &mut Vec<OutgoingFile>,
    sizes: &mut Vec<u64>,
) {
    sources.push(OutgoingFile { source, remote_path });
    sizes.push(size);
}

/// 同名目标去重：首次原样保留，后续碰撞追加序号（保留扩展名）
fn unique_remote_path(requested: String, used: &mut HashSet<String>) -> String {
    if used.insert(requested.clone()) {
        return requested;
    }
    let (stem, ext) = match requested.rsplit_once('.') {
        // 点号出现在末段且两侧非空才视为扩展名分隔（路径分隔/隐藏文件不误判）
        Some((s, e)) if !s.is_empty() && !e.is_empty() && !e.contains('/') => {
            (s.to_string(), e.to_string())
        }
        _ => (requested.clone(), String::new()),
    };
    for seq in 2..u32::MAX {
        let candidate = if ext.is_empty() {
            format!("{stem} ({seq})")
        } else {
            format!("{stem} ({seq}).{ext}")
        };
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("sequence space exhausted")
}

// ==================== 发送源选择 ====================

/// 选择待发送文件（Android：SAF 选择器优先 _data 直读真实路径，否则按
/// externalstorage/downloads raw: 解析；单次单个，可多次累加）
///
/// 宿主自有命令不经插件门控——发送表单是宿主内置 UI 而非插件面板。
#[tauri::command]
pub async fn peer_pick_files(_app_handle: AppHandle) -> crate::Result<Vec<String>> {
    match crate::plugin::android_plugins::pick_file_android().await? {
        Some(path) => Ok(vec![path]),
        // 用户取消选择
        None => Ok(Vec::new()),
    }
}

/// 选择待发送文件夹（SAF 目录树选择器解析为真实路径；不支持的 provider 报错，
/// 前端按平台隐藏该入口——发送目录仅桌面端提供）
#[tauri::command]
pub async fn peer_pick_folder(_app_handle: AppHandle) -> crate::Result<Vec<String>> {
    match crate::plugin::android_plugins::pick_directory_android().await? {
        Some(path) => Ok(vec![path]),
        // 用户取消选择
        None => Ok(Vec::new()),
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn dto(status: &str, created_at_ms: u64) -> PeerTransferDto {
        PeerTransferDto {
            batch_id: format!("b-{created_at_ms}"),
            node_id: "a".repeat(64),
            peer_name: "Peer".to_string(),
            direction: "send".to_string(),
            status: status.to_string(),
            files: vec![PeerTransferFileDto { path: "a.txt".to_string(), size: 3 }],
            total_bytes: 3,
            transferred_bytes: 3,
            rate_bps: 0.0,
            detail: None,
            reject_reason: None,
            created_at_ms,
            updated_at_ms: created_at_ms,
        }
    }

    #[test]
    fn retryable_covers_failure_states_only() {
        assert!(retryable("failed"));
        assert!(retryable("cancelled"));
        assert!(retryable("rejected"));
        assert!(!retryable("running"));
        assert!(!retryable("completed"));
    }

    #[test]
    fn dto_wire_format_is_camel_case() {
        // IPC/历史文件共用同一序列化形状，前端消费 camelCase 字段
        let json = serde_json::to_string(&dto("rejected", 1)).expect("serialize");
        assert!(json.contains("\"batchId\":\"b-1\""));
        assert!(json.contains("\"peerName\":\"Peer\""));
        assert!(json.contains("\"totalBytes\":3"));
        assert!(json.contains("\"createdAtMs\":1"));
        // None 字段不落盘（历史文件紧凑）
        assert!(!json.contains("detail"), "None detail must be skipped");
    }

    #[test]
    fn unique_remote_path_keeps_first_and_numbers_collisions() {
        let mut used = HashSet::new();
        assert_eq!(unique_remote_path("a.txt".into(), &mut used), "a.txt");
        assert_eq!(unique_remote_path("a.txt".into(), &mut used), "a (2).txt");
        assert_eq!(unique_remote_path("a.txt".into(), &mut used), "a (3).txt");
        // 无扩展名与多点路径同样稳定
        assert_eq!(unique_remote_path("Makefile".into(), &mut used), "Makefile");
        assert_eq!(unique_remote_path("Makefile".into(), &mut used), "Makefile (2)");
        assert_eq!(
            unique_remote_path("docs/my.photo.png".into(), &mut used),
            "docs/my.photo.png"
        );
        assert_eq!(
            unique_remote_path("docs/my.photo.png".into(), &mut used),
            "docs/my.photo (2).png"
        );
    }

    #[test]
    fn collect_expands_directories_and_dedupes_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = dir.path().join("photos");
        std::fs::create_dir_all(folder.join("sub")).expect("mkdir");
        std::fs::write(folder.join("a.png"), vec![0u8; 10]).expect("write a");
        std::fs::write(folder.join("sub").join("b.png"), vec![0u8; 5]).expect("write b");
        let loose = dir.path().join("notes.txt");
        std::fs::write(&loose, vec![0u8; 3]).expect("write notes");

        let collected = collect_outgoing_files(&[
            folder.to_string_lossy().into_owned(),
            loose.to_string_lossy().into_owned(),
        ])
        .expect("collect");

        assert_eq!(collected.total_bytes, 18);
        assert_eq!(
            collected.files_dto(),
            vec![
                PeerTransferFileDto { path: "photos/a.png".to_string(), size: 10 },
                PeerTransferFileDto { path: "photos/sub/b.png".to_string(), size: 5 },
                PeerTransferFileDto { path: "notes.txt".to_string(), size: 3 },
            ]
        );

        // 同名文件（不同目录来源）remote 目标自动编号
        let other = dir.path().join("elsewhere");
        std::fs::create_dir_all(&other).expect("mkdir elsewhere");
        std::fs::write(other.join("notes.txt"), vec![0u8; 1]).expect("write dup");
        let deduped = collect_outgoing_files(&[
            loose.to_string_lossy().into_owned(),
            other.join("notes.txt").to_string_lossy().into_owned(),
        ])
        .expect("collect dup")
        .files_dto();
        assert_eq!(deduped[0].path, "notes.txt");
        assert_eq!(deduped[1].path, "notes (2).txt");
    }

    #[test]
    fn collect_rejects_empty_selection_and_missing_paths() {
        assert!(collect_outgoing_files(&[]).is_err());
        assert!(collect_outgoing_files(&["missing.bin".to_string()]).is_err());
    }

    #[test]
    fn history_file_roundtrips_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entries = vec![dto("completed", 100), dto("failed", 200)];
        write_history_file(dir.path(), &entries).expect("write");
        let loaded = read_history_file(dir.path()).expect("read");
        assert_eq!(loaded, entries);

        // 未来版本 fail-fast：静默换格式会丢历史，禁止吞错降级
        let future = r#"{"version":999,"entries":[]}"#;
        std::fs::write(dir.path().join(HISTORY_FILE), future).expect("write future");
        assert!(read_history_file(dir.path()).is_err());
    }

    #[test]
    fn history_cap_evicts_oldest_terminal_and_keeps_running() {
        // 生产列表为最新在前：按时间戳降序构造，尾部即最旧
        let mut tasks: Vec<SendTask> = Vec::new();
        for i in (0..(HISTORY_CAP + 5)).rev() {
            tasks.push(SendTask { dto: dto("completed", i as u64), sources: Vec::new() });
        }
        tasks.push(SendTask { dto: dto("running", 9_999), sources: Vec::new() });

        evict_history_cap_locked(&mut tasks);

        let terminals = tasks.iter().filter(|t| t.dto.status != "running").count();
        assert_eq!(terminals, HISTORY_CAP);
        // 最旧（列表尾部低时间戳）被淘汰；最新终态与活跃任务保留
        assert!(!tasks.iter().any(|t| t.dto.batch_id == "b-0"));
        assert!(tasks.iter().any(|t| t.dto.batch_id == format!("b-{}", HISTORY_CAP + 4)));
        assert!(tasks.iter().any(|t| t.dto.status == "running"));
    }
}
