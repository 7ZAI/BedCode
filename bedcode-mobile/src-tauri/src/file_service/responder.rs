//! Intent 传输响应器（v2.1 服务器归零：桌面经 WS 指挥手机执行传输）
//!
//! 桌面发起方向切 intent 驱动后，本模块是移动端执行中枢：
//! - `dispatch_intent`：按 direction 分发（pull → UploadClient 上传给桌面；
//!   push → DownloadClient 下载落本地）；并发默认 3（可配置 1–8），每 intent
//!   独立句柄不共享文件锁
//! - 审批门（§4.1 定案：只有上传需落盘方审批）：push+ask 场景手机是落盘方，
//!   须经用户确认（前台对话框 / 后台通知 action）才回 `IntentAck{accepted}` 并
//!   执行 GET；pull 免审批，仅信息性通知
//! - ACK / 进度 / 心跳 / fail 偏移上报经 WS 已认证控制面；桌面 cancel → 中止
//! - 断点真源语义：push 断点真源在手机（本机已写字节，IntentAck.offset）；
//!   pull/自主上传断点真源在桌面（session received-offset）——两端不可混
//!
//! 测试面：状态机纯函数 + 分发决策 + 并发钳制在本模块单测；执行路径（HTTP/WS）
//! 由 client 模块 mock 测试与真机联调覆盖（分离策略：不 mock 内部函数）。

use crate::enums::file_service::FileServicePayload;
use crate::enums::sync::FileTransferIntent;
use crate::file_service::client::{
    client_cursor_store, download_with_retry, endpoint, urlencode_path, CreateUploadRequest, DownloadClient,
    DownloadRequest, TransferHandle, UploadClient, UploadError,
};
use crate::model::message::Message;
use crate::system::error_boundary::spawn_with_error_boundary;
use bedcode_plugin_api_mobile::FileOperation;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

/// 进度推送间隔
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// 心跳间隔：执行中每 10s 无 progress 发 `transfer_heartbeat`（桌面 30s 无回传判失联）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// 默认并发（沿用 01 选型）
const DEFAULT_CONCURRENCY: usize = 3;

// ==================== 状态机（纯函数，可单测） ====================

/// Intent 生命周期状态（内存态，不持久化；§3.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentState {
    /// 已收到（push+ask 时在此停留等待用户确认）
    Received,
    /// 用户已确认（push 门放行）
    Approved,
    /// 执行中（HTTP 数据流）
    Executing,
    /// 完成
    Completed,
    /// 拒绝（decision="rejected"）
    Rejected,
    /// 失败，携带已写偏移（断点真源 = 接收端已写字节）
    Failed { offset: u64 },
    /// 已取消（桌面 cancel / 用户取消）
    Cancelled,
}

/// 状态迁移合法性（§3.3：Received→[Approved 门]→Executing→Completed / 终态不可逆）
pub fn validate_transition(from: IntentState, to: IntentState) -> bool {
    use IntentState::*;
    match (from, to) {
        // Received 可直接执行（accept 策略 / 自主 pull）
        (Received, Executing) | (Received, Rejected) | (Received, Cancelled) => true,
        // push+ask 门：Received → Approved → Executing / Rejected
        (Received, Approved) => true,
        (Approved, Executing) | (Approved, Rejected) | (Approved, Cancelled) => true,
        // 执行中终态
        (Executing, Completed) | (Executing, Failed { .. }) | (Executing, Cancelled) => true,
        _ => false,
    }
}

/// 分发决策（§3.2）：direction 决定数据流引擎
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentAction {
    /// pull：手机把本地/SAF 文件 POST 给桌面（upload engine）
    Upload,
    /// push：手机 GET 桌面文件 + Range 落本地（download engine）
    Download,
}

/// 由 direction 决定执行引擎（未知 direction → None = 拒绝）
pub fn decide_action(direction: &str) -> Option<IntentAction> {
    match direction {
        "pull" => Some(IntentAction::Upload),
        "push" => Some(IntentAction::Download),
        _ => None,
    }
}

/// 并发钳制（1–8，越界回落）——纯函数
pub fn clamp_concurrency(n: usize) -> usize {
    n.clamp(
        crate::file_service::client::MIN_CONCURRENCY,
        crate::file_service::client::MAX_CONCURRENCY,
    )
}

// ==================== 记录 ====================

