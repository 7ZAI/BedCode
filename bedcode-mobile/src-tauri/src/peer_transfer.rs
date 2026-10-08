//! 对等网络发送侧引擎接入（票 06：纯句柄表 + 引擎事件桥）。
//!
//! **口径（票 06）**：本模块不再持有任何任务状态机——旧版所持的任务表
//! （`PeerTransferState.inner.tasks`）、发送并发闸门与队列泵（`pump_send_queue`
//! / `pick_pending_to_start` / `current_concurrency`）、终态历史文件
//! （`transfer_history.json` 读写）、原因码映射与「双端记账」任务登记，
//! 已随发送编排整体下沉 `file-transfer` 插件（事件归约状态机，真源在其私有
//! 存储）。宿主只保留「离宿主无法实现、且无业务语义」的引擎控制面：
//!
//! - **会话句柄表** `batch_id → SendSessionHandle`：CancelToken（取消）、
//!   PauseSlot（wire Pause/Resume 门控）、epoch（陈旧会话防护）、sources
//!   （redial 续传必需的源清单）、寻址与加密参数、字节记账（active-transfers
//!   投影用）；
//! - **send-files 收窄**：一次调用 = 一个会话立即发起（并发节流归调用方，
//!   插件侧闸门自控）；返回值收窄为传输句柄 batch-id；
//! - **引擎事件桥**：send 会话与 serve 供流通道的 TransferEvent 逐条直推
//!   `peer:transfer-event`（Progress 复用 150ms 节流窗口——纯性能无业务），
//!   不经任何状态机加工（原因码/终态判定归插件归约）；载荷构造与直推单点在
//!   [`super::peer_events`]（票 07 起双方向共用）。
//!
//! 防回接锁 `retired_peer_transfer_orchestration_is_not_reintroduced`
//! （移动版）钉住本口径：谁把任务表 / 并发闸门 / 历史文件加回来，谁就要先
//! 推翻票 06 裁决。
//!
//! 接收方向（`direction=receive`）的引擎控制面（询问回执表 + 策略闸门）与事件桥
//! 在 [`super::peer_receive`]（票 07 已收口）；[`PeerTransferFileDto`] 作为
//! `collect-outgoing` 原语的返回形状定义在本模块。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bedcode_peer_net::{
    CancelToken, Connection, DiscoveredPeerRecord, OutgoingFile, PauseCmd, PauseSlot, PeerNetNode, TerminalState,
    TransferEvent,
};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::peer_events::{
    engine_pause_payload, engine_progress_payload, engine_pull_served_payload, engine_terminal_payload, now_ms,
    publish_engine_event, PROGRESS_EMIT_INTERVAL, TOPIC_TRANSFER_EVENT,
};

// ==================== 常量 ====================

/// 单批文件数上限：目录递归收集的失控保护
const MAX_FILES_PER_BATCH: usize = 512;

// ==================== 共用 wire 形状 ====================

/// 批内单文件元数据（`collect-outgoing` 原语返回形状；camelCase 直跨 IPC 与
/// 总线载荷，两端前端同构消费）
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerTransferFileDto {
    /// 发送方相对路径（`/` 分隔；接收端落位形状）
    pub path: String,
    /// 文件字节数
    pub size: u64,
}

// ==================== 会话句柄表 ====================

/// 发送会话句柄（纯引擎控制面）：取消 / 暂停 / 代次 / 源清单 / 寻址。
///
/// `sources` 必须随句柄表保留——resume 的 redial 分支按它重拨续传
/// （断点真源在接收端落盘侧）：会话重启必需，非业务态。
pub(crate) struct SendSessionHandle {
    pub cancel_token: CancelToken,
    pub epoch: u64,
    pub pause_slot: Arc<PauseSlot>,
    pub sources: Vec<OutgoingFile>,
    pub node_id: String,
    pub encrypt: bool,
    pub total_bytes: u64,
    /// 最新已传字节（事件循环记账；active-transfers 投影用，纯引擎进度事实）
    pub transferred_bytes: u64,
    /// 会话控制面状态：wire Pause 已写（resume 判据；对端 Paused/Resumed 帧
    /// 同步）。终态结算随句柄摘除
    pub paused: bool,
}

/// Tauri 托管的发送会话句柄表（旧任务状态机与并发闸门已删）。
/// 单把 Mutex 串行化读写——临界区均为内存短操作。
#[derive(Default)]
pub struct PeerTransferState {
    sessions: Mutex<HashMap<String, SendSessionHandle>>,
}

