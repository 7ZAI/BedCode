//! 对等网络接收侧（issue 10，移动端镜像）：询问应答回流 + 接收任务登记 +
//! 接收策略设置。
//!
//! 与桌面端 peer_receive 同构但无落点设置：移动端接收落点恒为 app 私有
//! 下载目录（MediaLanding 尽力提升进 MediaStore 公共下载，issue 07 语义），
//! 设置面仅暴露接收策略（ask/always_accept/always_deny）与询问超时。
//!
//! 与发送侧（[`super::peer_transfer`]）零侵入：接收任务复用
//! [`PeerTransferDto`] 形状经独立的 `peer-receive-changed` 事件全量推送，
//! 前端按 batchId 合并双源列表后统一分组。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bedcode_peer_net::transfer::{
    DEFAULT_ASK_TIMEOUT_SECS, FileMeta, batch::validate_ask_timeout_secs,
};
use bedcode_peer_net::{
    NodeId, ReceivePolicy, SharedDirHandler, TerminalState, TransferConfig, TransferEvent,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::peer_transfer::{PeerTransferDto, PeerTransferFileDto};

// ==================== 常量 ====================

/// 接收设置文件名（数据目录内）
const SETTINGS_FILE: &str = "transfer_settings.json";
/// 设置文件格式版本（未来字段演进时 fail-fast）
const SETTINGS_FORMAT_VERSION: u32 = 1;
/// 进度事件最小发射间隔（与发送侧转发层同值：低于人眼感知阈值）
const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);
/// 接收侧内存终态封顶（会话内追溯用；活跃任务不受影响）
const RECEIVE_TERMINAL_CAP: usize = 100;

// ==================== 数据模型 ====================

/// 接收策略模式（wire/持久化字符串；前端分段控件直用）
pub const POLICY_ASK: &str = "ask";
pub const POLICY_ALWAYS_ACCEPT: &str = "always_accept";
pub const POLICY_ALWAYS_DENY: &str = "always_deny";

/// 接收设置磁盘形态（移动端无落点字段：落点恒为 app 私有下载目录）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct PeerTransferSettings {
    /// `ask` | `always_accept` | `always_deny`
    policy_mode: String,
    /// ask 模式询问窗口（秒；crate 校验 10..=600）
    ask_timeout_secs: u64,
    /// 传输加密开关（发送侧新批生效；接收侧自动适配，默认关）。
    /// pub(crate)：发送侧（peer_transfer）发起批前读取
    pub(crate) encryption_enabled: bool,
}

impl Default for PeerTransferSettings {
    fn default() -> Self {
        Self {
            policy_mode: POLICY_ASK.to_string(),
            ask_timeout_secs: DEFAULT_ASK_TIMEOUT_SECS,
            encryption_enabled: false,
        }
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

    fn is_valid(&self) -> bool {
        matches!(
            self.policy_mode.as_str(),
            POLICY_ASK | POLICY_ALWAYS_ACCEPT | POLICY_ALWAYS_DENY
        ) && validate_ask_timeout_secs(self.ask_timeout_secs).is_ok()
    }
}

/// 设置读取 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerReceiveSettingsDto {
    pub policy_mode: String,
    pub ask_timeout_secs: u64,
    /// 传输加密开关（发送侧语义；接收侧自动适配）
    pub encryption_enabled: bool,
}

// ==================== 状态容器 ====================

#[derive(Default)]
struct ReceiveInner {
    settings: Option<PeerTransferSettings>,
    tasks: Vec<PeerTransferDto>,
    last_emit: Option<tokio::time::Instant>,
}

/// Tauri 托管的接收侧状态容器（语义与桌面端一致，见桌面 peer_receive 文档）
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
    !matches!(task.status.as_str(), "pending" | "running")
}

// ==================== 设置持久化 ====================

/// 惰性加载设置（进程内一次；损坏文件按缺省重建并告警）
pub(crate) async fn ensure_settings_loaded(app: &AppHandle) -> PeerTransferSettings {
    let state = app.state::<PeerReceiveState>();
    if let Some(settings) = state
        .inner
        .lock()
        .expect("peer receive lock poisoned")
        .settings
        .clone()
    {
        return settings;
    }
    let loaded = match super::peer_net::app_data_dir(app) {
        Ok(dir) => tauri::async_runtime::spawn_blocking(move || read_settings_file(&dir))
            .await
            .ok()
            .and_then(|r| r.ok()),
        Err(e) => {
            tracing::error!("load transfer settings aborted: {e}");
            None
        }
    };
    let settings = match loaded {
        Some(settings) if settings.is_valid() => settings,
        Some(broken) => {
            tracing::warn!(
                mode = %broken.policy_mode,
                "invalid transfer settings on disk, resetting to defaults"
            );
            PeerTransferSettings::default()
        }
        None => PeerTransferSettings::default(),
    };
    let mut inner = state.inner.lock().expect("peer receive lock poisoned");
    if inner.settings.is_none() {
        inner.settings = Some(settings.clone());
    }
    inner.settings.clone().unwrap_or_default()
}

