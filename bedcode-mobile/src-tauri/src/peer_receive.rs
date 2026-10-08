//! 对等网络接收侧引擎接入（票 07：询问回执表 + 引擎事件桥 + 策略闸门）。
//!
//! **口径（票 07）**：本模块不再持有任何任务状态机——旧版所持的接收任务表
//! （`PeerReceiveState.inner.tasks`）、进度入账与终态结算（`update_progress` /
//! `settle_terminal`）、暂停状态同步（`set_receive_pause_status`）、终态封顶
//! （`RECEIVE_TERMINAL_CAP`）、原因码映射与全量快照推送（`peer-receive-changed`
//! → 总线 `peer:receive`）已随接收编排整体下沉 `file-transfer` 插件（事件归约
//! 状态机，真源在其私有存储）。宿主只保留「离宿主无法实现、且无业务语义」的
//! 引擎控制面：
//!
//! - **询问回执表** `batch_id → oneshot::Sender<bool>`：入站 offer 的应答闸门
//!   （安全闸门，ADR 0022 薄壳②）——回执不在册即视为超时/未知，不放行；
//! - **策略闸门**：`ask` / `always_accept` / `always_deny` + 询问超时 + 接收落点，
//!   以 `TransferConfig` 热更新推进引擎（产品真源在插件侧，经 host-peer
//!   `set-receive-policy` / `set-download-dir` 推送）；
//! - **引擎事件桥**：`TransferEvent` 逐条直推 `peer:receive-event`（Progress 复用
//!   150ms 节流窗口——纯性能无业务），不经任何状态机加工（原因码/终态判定/
//!   任务行建行归插件归约）。
//!
//! 防回接锁 `retired_mobile_receive_orchestration_is_not_reintroduced`
//! （移动版）钉住本口径：谁把接收任务表 / 快照推送加回来，谁就要先推翻票 07 裁决。
//!
//! 引擎落点说明：移动端接收落点恒为 app 私有下载目录（MediaLanding 尽力提升进
//! MediaStore 公共下载，issue 07 语义），策略设置面仅暴露接收策略与询问超时。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bedcode_peer_net::transfer::batch::validate_ask_timeout_secs;
use bedcode_peer_net::transfer::DEFAULT_ASK_TIMEOUT_SECS;
use bedcode_peer_net::{NodeId, ReceivePolicy, SharedDirHandler, TransferConfig, TransferEvent};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::peer_events::{
    engine_node_stopped_payload, engine_offer_pending_payload, engine_pause_payload, engine_progress_payload,
    engine_pull_started_payload, engine_terminal_payload, publish_engine_event, PROGRESS_EMIT_INTERVAL,
    TOPIC_RECEIVE_EVENT,
};

// ==================== 常量 ====================

/// 接收设置文件名（数据目录内）
const SETTINGS_FILE: &str = "transfer_settings.json";
/// 设置文件格式版本（未来字段演进时 fail-fast）
const SETTINGS_FORMAT_VERSION: u32 = 1;

// ==================== 数据模型 ====================

/// 接收策略模式（wire/持久化字符串；前端分段控件直用）
pub const POLICY_ASK: &str = "ask";
pub const POLICY_ALWAYS_ACCEPT: &str = "always_accept";
pub const POLICY_ALWAYS_DENY: &str = "always_deny";

/// 接收设置磁盘形态（落点缺省 = app 私有下载目录；host-peer `set-download-dir`
/// 可更换，票 04）
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
    /// 拉取方向并发上限（1..=8；`peer_remote` 拉取队列并发闸门读值。
    /// 发送方向并发真源已随票 06 迁插件，本字段只服务引擎侧拉取编排）
    pub(crate) concurrency: u8,
    /// 接收落点目录（None = 缺省 app 私有下载目录；`set-download-dir` 写入，票 04）
    download_dir: Option<String>,
}