impl PeerTransferState {
    /// 登记新会话并返回代次（每批从 0 起；redial 递增）
    fn register_session(
        &self,
        batch_id: &str,
        sources: Vec<OutgoingFile>,
        node_id: String,
        encrypt: bool,
        total_bytes: u64,
    ) -> u64 {
        let mut guard = self.sessions.lock().expect("peer transfer sessions poisoned");
        let epoch = guard.get(batch_id).map(|h| h.epoch + 1).unwrap_or(0);
        guard.insert(
            batch_id.to_string(),
            SendSessionHandle {
                cancel_token: CancelToken::new(),
                epoch,
                pause_slot: PauseSlot::new(),
                sources,
                node_id,
                encrypt,
                total_bytes,
                transferred_bytes: 0,
                paused: false,
            },
        );
        epoch
    }

    /// 当前会话令牌（取消入口取用；不存在或已结算返回 None）
    fn cancel_token_of(&self, batch_id: &str) -> Option<CancelToken> {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|h| h.cancel_token.clone())
    }

    /// 取会话暂停句柄克隆（锁内 clone Arc，锁外使用）
    fn pause_slot_of(&self, batch_id: &str) -> Option<Arc<PauseSlot>> {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|h| Arc::clone(&h.pause_slot))
    }

    /// 会话是否在册（pause/resume 路由判据：send 会话 vs serve 供流批）
    fn has_session(&self, batch_id: &str) -> bool {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .contains_key(batch_id)
    }

    /// wire Pause/Resume 控制面状态同步（对端帧 / 本端命令）
    fn set_paused(&self, batch_id: &str, paused: bool) {
        if let Some(handle) = self
            .sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get_mut(batch_id)
        {
            handle.paused = paused;
        }
    }

    /// 进度记账（事件循环内调用；纯引擎字节事实）
    fn record_progress(&self, batch_id: &str, transferred: u64) {
        if let Some(handle) = self
            .sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get_mut(batch_id)
        {
            handle.transferred_bytes = transferred;
        }
    }

    /// 会话结束/终态时摘除句柄
    fn unregister_session(&self, batch_id: &str) {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .remove(batch_id);
    }

    /// 对端取消发起方的陈旧终态防护：redial 后旧代次事件不得直推（错误结算
    /// 新一轮会话）。session_is_current 即代次校验
    fn session_is_current(&self, batch_id: &str, epoch: u64) -> bool {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|h| h.epoch == epoch)
            .unwrap_or(false)
    }

    fn sources_of(&self, batch_id: &str) -> Option<Vec<OutgoingFile>> {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|h| h.sources.clone())
    }

    fn encrypt_of(&self, batch_id: &str) -> Option<bool> {
        self.sessions
            .lock()
            .expect("peer transfer sessions poisoned")
            .get(batch_id)
            .map(|h| h.encrypt)
    }
}

// ==================== 原语适配面 ====================

/// 活跃（非终态）发送会话投影（active-transfers 原语数据源）：句柄表在册条目
/// 即活跃会话（终态时摘除），status 取控制面 paused/running 事实
pub(crate) fn active_send_transfer_rows(app: &AppHandle) -> Vec<serde_json::Value> {
    let state = app.state::<PeerTransferState>();
    let guard = state.sessions.lock().expect("peer transfer sessions poisoned");
    guard
        .iter()
        .map(|(batch_id, handle)| send_handle_row(batch_id, handle))
        .collect()
}

/// 单条句柄 → 引擎事实投影行（纯函数，单测锚点：业务字段不得进投影）
fn send_handle_row(batch_id: &str, handle: &SendSessionHandle) -> serde_json::Value {
    serde_json::json!({
        "batchId": batch_id,
        "direction": "send",
        "status": if handle.paused { "paused" } else { "running" },
        "totalBytes": handle.total_bytes,
        "transferredBytes": handle.transferred_bytes,
        "rateBps": 0.0,
        "updatedAtMs": now_ms(),
    })
}