/// 设置落盘（tmp+rename 原子替换；失败只记日志不阻断策略热更新）
async fn persist_settings(app: &AppHandle, settings: &PeerTransferSettings) {
    let dir = match super::peer_net::app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("persist transfer settings aborted: {e}");
            return;
        }
    };
    let snapshot = settings.clone();
    let result = tauri::async_runtime::spawn_blocking(move || write_settings_file(&dir, &snapshot))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join settings save failed: {e}")))
        .and_then(|r| r.map_err(crate::AppError::Io));
    if let Err(e) = result {
        tracing::error!("persist transfer settings failed: {e}");
    }
}

fn read_settings_file(dir: &std::path::Path) -> std::io::Result<PeerTransferSettings> {
    #[derive(serde::Deserialize)]
    struct Wrapper {
        version: u32,
        settings: PeerTransferSettings,
    }
    let raw = std::fs::read_to_string(dir.join(SETTINGS_FILE))?;
    let file: Wrapper = serde_json::from_str(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if file.version != SETTINGS_FORMAT_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported transfer settings version {}", file.version),
        ));
    }
    Ok(file.settings)
}

fn write_settings_file(
    dir: &std::path::Path,
    settings: &PeerTransferSettings,
) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct Wrapper<'a> {
        version: u32,
        settings: &'a PeerTransferSettings,
    }
    std::fs::create_dir_all(dir)?;
    let target = dir.join(SETTINGS_FILE);
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    let bytes =
        serde_json::to_vec(&Wrapper { version: SETTINGS_FORMAT_VERSION, settings })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &target)
}

// ==================== 节点装配接缝（peer_net 调用）====================

/// 装配期登记：处理器句柄 + 配置快照；按持久化设置纠正首份策略
/// （落点保持装配值——移动端恒为私有下载目录，不可配置）
pub(crate) async fn register_handler(
    app: &AppHandle,
    handler: Arc<SharedDirHandler>,
    mut config: TransferConfig,
) {
    let settings = ensure_settings_loaded(app).await;
    config.policy = settings.build_policy();
    handler.update_transfer_config(config.clone());
    let state = app.state::<PeerReceiveState>();
    *state.runtime.lock().await = Some((handler, config));
}

/// 节点停止时摘除句柄
pub(crate) async fn clear_handler(app: &AppHandle) {
    let state = app.state::<PeerReceiveState>();
    *state.runtime.lock().await = None;
}

/// 处理器与配置快照句柄（issue 11 远端拉取用：配置为落点/策略热更新基底）
pub(crate) async fn handler_and_config(
    app: &AppHandle,
) -> Option<(Arc<SharedDirHandler>, TransferConfig)> {
    let snapshot = {
        let state = app.state::<PeerReceiveState>();
        let guard = state.runtime.lock().await;
        guard.clone()
    };
    snapshot
}

/// 引擎事件消费主循环（替换日志占位消费者）
pub(crate) async fn drive_receive_events(app: AppHandle, mut rx: mpsc::Receiver<TransferEvent>) {
    while let Some(event) = rx.recv().await {
        match event {
            TransferEvent::OfferPending { remote, batch_id, files, total_size, reply } => {
                register_offer(&app, remote, batch_id, files, total_size, reply).await;
            }
            TransferEvent::Progress { batch_id, transferred, rate_bps, .. } => {
                tracing::debug!(
                    batch_id = %batch_id,
                    transferred,
                    rate_bps,
                    "receive progress event (drive_receive_events)"
                );
                update_progress(&app, &batch_id, transferred, rate_bps);
                throttle_publish(&app);
            }
            TransferEvent::Terminal { batch_id, state, .. } => {
                settle_terminal(&app, &batch_id, state);
            }
        }
    }
    fail_active_transfers(&app, "peer-net node stopped");
}

/// 对端展示名解析：发现缓存广播名优先，短指纹兜底
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
        files: vec![PeerTransferFileDto { path: rel_path, size: total_size }],
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