/// Intent 记录（响应器注册表 + 执行上下文）
pub struct IntentRecord {
    /// 意图 ID（ACK/进度/取消全程携带）
    pub intent_id: String,
    /// "pull" | "push"
    pub direction: String,
    /// "download" | "upload"（业务语义，通知/任务记录用）
    pub semantics: String,
    /// 批 ID（pull 且 ask 时桌面已自批准随 intent 下发）
    pub batch_id: Option<String>,
    /// 相对路径（pull：手机本地/SAF；push：桌面挂载内）
    pub relative_path: String,
    /// 字节大小（通知展示 + 断点预期）
    pub size: u64,
    /// 对端设备名（通知展示）
    pub device_name: String,
    /// 期望回执
    pub expect_response: bool,
    /// 当前状态
    pub state: IntentState,
    /// 已写字节（push 断点真源；pull 反映已发送量）
    pub offset: Arc<AtomicU64>,
    /// 执行句柄（取消令牌；未执行时为 None）
    pub handle: Option<TransferHandle>,
    /// 创建时刻（诊断用）
    pub created_at: Instant,
}

impl IntentRecord {
    /// 状态迁移（非法迁移拒绝）
    pub fn set_state(&mut self, to: IntentState) -> Result<(), String> {
        if !validate_transition(self.state, to) {
            return Err(format!("invalid intent transition {:?} -> {:?}", self.state, to));
        }
        self.state = to;
        Ok(())
    }
}

// ==================== 响应器 ====================

/// Intent 响应器（全局单例，见 `file_service::get_responder`）
pub struct IntentResponder {
    /// intent_id → 记录
    intents: Mutex<HashMap<String, IntentRecord>>,
    /// push 审批门：intent_id → 应答通道（approve/reject 命令填充）
    approvals: Mutex<HashMap<String, oneshot::Sender<Result<(), String>>>>,
    /// 并发信号量（acquire_owned 持有到任务结束）
    semaphore: RwLock<Arc<Semaphore>>,
}