/// 取消进行中的发送会话（幂等：句柄不在册返回 false）。serve 供流批经
/// handler 按 serve batch_id 取消——会话写 Cancel 帧告知拉取方并停供流。
/// 任务行的 cancelled 落态由插件归约 terminal 事件完成（宿主无任务表）。
///
/// 真入口 = host-peer `close`（票 10 起无前端命令面；取消/暂停/恢复三条同族）
pub(crate) async fn cancel_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerTransferState>();
    if let Some(token) = state.cancel_token_of(&batch_id) {
        tracing::info!(batch_id = %batch_id, "peer transfer cancel requested");
        token.cancel();
        return Ok(true);
    }
    // 服务侧拉取批（本端供流）：经 handler 按 serve batch_id 取消
    Ok(cancel_serve_session(&app, &batch_id).await)
}

/// 经 handler 取消服务侧拉取会话（本端在对端拉取中供流）：会话按 serve
/// batch_id 寻址，命中即写 Cancel 帧并停供流。
async fn cancel_serve_session(app: &AppHandle, batch_id: &str) -> bool {
    match super::peer_receive::handler_and_config(app).await {
        Some((handler, _)) => handler.cancel_serve_transfer(batch_id),
        None => false,
    }
}

/// 显式暂停发送批（数据面门控）：本端发起的活跃会话查句柄表写 Pause 帧
/// （连接保持、供方停推流）；serve 供流批查 handler 注册表；两者皆无命中时
/// 回落接收方向（push 接收批 / 拉取会话，票 07 收口）。任务行的 paused 落态
/// 由插件归约 Paused 事件完成。
///
/// 真入口 = host-peer `pause-transfer`
pub(crate) async fn pause_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerTransferState>();
    if state.has_session(&batch_id) {
        let handled = match state.pause_slot_of(&batch_id) {
            Some(slot) => slot.send(PauseCmd::Pause).await,
            None => false,
        };
        state.set_paused(&batch_id, true);
        if !handled {
            // 会话已死（slot 命令通道关闭）：取消兜底，插件按 terminal 结算
            if let Some(token) = state.cancel_token_of(&batch_id) {
                token.cancel();
            }
        }
        tracing::info!(batch_id = %batch_id, "peer transfer paused");
        return Ok(true);
    }
    // serve 供流批：门控入口在 SharedDirHandler 的 serve 暂停注册表
    match super::peer_receive::handler_and_config(&app).await {
        Some((handler, _)) if handler.set_serve_paused(&batch_id, true).await => {
            tracing::info!(batch_id = %batch_id, "pull serve transfer paused");
            Ok(true)
        }
        // 接收方向回落（push 接收批 / 拉取会话）
        _ => super::peer_receive::pause_peer_receiving(app, batch_id).await,
    }
}

/// 恢复暂停的发送批：活跃会话写 Resume 帧续流；会话已中断（重启/断线残留）
/// 的以句柄表记忆的源清单重新拨号续传（断点真源在接收端落盘侧，保留已传
/// 字节——引擎 Progress 首帧会重设偏移）。句柄表与 serve 均未命中时回落
/// 接收方向恢复。
///
/// 真入口 = host-peer `resume-transfer`
pub(crate) async fn resume_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerTransferState>();
    if state.has_session(&batch_id) {
        let slot_sent = match state.pause_slot_of(&batch_id) {
            Some(slot) => slot.send(PauseCmd::Resume).await,
            None => false,
        };
        if slot_sent {
            state.set_paused(&batch_id, false);
            tracing::info!(batch_id = %batch_id, "peer transfer resumed (live session)");
            return Ok(true);
        }
        // 会话已死（slot 命令通道关闭）：以句柄表记忆的源清单重新拨号
        return Ok(resume_via_redial(&app, &batch_id).await);
    }
    // serve 供流批：门控入口在 SharedDirHandler 的 serve 暂停注册表
    match super::peer_receive::handler_and_config(&app).await {
        Some((handler, _)) if handler.set_serve_paused(&batch_id, false).await => {
            tracing::info!(batch_id = %batch_id, "pull serve transfer resumed");
            Ok(true)
        }
        // 接收方向回落（push 接收批 / 拉取会话）
        _ => super::peer_receive::resume_peer_receiving(app, batch_id).await,
    }
}