impl Default for PeerTransferSettings {
    fn default() -> Self {
        Self {
            policy_mode: POLICY_ASK.to_string(),
            ask_timeout_secs: DEFAULT_ASK_TIMEOUT_SECS,
            encryption_enabled: false,
            concurrency: DEFAULT_CONCURRENCY,
            download_dir: None,
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

    fn is_valid(&self) -> bool {
        matches!(
            self.policy_mode.as_str(),
            POLICY_ASK | POLICY_ALWAYS_ACCEPT | POLICY_ALWAYS_DENY
        ) && validate_ask_timeout_secs(self.ask_timeout_secs).is_ok()
    }
}

// 注（票 10）：设置读面 DTO `PeerReceiveSettingsDto` 随 `get_peer_receive_settings`
// 一并退役——插件侧读自己的 settings store，宿主不再有前端读面。

// ==================== 状态容器 ====================

/// 接收侧引擎控制面状态（询问回执表 + 设置缓存 + 处理器句柄；任务表已随票 07 退役）
#[derive(Default)]
pub struct PeerReceiveState {
    settings: Mutex<Option<PeerTransferSettings>>,
    /// 入站 offer 应答回执表：`batch_id → 引擎侧等待中的应答通道`。
    /// 应答不在册（超时/已终态/ID 未知）一律不写通道——闸门 fail-safe 语义
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    runtime: tokio::sync::Mutex<Option<(Arc<SharedDirHandler>, TransferConfig)>>,
}

// ==================== 设置持久化 ====================

/// 惰性加载设置（进程内一次；损坏文件按缺省重建并告警）
pub(crate) async fn ensure_settings_loaded(app: &AppHandle) -> PeerTransferSettings {
    let state = app.state::<PeerReceiveState>();
    if let Some(settings) = state
        .settings
        .lock()
        .expect("peer receive settings lock poisoned")
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
    let mut guard = state.settings.lock().expect("peer receive settings lock poisoned");
    if guard.is_none() {
        *guard = Some(settings.clone());
    }
    guard.clone().unwrap_or_default()
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
    let file: Wrapper =
        serde_json::from_str(&raw).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if file.version != SETTINGS_FORMAT_VERSION {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unsupported transfer settings version {}", file.version),
        ));
    }
    Ok(file.settings)
}

fn write_settings_file(dir: &std::path::Path, settings: &PeerTransferSettings) -> std::io::Result<()> {
    #[derive(serde::Serialize)]
    struct Wrapper<'a> {
        version: u32,
        settings: &'a PeerTransferSettings,
    }
    std::fs::create_dir_all(dir)?;
    let target = dir.join(SETTINGS_FILE);
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    let bytes = serde_json::to_vec(&Wrapper {
        version: SETTINGS_FORMAT_VERSION,
        settings,
    })
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &target)
}

// ==================== 节点装配接缝（peer_net 调用）====================

