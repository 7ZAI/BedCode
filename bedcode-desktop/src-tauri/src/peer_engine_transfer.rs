//! 对等网络发送侧引擎适配器（issue 09 编排下沉后的宿主残面，票 06 收敛）。
//!
//! 领域模型（spec 决策 2）：「群发」仅是插件前端编排概念——每个接收方一条
//! 相互独立的传输批（独立 batch_id / 独立进度 / 独立策略闸门），任一接收方
//! 拒绝不影响其余。产品编排（扇出、重试、任务列表、历史持久化）在
//! file-transfer 插件（host-peer 原语 + `peer:transfer` 总线快照驱动）；
//! 本模块只做 WIT `send-files` / `pause-transfer` / `resume-transfer` /
//! `close` 原语的引擎接入：
//!
//! - 会话接入：收集源文件 → 拨号 → `send_batch` 后台会话；并发闸门
//!   （插件设置真源经发送载荷脉冲同步）与暂停句柄登记；
//! - 快照推送：引擎 Progress 按 chunk 高频发射，适配器做时间窗节流后把
//!   活动批快照推 `peer:transfer` 总线 topic（仅总线：桌面 Tauri 前端
//!   事件桥随宿主命令面退役，票 06）；
//! - 供流记账：对端拉取本机文件时在引擎事件侧登记 direction=send 任务，
//!   双端对同一次传输各自展示（pull 发起方的 receive 任务在接收侧模块）。
//!
//! 历史只在内存存活于节点生命周期：真源在插件私有库，升级/重启后由插件
//! 恢复；重试由插件回放 `send-files` 原语实现（新批 ID，断点真源在接收端
//! 落盘侧），宿主不再持有重试编排。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bedcode_peer_net::{
    CancelToken, Connection, DiscoveredPeerRecord, FileMeta, NodeId, OutgoingFile, PauseCmd,
    PauseSlot, PeerNetNode, TerminalState, TransferEvent,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

// ==================== 常量 ====================

/// 历史封顶条数：超出后按最旧优先滚动淘汰（spec 用户故事 26）
const HISTORY_CAP: usize = 200;
/// 进度事件最小发射间隔：引擎按 ≤64KiB chunk 发射 Progress，
/// 不加节流会在高速链路打爆 IPC；数值远低于人眼感知阈值
const PROGRESS_EMIT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);
/// 单批文件数上限：目录递归收集的失控保护
const MAX_FILES_PER_BATCH: usize = 512;

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
    /// 本批强制加密（插件载荷聚合；None = 回落接收设置全局开关）
    encrypt: Option<bool>,
}

/// Tauri 托管的传输任务状态容器
///
/// - `inner` 单把 Mutex 串行化全部读写（临界区均为内存短操作），磁盘 IO 一律
///   锁外：首次惰性加载与终态落盘都经 spawn_blocking 移出异步上下文；
/// - `sessions` 登记活跃会话的取消令牌与代次：重试换发新令牌并递增代次，
///   陈旧会话迟到的事件按代次丢弃，防止覆盖新一轮状态；
/// - `pauses` 登记活跃会话的暂停句柄（wire Pause/Resume 门控）：暂停/恢复
///   命令按 batch_id 下发，会话终态时摘除（生命周期与 sessions 同步）。
#[derive(Default)]
pub struct PeerTransferState {
    inner: Mutex<PeerTransferInner>,
    sessions: Mutex<HashMap<String, (CancelToken, u64)>>,
    pauses: Mutex<HashMap<String, Arc<PauseSlot>>>,
}

#[derive(Default)]
struct PeerTransferInner {
    /// 全量任务列表（活跃 + 历史；最新在前）
    tasks: Vec<SendTask>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 终态判断：completed/failed/rejected/cancelled（pending 排队中、paused 用户暂停、
/// running 传输中均非终态——历史封顶淘汰只逐出真终态）
fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "rejected" | "cancelled")
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

    /// 登记会话暂停句柄（发送会话启动时挂载；暂停/恢复命令按 batch_id 下发）
    fn register_pause(&self, batch_id: &str, slot: Arc<PauseSlot>) {
        self.pauses
            .lock()
            .expect("peer transfer pauses poisoned")
            .insert(batch_id.to_string(), slot);
    }

    /// 取会话暂停句柄（无句柄返回 None：发送会话未启动/已结算/非本端会话）
    fn pause_slot_of(&self, batch_id: &str) -> Option<Arc<PauseSlot>> {
        self.pauses
            .lock()
            .expect("peer transfer pauses poisoned")
            .get(batch_id)
            .cloned()
    }

    /// 会话结束/终态时摘除暂停句柄（与 unregister_session 同生命周期）
    fn unregister_pause(&self, batch_id: &str) {
        self.pauses
            .lock()
            .expect("peer transfer pauses poisoned")
            .remove(batch_id);
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

// ==================== 原语适配面 ====================

/// 取消进行中的发送任务（幂等：已终态返回 false）。排队中（pending）任务
/// 无活动会话，直接标记终态；运行中任务触发 CancelToken 中断会话。
pub async fn cancel_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    let state = app.state::<PeerTransferState>();
    match state.cancel_token_of(&batch_id) {
        Some(token) => {
            tracing::info!(batch_id = %batch_id, "peer transfer cancel requested");
            token.cancel();
            Ok(true)
        }
        None => {
            // 服务侧拉取记账任务（本端供流）：经 handler 按 serve batch_id
            // 取消——会话写 Cancel 帧告知拉取方并停供流，双端各自落 Cancelled
            if cancel_serve_session(&app, &batch_id).await {
                return Ok(true);
            }
            // 无活动会话：pending 排队任务直接乐观结算为 cancelled（引擎终态
            // 不会再到来）；paused 任务保留（用户稍后可恢复/重试）
            let mut hit = false;
            {
                let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
                if let Some(task) = inner
                    .tasks
                    .iter_mut()
                    .find(|t| t.dto.batch_id == batch_id && t.dto.status == "pending")
                {
                    task.dto.status = "cancelled".to_string();
                    task.dto.rate_bps = 0.0;
                    task.dto.updated_at_ms = now_ms();
                    hit = true;
                }
            }
            if hit {
                evict_history_cap_locked(&mut state.inner.lock().expect("peer transfer lock poisoned").tasks);
                publish(&app);
            }
            Ok(hit)
        }
    }
}