/// 重新拨号恢复（resume 的会话已死分支）：登记新代次会话并立即发起——
/// 并发节流归调用方（插件逐批调 resume-transfer），宿主不再排队。
/// total_bytes 沿用句柄表记忆值（已传字节由引擎 Progress 首帧重设偏移）。
async fn resume_via_redial(app: &AppHandle, batch_id: &str) -> bool {
    let state = app.state::<PeerTransferState>();
    let (sources, node_id, encrypt, total_bytes) = {
        let mut guard = state.sessions.lock().expect("peer transfer sessions poisoned");
        let Some(handle) = guard.get_mut(batch_id) else {
            return false;
        };
        handle.paused = false;
        (
            handle.sources.clone(),
            handle.node_id.clone(),
            handle.encrypt,
            handle.total_bytes,
        )
    };
    if sources.is_empty() {
        tracing::warn!(batch_id = %batch_id, "resume transfer: batch has no remembered sources");
        return false;
    }
    let Some((node, cache)) = super::peer_net::runtime_snapshot(app).await else {
        settle_failed(app, batch_id, "peer transfer resume failed: node not started").await;
        return true;
    };
    let parsed = match super::peer_net::parse_node_id(&node_id) {
        Ok(p) => p,
        Err(e) => {
            settle_failed(app, batch_id, &format!("peer id invalid: {e}")).await;
            return true;
        }
    };
    let Some(record) = cache.get(&parsed) else {
        settle_failed(
            app,
            batch_id,
            "peer transfer resume failed: peer not in discovery cache (offline or unknown)",
        )
        .await;
        return true;
    };
    let _epoch = state.register_session(batch_id, sources, node_id, encrypt, total_bytes);
    drive_send_session(app.clone(), node, record, batch_id.to_string());
    tracing::debug!(batch_id = %batch_id, "peer transfer session redialed for resume");
    true
}

// ==================== 发送入口（票 06 收窄：即发即会话） ====================

/// 发送入口（票 06 收窄）：一次调用 = 一个会话立即发起——目录递归收集源清单
/// 后直接拨号启动，不再有宿主并发闸门排队（旧 pending 态删除）；发送方向并发
/// 节流由调用方在调用前自控（插件侧 `PENDING_SENDS` 闸门）。返回传输句柄
/// （batch-id）。
///
/// `encrypt_override` 为 `Some(true)` 时本批强制加密（插件载荷元素级 flag
/// 聚合），`None` 回落引擎接收设置的全局开关。
pub(crate) async fn send_files_to_peer_with_policy(
    app: AppHandle,
    node_id: String,
    paths: Vec<String>,
    encrypt_override: Option<bool>,
) -> crate::Result<String> {
    let parsed = super::peer_net::parse_node_id(&node_id)?;
    if paths.is_empty() || paths.iter().all(|p| p.trim().is_empty()) {
        return Err(crate::AppError::InvalidInput(
            "send_files_to_peer: paths must not be empty".to_string(),
        ));
    }
    // `node` 仅用于确认节点已启动（runtime_snapshot 的可用性即启动判据）
    let Some((node, cache)) = super::peer_net::runtime_snapshot(&app).await else {
        return Err(crate::AppError::Internal(
            "peer transfer send failed: node not started".to_string(),
        ));
    };
    let record = cache.get(&parsed).ok_or_else(|| {
        crate::AppError::Internal(
            "peer transfer send failed: peer not in discovery cache (offline or unknown)".to_string(),
        )
    })?;

    // 目录递归是潜在阻塞 IO：移出异步上下文（上限保护在收集函数内）
    let trimmed = paths.into_iter().map(|p| p.trim().to_string()).collect::<Vec<_>>();
    let collected = tauri::async_runtime::spawn_blocking(move || collect_outgoing_files(&trimmed))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join source collection failed: {e}")))??;

    let batch_id = uuid::Uuid::new_v4().to_string();
    let encrypt = match encrypt_override {
        Some(v) => v,
        None => {
            super::peer_receive::ensure_settings_loaded(&app)
                .await
                .encryption_enabled
        }
    };
    let total_bytes = collected.total_bytes;
    let sources = collected.sources;
    let state = app.state::<PeerTransferState>();
    let _epoch = state.register_session(&batch_id, sources, parsed.to_string(), encrypt, total_bytes);

    tracing::info!(
        batch_id = %batch_id,
        node_id = %node_id,
        total = total_bytes,
        "peer transfer send session started (no host-side queue, ticket 06)"
    );
    drive_send_session(app.clone(), node, record, batch_id.clone());
    Ok(batch_id)
}

// ==================== 会话驱动（引擎事件桥） ====================