/// 装配期登记：处理器句柄 + 配置快照；按持久化设置纠正首份策略
/// （落点保持装配值——移动端恒为私有下载目录，不可配置）
pub(crate) async fn register_handler(app: &AppHandle, handler: Arc<SharedDirHandler>, mut config: TransferConfig) {
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

/// 处理器与配置快照句柄（远端拉取用：配置为落点/策略热更新基底）
pub(crate) async fn handler_and_config(app: &AppHandle) -> Option<(Arc<SharedDirHandler>, TransferConfig)> {
    let snapshot = {
        let state = app.state::<PeerReceiveState>();
        let guard = state.runtime.lock().await;
        guard.clone()
    };
    snapshot
}

// ==================== 引擎事件桥（接收方向） ====================

/// 接收侧事件消费主循环（纯直推 + 询问回执表维护）：
/// - `OfferPending`：登记应答回执闸门 + 直推建行事件（插件自建待应答行）；
/// - `Progress`：150ms 节流后直推（纯性能）；
/// - `Terminal`：摘除回执表条目（引擎超时自动拒时通道在此失效）+ 直推终态；
/// - `Paused` / `Resumed`：直推（插件归约落态）。
///
/// 通道关闭 = 引擎节点下线：清空回执表（未应答询问随之失效——闸门 fail-safe）
/// 并直推 `node-stopped`，插件把在册进行中条目标注 `interrupted`。
pub(crate) async fn drive_receive_events(app: AppHandle, mut rx: mpsc::Receiver<TransferEvent>) {
    let mut last_emit = tokio::time::Instant::now() - PROGRESS_EMIT_INTERVAL;
    while let Some(event) = rx.recv().await {
        match event {
            TransferEvent::OfferPending {
                remote,
                batch_id,
                files,
                total_size,
                reply,
            } => {
                {
                    let state = app.state::<PeerReceiveState>();
                    state
                        .pending
                        .lock()
                        .expect("peer receive pending lock poisoned")
                        .insert(batch_id.clone(), reply);
                }
                publish_engine_event(
                    TOPIC_RECEIVE_EVENT,
                    engine_offer_pending_payload(&remote, &batch_id, &files, total_size),
                );
                tracing::info!(
                    batch_id = %batch_id,
                    remote = %remote,
                    "incoming peer transfer offer pending user reply"
                );
            }
            TransferEvent::Progress {
                batch_id,
                transferred,
                total,
                rate_bps,
                ..
            } => {
                if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                    last_emit = tokio::time::Instant::now();
                    publish_engine_event(
                        TOPIC_RECEIVE_EVENT,
                        engine_progress_payload(&batch_id, transferred, total, rate_bps),
                    );
                } else {
                    // 诊断插桩：节流跳帧（排查接收进度不更新时确认事件流活跃）
                    tracing::debug!(batch_id = %batch_id, "receive progress throttled (within emit interval)");
                }
            }
            TransferEvent::Terminal { batch_id, state, .. } => {
                // 回执表清理：批已终态（引擎超时自拒 / 用户应答后传输失败），
                // 迟到的应答不再写通道
                {
                    let state = app.state::<PeerReceiveState>();
                    state
                        .pending
                        .lock()
                        .expect("peer receive pending lock poisoned")
                        .remove(&batch_id);
                }
                publish_engine_event(TOPIC_RECEIVE_EVENT, engine_terminal_payload(&batch_id, &state));
                tracing::info!(batch_id = %batch_id, state = ?state, "peer receive session ended");
            }
            // 数据供方暂停/恢复：本端接收任务状态同步（对端门控推流）
            TransferEvent::Paused { batch_id, .. } => {
                publish_engine_event(TOPIC_RECEIVE_EVENT, engine_pause_payload("paused", &batch_id));
            }
            TransferEvent::Resumed { batch_id, .. } => {
                publish_engine_event(TOPIC_RECEIVE_EVENT, engine_pause_payload("resumed", &batch_id));
            }
            // 服务侧拉取事件走独立 serve 通道（peer_transfer::drive_serve_events），
            // 本接收通道理论上收不到；防御性忽略
            TransferEvent::PullServed { .. } => {}
        }
    }
    {
        let state = app.state::<PeerReceiveState>();
        let dropped = {
            let mut pending = state.pending.lock().expect("peer receive pending lock poisoned");
            let dropped = pending.len();
            pending.clear();
            dropped
        };
        if dropped > 0 {
            tracing::warn!(
                dropped,
                "peer-net stopped with unanswered receive offers, replies dropped (gate fail-safe)"
            );
        }
    }
    publish_engine_event(TOPIC_RECEIVE_EVENT, engine_node_stopped_payload());
    tracing::warn!("peer-net node stopped: receive event channel closed");
}