/// 显式暂停进行中的发送任务：数据面门控（wire Pause 帧 + 供方停推流），
/// 连接保持不断开，任务保留（含已传字节）状态置 `paused` 不落历史；
/// 对端任务经 Pause 帧同步为 paused，恢复续流无缝衔接。
///
/// 服务侧拉取记账任务（sources 空）经宿主 handler 按 serve batch_id 门控。
pub async fn pause_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    // 非发送任务（拉取接收等）：回落接收方向暂停
    if !send_task_exists(&app, &batch_id) {
        return super::peer_engine_receive::pause_peer_receiving(app, batch_id).await;
    }
    let state = app.state::<PeerTransferState>();
    let route = {
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let Some(task) = inner.tasks.iter_mut().find(|t| t.dto.batch_id == batch_id) else {
            return Ok(false);
        };
        if task.dto.status != "running" || task.dto.direction != "send" {
            return Ok(false);
        }
        // 已完成校验：字节已满的发送任务不允许暂停（UI 滞后显示未完成，
        // 实际已全部传输），直接结算 completed，避免误标 paused
        if transfer_complete(task.dto.total_bytes, task.dto.transferred_bytes) {
            task.dto.status = "completed".to_string();
            task.dto.rate_bps = 0.0;
            task.dto.updated_at_ms = now_ms();
            drop(inner);
            publish(&app);
            tracing::debug!(batch_id = %batch_id, "pause skipped: send transfer already complete");
            return Ok(true);
        }
        task.dto.status = "paused".to_string();
        task.dto.rate_bps = 0.0;
        task.dto.updated_at_ms = now_ms();
        pause_route(task.sources.is_empty())
    };
    match route {
        // 服务侧拉取记账任务（sources 空）：门控入口在 SharedDirHandler 的
        // serve 暂停注册表（会话内写 Pause 帧 + 停供流）
        PauseRoute::Serve => match super::peer_engine_receive::handler_and_config(&app).await {
            Some((handler, _)) if handler.set_serve_paused(&batch_id, true).await => {
                publish(&app);
                tracing::info!(batch_id = %batch_id, "pull serve transfer paused");
                Ok(true)
            }
            _ => {
                // 无活动 serve 会话（对端已断开/会话已结束）：还原状态，避免假暂停
                set_running_status(&app, &batch_id).await;
                tracing::debug!(batch_id = %batch_id, "pause transfer miss: no live serve session");
                Ok(false)
            }
        },
        // 本端发起的发送会话：门控暂停（写 Pause 帧并停推流，连接保持）；
        // 无活动会话回落取消令牌
        PauseRoute::SendSession => {
            let handled = if let Some(slot) = state.pause_slot_of(&batch_id) {
                slot.send(PauseCmd::Pause).await
            } else {
                false
            };
            if !handled {
                if let Some(token) = state.cancel_token_of(&batch_id) {
                    token.cancel();
                }
            }
            publish(&app);
            // 暂停释放一个并发槽：推进队列中下一个 pending
            pump_send_queue(app.clone()).await;
            tracing::info!(batch_id = %batch_id, "peer transfer paused");
            Ok(true)
        }
    }
}

/// 暂停路由：暂停的目标会话类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseRoute {
    /// 服务侧拉取记账任务（本端在对端拉取中供流）
    Serve,
    /// 本端发起的发送会话
    SendSession,
}

/// 纯函数：暂停路由裁决（回归护栏）
///
/// 这两个分支曾经写反：serve 记账任务落进「发送会话」分支 → 查不到发送暂停
/// 句柄、也没有取消令牌 → 什么都没门控却返回成功，表现为真机现象
/// 「本端显示已暂停，对端仍在持续传输」。
fn pause_route(sources_empty: bool) -> PauseRoute {
    if sources_empty {
        PauseRoute::Serve
    } else {
        PauseRoute::SendSession
    }
}