impl IntentResponder {
    /// 创建响应器（并发默认 3）
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            intents: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            semaphore: RwLock::new(Arc::new(Semaphore::new(DEFAULT_CONCURRENCY))),
        })
    }

    /// 设置并发（1–8 钳制；动态重建信号量）
    pub async fn set_concurrency(&self, n: usize) {
        let n = clamp_concurrency(n);
        let new = Arc::new(Semaphore::new(n));
        *self.semaphore.write().await = new;
        tracing::info!(concurrency = n, "intent responder concurrency updated");
    }

    /// 当前活跃记录数（诊断/测试用）
    pub fn active_count(&self) -> usize {
        self.intents.lock().map(|m| m.len()).unwrap_or(0)
    }

    async fn set_state(&self, id: &str, to: IntentState) -> Result<(), String> {
        {
            let mut intents = self.intents.lock().unwrap_or_else(|e| e.into_inner());
            let record = intents.get_mut(id).ok_or_else(|| format!("intent not found: {}", id))?;
            record.set_state(to)?;
        }
        let payload = serde_json::json!(
            {
                "intentId": id,
                "state": state_name(to),
            }
        );
        let fs = crate::state::get_file_service();
        fs.registry
            .emit_filesrv_event("filesrv:intent_state_changed", payload)
            .await;
        Ok(())
    }

    // ==================== 入口 ====================

    /// 收到桌面 intent（SyncHandler 路由）
    ///
    /// - pull：免审批（发起人即落盘者本人），信息性通知 + ACK + 立即执行
    /// - push：Approved 门（ask 策略）——用户确认（approve_intent）后才回 ACK 并
    ///   执行；拒绝（reject_intent）→ IntentAck{rejected}，不执行（防未授权流入）
    pub async fn dispatch_intent(&self, intent: FileTransferIntent) {
        let id = intent.intent_id.clone();

        // 去重：相同 intent_id 已存在（WS 重发）→ 忽略
        if self.intents.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id) {
            tracing::debug!(intent_id = %id, "intent already registered, skip");
            return;
        }

        let record = IntentRecord {
            intent_id: id.clone(),
            direction: intent.direction.clone(),
            semantics: intent.semantics.clone(),
            batch_id: intent.batch_id.clone(),
            relative_path: intent.relative_path.clone(),
            size: intent.size,
            device_name: intent.device_name.clone(),
            expect_response: intent.expect_response,
            state: IntentState::Received,
            offset: Arc::new(AtomicU64::new(0)),
            handle: None,
            created_at: Instant::now(),
        };
        self.intents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), record);
        tracing::info!(
            intent_id = %id,
            direction = %intent.direction,
            semantics = %intent.semantics,
            path = %intent.relative_path,
            size = intent.size,
            "intent received"
        );

        // 事件载荷快照（intent 所有权将在 push 分支移入执行任务）；门/执行
        // 注册完成后才广播，插件接收策略自动应答依赖 Approved 门已同步存续
        let intent_event_payload = serde_json::json!({
            "intentId": id.clone(),
            "direction": intent.direction.clone(),
            "semantics": intent.semantics.clone(),
            "relativePath": intent.relative_path.clone(),
            "size": intent.size,
            "deviceName": intent.device_name.clone(),
            "expectResponse": intent.expect_response,
        });
        let event_emitter = crate::state::get_file_service().registry.clone();

        match decide_action(&intent.direction) {
            Some(IntentAction::Upload) => {
                // pull：免审批，信息性通知（无 action 按钮，不作为审批门）
                crate::file_service::notify::show_pull_notice(&intent).await;
                if intent.expect_response {
                    send_wire(FileServicePayload::IntentAck {
                        intent_id: id.clone(),
                        decision: "accepted".to_string(),
                        // 断点续传真源 = 手机本地已写字节（上次失败续传点）
                        offset: resume_offset(&id),
                        // session_id 空：pull 的 upload session 在 ACK 之后才由
                        // UploadClient POST 创建，时序上无法预知；续传由 offset +
                        // 桌面 session 重查完成（协议 §3.1 的 sid 字段对 pull 不适用）
                        session_id: String::new(),
                    })
                    .await;
                }
                self.spawn_execution(id.clone(), IntentAction::Upload, intent).await;
            }
            Some(IntentAction::Download) => {
                // push：Approved 门（等待用户确认；后台/锁屏经通知 action 应答）
                crate::file_service::notify::show_intent_ask_notification(&intent).await;
                let (tx, rx) = oneshot::channel();
                self.approvals
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id.clone(), tx);
                let responder = get_responder();
                let intent_id = id.clone();
                spawn_with_error_boundary("intent_push_gate", async move {
                    match rx.await {
                        Ok(Ok(())) => {
                            // 用户确认：ACK(accepted) + 执行（无确认不执行）
                            responder.set_state(&intent_id, IntentState::Approved).await.ok();
                            if intent.expect_response {
                                send_wire(FileServicePayload::IntentAck {
                                    intent_id: intent_id.clone(),
                                    decision: "accepted".to_string(),
                                    // push 断点真源在手机：续传点 = 本地游标
                                    offset: resume_offset(&intent_id),
                                    session_id: String::new(),
                                })
                                .await;
                            }
                            responder
                                .spawn_execution(intent_id.clone(), IntentAction::Download, intent)
                                .await;
                        }
                        Ok(Err(_reason)) => {
                            responder.set_state(&intent_id, IntentState::Rejected).await.ok();
                            if intent.expect_response {
                                send_wire(FileServicePayload::IntentAck {
                                    intent_id: intent_id.clone(),
                                    decision: "rejected".to_string(),
                                    offset: 0,
                                    session_id: String::new(),
                                })
                                .await;
                            }
                            responder.remove_record(&intent_id);
                            tracing::info!(intent_id = %intent_id, "intent rejected by user");
                        }
                        Err(_) => {
                            responder.remove_record(&intent_id);
                        }
                    }
                });
            }
            None => {
                tracing::warn!(intent_id = %id, direction = %intent.direction, "intent rejected: unknown direction");
                self.set_state(&id, IntentState::Rejected).await.ok();
                self.remove_record(&id);
            }
        }

        // 本地事件：插件订阅 `filesrv:intent_received`（前端 pending 卡 / 通知 /
        // 接收策略自动应答）。放在 decide_action 之后：push 的 Approved 门已在此前
        // 经 approvals.insert 同步注册，插件按 receiving_policy 自动响应时才不会
        // 因门缺失而 NotFound 丢失
        event_emitter
            .emit_filesrv_event("filesrv:intent_received", intent_event_payload)
            .await;
    }

    /// 桌面 cancel（FileTransferCancel）→ 中止对应 HTTP 会话
    pub fn cancel_intent(&self, intent_id: &str) -> bool {
        let confirmed;
        {
            let mut intents = self.intents.lock().unwrap_or_else(|e| e.into_inner());
            confirmed = match intents.get_mut(intent_id) {
                Some(record) => {
                    // 执行句柄存在 → 取消令牌中止 HTTP 流（终态由执行任务收口）；
                    // 审批门等待中（handle None）→ 直接 Cancelled + 清理
                    match record.handle.as_ref() {
                        Some(h) => {
                            h.cancel();
                            true
                        }
                        None => {
                            intents.remove(intent_id);
                            true
                        }
                    }
                }
                None => false,
            };
        }
        if confirmed {
            tracing::info!(intent_id = %intent_id, "intent cancel requested");
        }
        confirmed
    }

    /// 用户批准 push intent（前台对话框 / 后台通知 action 调用）
    ///
    /// 应答成功后清理审批通知——accept 接收策略的插件自动应答也走这里，
    /// 不留残响的「接受/拒绝」通知
    pub fn approve_intent(&self, intent_id: &str) -> Result<(), crate::AppError> {
        let tx = self
            .approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(intent_id)
            .ok_or_else(|| crate::AppError::NotFound(format!("intent approval not found: {}", intent_id)))?;
        let _ = tx.send(Ok(()));
        let intent_id_owned = intent_id.to_string();
        crate::system::error_boundary::spawn_with_error_boundary("intent_approve_cancel_notification", async move {
            crate::file_service::notify::cancel_intent_notification(&intent_id_owned).await;
        });
        Ok(())
    }

    /// 用户拒绝 push intent（decision="rejected"）
    pub fn reject_intent(&self, intent_id: &str) -> Result<(), crate::AppError> {
        let tx = self
            .approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(intent_id)
            .ok_or_else(|| crate::AppError::NotFound(format!("intent approval not found: {}", intent_id)))?;
        let _ = tx.send(Err("user-rejected".to_string()));
        let intent_id_owned = intent_id.to_string();
        crate::system::error_boundary::spawn_with_error_boundary("intent_reject_cancel_notification", async move {
            crate::file_service::notify::cancel_intent_notification(&intent_id_owned).await;
        });
        Ok(())
    }

    fn remove_record(&self, intent_id: &str) {
        self.intents.lock().unwrap_or_else(|e| e.into_inner()).remove(intent_id);
        self.approvals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(intent_id);
    }

    // ==================== 执行 ====================

    /// 启动执行任务（并发信号量护栏；状态 Received/Approved → Executing）
    async fn spawn_execution(&self, intent_id: String, action: IntentAction, intent: FileTransferIntent) {
        let semaphore = { self.semaphore.read().await.clone() };
        let permit = match semaphore.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                tracing::error!(intent_id = %intent_id, "intent semaphore closed");
                return;
            }
        };
        if let Err(e) = self.set_state(&intent_id, IntentState::Executing).await {
            tracing::warn!(intent_id = %intent_id, error = %e, "intent execution start rejected");
            self.set_state(&intent_id, IntentState::Rejected).await.ok();
            self.remove_record(&intent_id);
            return;
        }

        // 执行句柄交由任务持有；取消令牌存记录（桌面 cancel 可中止）
        let handle = TransferHandle::new(intent_id.clone());
        if let Some(record) = self
            .intents
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&intent_id)
        {
            record.handle = Some(handle.clone());
        }

        let responder = get_responder();
        spawn_with_error_boundary("intent_execution", async move {
            let _permit = permit;
            let handled = run_action(&handle, &intent, action).await;
            responder.finalize(&intent_id, handled).await;
        });
    }

    /// 终态收口（状态迁移 + WS 终态 + 记录/游标清理）
    async fn finalize(&self, intent_id: &str, handled: HandledTransfer) {
        let outcome = handled.outcome;
        let transferred = handled.transferred;
        let total = handled.total;
        let state = match &outcome {
            Outcome::Completed => IntentState::Completed,
            Outcome::Cancelled => IntentState::Cancelled,
            Outcome::Failed(_) => IntentState::Failed { offset: transferred },
        };
        if let Err(e) = self.set_state(intent_id, state).await {
            tracing::warn!(intent_id = %intent_id, error = %e, "intent finalize state rejected");
        }

        match &outcome {
            Outcome::Completed => {
                send_wire(FileServicePayload::TransferProgress {
                    intent_id: Some(intent_id.to_string()),
                    task_id: intent_id.to_string(),
                    transferred,
                    total,
                    bytes_per_sec: 0,
                    state: "completed".to_string(),
                })
                .await;
                tracing::info!(intent_id = %intent_id, bytes = transferred, "intent execution completed");
            }
            Outcome::Cancelled => {
                send_wire(FileServicePayload::TransferProgress {
                    intent_id: Some(intent_id.to_string()),
                    task_id: intent_id.to_string(),
                    transferred,
                    total,
                    bytes_per_sec: 0,
                    state: "cancelled".to_string(),
                })
                .await;
                tracing::info!(intent_id = %intent_id, bytes = transferred, "intent execution cancelled");
            }
            Outcome::Failed(reason) => {
                // fail 偏移上报（接收端即断点真源，桌面重发 intent 从 offset 续传）；
                // reason 携带标准分类（如 duplicate-name）供桌面置任务终态 reason
                send_wire(FileServicePayload::IntentFail {
                    intent_id: intent_id.to_string(),
                    offset: transferred,
                    reason: Some(reason.clone()),
                })
                .await;
                send_wire(FileServicePayload::TransferProgress {
                    intent_id: Some(intent_id.to_string()),
                    task_id: intent_id.to_string(),
                    transferred,
                    total,
                    bytes_per_sec: 0,
                    state: "failed".to_string(),
                })
                .await;
                tracing::error!(intent_id = %intent_id, reason = %reason, "intent execution failed");
            }
        }

        self.remove_record(intent_id);
        // 游标：Completed 由 download 内部清理（rename 后 .part 已移走）；
        // Failed/Cancelled 保留游标供桌面重发 intent 从 offset 续传（断点真源）
        if matches!(outcome, Outcome::Completed) {
            client_cursor_store().remove(intent_id);
        }
    }
}