/// 后台发送会话：拨号 → `send_batch` → 事件直推（节流）/ 终态摘除句柄。
///
/// 会话句柄须已登记（send_files_to_peer_with_policy / resume 的 redial 分支
/// 先行 register_session）。`epoch` 是陈旧会话防护：redial 递增代次，旧会话
/// 迟到的终态事件按代次丢弃，不直推（防止插件归约误结算新一轮）。
fn drive_send_session(app: AppHandle, node: PeerNetNode, record: DiscoveredPeerRecord, batch_id: String) {
    let state = app.state::<PeerTransferState>();
    let Some((epoch, pause_slot)) = ({
        let guard = state.sessions.lock().expect("peer transfer sessions poisoned");
        guard.get(&batch_id).map(|h| (h.epoch, Arc::clone(&h.pause_slot)))
    }) else {
        tracing::warn!(batch_id = %batch_id, "drive_send_session: handle not registered, aborted");
        return;
    };

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
                    // 拨号前置失败：直推 failed 终态事件（插件归约结算）；
                    // 句柄随终态摘除（settle_failed 同路径）
                    settle_failed(&session_app, &session_batch_id, &detail).await;
                    return;
                }
            };
            let cancel = session_app
                .state::<PeerTransferState>()
                .cancel_token_of(&session_batch_id)
                .unwrap_or_else(CancelToken::new);
            let sources = session_app
                .state::<PeerTransferState>()
                .sources_of(&session_batch_id)
                .unwrap_or_default();
            let encrypt = session_app
                .state::<PeerTransferState>()
                .encrypt_of(&session_batch_id)
                .unwrap_or(false);
            let _ = bedcode_peer_net::send_batch(
                conn,
                session_batch_id,
                sources,
                events_tx,
                cancel,
                encrypt,
                Some(pause_slot),
            )
            .await;
        });

        // 事件直推循环：Progress 节流（纯性能），Terminal/Paused/Resumed 直推；
        // 宿主零状态机加工（原因码/终态判定归插件归约，票 06）
        let mut last_emit = tokio::time::Instant::now() - PROGRESS_EMIT_INTERVAL;
        while let Some(event) = events_rx.recv().await {
            match event {
                TransferEvent::OfferPending { .. } => {}
                // 服务侧拉取事件走独立 serve 通道（drive_serve_events），
                // 发起方会话通道收不到；防御性忽略
                TransferEvent::PullServed { .. } => {}
                TransferEvent::Progress {
                    batch_id: bid,
                    transferred,
                    total,
                    rate_bps,
                    ..
                } => {
                    app.state::<PeerTransferState>().record_progress(&bid, transferred);
                    if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                        last_emit = tokio::time::Instant::now();
                        publish_engine_event(
                            TOPIC_TRANSFER_EVENT,
                            engine_progress_payload(&bid, transferred, total, rate_bps),
                        );
                    }
                }
                TransferEvent::Terminal {
                    batch_id: bid,
                    state: terminal,
                    remote: _,
                } => {
                    let payload = engine_terminal_payload(&bid, &terminal);
                    // 陈旧会话（redial 后旧代次迟到事件）：不直推、不摘新句柄
                    let stale = !app.state::<PeerTransferState>().session_is_current(&bid, epoch);
                    app.state::<PeerTransferState>().unregister_session(&bid);
                    if !stale {
                        publish_engine_event(TOPIC_TRANSFER_EVENT, payload);
                    }
                    tracing::info!(batch_id = %bid, state = ?terminal, "peer transfer session ended");
                    break;
                }
                // 对端暂停/恢复帧：同步句柄控制面状态 + 直推（插件归约落态）
                TransferEvent::Paused { batch_id: bid, .. } => {
                    app.state::<PeerTransferState>().set_paused(&bid, true);
                    publish_engine_event(TOPIC_TRANSFER_EVENT, engine_pause_payload("paused", &bid));
                }
                TransferEvent::Resumed { batch_id: bid, .. } => {
                    app.state::<PeerTransferState>().set_paused(&bid, false);
                    publish_engine_event(TOPIC_TRANSFER_EVENT, engine_pause_payload("resumed", &bid));
                }
            }
        }
        session.abort();
    });
}

/// 发送专用拨号（每次会话独立握手）：可信对端静默放行；拒绝/不可达归一为
/// 失败明细（业务文案由前端按状态渲染）
async fn dial_for_send(node: &PeerNetNode, record: &DiscoveredPeerRecord) -> Result<Connection, String> {
    match node.dial(&record.to_static_peer_record()).await {
        Ok(conn) => Ok(conn),
        Err(bedcode_peer_net::PeerNetError::DialDeniedByPeer { .. }) => {
            Err("dial denied by peer (trust revoked or first-connect pending)".to_string())
        }
        Err(e) => Err(format!("dial unreachable: {e}")),
    }
}