/// 恢复暂停的发送任务：连接未断则直接续流（wire Resume 帧 + 供方续推流），
/// 无活动会话（重启/断线后残留 paused）回落旧路径：入队（pending）并经并发
/// 闸门重新拨号，接收端按已写偏移续传（断点真源在落盘侧）；保留已传字节。
pub async fn resume_peer_transfer(app: AppHandle, batch_id: String) -> crate::Result<bool> {
    // 非发送任务（拉取接收等）：回落接收方向恢复
    if !send_task_exists(&app, &batch_id) {
        return super::peer_engine_receive::resume_peer_receiving(app, batch_id).await;
    }
    let resume_mode = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let Some(task) = inner.tasks.iter_mut().find(|t| t.dto.batch_id == batch_id) else {
            return Ok(false);
        };
        if task.dto.status != "paused" || task.dto.direction != "send" {
            return Ok(false);
        }
        task.dto.updated_at_ms = now_ms();
        // 服务侧记账任务（sources 空）走 serve 门控恢复；否则若活动会话仍在
        // （连接保持），直接续流；会话已结算则回落重新拨号路径
        if task.sources.is_empty() {
            ResumeMode::Serve
        } else if peer_transfer_state_has(&app, &batch_id) {
            ResumeMode::Live
        } else {
            ResumeMode::Redial
        }
    };
    match resume_mode {
        ResumeMode::Serve => {
            match super::peer_engine_receive::handler_and_config(&app).await {
                Some((handler, _)) if handler.set_serve_paused(&batch_id, false).await => {
                    set_running_status(&app, &batch_id).await;
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
        ResumeMode::Live => {
            let sent = match app.state::<PeerTransferState>().pause_slot_of(&batch_id) {
                Some(slot) => slot.send(PauseCmd::Resume).await,
                None => false,
            };
            if !sent {
                // 会话已死（slot 存在但命令通道已关，或 slot 已被摘除）：
                // 摘除残留 slot 并回落重新拨号续传，避免下次恢复仍误判活跃
                app.state::<PeerTransferState>().unregister_pause(&batch_id);
                tracing::debug!(batch_id = %batch_id, "resume live miss: session dead, redial fallback");
                return Ok(resume_via_redial(&app, &batch_id).await);
            }
            set_running_status(&app, &batch_id).await;
            Ok(true)
        }
        ResumeMode::Redial => Ok(resume_via_redial(&app, &batch_id).await),
    }
}

/// 重新拨号恢复（Live 失败回落 / Redial 分支共用）：入队 pending，
/// 由并发闸门重新拨号，接收端按已写偏移续传（断点真源在落盘侧）；
/// 保留已传字节（引擎 Progress 首帧会重设为偏移）。
async fn resume_via_redial(app: &AppHandle, batch_id: &str) -> bool {
    let resumed = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let Some(task) = inner.tasks.iter_mut().find(|t| t.dto.batch_id == batch_id) else {
            return false;
        };
        if task.sources.is_empty() {
            return false;
        }
        task.dto.status = "pending".to_string();
        task.dto.rate_bps = 0.0;
        task.dto.detail = None;
        task.dto.reject_reason = None;
        task.dto.updated_at_ms = now_ms();
        true
    };
    if resumed {
        publish(app);
        pump_send_queue(app.clone()).await;
        tracing::info!(batch_id = %batch_id, "peer transfer resume queued");
    }
    resumed
}

/// 经 handler 取消服务侧拉取会话（本端在对端拉取中供流）：会话按 serve
/// batch_id（= 传输任务行 ID）寻址，命中即写 Cancel 帧并停供流。
async fn cancel_serve_session(app: &AppHandle, batch_id: &str) -> bool {
    match super::peer_engine_receive::handler_and_config(app).await {
        Some((handler, _)) => handler.cancel_serve_transfer(batch_id),
        None => false,
    }
}

/// 发送方向任务是否存在（含历史）：pause/resume 的接收方向回落判据
fn send_task_exists(app: &AppHandle, batch_id: &str) -> bool {
    let state = app.state::<PeerTransferState>();
    let inner = state.inner.lock().expect("peer transfer lock poisoned");
    inner
        .tasks
        .iter()
        .any(|t| t.dto.batch_id == batch_id && t.dto.direction == "send")
}

/// 恢复模式：serve 门控 / 活跃会话续流 / 重新拨号续传
enum ResumeMode {
    Serve,
    Live,
    Redial,
}

/// 活跃发送会话是否存在（pauses 表在会话周期内存在即视为活跃）
fn peer_transfer_state_has(app: &AppHandle, batch_id: &str) -> bool {
    app.state::<PeerTransferState>()
        .pause_slot_of(batch_id)
        .is_some()
}

/// 任务状态置回 running 并推送（resume 的 Live/Serve 分支共用）
async fn set_running_status(app: &AppHandle, batch_id: &str) {
    let state = app.state::<PeerTransferState>();
    {
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id && t.dto.status == "paused")
        {
            task.dto.status = "running".to_string();
            task.dto.updated_at_ms = now_ms();
        }
    }
    publish(app);
}

/// 恢复全部暂停的传输任务：发送方向逐个入队（受并发闸门约束）+ 同端已暂停的
/// 接收任务经 wire Resume 帧续流。返回恢复数。
pub async fn resume_all_peer_transfers(app: AppHandle) -> crate::Result<usize> {
    // 活跃会话（连接保持）直接续流；无会话的回落重新拨号入队；serve
    // 记账任务经 handler 门控恢复（本端暂停的供流批同样要能被「全部继续」拉起）
    let mut live_ids: Vec<String> = Vec::new();
    let mut serve_ids: Vec<String> = Vec::new();
    let queued_ids: Vec<String> = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let mut queued = Vec::new();
        for task in inner
            .tasks
            .iter_mut()
            .filter(|t| t.dto.direction == "send" && t.dto.status == "paused")
        {
            if task.sources.is_empty() {
                task.dto.updated_at_ms = now_ms();
                serve_ids.push(task.dto.batch_id.clone());
                continue;
            }
            if state.pause_slot_of(&task.dto.batch_id).is_some() {
                task.dto.updated_at_ms = now_ms();
                live_ids.push(task.dto.batch_id.clone());
            } else {
                task.dto.status = "pending".to_string();
                task.dto.rate_bps = 0.0;
                task.dto.detail = None;
                task.dto.reject_reason = None;
                task.dto.updated_at_ms = now_ms();
                queued.push(task.dto.batch_id.clone());
            }
        }
        queued
    };
    for bid in &live_ids {
        if let Some(slot) = app.state::<PeerTransferState>().pause_slot_of(bid) {
            slot.send(PauseCmd::Resume).await;
        }
        set_running_status(&app, bid).await;
    }
    // serve 门控恢复：成功才置回 running（无活动会话保持 paused，等取消/重试）
    let mut resumed_serve = 0usize;
    for bid in &serve_ids {
        let resumed = match super::peer_engine_receive::handler_and_config(&app).await {
            Some((handler, _)) => handler.set_serve_paused(bid, false).await,
            None => false,
        };
        if resumed {
            set_running_status(&app, bid).await;
            resumed_serve += 1;
        }
    }
    if !queued_ids.is_empty() {
        publish(&app);
        pump_send_queue(app.clone()).await;
        tracing::info!(count = queued_ids.len(), "peer transfer resume all queued");
    }
    // 接收方向（拉取 / push 接收）暂停任务：与单条恢复同路径（wire Resume 帧
    // 请求对端数据供方解除门控）；「全部继续」按钮对双方向一致生效
    let resumed_receive = super::peer_engine_receive::resume_all_peer_receiving(&app).await;
    Ok(live_ids.len() + queued_ids.len() + resumed_serve + resumed_receive)
}