/// 对端展示名解析：发现缓存广播名优先，短指纹兜底
///
/// 必须 async：本函数只在 Tokio 运行时内被调用（pull 队列 / 事件消费循环，
/// 均经 tokio::spawn），不可 block_on——嵌套 runtime 会 panic
/// （"Cannot start a runtime from within a runtime"）。
async fn resolve_peer_name(app: &AppHandle, remote: &NodeId) -> String {
    let fallback = || short_fingerprint(remote.as_str());
    match super::peer_net::runtime_snapshot(app).await {
        Some((_, cache)) => cache
            .get(remote)
            .map(|record| record.device_name)
            .unwrap_or_else(fallback),
        None => fallback(),
    }
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
            .map(|f| PeerTransferFileDto { path: f.path, size: f.size })
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

/// 进度入账（pending → running：AlwaysAccept 策略无询问阶段直接进数据面）
fn update_progress(app: &AppHandle, batch_id: &str, transferred: u64, rate_bps: f64) {
    let state = app.state::<PeerReceiveState>();
    let mut inner = state.inner.lock().expect("peer receive lock poisoned");
    if let Some(task) = inner
        .tasks
        .iter_mut()
        .find(|t| t.batch_id == batch_id && !is_terminal(t))
    {
        task.status = "running".to_string();
        task.transferred_bytes = transferred;
        task.rate_bps = rate_bps;
        task.updated_at_ms = now_ms();
    }
}

/// 终态结算（状态映射与发送侧同构；Cancelled 的对端语义为 sender）
fn settle_terminal(app: &AppHandle, batch_id: &str, terminal: TerminalState) {
    let (status, detail, reject_reason) = match &terminal {
        TerminalState::Completed => ("completed".to_string(), None, None),
        TerminalState::Rejected { reason } => (
            "rejected".to_string(),
            None,
            Some(reason.as_str().to_string()),
        ),
        TerminalState::Cancelled { by_peer } => (
            "cancelled".to_string(),
            Some(
                if *by_peer {
                    "cancelled by sender"
                } else {
                    "cancelled by self"
                }
                .to_string(),
            ),
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
            task.status = status;
            task.detail = detail;
            task.reject_reason = reject_reason;
            task.updated_at_ms = now_ms();
        }
        evict_terminal_cap_locked(&mut inner.tasks);
    }
    tracing::info!(batch_id = %batch_id, state = ?terminal, "peer receive session ended");
    publish(app);
}

/// 节点停止收尾：全部活跃接收落 failed 并推送
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

/// 全量列表推送（与发送侧 peer-transfer-changed 平行的独立通道）
fn publish(app: &AppHandle) {
    let payload = serde_json::to_value(snapshot(app)).unwrap_or_default();
    {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        inner.last_emit = Some(tokio::time::Instant::now());
    }
    super::peer_net::emit_json(app, "peer-receive-changed", payload);
}

/// 进度节流推送：距上次发射不足间隔则跳过
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
    } else {
        // 诊断插桩：节流跳帧（排查接收进度不更新时确认事件流活跃）
        tracing::debug!("receive progress throttled (within emit interval)");
    }
}

// ==================== 命令面 ====================

/// 当前接收任务列表（活跃 + 会话内终态，最新在前）
#[tauri::command]
pub async fn list_peer_receiving(app: AppHandle) -> crate::Result<Vec<PeerTransferDto>> {
    ensure_settings_loaded(&app).await;
    Ok(snapshot(&app))
}

/// 应答传输询问（接受全部 / 拒绝全部）。返回是否成功送达回执——false 表示
/// 批已超时被引擎自动拒或 ID 未知（前端应关闭对应弹窗）。
#[tauri::command]
pub async fn respond_peer_transfer(
    app: AppHandle,
    batch_id: String,
    accepted: bool,
) -> crate::Result<bool> {
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

/// 取消接收任务：pending 批视同拒绝；running 批经 handler 按批取消令牌中止。
/// 返回是否命中任务。
#[tauri::command]
pub async fn cancel_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerReceiveState>();

    let reply = state
        .pending
        .lock()
        .expect("peer receive pending lock poisoned")
        .remove(&batch_id);
    if let Some(reply) = reply {
        tracing::info!(batch_id = %batch_id, "pending receive dismissed as rejected");
        let _ = reply.send(false);
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
        drop(inner);
        publish(&app);
        return Ok(true);
    }

    let hit = match super::peer_remote::cancel_pull(&app, &batch_id) {
        true => true,
        false => match state.runtime.lock().await.as_ref() {
            Some((handler, _)) => handler.cancel_transfer(&batch_id),
            None => false,
        },
    };
    tracing::debug!(batch_id = %batch_id, hit, "cancel receiving requested");
    Ok(hit)
}