/// 本地已写字节（断点续传真源；无记录时为 0）
fn resume_offset(intent_id: &str) -> u64 {
    client_cursor_store().get(intent_id).map(|c| c.position).unwrap_or(0)
}

/// 执行终局
enum Outcome {
    Completed,
    Cancelled,
    /// 失败原因（供日志；偏移经 transferred 读）
    Failed(String),
}

/// 执行结果（偏移 + 结局）
struct HandledTransfer {
    transferred: u64,
    total: u64,
    outcome: Outcome,
}

/// 分派执行动作（handle 由 spawn_execution 创建并注入，保证取消可达）
///
/// 命名视角 = 手机侧动作：intent{push}（桌面推文件给手机）→ 手机 Download
/// （GET 桌面文件落盘）；intent{pull}（桌面拉手机文件）→ 手机 Upload（POST
/// 桌面上传引擎）。函数名与手机动作一致，与业务方向互补而非反转
async fn run_action(handle: &TransferHandle, intent: &FileTransferIntent, action: IntentAction) -> HandledTransfer {
    match action {
        IntentAction::Download => run_download(handle, intent).await,
        IntentAction::Upload => run_upload(handle, intent).await,
    }
}

// ==================== 执行引擎（HTTP 数据流） ====================

/// 解析桌面 HTTP 端点（base + JWT）：共享实现见
/// [`crate::file_service::client::desktop_http_endpoint`]