/// 发送入口（issue 13 Phase 3 步骤 5）：`encrypt_override` 为
/// `Some(true)` 时本批强制加密（插件载荷元素级 flag 聚合），`None` 回落引擎
/// 接收设置全局开关——插件经 host-peer `send-files` 原语调用并解析载荷。
pub(crate) async fn send_files_to_peer_with_policy(
    app: AppHandle,
    node_id: String,
    paths: Vec<String>,
    encrypt_override: Option<bool>,
) -> crate::Result<PeerTransferDto> {
    let parsed = super::peer_net::parse_node_id(&node_id)?;
    if paths.is_empty() || paths.iter().all(|p| p.trim().is_empty()) {
        return Err(crate::AppError::InvalidInput(
            "send_files_to_peer: paths must not be empty".to_string(),
        ));
    }
    // `node` 仅用于确认节点已启动（runtime_snapshot 的可用性即启动判据）
    let Some((_node, cache)) = super::peer_net::runtime_snapshot(&app).await else {
        return Err(crate::AppError::Internal(
            "peer transfer send failed: node not started".to_string(),
        ));
    };
    let record = cache.get(&parsed).ok_or_else(|| {
        crate::AppError::Internal(
            "peer transfer send failed: peer not in discovery cache (offline or unknown)".to_string(),
        )
    })?;
    let peer_name = record.device_name.clone();

    // 目录递归是潜在阻塞 IO：移出异步上下文（上限保护在收集函数内）
    let trimmed = paths.into_iter().map(|p| p.trim().to_string()).collect::<Vec<_>>();
    let collected = tauri::async_runtime::spawn_blocking(move || collect_outgoing_files(&trimmed))
        .await
        .map_err(|e| crate::AppError::Internal(format!("join source collection failed: {e}")))??;

    let batch_id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    let dto = PeerTransferDto {
        batch_id: batch_id.clone(),
        node_id: parsed.to_string(),
        peer_name,
        direction: "send".to_string(),
        // 入队即排队态：并发闸门（pump_send_queue）负责在槽位空出时启动
        status: "pending".to_string(),
        files: collected.files_dto(),
        total_bytes: collected.total_bytes,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: now,
        updated_at_ms: now,
    };

    {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        inner.tasks.insert(
            0,
            SendTask {
                dto: dto.clone(),
                sources: collected.sources.clone(),
                encrypt: encrypt_override,
            },
        );
    }

    publish(&app);
    pump_send_queue(app.clone()).await;
    tracing::info!(
        batch_id = %dto.batch_id,
        node_id = %node_id,
        files = dto.files.len(),
        total = dto.total_bytes,
        "peer transfer send enqueued"
    );
    Ok(dto)
}

// ==================== 并发闸门（限制同时传输数量） ====================

/// 发送方向并发上限（插件设置真源，经发送载荷脉冲同步宿主；缺省 3）
async fn current_concurrency(app: &AppHandle) -> usize {
    super::peer_engine_receive::ensure_settings_loaded(app).await.concurrency as usize
}

/// 锁内统计发送方向 running 数（并发槽占用；服务侧拉取记账任务 sources 为空，
/// 属响应式供流不计入发起方向并发槽）
fn running_send_count_locked(tasks: &[SendTask]) -> usize {
    tasks
        .iter()
        .filter(|t| t.dto.direction == "send" && t.dto.status == "running" && !t.sources.is_empty())
        .count()
}

/// 纯函数：并发闸门裁决——返回应启动的 pending 批 ID（最旧优先，受
/// `limit - running` 槽位约束），并把它们置为 running。拨号/会话在锁外。
/// 服务侧记账任务（sources 空）不可重启动、paused 不自动启动。
fn pick_pending_to_start(tasks: &mut [SendTask], limit: usize) -> Vec<String> {
    let running = running_send_count_locked(tasks);
    let mut budget = limit.saturating_sub(running);
    let mut picked = Vec::new();
    // tasks 最新在前；倒序遍历取最旧 pending 保证先入先启动
    for task in tasks.iter_mut().rev() {
        if budget == 0 {
            break;
        }
        if task.dto.direction == "send" && task.dto.status == "pending" && !task.sources.is_empty() {
            task.dto.status = "running".to_string();
            task.dto.updated_at_ms = now_ms();
            picked.push(task.dto.batch_id.clone());
            budget -= 1;
        }
    }
    picked
}

/// 并发闸门：槽位空出时从最旧 pending 启动发送批（锁内裁决状态迁移，
/// 拨号/会话在锁外）。所有「进入 running」的路径（enqueue/retry/resume/
/// 终态推进）都经此闸门——批量上传受并发数限制，逐个排队启动。
async fn pump_send_queue(app: AppHandle) {
    let limit = current_concurrency(&app).await.max(1);
    let to_start: Vec<String> = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        pick_pending_to_start(&mut inner.tasks, limit)
    };
    for batch_id in to_start {
        start_send_session(&app, &batch_id).await;
    }
}