/// 会话前置失败（拨号被拒/不可达/节点未起）：直推 failed 终态事件并摘除
/// 句柄——插件归约按 terminal 结算任务行，宿主无状态可写
async fn settle_failed(app: &AppHandle, batch_id: &str, detail: &str) {
    let payload = engine_terminal_payload(
        batch_id,
        &TerminalState::Failed {
            detail: detail.to_string(),
        },
    );
    app.state::<PeerTransferState>().unregister_session(batch_id);
    publish_engine_event(TOPIC_TRANSFER_EVENT, payload);
}

// ==================== 服务侧供流事件桥 ====================

/// 服务侧拉取会话事件消费（纯直推）：本端作为拉取源为对端供流时，引擎经独立
/// serve 通道上报 PullServed/Progress/Terminal——逐条直推 `peer:transfer-event`，
/// 供流记账任务行由插件收 `pull-served` 事件自建（票 06：宿主不再代记 send
/// 任务）。Progress 复用 150ms 节流窗口。
pub(crate) async fn drive_serve_events(_app: AppHandle, mut rx: mpsc::Receiver<TransferEvent>) {
    let mut last_emit = tokio::time::Instant::now() - PROGRESS_EMIT_INTERVAL;
    while let Some(event) = rx.recv().await {
        match event {
            TransferEvent::PullServed {
                remote,
                batch_id,
                files,
                total_size,
            } => {
                publish_engine_event(
                    TOPIC_TRANSFER_EVENT,
                    engine_pull_served_payload(&remote, &batch_id, &files, total_size),
                );
                tracing::info!(
                    batch_id = %batch_id,
                    remote = %remote,
                    "pull serve event bridged (accounting owned by plugin)"
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
                        TOPIC_TRANSFER_EVENT,
                        engine_progress_payload(&batch_id, transferred, total, rate_bps),
                    );
                }
            }
            TransferEvent::Terminal { batch_id, state, .. } => {
                publish_engine_event(TOPIC_TRANSFER_EVENT, engine_terminal_payload(&batch_id, &state));
                tracing::info!(batch_id = %batch_id, state = ?state, "pull serve session ended");
            }
            // 拉取方暂停/恢复：直推（插件归约同步记账行状态）
            TransferEvent::Paused { batch_id, .. } => {
                publish_engine_event(TOPIC_TRANSFER_EVENT, engine_pause_payload("paused", &batch_id));
            }
            TransferEvent::Resumed { batch_id, .. } => {
                publish_engine_event(TOPIC_TRANSFER_EVENT, engine_pause_payload("resumed", &batch_id));
            }
            TransferEvent::OfferPending { .. } => {}
        }
    }
}

// ==================== 源文件收集 ====================