/// 拉取会话发起事实回灌（票 07）：pull 的 batch_id 由引擎铸造、调用方（插件）
/// 事前拿不到，故经 `pull-started` 事件把建行事实推给插件——否则首条 Progress
/// 无处归约（插件归约不凭空建行）。引擎侧进度/终态仍走同一 receive 事件通道。
pub(crate) fn announce_remote_pull_started(remote: NodeId, batch_id: &str, rel_path: &str, size: u64) {
    publish_engine_event(
        TOPIC_RECEIVE_EVENT,
        engine_pull_started_payload(&remote, batch_id, rel_path, size),
    );
    tracing::info!(
        batch_id = %batch_id,
        remote = %remote,
        "remote pull session started (accounting owned by plugin)"
    );
}

/// 本端侧失败落终态（拉取队列拨号失败等）：直推 failed 终态事件——插件归约结算，
/// 宿主无任务表可写
pub(crate) fn announce_local_failure(batch_id: &str, detail: &str) {
    publish_engine_event(
        TOPIC_RECEIVE_EVENT,
        engine_terminal_payload(
            batch_id,
            &bedcode_peer_net::TerminalState::Failed {
                detail: detail.to_string(),
            },
        ),
    );
    tracing::info!(batch_id = %batch_id, detail, "peer receive session failed locally");
}

// ==================== 闸门操作面（host-peer 原语入口） ====================

/// 接收批应答（host-peer `respond-transfer`）：回执表取出通道并写应答——不在册
/// （超时/已终态/ID 未知）视为不送达，插件侧据此关闭弹窗。不改任何任务状态
/// （任务行真源在插件侧，由 Progress/Terminal 事件推进）。
pub(crate) async fn respond_peer_transfer(app: AppHandle, batch_id: String, accepted: bool) -> crate::Result<bool> {
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
    tracing::info!(batch_id = %batch_id, accepted, delivered, "peer transfer offer answered");
    Ok(delivered)
}

/// 取消接收批：pending 批视同拒绝（回执表闸门）；其余经拉取会话表 / 接收
/// 会话表按批中止。返回是否命中。
pub(crate) async fn cancel_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerReceiveState>();
    let reply = state
        .pending
        .lock()
        .expect("peer receive pending lock poisoned")
        .remove(&batch_id);
    if let Some(reply) = reply {
        tracing::info!(batch_id = %batch_id, "pending receive dismissed as rejected");
        let _ = reply.send(false);
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

/// 暂停接收批：两条路径都经 wire Pause 帧请求对端数据供方门控推流
/// （连接保持、断点不丢）；任务行的 paused 落态由插件归约 Paused 事件完成。
///
/// - 拉取会话（本端是拉取发起方，对端 serve 供流）：经 pull 暂停句柄下发；
/// - push 接收批（对端是发送方）：经接收会话暂停句柄下发。
pub(crate) async fn pause_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let mut hit = super::peer_remote::pause_pull(&app, &batch_id).await;
    if !hit {
        // push 接收批：经接收会话暂停句柄写 Pause 帧（对端发送会话门控推流）
        hit = pause_receive_session(&app, &batch_id, true).await;
    }
    tracing::debug!(batch_id = %batch_id, hit, "pause receiving requested");
    Ok(hit)
}