/// 终态/暂停后推进队列（任务化：打断 apply_terminal → pump → start →
/// settle_failed → apply_terminal 的异步递归链，避免无限栈深）
fn pump_after_settle(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        pump_send_queue(app).await;
    });
}

/// 启动单个发送批会话（批必须已在 tasks 中且 status == running）
///
/// 先登记会话代次再拨号——拨号前置失败（对端不可达/信任被撤）经该代次
/// 直接落 failed 终态并继续推进队列（与 drive_send_session 内部同构）。
async fn start_send_session(app: &AppHandle, batch_id: &str) {
    let (node_id, sources, encrypt) = {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let Some(task) = inner.tasks.iter_mut().find(|t| t.dto.batch_id == batch_id) else {
            return;
        };
        if task.dto.status != "running" || task.dto.direction != "send" {
            return;
        }
        if task.sources.is_empty() {
            // 服务侧拉取记账任务（sources 空）不可由本闸门启动；置回 pending 防卡死
            task.dto.status = "pending".to_string();
            return;
        }
        (task.dto.node_id.clone(), task.sources.clone(), task.encrypt)
    };

    let epoch = {
        let state = app.state::<PeerTransferState>();
        state.register_session(batch_id)
    };
    let Some((node, cache)) = super::peer_net::runtime_snapshot(app).await else {
        settle_failed(app, batch_id, "peer transfer start failed: node not started", epoch).await;
        return;
    };
    let parsed = match super::peer_net::parse_node_id(&node_id) {
        Ok(p) => p,
        Err(e) => {
            settle_failed(app, batch_id, &format!("peer id invalid: {e}"), epoch).await;
            return;
        }
    };
    let Some(record) = cache.get(&parsed) else {
        settle_failed(
            app,
            batch_id,
            "peer transfer start failed: peer not in discovery cache (offline or unknown)",
            epoch,
        )
        .await;
        return;
    };
    let encrypt = match encrypt {
        Some(v) => v,
        // 插件不显式下发加密时恒不加密（原 settings.encryption_enabled 已无写入
        // 方、恒 false，④1 清理后显式化）：应用层 AES-256-GCM 由 send-files 载荷
        // `encrypt` 字段逐次开关，不再有全局接受侧设置真源
        None => false,
    };
    drive_send_session(app.clone(), node, record, batch_id.to_string(), sources, epoch, encrypt);
    tracing::debug!(batch_id = %batch_id, "peer transfer session started by queue");
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
        // 暂停句柄：会话启动即挂载（宿主暂停/恢复命令经它写 Pause/Resume 帧），
        // 会话终态时摘除（与 sessions 生命周期同步）
        let pause_slot = PauseSlot::new();
        session_app
            .state::<PeerTransferState>()
            .register_pause(&session_batch_id, Arc::clone(&pause_slot));
        let session = tauri::async_runtime::spawn(async move {
            let conn: Connection = match dial_for_send(&node, &record).await {
                Ok(conn) => conn,
                Err(detail) => {
                    // 摘除暂停句柄：dial 失败不进入事件循环，Terminal 分支的
                    // unregister_pause 不会执行；残留 slot 会让后续 resume 误判活跃
                    session_app
                        .state::<PeerTransferState>()
                        .unregister_pause(&session_batch_id);
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
                Some(pause_slot),
            )
            .await;
        });

        // 事件转发循环：Progress 节流推送，Terminal 结算历史并即时推送
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
                    rate_bps,
                    ..
                } => {
                    update_progress(&app, &bid, transferred, rate_bps);
                    if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                        last_emit = tokio::time::Instant::now();
                        publish(&app);
                    }
                }
                TransferEvent::Terminal {
                    batch_id: bid,
                    state,
                    remote: _,
                } => {
                    app.state::<PeerTransferState>().unregister_pause(&bid);
                    apply_terminal(&app, &bid, state, epoch).await;
                    break;
                }
                // 对端暂停/恢复帧：同步本端发送任务状态（接收方暂停/恢复推送）
                TransferEvent::Paused { batch_id: bid, .. } => {
                    set_pause_status(&app, &bid, true).await;
                }
                TransferEvent::Resumed { batch_id: bid, .. } => {
                    set_pause_status(&app, &bid, false).await;
                }
            }
        }
        session.abort();
    });
}