/// 从桌面 peer 记录解析端点挂载（plugin_id + mount_name）
///
/// 常见场景 = 单 file-transfer 挂载；多挂载时优先命中支持对应操作的挂载
async fn resolve_desktop_mount(op: FileOperation) -> Result<(String, String), String> {
    let Some(peer_id) = crate::handler::sync::desktop_peer_id().await else {
        return Err("desktop peer id unavailable".to_string());
    };
    let fs = crate::state::get_file_service();
    let Some(peer) = fs.registry.get_peer(&peer_id).await else {
        return Err(format!("desktop peer not available: {}", peer_id));
    };
    let mount = peer
        .mounts
        .iter()
        .find(|m| m.operations.contains(&op))
        .or_else(|| peer.mounts.first())
        .ok_or_else(|| "desktop peer has no file mounts".to_string())?;
    Ok((mount.plugin_id.clone(), mount.mount_path.clone()))
}

/// push：下载桌面文件落本地（Range 续传 + HEAD 指纹比对）
async fn run_download(handle: &TransferHandle, intent: &FileTransferIntent) -> HandledTransfer {
    let fail = |e: &str| HandledTransfer {
        transferred: handle_progress(handle).0,
        total: intent.size,
        outcome: Outcome::Failed(e.to_string()),
    };
    let (base, auth) = match crate::file_service::client::desktop_http_endpoint().await {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let (plugin_id, mount) = match resolve_desktop_mount(FileOperation::Download).await {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    // 落点：app 下载目录按文件名（桌面挂载内相对路径取最后一段）
    let fs = crate::state::get_file_service();
    let Some(dir) = fs.registry.downloads_dir().await else {
        return fail("downloads dir unavailable");
    };
    let fname = std::path::Path::new(&intent.relative_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    let dest = dir.join(&fname);
    let url = endpoint(
        &base,
        &plugin_id,
        &mount,
        &format!("file?path={}", urlencode_path(&intent.relative_path)),
    );
    let part_path = std::path::PathBuf::from(format!("{}.part", dest.display()));
    let req = DownloadRequest {
        url,
        auth,
        dest_path: part_path,
        final_path: dest,
        total: intent.size,
        media: None,
    };
    let progress_token = spawn_progress_loop(handle, intent);

    let store = client_cursor_store();
    let result = download_with_retry(&DownloadClient::new(), &req, store, handle, 3).await;
    progress_token.cancel();

    let transferred = handle_progress(handle).0;
    let outcome = match result {
        Ok(_) => Outcome::Completed,
        Err(_) if handle.is_cancelled() => Outcome::Cancelled,
        Err(e) => Outcome::Failed(e.to_string()),
    };
    HandledTransfer {
        transferred,
        total: intent.size,
        outcome,
    }
}

/// pull：本地/SAF 文件 POST 给桌面（session 编排；batch_id 随 intent 下发）
async fn run_upload(handle: &TransferHandle, intent: &FileTransferIntent) -> HandledTransfer {
    let fail = |e: &str| HandledTransfer {
        transferred: handle_progress(handle).0,
        total: intent.size,
        outcome: Outcome::Failed(e.to_string()),
    };
    let (base, auth) = match crate::file_service::client::desktop_http_endpoint().await {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    let (plugin_id, mount) = match resolve_desktop_mount(FileOperation::Upload).await {
        Ok(v) => v,
        Err(e) => return fail(&e),
    };
    // 上传源 = intent.relative_path（手机本地/SAF 路径）；桌面目标取文件名
    let source = intent.relative_path.clone();
    let fname = std::path::Path::new(&source)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "upload".to_string());
    let create = CreateUploadRequest {
        relative_path: fname,
        size: intent.size,
        batch_id: intent.batch_id.clone(),
    };
    let fs = crate::state::get_file_service();
    let saf = fs.registry.saf_io().await;
    let progress_token = spawn_progress_loop(handle, intent);

    let info = UploadClient::new()
        .upload_file(&base, &plugin_id, &mount, &create, &auth, &source, saf, handle)
        .await;
    progress_token.cancel();

    let transferred = handle_progress(handle).0.min(intent.size);
    let outcome = match &info {
        Ok(_) => Outcome::Completed,
        Err(_) if handle.is_cancelled() => Outcome::Cancelled,
        // complete 409 duplicate-name（沿用 v1 语义）：标准分类供桌面置终态 reason
        Err(UploadError::DuplicateName(_)) => Outcome::Failed("duplicate-name".to_string()),
        Err(e) => Outcome::Failed(e.to_string()),
    };
    HandledTransfer {
        transferred,
        total: intent.size,
        outcome,
    }
}

fn handle_progress(handle: &TransferHandle) -> (u64, u64) {
    handle.progress()
}

// ==================== 进度/心跳回推 ====================
//
// 每 500ms 一次 tick：有推进 → `transfer_progress`；静默 ≥10s → `transfer_heartbeat`。
// 循环句柄为 handle 的 child token（父取消即停；任务正常结束时由调用方 cancel）。

fn spawn_progress_loop(handle: &TransferHandle, intent: &FileTransferIntent) -> CancellationToken {
    let token = handle.token().child_token();
    let token_for_task = token.clone();
    let handle_cl = handle.clone();
    let intent_id = intent.intent_id.clone();
    let total = intent.size;

    spawn_with_error_boundary("intent_progress_loop", async move {
        let mut interval = tokio::time::interval(PROGRESS_INTERVAL);
        interval.tick().await;
        let mut last_progress = Instant::now();
        let mut last_bytes = 0u64;
        loop {
            tokio::select! {
                _ = token_for_task.cancelled() => break,
                _ = interval.tick() => {}
            }
            let (transferred, _t) = handle_cl.progress();
            let silent = last_progress.elapsed() >= HEARTBEAT_INTERVAL;
            if transferred != last_bytes || silent {
                last_bytes = transferred;
                if transferred > 0 {
                    send_wire(FileServicePayload::TransferProgress {
                        intent_id: Some(intent_id.clone()),
                        task_id: intent_id.clone(),
                        transferred,
                        total,
                        bytes_per_sec: 0,
                        state: "running".to_string(),
                    })
                    .await;
                    last_progress = Instant::now();
                } else if silent {
                    // 无推进且静默 ≥10s：心跳保活（对端 30s 无回传判失联）
                    send_wire(FileServicePayload::TransferHeartbeat {
                        intent_id: intent_id.clone(),
                    })
                    .await;
                    last_progress = Instant::now();
                }
            }
        }
    });

    token
}

// ==================== WS 发送 ====================

/// 经 ConnectionManager 发送 FileService 消息（自动注入 JWT；断开静默跳过）
pub(crate) async fn send_wire(payload: FileServicePayload) {
    let conn = crate::state::get_connection_manager();
    if !conn.is_connected().await {
        tracing::debug!("file service wire message skipped: WS not connected");
        return;
    }
    let msg = Message::file_service(payload);
    if let Err(e) = conn.send(&msg).await {
        tracing::warn!("file service wire message send failed: {}", e);
    }
}

/// 状态名（事件/日志）
fn state_name(s: IntentState) -> &'static str {
    match s {
        IntentState::Received => "received",
        IntentState::Approved => "approved",
        IntentState::Executing => "executing",
        IntentState::Completed => "completed",
        IntentState::Rejected => "rejected",
        IntentState::Failed { .. } => "failed",
        IntentState::Cancelled => "cancelled",
    }
}

// ==================== 全局单例 ====================

static RESPONDER: std::sync::OnceLock<Arc<IntentResponder>> = std::sync::OnceLock::new();

/// 获取 intent 响应器全局单例（构造不启动任务）
pub fn get_responder() -> Arc<IntentResponder> {
    RESPONDER.get_or_init(IntentResponder::new).clone()
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_matrix_valid_and_invalid() {
        use IntentState::*;
        assert!(validate_transition(Received, Executing));
        assert!(validate_transition(Received, Approved));
        assert!(validate_transition(Received, Rejected));
        assert!(validate_transition(Approved, Executing));
        assert!(validate_transition(Executing, Completed));
        assert!(validate_transition(Executing, Failed { offset: 10 }));
        assert!(validate_transition(Executing, Cancelled));
        assert!(!validate_transition(Completed, Executing));
        assert!(!validate_transition(Executing, Received));
        assert!(!validate_transition(Failed { offset: 0 }, Executing));
        assert!(!validate_transition(Rejected, Approved));
        assert!(!validate_transition(Cancelled, Executing));
        // 门不可跳：Approved 直接 Completed 非法（须先 Executing）
        assert!(!validate_transition(Approved, Completed));
    }

    #[test]
    fn record_rejects_invalid_transition() {
        let mut record = IntentRecord {
            intent_id: "i1".to_string(),
            direction: "push".to_string(),
            semantics: "upload".to_string(),
            batch_id: None,
            relative_path: "a.mp4".to_string(),
            size: 10,
            device_name: "d".to_string(),
            expect_response: true,
            state: IntentState::Received,
            offset: Arc::new(AtomicU64::new(0)),
            handle: None,
            created_at: Instant::now(),
        };
        record.set_state(IntentState::Approved).unwrap();
        record.set_state(IntentState::Executing).unwrap();
        record.set_state(IntentState::Completed).unwrap();
        // 终态回退非法
        assert!(record.set_state(IntentState::Executing).is_err());
    }

    #[test]
    fn decide_action_by_direction() {
        assert_eq!(decide_action("pull"), Some(IntentAction::Upload));
        assert_eq!(decide_action("push"), Some(IntentAction::Download));
        assert_eq!(decide_action("unknown"), None);
    }

    #[test]
    fn clamp_concurrency_bounds() {
        assert_eq!(clamp_concurrency(0), 1);
        assert_eq!(clamp_concurrency(3), 3);
        assert_eq!(clamp_concurrency(99), 8);
        assert_eq!(clamp_concurrency(8), 8);
    }
}