/// 收集结果：待发文件清单 + 各文件大小 + 总字节数
pub(crate) struct CollectedSources {
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
/// pub(crate)：host-peer `collect-outgoing` 原语经本函数收集发送源（票 04）
pub(crate) fn collect_outgoing_files(paths: &[String]) -> crate::Result<CollectedSources> {
    let mut sources: Vec<OutgoingFile> = Vec::new();
    let mut sizes: Vec<u64> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();

    for raw in paths {
        let path = PathBuf::from(raw.trim());
        let meta = std::fs::metadata(&path)
            .map_err(|e| crate::AppError::InvalidInput(format!("send source '{}' unreadable: {e}", path.display())))?;
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
            push_outgoing(
                path,
                unique_remote_path(name, &mut used),
                meta.len(),
                &mut sources,
                &mut sizes,
            );
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
    Ok(CollectedSources {
        sources,
        sizes,
        total_bytes,
    })
}

/// 发送源收集原语（host-peer `collect-outgoing`，票 04）：目录递归展开 + 批内
/// 同名去重 → `[{ path, size }]` JSON。纯文件系统事实枚举（仅元数据），
/// 潜在阻塞 IO 移出异步上下文；路径应来自 pick-* 用户选择（选择即授权）
pub(crate) async fn collect_outgoing_for_plugin(paths: Vec<String>) -> crate::Result<String> {
    let collected = tokio::task::spawn_blocking(move || collect_outgoing_files(&paths))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join source collection failed: {e}")))?
        .map_err(|e| crate::AppError::Internal(format!("collect outgoing sources failed: {e}")))?;
    serde_json::to_string(&collected.files_dto())
        .map_err(|e| crate::AppError::Internal(format!("serialize collected sources failed: {e}")))
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
        .map_err(|e| crate::AppError::InvalidInput(format!("read directory '{}' failed: {e}", dir.display())))?
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
        Some((s, e)) if !s.is_empty() && !e.is_empty() && !e.contains('/') => (s.to_string(), e.to_string()),
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
//
// 选择待发送文件（Android：SAF 选择器 → 可读路径优先 / SAF 中转复制兜底；
// 单次单个，可多次累加）。属 `host-platform` 平台原语（与传输编排无关），
// 不随票 06 下沉。

/// 分区存储下 `_data` 直读路径（/storage/emulated/0/...）经 std::fs::open 会
/// EACCES（真机实证：上传 72MB 文件 `Permission denied (os error 13)`）——
/// 需先探活可读性，不可读则把 content URI 中转复制到 app 私有 cache 再发送
/// （私有路径 std::fs 恒可读）。
///
/// 真入口 = host-platform `pick-files`（票 10 起无前端命令面）
pub(crate) async fn peer_pick_files(app_handle: AppHandle) -> crate::Result<Vec<String>> {
    let Some(meta) = crate::plugin::android_plugins::pick_file_android_meta().await? else {
        // 用户取消选择
        return Ok(Vec::new());
    };
    // 快速路径：`_data` 直读路径可打开（All Files Access 已授权等场景）→ 零复制
    if !meta.data_path.is_empty() && std::fs::File::open(&meta.data_path).is_ok() {
        return Ok(vec![meta.data_path]);
    }
    // 分区存储兜底：content URI → app 私有 cache 中转复制（保留原始文件名）
    if meta.uri.is_empty() {
        return Err(crate::AppError::Plugin(
            "file pick returned neither readable path nor SAF uri".to_string(),
        ));
    }
    let dest_name = relay_upload_dest_name(&meta.display_name);
    let path = relay_copy_upload_source(&app_handle, &meta.uri, &dest_name).await?;
    Ok(vec![path])
}

async fn relay_copy_upload_source(app: &AppHandle, uri: &str, dest_name: &str) -> crate::Result<String> {
    let saf = app.state::<crate::plugin::saf_io::SafIoState>();
    let handle = saf
        .read_to_cache(uri, dest_name)
        .map_err(|e| crate::AppError::Plugin(format!("upload relay copy start failed: {e}")))?;
    loop {
        let status = saf
            .copy_status(&handle.copy_id)
            .map_err(|e| crate::AppError::Plugin(format!("upload relay copy poll failed: {e}")))?;
        if status.finished {
            if status.cancelled {
                return Err(crate::AppError::Plugin("upload relay copy cancelled".to_string()));
            }
            if let Some(err) = status.error {
                return Err(crate::AppError::Plugin(format!("upload relay copy failed: {err}")));
            }
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(handle.dest_path)
}

/// 上传源中转副本文件名：保留原始名；空/含路径分隔符兜底为安全名
fn relay_upload_dest_name(display_name: &str) -> String {
    let name: String = display_name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    let name = name.trim().to_string();
    if name.is_empty() || name == "." || name == ".." {
        "upload-file".to_string()
    } else {
        name
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn sources(n: usize) -> Vec<OutgoingFile> {
        (0..n).map(|i| OutgoingFile::new(format!("C:/f-{i}.bin"))).collect()
    }

    fn state_with_session(batch_id: &str) -> PeerTransferState {
        let state = PeerTransferState::default();
        state.register_session(batch_id, sources(1), "n".into(), false, 1);
        state
    }

    // ==================== 会话句柄表（票 06 终态） ====================

    /// 登记与代次：同批重复登记递增 epoch（redial 陈旧事件防护）
    #[test]
    fn register_session_increments_epoch_per_batch() {
        let state = PeerTransferState::default();
        assert_eq!(state.register_session("b", sources(1), "n".into(), false, 1), 0);
        assert_eq!(state.register_session("b", sources(1), "n".into(), false, 1), 1);
        assert_eq!(state.register_session("other", sources(1), "n".into(), false, 1), 0);
        assert!(state.session_is_current("b", 1));
        assert!(!state.session_is_current("b", 0), "旧代次事件必须被判陈旧");
    }

    /// 取消/暂停/进度记账走句柄表；终态摘除后全部查询落空
    #[test]
    fn handle_table_lifecycle_register_to_unregister() {
        let state = PeerTransferState::default();
        state.register_session("b", sources(2), "n".into(), true, 10);
        assert!(state.has_session("b"));
        assert!(state.cancel_token_of("b").is_some());
        assert!(state.pause_slot_of("b").is_some());
        assert_eq!(state.sources_of("b").map(|s| s.len()), Some(2));
        assert_eq!(state.encrypt_of("b"), Some(true));

        state.record_progress("b", 7);
        state.set_paused("b", true);
        {
            let guard = state.sessions.lock().expect("lock");
            let handle = guard.get("b").expect("handle");
            assert_eq!(handle.transferred_bytes, 7);
            assert!(handle.paused);
        }

        state.unregister_session("b");
        assert!(!state.has_session("b"));
        assert!(state.cancel_token_of("b").is_none());
        assert!(state.pause_slot_of("b").is_none());
        assert!(!state.session_is_current("b", 0), "已摘除会话一切事件视为陈旧");
    }

    /// 幂等取消/暂停语义：不在册批次查句柄表返回空/false（宿主不猜产品态）
    #[test]
    fn unknown_batch_is_absent_from_handle_table() {
        let state = PeerTransferState::default();
        assert!(!state.has_session("nope"));
        assert!(state.cancel_token_of("nope").is_none());
        assert!(state.pause_slot_of("nope").is_none());
        assert!(state.sources_of("nope").is_none());
        // 记进度/置暂停对不在册批次是 no-op（不凭空建句柄）
        state.record_progress("nope", 5);
        state.set_paused("nope", true);
        assert!(!state.has_session("nope"));
    }

    // ==================== 引擎事实投影（active-transfers） ====================

    /// 句柄表投影：仅引擎会话事实（batchId/direction/status/字节/时间），
    /// 业务字段（peerName/files/原因码）不得进投影（fail-visible 前置：
    /// 插件拿不到就自持，不会静默缺名）
    #[test]
    fn send_handle_row_carries_engine_facts_only() {
        let mut handle = SendSessionHandle {
            cancel_token: CancelToken::new(),
            epoch: 0,
            pause_slot: PauseSlot::new(),
            sources: sources(2),
            node_id: "n".to_string(),
            encrypt: true,
            total_bytes: 3,
            transferred_bytes: 3,
            paused: true,
        };
        let row = send_handle_row("b-1", &handle);
        assert_eq!(row["direction"], "send");
        assert_eq!(row["status"], "paused", "控制面 paused 事实入投影");
        assert_eq!(row["totalBytes"], 3);
        assert_eq!(row["transferredBytes"], 3);
        assert!(row.get("peerName").is_none(), "peerName 是业务字段，不得投影");
        assert!(row.get("files").is_none(), "files 是业务字段，不得投影");
        assert!(row.get("rejectReason").is_none(), "原因码是业务字段，不得投影");

        handle.paused = false;
        assert_eq!(send_handle_row("b-2", &handle)["status"], "running");
    }

    // ==================== 源收集（collect-outgoing 原语） ====================

    /// 目录递归展开 + 批内同名去重（与 collect_outgoing_files 契约一致）
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
                PeerTransferFileDto {
                    path: "photos/a.png".to_string(),
                    size: 10
                },
                PeerTransferFileDto {
                    path: "photos/sub/b.png".to_string(),
                    size: 5
                },
                PeerTransferFileDto {
                    path: "notes.txt".to_string(),
                    size: 3
                },
            ]
        );

        // 同名文件（不同目录来源）remote 目标自动编号
        let other = dir.path().join("elsewhere");
        std::fs::create_dir_all(&other).expect("mkdir");
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

    /// send-files 收窄契约：一次调用即发一会话——源清单/寻址/加密全部落句柄表，
    /// 插件归约事件时不再需要宿主任务表
    #[test]
    fn registered_session_carries_send_parameters_for_event_bridge() {
        let state = state_with_session("batch-x");
        assert!(state.has_session("batch-x"));
        assert_eq!(state.sources_of("batch-x").map(|s| s.len()), Some(1));
        assert_eq!(state.encrypt_of("batch-x"), Some(false));
    }
}