/// 对端暂停/恢复事件 → 本端发送任务状态同步（paused ↔ running）
async fn set_pause_status(app: &AppHandle, batch_id: &str, paused: bool) {
    let state = app.state::<PeerTransferState>();
    {
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        let Some(task) = inner.tasks.iter_mut().find(|t| t.dto.batch_id == batch_id) else {
            tracing::debug!(batch_id = %batch_id, "set_pause_status: task not found, ignored");
            return;
        };
        if task.dto.status != "running" {
            // 对端 Pause/Resume 帧迟到/状态错乱：尊重本端状态，不覆盖终态/暂停意图
            tracing::debug!(
                batch_id = %batch_id,
                current = %task.dto.status,
                paused,
                "set_pause_status: task not in running state, ignored"
            );
            return;
        }
        task.dto.status = if paused {
            "paused".to_string()
        } else {
            "running".to_string()
        };
        task.dto.rate_bps = 0.0;
        task.dto.updated_at_ms = now_ms();
    }
    publish(app);
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

/// 终态结算：状态映射 → 历史落盘 → 即时推送（epoch 不匹配的陈旧会话丢弃）。
/// 用户暂停（任务已标 paused）的会话终态事件跳过结算——任务保留，仅归还
/// 并发槽并推进队列。
async fn apply_terminal(app: &AppHandle, batch_id: &str, terminal: TerminalState, epoch: u64) {
    let state = app.state::<PeerTransferState>();
    if !state.session_is_current(batch_id, epoch) {
        tracing::debug!(batch_id = %batch_id, "stale transfer session event ignored");
        return;
    }
    state.unregister_session(batch_id);

    // 用户暂停分支：pause_peer_transfer 已把任务标 paused 并触发取消，
    // 会话终态事件只是中断确认——不落终态、不覆盖进度展示
    let user_paused = {
        let inner = state.inner.lock().expect("peer transfer lock poisoned");
        inner
            .tasks
            .iter()
            .find(|t| t.dto.batch_id == batch_id)
            .map(|t| t.dto.status == "paused")
            .unwrap_or(false)
    };
    if user_paused {
        tracing::info!(batch_id = %batch_id, state = ?terminal, "session ended by user pause");
        // 归还并发槽，推进队列中下一个 pending
        pump_after_settle(app);
        return;
    }

    let (status, detail, reject_reason) = match &terminal {
        TerminalState::Completed => ("completed".to_string(), None, None),
        TerminalState::Rejected { reason } => ("rejected".to_string(), None, Some(reason.as_str().to_string())),
        TerminalState::Cancelled { by_peer } => (
            "cancelled".to_string(),
            // 机器可读原因码（前端 i18n 映射；兼容映射旧本地化文本），
            // 禁止把人类文案直接落 wire
            Some(if *by_peer {
                "cancelled-by-receiver".to_string()
            } else {
                "cancelled-by-self".to_string()
            }),
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
    publish(app);
    // 终态归还并发槽：推进队列中下一个 pending（批量上传排队自动衔接）
    pump_after_settle(app);
}

/// 会话前置失败（拨号被拒/不可达等）：直接落 failed 终态
async fn settle_failed(app: &AppHandle, batch_id: &str, detail: &str, epoch: u64) {
    apply_terminal(
        app,
        batch_id,
        TerminalState::Failed {
            detail: detail.to_string(),
        },
        epoch,
    )
    .await;
}

// ==================== 服务侧拉取记账（双端记账） ====================

/// 服务侧拉取会话事件消费（双端记账）：本端作为拉取源为对端供流时，引擎
/// 经独立 serve 通道上报 PullServed/Progress/Terminal——此处登记/推进/结算
/// 一条 direction=send 的任务并推 peer-transfer-changed。拉取发起方（对端）
/// 另有自己的 receive 任务，两端各自展示同一次传输（spec 用户故事 26）。
pub(crate) async fn drive_serve_events(app: AppHandle, mut rx: mpsc::Receiver<TransferEvent>) {
    let mut last_emit = tokio::time::Instant::now() - PROGRESS_EMIT_INTERVAL;
    while let Some(event) = rx.recv().await {
        match event {
            TransferEvent::PullServed {
                remote,
                batch_id,
                files,
                total_size,
            } => {
                register_serve_task(&app, &remote, &batch_id, files, total_size).await;
                publish(&app);
            }
            TransferEvent::Progress {
                batch_id,
                transferred,
                rate_bps,
                ..
            } => {
                update_progress(&app, &batch_id, transferred, rate_bps);
                if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                    last_emit = tokio::time::Instant::now();
                    publish(&app);
                }
            }
            TransferEvent::Terminal { batch_id, state, .. } => {
                settle_serve_terminal(&app, &batch_id, state).await;
            }
            // 拉取方暂停/恢复：同步服务侧记账任务状态（数据面门控在 serve 会话）
            TransferEvent::Paused { batch_id, .. } => {
                set_serve_pause_status(&app, &batch_id, true).await;
            }
            TransferEvent::Resumed { batch_id, .. } => {
                set_serve_pause_status(&app, &batch_id, false).await;
            }
            TransferEvent::OfferPending { .. } => {}
        }
    }
}

/// 服务侧记账任务暂停/恢复状态同步（拉取方 Pause/Resume 帧到达时）
async fn set_serve_pause_status(app: &AppHandle, batch_id: &str, paused: bool) {
    let state = app.state::<PeerTransferState>();
    {
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id && serve_status_tracked(&t.dto.status))
        {
            task.dto.status = if paused { "paused".to_string() } else { "running".to_string() };
            task.dto.rate_bps = 0.0;
            task.dto.updated_at_ms = now_ms();
        }
    }
    publish(app);
}

/// 服务侧任务登记：解析对端展示名后插入 send 任务（服务侧不可重试：sources 空）
async fn register_serve_task(app: &AppHandle, remote: &NodeId, batch_id: &str, files: Vec<FileMeta>, total_size: u64) {
    let peer_name = super::peer_net::runtime_snapshot(app)
        .await
        .and_then(|(_, cache)| cache.get(remote).map(|r| r.device_name.clone()))
        .unwrap_or_default();
    let now = now_ms();
    let dto = PeerTransferDto {
        batch_id: batch_id.to_string(),
        node_id: remote.to_string(),
        peer_name,
        direction: "send".to_string(),
        status: "running".to_string(),
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
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        inner.tasks.insert(
            0,
            SendTask {
                dto,
                sources: Vec::new(),
                encrypt: None,
            },
        );
    }
    tracing::info!(
        batch_id = %batch_id,
        remote = %remote,
        "pull serve task registered (double-sided accounting)"
    );
}

/// 纯函数：传输是否已完成（字节已满且总量已知；total==0 视为未知不可判）
fn transfer_complete(total: u64, transferred: u64) -> bool {
    total > 0 && transferred >= total
}

/// 纯函数：serve 记账任务终态/暂停恢复可同步状态（running/paused 才允许改）
fn serve_status_tracked(status: &str) -> bool {
    matches!(status, "running" | "paused")
}

/// 服务侧任务终态结算：状态映射与发送侧 apply_terminal 同构（无 epoch——
/// 服务侧批单次会话；取消码 -receiver 指拉取发起方取消，-self 指本端中止）
async fn settle_serve_terminal(app: &AppHandle, batch_id: &str, terminal: TerminalState) {
    let (status, detail, reject_reason) = match &terminal {
        TerminalState::Completed => ("completed".to_string(), None, None),
        TerminalState::Rejected { reason } => ("rejected".to_string(), None, Some(reason.as_str().to_string())),
        TerminalState::Cancelled { by_peer } => (
            "cancelled".to_string(),
            // 机器可读原因码（前端 i18n 映射），禁止把人类文案直接落 wire
            Some(if *by_peer {
                "cancelled-by-receiver".to_string()
            } else {
                "cancelled-by-self".to_string()
            }),
            None,
        ),
        TerminalState::Failed { detail } => ("failed".to_string(), Some(detail.clone()), None),
    };
    {
        let state = app.state::<PeerTransferState>();
        let mut inner = state.inner.lock().expect("peer transfer lock poisoned");
        if let Some(task) = inner
            .tasks
            .iter_mut()
            .find(|t| t.dto.batch_id == batch_id && serve_status_tracked(&t.dto.status))
        {
            task.dto.status = status;
            // 完成结算：最后一条 Progress 可能略低于总量，归整为满额
            if task.dto.status == "completed" {
                task.dto.transferred_bytes = task.dto.total_bytes;
            }
            task.dto.detail = detail;
            task.dto.reject_reason = reject_reason;
            task.dto.updated_at_ms = now_ms();
        }
        evict_history_cap_locked(&mut inner.tasks);
    }
    tracing::info!(batch_id = %batch_id, state = ?terminal, "pull serve session ended");
    publish(app);
}

// ==================== 发布 ====================

/// 全量列表快照（锁内克隆，锁外使用）
fn snapshot(app: &AppHandle) -> Vec<PeerTransferDto> {
    let state = app.state::<PeerTransferState>();
    let inner = state.inner.lock().expect("peer transfer lock poisoned");
    inner.tasks.iter().map(|t| t.dto.clone()).collect()
}

/// 全量列表推送（锁外发布；仅总线单路，票 06——桌面 Tauri 前端事件桥
/// 随宿主命令面退役，产品状态由插件经 `peer:transfer` 快照驱动）
fn publish(app: &AppHandle) {
    let payload = serde_json::to_value(snapshot(app)).unwrap_or_default();
    super::peer_net::publish_bus_only("peer-transfer-changed", payload);
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

/// 选择待发送文件（桌面端多选；用户取消返回空数组）
///
/// 宿主自有命令不经插件门控——发送表单是宿主内置 UI 而非插件面板。
pub async fn peer_pick_files(app_handle: AppHandle) -> crate::Result<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_files(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("peer_pick_files: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "peer_pick_files: dialog channel closed: {e}"
        ))),
    }
}