/// 清空接收侧终态记录（活跃任务不受影响）；返回清除条数
#[tauri::command]
pub async fn clear_peer_receiving_history(app: AppHandle) -> crate::Result<usize> {
    ensure_settings_loaded(&app).await;
    let removed = {
        let state = app.state::<PeerReceiveState>();
        let mut inner = state.inner.lock().expect("peer receive lock poisoned");
        let before = inner.tasks.len();
        inner.tasks.retain(|t| !is_terminal(t));
        before - inner.tasks.len()
    };
    if removed > 0 {
        publish(&app);
    }
    tracing::info!(count = removed, "peer receiving history cleared");
    Ok(removed)
}

// ==================== 设置命令面 ====================

/// 当前接收设置
#[tauri::command]
pub async fn get_peer_receive_settings(
    app: AppHandle,
) -> crate::Result<PeerReceiveSettingsDto> {
    let settings = ensure_settings_loaded(&app).await;
    Ok(PeerReceiveSettingsDto {
        policy_mode: settings.policy_mode,
        ask_timeout_secs: settings.ask_timeout_secs,
        encryption_enabled: settings.encryption_enabled,
    })
}

/// 更新接收策略与询问超时（校验后持久化 + 运行中节点热生效）
#[tauri::command]
pub async fn set_peer_receive_policy(
    app: AppHandle,
    mode: String,
    timeout_secs: u64,
) -> crate::Result<()> {
    if !matches!(mode.as_str(), POLICY_ASK | POLICY_ALWAYS_ACCEPT | POLICY_ALWAYS_DENY) {
        return Err(crate::AppError::InvalidInput(format!(
            "set receive policy: unknown mode '{mode}'"
        )));
    }
    validate_ask_timeout_secs(timeout_secs).map_err(|detail| {
        crate::AppError::InvalidInput(format!("set receive policy: ask timeout {detail}"))
    })?;
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

/// 设置发送加密开关（应用层 AES-256-GCM；接收端经 Offer 加密头自动解密）。
/// 仅持久化：发送会话在发起时读开关，无需热更新接收侧运行时配置。
#[tauri::command]
pub async fn set_peer_transfer_encryption(app: AppHandle, enabled: bool) -> crate::Result<()> {
    let mut settings = ensure_settings_loaded(&app).await;
    settings.encryption_enabled = enabled;
    apply_settings(&app, &settings).await;
    tracing::info!(enabled, "transfer encryption toggled");
    Ok(())
}

/// 应用新设置：持久化 + 运行中处理器热更新（以装配快照为基底只换策略）
async fn apply_settings(app: &AppHandle, settings: &PeerTransferSettings) {
    let state = app.state::<PeerReceiveState>();
    state
        .inner
        .lock()
        .expect("peer receive lock poisoned")
        .settings = Some(settings.clone());
    persist_settings(app, settings).await;

    let mut runtime = state.runtime.lock().await;
    if let Some((handler, base)) = runtime.as_ref() {
        let mut config = base.clone();
        config.policy = settings.build_policy();
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
            encryption_enabled: false,
        }
    }

    #[test]
    fn settings_default_is_ask_with_default_window() {
        let settings = PeerTransferSettings::default();
        assert_eq!(settings.policy_mode, POLICY_ASK);
        assert_eq!(settings.ask_timeout_secs, DEFAULT_ASK_TIMEOUT_SECS);
        assert!(settings.is_valid());
        assert!(matches!(settings.build_policy(), ReceivePolicy::Ask { .. }));
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
        assert!(matches!(
            settings(POLICY_ASK, 30).build_policy(),
            ReceivePolicy::Ask { .. }
        ));
    }

    #[test]
    fn invalid_modes_and_timeouts_are_rejected_on_load() {
        assert!(!settings("auto_accept", 60).is_valid());
        assert!(!settings(POLICY_ASK, 9).is_valid());
        assert!(!settings(POLICY_ASK, 601).is_valid());
        assert!(settings(POLICY_ASK, 10).is_valid());
        assert!(settings(POLICY_ASK, 600).is_valid());
    }

    #[test]
    fn settings_file_roundtrips_and_rejects_future_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings = PeerTransferSettings {
            policy_mode: POLICY_ALWAYS_DENY.to_string(),
            ask_timeout_secs: 120,
            encryption_enabled: true,
        };
        write_settings_file(dir.path(), &settings).expect("write");
        assert_eq!(read_settings_file(dir.path()).expect("read"), settings);

        let future = r#"{"version":999,"settings":{}}"#;
        std::fs::write(dir.path().join(SETTINGS_FILE), future).expect("write future");
        assert!(read_settings_file(dir.path()).is_err());
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
}