/// 恢复暂停的接收批：写 Resume 帧续流（对端数据供方解除门控）
pub(crate) async fn resume_peer_receiving(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let mut hit = super::peer_remote::resume_pull(&app, &batch_id).await;
    if !hit {
        hit = pause_receive_session(&app, &batch_id, false).await;
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

// 注（票 06）：原 `resume_all_peer_receiving`（宿主侧「全部继续」编排）随发送
// 侧 `resume_all_peer_transfers` 一并退役——批量恢复编排归插件（逐条调
// `resume-transfer` 原语），宿主不再持有跨方向的产品编排。

// ==================== 策略闸门设置面 ====================

/// 更新接收策略与询问超时（校验后持久化 + 运行中节点热生效）
///
/// 真入口 = host-peer `set-receive-policy`（票 10 起无前端命令面；设置真源在
/// 插件侧 settings store，本函数只写引擎闸门副本）
pub(crate) async fn set_peer_receive_policy(app: AppHandle, mode: String, timeout_secs: u64) -> crate::Result<()> {
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

// 注（票 10）：`get_peer_receive_settings` / `set_peer_transfer_encryption` /
// `set_peer_transfer_concurrency` 三个前端命令面随本票退役——设置真源在插件
// settings store，宿主副本只服务引擎闸门（策略 / 落点）与引擎侧拉取编排：
// - 加密开关的裁决权在插件（`send-files` 载荷逐项带 `encrypt`），宿主字段只
//   作为「载荷未带 encrypt 时的兜底」，退役写入侧后该兜底恒为 false；
// - 拉取并发上限的写入侧同时消失（拉取编排本身仍是宿主 B2 遗留，见 spec
//   票 09 §5.2），引擎读值退化为「磁盘既有值或缺省 3」，无用户可见回退
//   （票 06/07 后前端已无该命令的消费者）。
// 两个字段本身保留：它们仍被引擎侧读取，且是旧安装的持久化兼容位。

/// 应用新设置：持久化 + 运行中处理器热更新（以装配快照为基底只换策略）
async fn apply_settings(app: &AppHandle, settings: &PeerTransferSettings) {
    let state = app.state::<PeerReceiveState>();
    *state.settings.lock().expect("peer receive settings lock poisoned") = Some(settings.clone());
    persist_settings(app, settings).await;

    let mut runtime = state.runtime.lock().await;
    if let Some((handler, base)) = runtime.as_ref() {
        let mut config = base.clone();
        config.policy = settings.build_policy();
        // 下载落点热生效：None = 恢复装配时缺省（app 私有下载目录），票 04
        config.download_dir = settings
            .download_dir
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| base.download_dir.clone());
        handler.update_transfer_config(config.clone());
        *runtime = Some((Arc::clone(handler), config));
    }
}

/// 更换接收落点目录（host-peer `set-download-dir` 引擎入口，票 04；None = 恢复
/// 缺省 app 私有下载目录）。目录即时创建；运行中节点热生效（经 apply_settings）
pub(crate) async fn set_peer_download_dir(app: &AppHandle, path: Option<String>) -> crate::Result<()> {
    let dir = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(d) => {
            tokio::fs::create_dir_all(d).await.map_err(|e| {
                crate::AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("create download dir '{d}' failed: {e}"),
                ))
            })?;
            Some(d.to_string())
        }
        None => None,
    };
    let mut settings = ensure_settings_loaded(app).await;
    settings.download_dir = dir;
    apply_settings(app, &settings).await;
    tracing::info!(dir = ?settings.download_dir, "receive download dir updated");
    Ok(())
}

/// 活跃接收批投影（host-peer `active-transfers` 的 receive 段，票 04）：pending
/// 询问表投影——仅引擎会话事实（batchId/status="pending"）。移动端 pending 表
/// 不持有 total_size（桌面 offer 有），显性给 0
pub(crate) fn active_receive_rows(app: &AppHandle) -> Vec<serde_json::Value> {
    let state = app.state::<PeerReceiveState>();
    let pending = state.pending.lock().expect("peer receive pending lock poisoned");
    let now = super::peer_events::now_ms();
    pending
        .keys()
        .map(|batch_id| {
            serde_json::json!({
                "batchId": batch_id,
                "direction": "receive",
                "status": "pending",
                "totalBytes": 0,
                "transferredBytes": 0,
                "rateBps": 0.0,
                "updatedAtMs": now,
            })
        })
        .collect()
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）

    fn settings(mode: &str, timeout: u64) -> PeerTransferSettings {
        PeerTransferSettings {
            policy_mode: mode.to_string(),
            ask_timeout_secs: timeout,
            encryption_enabled: false,
            concurrency: DEFAULT_CONCURRENCY,
            download_dir: None,
        }
    }

    mod gate;
    mod settings_default_is_ask;
}