/// 选择待发送文件夹（桌面端单次单个，可多次累加；用户取消返回空数组）
pub async fn peer_pick_folder(app_handle: AppHandle) -> crate::Result<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folder(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("peer_pick_folder: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(path)) => Ok(vec![path_to_string(path)?]),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "peer_pick_folder: dialog channel closed: {e}"
        ))),
    }
}

/// 系统多目录选择器（一次可选多个；用户取消返回空数组）
pub async fn peer_pick_folders(app_handle: AppHandle) -> crate::Result<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folders(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("peer_pick_folders: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => paths.into_iter().map(path_to_string).collect(),
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "peer_pick_folders: dialog channel closed: {e}"
        ))),
    }
}

/// Dialog FilePath → UTF-8 绝对路径串（非 UTF-8 路径显式报错而非静默丢弃）
fn path_to_string(file_path: tauri_plugin_dialog::FilePath) -> crate::Result<String> {
    let path = file_path
        .into_path()
        .map_err(|e| crate::AppError::InvalidInput(format!("peer pick: failed to convert selected path: {e}")))?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| crate::AppError::InvalidInput("peer pick: selected path is not valid UTF-8".to_string()))
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
            files: vec![PeerTransferFileDto {
                path: "a.txt".to_string(),
                size: 3,
            }],
            total_bytes: 3,
            transferred_bytes: 3,
            rate_bps: 0.0,
            detail: None,
            reject_reason: None,
            created_at_ms,
            updated_at_ms: created_at_ms,
        }
    }

    /// 回归护栏（真机：本端显示已暂停、对端仍在传输）：serve 记账任务
    /// （sources 空）必须路由到 serve 门控，本端发送会话才走发送暂停句柄。
    /// 两分支写反时本用例转红。
    #[test]
    fn pause_route_splits_serve_accounting_from_send_sessions() {
        assert_eq!(pause_route(true), PauseRoute::Serve);
        assert_eq!(pause_route(false), PauseRoute::SendSession);
    }

    #[test]
    fn dto_wire_format_is_camel_case() {
        // IPC/总线快照共用同一序列化形状，插件消费 camelCase 字段
        let json = serde_json::to_string(&dto("rejected", 1)).expect("serialize");
        assert!(json.contains("\"batchId\":\"b-1\""));
        assert!(json.contains("\"peerName\":\"Peer\""));
        assert!(json.contains("\"totalBytes\":3"));
        assert!(json.contains("\"createdAtMs\":1"));
        // None 字段不进 wire（快照紧凑）
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
    fn history_cap_evicts_oldest_terminal_and_keeps_running() {
        // 生产列表为最新在前：按时间戳降序构造，尾部即最旧
        let mut tasks: Vec<SendTask> = Vec::new();
        for i in (0..(HISTORY_CAP + 5)).rev() {
            tasks.push(SendTask {
                dto: dto("completed", i as u64),
                sources: Vec::new(),
                encrypt: None,
            });
        }
        tasks.push(SendTask {
            dto: dto("running", 9_999),
            sources: Vec::new(),
            encrypt: None,
        });

        evict_history_cap_locked(&mut tasks);

        let terminals = tasks.iter().filter(|t| t.dto.status != "running").count();
        assert_eq!(terminals, HISTORY_CAP);
        // 最旧（列表尾部低时间戳）被淘汰；最新终态与活跃任务保留
        assert!(!tasks.iter().any(|t| t.dto.batch_id == "b-0"));
        assert!(tasks.iter().any(|t| t.dto.batch_id == format!("b-{}", HISTORY_CAP + 4)));
        assert!(tasks.iter().any(|t| t.dto.status == "running"));
    }

    // ==================== 并发闸门（限制同时传输数量） ====================

    fn send_task(status: &str, id: u64, sources: bool) -> SendTask {
        let mut t = SendTask {
            dto: dto(status, id),
            sources: Vec::new(),
            encrypt: None,
        };
        if sources {
            t.sources.push(OutgoingFile::new(format!("C:/f-{id}.bin")));
        }
        t
    }

    #[test]
    fn pump_fills_empty_slots_with_oldest_pending_first() {
        // 最新在前：p5(新) … p1(旧)，倒序即最旧优先启动
        let mut tasks = vec![
            send_task("pending", 5, true),
            send_task("pending", 4, true),
            send_task("running", 3, true),
            send_task("pending", 2, true),
            send_task("pending", 1, true),
        ];
        // running=1，limit=3 → 再启动 2 个最旧 pending（b-1、b-2）
        let picked = pick_pending_to_start(&mut tasks, 3);
        assert_eq!(picked, vec!["b-1", "b-2"]);
        assert_eq!(tasks.iter().filter(|t| t.dto.status == "running").count(), 3);
        assert_eq!(tasks.iter().filter(|t| t.dto.status == "pending").count(), 2);
    }

    #[test]
    fn pump_skips_serve_tasks_and_paused_when_filling_slots() {
        let mut tasks = vec![
            send_task("pending", 4, true),
            // 服务侧记账任务：sources 空，不可由闸门重启动
            send_task("pending", 3, false),
            // 用户暂停：不自动启动
            send_task("paused", 2, true),
            send_task("running", 1, true),
        ];
        let picked = pick_pending_to_start(&mut tasks, 2);
        // 只启动 1 个（p4）；p3/serve 与 p2/paused 保持原状
        assert_eq!(picked, vec!["b-4"]);
        assert!(tasks
            .iter()
            .any(|t| t.dto.batch_id == "b-3" && t.dto.status == "pending"));
        assert!(tasks
            .iter()
            .any(|t| t.dto.batch_id == "b-2" && t.dto.status == "paused"));
    }

    #[test]
    fn pump_respects_zero_remaining_budget() {
        let mut tasks = vec![
            send_task("running", 3, true),
            send_task("running", 2, true),
            send_task("pending", 1, true),
        ];
        // running == limit：不启动任何 pending
        assert!(pick_pending_to_start(&mut tasks, 2).is_empty());
        assert!(tasks
            .iter()
            .all(|t| t.dto.status != "pending" || t.dto.batch_id == "b-1"));
        assert_eq!(tasks.iter().filter(|t| t.dto.status == "pending").count(), 1);
    }

    #[test]
    fn pump_starts_all_when_slots_available() {
        let mut tasks = vec![
            send_task("pending", 3, true),
            send_task("pending", 2, true),
            send_task("pending", 1, true),
        ];
        let picked = pick_pending_to_start(&mut tasks, 8);
        assert_eq!(picked.len(), 3);
        assert!(tasks.iter().all(|t| t.dto.status == "running"));
    }

    #[test]
    fn terminal_and_paused_never_evicted_by_history_cap() {
        // is_terminal_status 只认真终态：pending/paused 不参与封顶淘汰
        assert!(is_terminal_status("completed"));
        assert!(is_terminal_status("failed"));
        assert!(!is_terminal_status("pending"));
        assert!(!is_terminal_status("paused"));
        assert!(!is_terminal_status("running"));
    }

    // ==================== 暂停/恢复状态迁移（Bug：暂停成功仍显暂停 / 取消不同步） ====================

    /// 发送侧已完成判定：总量已知且字节已满（total==0 不可判）——暂停前校验依赖
    #[test]
    fn transfer_complete_requires_known_total_and_full_bytes() {
        assert!(transfer_complete(100, 100));
        assert!(transfer_complete(100, 120), "overshoot defensive");
        assert!(!transfer_complete(100, 99));
        assert!(!transfer_complete(0, 0), "unknown total");
        assert!(!transfer_complete(0, 42), "unknown total with bytes");
    }

    /// serve 记账任务可同步状态：running/paused 才允许终态结算或暂停/恢复改写
    /// （取消不同步根因：paused 的 serve 任务不被 settle_serve_terminal 命中）
    #[test]
    fn serve_status_tracked_covers_run_and_paused_only() {
        assert!(serve_status_tracked("running"));
        assert!(serve_status_tracked("paused"));
        assert!(!serve_status_tracked("pending"));
        assert!(!serve_status_tracked("completed"));
        assert!(!serve_status_tracked("cancelled"));
        assert!(!serve_status_tracked("failed"));
    }
}
