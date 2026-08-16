//! 命令处理
//!
//! 16 个命令的实现（plugin.json 声明），由 lib.rs invoke_command 路由。
//! 每个命令接收 PluginState 引用和参数 JSON，返回结果 JSON。
//!
//! 宿主调用（transfer_start 等）在释放状态锁后执行，
//! 避免 on_message 回调死锁。

use crate::handshake::{
    self, request_transfer, CreateSessionError, QuerySessionError, TransferRequestError,
    TransferRequestOutcome,
};
use crate::peer::{PeerStore, MOUNT_PATH};
use crate::queue::{Queue, DEFAULT_CONCURRENCY};
use crate::state::{
    Direction, Fingerprint, HistoryEntry, HistoryStore, PeerInfo, Task, TaskState, TaskStore,
};
use bedcode_plugin_api::host::{
    ConfigKey, HostBus, HostConfig, HostEvents, HostFileService, HostFs, HostHttp, HostLog,
    HostStorage, HostTransfer,
};
use bedcode_plugin_api::types::{
    FileOperation, MountOptions, TransferDirection, TransferProgress, TransferRequest,
    TransferState, UploadHookDecision, UploadRequestMeta,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 设置 storage key
const SETTINGS_KEY: &str = "file-transfer-settings";
/// v2 接收策略默认值：每次询问
const DEFAULT_RECEIVING_POLICY: &str = "ask";
/// v2 同意超时默认值（秒）
const DEFAULT_APPROVAL_TIMEOUT_SEC: u64 = 60;
/// v2 同意超时边界（秒，与宿主校验一致）
const MIN_APPROVAL_TIMEOUT_SEC: u64 = 10;
const MAX_APPROVAL_TIMEOUT_SEC: u64 = 600;

/// 插件设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 共享目录根列表（绝对路径）
    #[serde(default)]
    pub roots: Vec<String>,
    /// 下载目录（绝对路径，桌面端必须配置）
    #[serde(default)]
    pub download_dir: String,
    /// 并发数（1..=8）
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// v2 接收策略：ask(默认) | accept | reject
    #[serde(default = "default_receiving_policy")]
    pub receiving_policy: String,
    /// v2 同意超时秒（10–600，仅 ask 生效，默认 60）
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_sec: u64,
}

fn default_concurrency() -> usize {
    DEFAULT_CONCURRENCY
}

fn default_receiving_policy() -> String {
    DEFAULT_RECEIVING_POLICY.to_string()
}

fn default_approval_timeout() -> u64 {
    DEFAULT_APPROVAL_TIMEOUT_SEC
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            download_dir: String::new(),
            concurrency: DEFAULT_CONCURRENCY,
            receiving_policy: DEFAULT_RECEIVING_POLICY.to_string(),
            approval_timeout_sec: DEFAULT_APPROVAL_TIMEOUT_SEC,
        }
    }
}

/// v2 发送方批记录（内存，不持久化；批上下文只在当次会话有效）
#[derive(Debug, Clone)]
pub struct BatchRecord {
    /// 批 ID
    pub batch_id: String,
    /// 对端设备 ID（任务绑定对端，批请求发往同一对端）
    pub peer_id: String,
    /// 批状态
    pub state: BatchRecordState,
}

/// v2 发送方批记录状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchRecordState {
    /// 已发起 transfer-request，等待接收端应答
    Pending,
    /// 接收端已批准（批内任务可建 session）
    Approved,
    /// 接收端拒绝 / 超时 / 策略拒绝（reason 为 wire 值）
    Rejected { reason: String },
}

/// v2 接收端 pending 批（应答卡数据源，内存不持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingBatch {
    /// 批 ID
    pub batch_id: String,
    /// 发送方设备 ID
    pub peer_id: String,
    /// 发送方设备名（展示用）
    pub peer_name: String,
    /// 批内文件清单
    pub files: Vec<UploadRequestMeta>,
    /// 批总大小（字节）
    pub total_size: u64,
    /// 创建时间（Unix 毫秒）
    pub created_at: u64,
}

/// v2 接收中任务（「正在接收」tab；仅 session 级取消，无暂停/恢复）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivingTask {
    /// 上传 session ID（宿主侧标识）
    pub session_id: String,
    /// 所属批 ID（ask 模式有值；accept 模式为空）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// 远端文件相对路径
    pub remote_path: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 当前状态（Transferring | 终态，终态随即归档移除）
    pub state: TaskState,
    /// 失败/拒绝原因
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 发送方设备 ID
    pub peer_id: String,
    /// 创建时间（Unix 毫秒）
    pub created_at: u64,
    /// 更新时间（Unix 毫秒）
    pub updated_at: u64,
}

/// 插件全局状态（Mutex 保护，WASM 单线程）
pub struct PluginState {
    /// 任务存储
    pub tasks: TaskStore,
    /// 传输队列
    pub queue: Queue,
    /// 插件设置
    pub settings: Settings,
    /// 对端存储（多对端 + 激活）
    pub peer: PeerStore,
    /// 是否已挂载
    pub mounted: bool,
    /// v2 发送方批记录（batch_id → 记录，内存不持久化）
    pub batches: HashMap<String, BatchRecord>,
    /// v2 接收端 pending 批（batch_id → 批）
    pub pending_batches: HashMap<String, PendingBatch>,
    /// v2 接收中任务（session_id → 任务）
    pub receiving: HashMap<String, ReceivingTask>,
    /// v2 传输历史（终态归档，封顶 200）
    pub history: HistoryStore,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            tasks: TaskStore::new(),
            queue: Queue::new(DEFAULT_CONCURRENCY),
            settings: Settings::default(),
            peer: PeerStore::new(false),
            mounted: false,
            batches: HashMap::new(),
            pending_batches: HashMap::new(),
            receiving: HashMap::new(),
            history: HistoryStore::new(),
        }
    }
}

// ==================== 命令实现 ====================

/// list-tasks：返回任务快照数组
pub fn list_tasks(state: &PluginState) -> serde_json::Value {
    serde_json::to_value(state.tasks.snapshot()).unwrap_or(serde_json::Value::Array(vec![]))
}

/// query-peer：主动询问对端文件服务状态
///
/// 经宿主 WS 控制面广播 Query；对端回复 Announce/Withdraw 后宿主注册表
/// 更新并推送 `filesrv:peer_changed`，前端状态随之刷新。
/// 用于对端状态事件遗漏（先挂载后连接/广播丢失）时主动恢复。
pub fn query_peer(host: &(impl HostFileService + HostLog)) -> anyhow::Result<serde_json::Value> {
    host.log_info("query-peer: probing remote file service state");
    host.filesrv_query_peer("")
        .map_err(|e| {
            host.log_warn(&format!("query-peer FAILED: {}", e));
            anyhow::anyhow!("query-peer: {}", e)
        })?;
    host.log_info("query-peer: query broadcast sent");
    Ok(serde_json::json!({ "ok": true }))
}

/// list-remote：列举对端目录
pub fn list_remote(
    state: &PluginState,
    host: &(impl HostHttp + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (base, auth) = state.peer.base_and_auth()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    host.log_info(&format!(
        "list-remote: path='{}' base={} auth_len={}",
        path, base, auth.len()
    ));
    let result = handshake::list_remote(host, &base, &auth, path)
        .map_err(|e| {
            host.log_warn(&format!(
                "list-remote FAILED: {} (base={} auth_len={})",
                e, base, auth.len()
            ));
            anyhow::anyhow!("{}", e)
        })?;
    host.log_info(&format!(
        "list-remote OK: path='{}' entries={} notice={:?}",
        path,
        result.entries.len(),
        result.notice
    ));
    Ok(serde_json::json!({
        "entries": result.entries,
        "notice": result.notice,
    }))
}

/// enqueue：入队传输任务
///
/// v2：`batchId` 可选参数（上传任务的批上下文，一次「发送」动作一匹）；
/// 非空时任务归属该批，批内首个任务启动时发起 transfer-request
pub fn enqueue(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let direction = args
        .get("direction")
        .and_then(|v| v.as_str())
        .unwrap_or("download");
    let remote_path = args
        .get("remotePath")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing remotePath"))?;
    let peer_id = args
        .get("peerId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let peer_name = args
        .get("peerName")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let batch_id = args
        .get("batchId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    match direction {
        "download" => enqueue_download(state, host, remote_path, peer_id, peer_name, args),
        "upload" => enqueue_upload(state, host, remote_path, peer_id, peer_name, batch_id, args),
        _ => Err(anyhow::anyhow!("invalid direction: {}", direction)),
    }
}

/// 下载入队
fn enqueue_download(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
    remote_path: &str,
    peer_id: &str,
    peer_name: &str,
    _args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    // 确定下载目录
    let download_dir = resolve_download_dir(state, host)?;

    // 文件名
    let file_name = remote_path
        .rsplit('/')
        .next()
        .unwrap_or(remote_path);

    // 路径拼接不用 PathBuf::join：wasm32-unknown-unknown 的 PathBuf 是 POSIX
    // 语义（MAIN_SEP='/'），`Path::new(r"C:\Users\x").join("Downloads")` 会把
    // `C:\Users\x` 视为单组件再追加 `/`，产出 `C:\Users\x/Downloads` 混合
    // 分隔符路径；若下载目录带 `\\?\` verbatim 前缀，混合路径在宿主 Windows
    // API（CreateFileW 等）下直接报 os error 123。join_download_path 统一剥
    // 前缀 + 转正斜杠，宿主 fs_exists / rename / explorer canonicalize 均接受。
    let local_path = join_download_path(&download_dir, &format!("{}.part", file_name));
    let final_path = join_download_path(&download_dir, file_name);

    // 目标存在性预检（spec §7.4：目标已存在 → rejected duplicate-name）
    if let Ok(true) = host.fs_exists(&final_path) {
        let mut task = make_task(
            Direction::Download,
            peer_id,
            peer_name,
            remote_path,
            &local_path,
            0,
            now_ms(host),
        );
        task.state = TaskState::Rejected;
        task.reason = Some("duplicate-name".to_string());
        let task_json = serde_json::to_value(&task)?;
        let task_id = task.id.clone();
        // v2：终态即归档（本地同名预检的 rejected 也进历史，不在当前队列留痕）
        state.tasks.insert(task);
        archive_task_if_terminal(state, host, &task_id);
        state.tasks.save(host);
        emit_tasks_changed(host, &state.tasks);
        return Ok(task_json);
    }

    let task = make_task(
        Direction::Download,
        peer_id,
        peer_name,
        remote_path,
        &local_path,
        0,
        now_ms(host),
    );
    let task_json = serde_json::to_value(&task)?;
    let task_id = task.id.clone();
    state.tasks.insert(task);
    state.queue.enqueue(&task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);

    Ok(task_json)
}

/// 上传入队
fn enqueue_upload(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
    remote_path: &str,
    peer_id: &str,
    peer_name: &str,
    batch_id: Option<String>,
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let local_path = args
        .get("localPath")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing localPath for upload"))?;

    // 本地文件必须存在
    if let Ok(false) = host.fs_exists(local_path) {
        return Err(anyhow::anyhow!("local file not found: {}", local_path));
    }

    let mut task = make_task(
        Direction::Upload,
        peer_id,
        peer_name,
        remote_path,
        local_path,
        0,
        now_ms(host),
    );
    task.batch_id = batch_id;
    let task_json = serde_json::to_value(&task)?;
    let task_id = task.id.clone();
    state.tasks.insert(task);
    state.queue.enqueue(&task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);

    Ok(task_json)
}

/// pause：暂停传输中的任务
pub fn pause(
    state: &mut PluginState,
    host: &(impl HostTransfer + HostStorage + HostEvents + HostLog),
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let host_task_id = {
        let task = state.tasks.get_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
        if task.state != TaskState::Transferring {
            return Err(anyhow::anyhow!("task not transferring: {}", task_id));
        }
        task.transition(TaskState::Paused)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        task.host_task_id.clone()
    };

    // 取消宿主传输（释放锁后执行）
    if let Some(ref htid) = host_task_id {
        let _ = host.transfer_cancel(htid);
    }

    state.queue.release(task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    Ok(serde_json::json!({"ok": true}))
}

/// resume：恢复暂停/可恢复的任务
pub fn resume(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.log_info(&format!(
        "resume: enter task_id={} state={:?} offset={} peer_id={} peer_online={}",
        task_id,
        state.tasks.get(task_id).map(|t| t.state),
        state.tasks.get(task_id).map(|t| t.offset).unwrap_or(0),
        state.tasks.get(task_id).map(|t| t.peer.device_id.clone()).unwrap_or_default(),
        state.peer.base_and_auth_for(
            &state.tasks.get(task_id).map(|t| t.peer.device_id.clone()).unwrap_or_default()
        ).is_ok(),
    ));
    let task = state.tasks.get(task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
    if !matches!(task.state, TaskState::Paused | TaskState::Resumable) {
        return Err(anyhow::anyhow!("task not paused/resumable: {}", task_id));
    }
    state.tasks.get_mut(task_id)
        .unwrap()
        .transition(TaskState::Queued)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    state.queue.enqueue(task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    host.log_info(&format!("resume: enqueued task_id={} queue_active={} pending={}", task_id, state.queue.active_count(), state.queue.pending_count()));
    Ok(serde_json::json!({"ok": true}))
}

/// cancel：取消任务
pub fn cancel(
    state: &mut PluginState,
    host: &(impl HostTransfer + HostFs + HostHttp + HostStorage + HostEvents + HostLog + HostConfig),
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let (host_task_id, direction, upload_session_id, local_path, peer_id) = {
        let task = state.tasks.get_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
        if task.state.is_terminal() {
            return Ok(serde_json::json!({"ok": true}));
        }
        task.transition(TaskState::Cancelled)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        (
            task.host_task_id.clone(),
            task.direction,
            task.upload_session_id.clone(),
            task.local_path.clone(),
            task.peer.device_id.clone(),
        )
    };

    // 取消宿主传输（token 取消瞬时完成，不阻塞）
    if let Some(ref htid) = host_task_id {
        let _ = host.transfer_cancel(htid);
    }

    // 下载：删除 .part 文件（桌面端有 fs_delete；移动端没有，跳过）
    if direction == Direction::Download {
        delete_part_file(host, &local_path);
    }

    // 本地终态先落地并推送：UI 即时响应取消，不依赖对端可达性。
    // 远端 cancel_session 为同步 HTTP（对端失联时最长卡 120s），
    // 若放在 emit 之后执行，WASM 单线程被阻塞，前端表现为「取消无反应」
    state.queue.release(task_id);
    state.queue.remove(task_id);
    // v2：取消即终态 → 先归档进历史并从当前队列移除，再持久化任务表
    archive_task_if_terminal(state, host, task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);

    // 上传：取消远端 session（尽力而为；失败仅记日志，不阻塞本地终态）
    if direction == Direction::Upload {
        if let Some(ref sid) = upload_session_id {
            if let Ok((base, auth)) = state.peer.base_and_auth_for(&peer_id) {
                if let Err(e) = handshake::cancel_session(host, &base, &auth, sid) {
                    host.log_error(&format!(
                        "upload cancel_session failed for task {}: {}",
                        task_id, e
                    ));
                }
            }
        }
    }

    Ok(serde_json::json!({"ok": true}))
}

/// remove-task：从传输队列移除任务（任意状态，含终态）
///
/// 终态任务（completed/cancelled/failed/rejected）无生命周期动作可做，
/// 列表只增不减；remove 是唯一清理途径：摘除队列引用、移除任务存储、
/// 持久化并推送变更。活跃任务先取消宿主传输并清理 .part 残留；
/// 上传的远端 session 不做显式取消（本地移除，对端 session 由 TTL/取消兜底）。
pub fn remove_task(
    state: &mut PluginState,
    host: &(impl HostTransfer + HostFs + HostStorage + HostEvents + HostLog),
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let (host_task_id, direction, local_path) = {
        let task = state
            .tasks
            .get(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
        (
            task.host_task_id.clone(),
            task.direction,
            task.local_path.clone(),
        )
    };

    // 活跃任务：取消宿主传输（token 取消瞬时完成，不阻塞）
    if let Some(ref htid) = host_task_id {
        let _ = host.transfer_cancel(htid);
    }

    // 下载：删除 .part 临时文件（幂等，残留不阻塞移除）
    if direction == Direction::Download {
        delete_part_file(host, &local_path);
    }

    state.queue.remove(task_id);
    state.tasks.remove(task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    Ok(serde_json::json!({"ok": true}))
}

/// resume-all：恢复所有 paused/resumable 任务
pub fn resume_all(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
) -> anyhow::Result<serde_json::Value> {
    let resumable_ids: Vec<String> = state
        .tasks
        .values()
        .filter(|t| matches!(t.state, TaskState::Paused | TaskState::Resumable))
        .map(|t| t.id.clone())
        .collect();

    for id in &resumable_ids {
        if let Some(task) = state.tasks.get_mut(id) {
            let _ = task.transition(TaskState::Queued);
            state.queue.enqueue(id);
        }
    }

    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    Ok(serde_json::json!({"ok": true, "count": resumable_ids.len()}))
}

/// retry：重试失败的任务
///
/// duplicate-name 拒绝（下载方向）先清理本地目标与残留 .part，
/// 否则重试必然再次同名被拒（spec §7.4）；上传方向远端文件不可删
/// （spec 禁止删除远端），重试前需用户在对端处理。
pub fn retry(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog + HostFs + HostConfig),
    task_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let (direction, reason, local_path, batch_id) = {
        let task = state.tasks.get_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
        if task.state != TaskState::Failed && task.state != TaskState::Rejected {
            return Err(anyhow::anyhow!("task not failed/rejected: {}", task_id));
        }
        (
            task.direction,
            task.reason.clone(),
            task.local_path.clone(),
            task.batch_id.clone(),
        )
    };

    // duplicate-name（下载）：清理本地目标文件与残留 .part，使重试可成功；
    // 上传方向远端文件不可删（spec 禁止删除远端），重试前需用户在对端处理
    if direction == Direction::Download
        && reason.as_deref() == Some("duplicate-name")
        && !local_path.is_empty()
    {
        // 目标文件 = .part 路径去掉后缀（enqueue 预检与 rename 冲突均源于目标存在）
        let final_path = local_path.strip_suffix(".part").unwrap_or(&local_path);
        for p in [final_path, local_path.as_str()] {
            if let Err(e) = host.fs_delete(p) {
                host.log_warn(&format!(
                    "retry: delete {} for duplicate-name failed (ignored): {}",
                    p, e
                ));
            }
        }
    }

    let task = state.tasks.get_mut(task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
    task.transition(TaskState::Queued)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    task.reason = None;
    task.offset = 0;
    task.host_task_id = None;
    task.upload_session_id = None;
    // v2：approval 相关拒绝（user-rejected/timeout/policy-denied）重试 = 重新询问——
    // 换新批 ID（启动时重新发起 transfer-request）。清空批 ID 不可行：ask 策略下
    // 无批上下文的 session 创建必 403（batch-context-required），重试必然死胡同；
    // 与移动端行为逐字一致。duplicate-name 保留批上下文（批已批准，重试免问直接传）
    if batch_id.is_some() {
        if matches!(
            reason.as_deref(),
            Some("user-rejected" | "timeout" | "policy-denied")
        ) {
            task.batch_id = Some(format!("b-{}", generate_id(now_ms(host))));
        }
    }
    let id = task.id.clone();
    state.queue.enqueue(&id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    Ok(serde_json::json!({"ok": true}))
}

/// set-concurrency：设置并发数
pub fn set_concurrency(
    state: &mut PluginState,
    host: &(impl HostStorage + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let n = args
        .get("concurrency")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("missing concurrency"))? as usize;
    state.queue.set_concurrency(n);
    state.settings.concurrency = state.queue.concurrency();
    save_settings(host, &state.settings);
    Ok(serde_json::json!({"ok": true, "concurrency": state.queue.concurrency()}))
}

/// get-settings：返回当前设置
pub fn get_settings(state: &PluginState) -> serde_json::Value {
    serde_json::to_value(&state.settings).unwrap_or_default()
}

/// set-settings：更新设置
pub fn set_settings(
    state: &mut PluginState,
    host: &(impl HostStorage + HostLog + HostFileService + HostConfig),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    if let Some(roots) = args.get("roots").and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()) {
        state.settings.roots = roots.clone();
        if roots.is_empty() {
            // 清空全部共享目录 = 停止共享：卸载挂载（宿主拒绝空 roots 挂载）
            if state.mounted {
                let _ = host.filesrv_unmount(MOUNT_PATH);
                state.mounted = false;
                host.log_info("all shared roots removed, file service unmounted");
            }
        } else if state.mounted {
            let _ = host.filesrv_update_roots(MOUNT_PATH, &roots);
        } else {
            // 之前未挂载（如清空后重配目录）：与激活逻辑一致重新挂载
            let options = build_mount_options(&roots, &resolve_download_dir(state, host).ok());
            match host.filesrv_mount(&options) {
                Ok(result) => {
                    state.mounted = true;
                    host.log_info(&format!("mounted at {}", result.base_path));
                }
                // 挂载失败必须回报（否则设置显示已保存但共享目录实际未生效，
                // 且不会发布公告导致对端永远看不到服务）
                Err(e) => return Err(anyhow::anyhow!("mount failed: {}", e)),
            }
        }
    }
    if let Some(dir) = args.get("downloadDir").and_then(|v| v.as_str()) {
        let changed = state.settings.download_dir != dir;
        state.settings.download_dir = dir.to_string();
        // 挂载后刷新接收落点：宿主只提供 filesrv_update_roots，downloads_dir 变化
        // 需 unmount + remount 生效——否则同名预检按新目录、实际落盘仍旧目录，
        // 与“下载目录 = 接收落点”模型自相矛盾
        if changed && state.mounted {
            if let Err(e) = host.filesrv_unmount(MOUNT_PATH) {
                return Err(anyhow::anyhow!(
                    "unmount failed before download dir change: {}",
                    e
                ));
            }
            let options = build_mount_options(
                &state.settings.roots.clone(),
                &resolve_download_dir(state, host).ok(),
            );
            match host.filesrv_mount(&options) {
                Ok(result) => {
                    state.mounted = true;
                    host.log_info(&format!(
                        "remounted with new downloads_dir, base={}",
                        result.base_path
                    ));
                }
                // 重挂失败必须回报（挂载失败即失效，对端上传会 403/500）
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "remount failed after download dir change: {}",
                        e
                    ))
                }
            }
        }
    }
    if let Some(n) = args.get("concurrency").and_then(|v| v.as_u64()) {
        state.queue.set_concurrency(n as usize);
        state.settings.concurrency = state.queue.concurrency();
    }
    // v2 接收策略与同意超时：校验取值后写入设置，并在已挂载时同步到宿主
    // （宿主 TTL 扫描用 per-mount 配置；每次挂载时也同步一次，保证两侧一致）
    if let Some(policy) = args.get("receivingPolicy").and_then(|v| v.as_str()) {
        if matches!(policy, "ask" | "accept" | "reject") {
            state.settings.receiving_policy = policy.to_string();
        } else {
            return Err(anyhow::anyhow!(
                "invalid receivingPolicy '{}' (expected ask|accept|reject)",
                policy
            ));
        }
    }
    if let Some(secs) = args.get("approvalTimeoutSec").and_then(|v| v.as_u64()) {
        if secs < MIN_APPROVAL_TIMEOUT_SEC || secs > MAX_APPROVAL_TIMEOUT_SEC {
            return Err(anyhow::anyhow!(
                "approvalTimeoutSec {} out of range {}-{}",
                secs,
                MIN_APPROVAL_TIMEOUT_SEC,
                MAX_APPROVAL_TIMEOUT_SEC
            ));
        }
        state.settings.approval_timeout_sec = secs;
    }
    save_settings(host, &state.settings);
    sync_approval_timeout(state, host);
    Ok(serde_json::json!({"ok": true}))
}

/// pick-download-dir：返回错误码（WASM 无法弹窗，需前端走 context.fileService.pickDirectory）
pub fn pick_download_dir() -> anyhow::Result<serde_json::Value> {
    // 前端应使用 context.fileService.pickDirectory 选择目录后调用 set-settings
    Ok(serde_json::json!({
        "error": "use-frontend-picker",
        "message": "WASM cannot open directory picker; use context.fileService.pickDirectory"
    }))
}

/// mount-local：挂载本地目录
pub fn mount_local(
    state: &mut PluginState,
    host: &(impl HostFileService + HostLog + HostConfig),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let roots = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_else(|| state.settings.roots.clone());

    if roots.is_empty() {
        return Err(anyhow::anyhow!("no roots to mount"));
    }

    let options = build_mount_options(&roots, &resolve_download_dir(state, host).ok());
    match host.filesrv_mount(&options) {
        Ok(result) => {
            state.mounted = true;
            state.settings.roots = roots;
            host.log_info(&format!("mounted at {}", result.base_path));
            // v2：挂载后同步批准超时（宿主 TTL 扫描配置）
            sync_approval_timeout(state, host);
            Ok(serde_json::json!({"ok": true, "basePath": result.base_path}))
        }
        Err(e) => Err(anyhow::anyhow!("mount failed: {}", e)),
    }
}

/// update-roots：更新挂载根
pub fn update_roots(
    state: &mut PluginState,
    host: &(impl HostFileService + HostStorage + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let roots = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .ok_or_else(|| anyhow::anyhow!("missing roots"))?;

    if state.mounted {
        if roots.is_empty() {
            // 清空全部共享目录 = 停止共享：卸载挂载（宿主拒绝空 roots 挂载）
            host.filesrv_unmount(MOUNT_PATH)
                .map_err(|e| anyhow::anyhow!("unmount failed: {}", e))?;
            state.mounted = false;
        } else {
            host.filesrv_update_roots(MOUNT_PATH, &roots)
                .map_err(|e| anyhow::anyhow!("update_roots failed: {}", e))?;
        }
    }
    state.settings.roots = roots;
    save_settings(host, &state.settings);
    Ok(serde_json::json!({"ok": true}))
}

// ==================== 传输启动 ====================

/// 调度并启动待处理任务
///
/// 返回需要 emit 的任务变更事件（调用方在释放锁后执行）
pub fn schedule_and_start(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService + HostBus),
) {
    let actions = state.queue.schedule();
    host.log_info(&format!(
        "schedule_and_start: actions={:?} active={} pending={}",
        actions, state.queue.active_count(), state.queue.pending_count()
    ));
    for task_id in actions {
        if let Err(e) = start_single_task(state, host, &task_id) {
            host.log_error(&format!("start task {} failed: {}", task_id, e));
            if let Some(task) = state.tasks.get_mut(&task_id) {
                task.state = TaskState::Failed;
                task.reason = Some(e);
            }
            state.queue.release(&task_id);
            // v2：启动即终态（失败）→ 归档历史并从当前队列移除
            archive_task_if_terminal(state, host, &task_id);
        }
    }
    if state.tasks.is_dirty() {
        state.tasks.save(host);
        // 终态归档/等待同意等迁移会改变列表，主动推送（v1 仅靠进度事件被动推送，
        // 启动即失败的任务会一直显示「排队」）
        emit_tasks_changed(host, &state.tasks);
    }
}

/// 启动单个任务传输
fn start_single_task(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService + HostBus),
    task_id: &str,
) -> Result<(), String> {
    host.log_info(&format!("start_single_task: enter task_id={}", task_id));
    let task = state.tasks.get(task_id).ok_or("task not found")?;
    // 任务从入队起绑定对端：调度用任务自己的 endpoint，切换激活对端不影响排队任务
    let (base, auth) = state.peer.base_and_auth_for(&task.peer.device_id)?;
    host.log_info(&format!(
        "start_single_task: task_id={} base={} auth_present={} offset={} direction={:?}",
        task_id, base, !auth.is_empty(), task.offset, task.direction
    ));
    let direction = task.direction;
    let remote_path = task.remote_path.clone();
    let local_path = task.local_path.clone();
    let offset = task.offset;
    let upload_session_id = task.upload_session_id.clone();
    let fingerprint = task.fingerprint.clone();
    let task_peer_id = task.peer.device_id.clone();

    match direction {
        Direction::Download => {
            // 续传指纹校验（spec §7.4）
            host.log_info(&format!(
                "start_single_task: HEAD fingerprint task_id={} base={} remote={}",
                task_id, base, remote_path
            ));
            let remote_fp = handshake::fingerprint(host, &base, &auth, &remote_path)?;
            host.log_info(&format!(
                "start_single_task: fingerprint size={} mtime={}",
                remote_fp.size, remote_fp.mtime
            ));

            if let Some(ref saved_fp) = fingerprint {
                if saved_fp.size != remote_fp.size || saved_fp.mtime != remote_fp.mtime {
                    // 远端文件变化 → failed
                    if let Some(task) = state.tasks.get_mut(task_id) {
                        task.state = TaskState::Failed;
                        task.reason = Some("remote-changed".to_string());
                    }
                    state.queue.release(task_id);
                    return Err("remote-changed".to_string());
                }
            }

            // 新任务：保存指纹
            if fingerprint.is_none() {
                if let Some(task) = state.tasks.get_mut(task_id) {
                    task.fingerprint = Some(Fingerprint {
                        size: remote_fp.size,
                        mtime: remote_fp.mtime,
                    });
                    task.size = remote_fp.size;
                }
            }

            let task = state.tasks.get(task_id).unwrap();
            // 只剥一次后缀，与前端 openInDir 的 strip 逻辑及 remove_task 清理一致；
            // trim_end_matches 会重复剥除（文件名为 `x.part` 时 `x.part.part` → `x`），
            // 与宿主 rename 目标不一致会导致完成后 reveal 定位失败。
            let final_path = task
                .local_path
                .strip_suffix(".part")
                .unwrap_or(&task.local_path)
                .to_string();

            let request = TransferRequest {
                task_id: task_id.to_string(),
                direction: TransferDirection::Download,
                url: format!("{}/file?path={}", base, urlencoded(&remote_path)),
                headers: auth_headers(&auth),
                local_path: local_path.clone(),
                offset,
                expected_size: task.size,
                final_path: Some(final_path),
            };

            // 先订阅再启动：宿主以插件 task_id 为进度总线 topic（transfer:{task_id}），
            // 订阅先于 transfer_start 执行，进度/终态消息不会因「宿主先完成」而丢失。
            // 订阅失败仅告警（进度事件缺失由任务状态兜底）；启动失败时退订防泄漏
            let progress_topic = format!("transfer:{}", task_id);
            if let Err(e) = host.bus_subscribe(&progress_topic) {
                host.log_warn(&format!("bus_subscribe {} failed: {}", progress_topic, e));
            }
            let host_task_id = match host.transfer_start(&request) {
                Ok(id) => id,
                Err(e) => {
                    if let Err(uerr) = host.bus_unsubscribe(&progress_topic) {
                        host.log_warn(&format!("bus_unsubscribe {} failed: {}", progress_topic, uerr));
                    }
                    return Err(format!("transfer_start failed: {}", e));
                }
            };

            if let Some(task) = state.tasks.get_mut(task_id) {
                task.host_task_id = Some(host_task_id);
                task.state = TaskState::Transferring;
            }
        }
        Direction::Upload => {
            // v2 批 gating（spec 14.2）：批内任务先确认批状态再建 session
            // - 批记录不存在 → 发起 POST /transfer-request（首个任务触发）
            // - 批记录 Pending → 任务转 waiting-approval（等批准后再调度）
            // - 批记录 Approved → 免钩子建 session（带 batchId）
            // - 批记录 Rejected → 任务终态（reason 透传）
            // - 网络错误 → 任务 failed
            let task_batch_id = state
                .tasks
                .get(task_id)
                .and_then(|t| t.batch_id.clone());
            // 克隆：批 ID 在 gating 分支内被 move，续传分支还要用它带批上下文
            let task_batch_id = task_batch_id.clone();
            let session_batch_id = task_batch_id.clone();
            if let Some(batch_id) = task_batch_id {
                let record = state.batches.get(&batch_id).cloned();
                match record {
                    Some(rec) => match rec.state {
                        BatchRecordState::Approved => {}
                        BatchRecordState::Pending => {
                            // 等待同意：释放槽位，任务保持 waiting-approval
                            if let Some(task) = state.tasks.get_mut(task_id) {
                                let _ = task.transition(TaskState::WaitingApproval);
                            }
                            state.queue.release(task_id);
                            host.log_info(&format!(
                                "start_single_task: task {} waiting approval (batch {})",
                                task_id, batch_id
                            ));
                            return Ok(());
                        }
                        BatchRecordState::Rejected { reason } => {
                            if let Some(task) = state.tasks.get_mut(task_id) {
                                task.state = TaskState::Rejected;
                                task.reason = Some(reason);
                            }
                            state.queue.release(task_id);
                            archive_task_if_terminal(state, host, task_id);
                            return Err("batch rejected".to_string());
                        }
                    },
                    None => {
                        // 批记录不存在：发起 transfer-request（files = 当前已入队的
                        // 同批任务清单；批准后后续入队的任务由记录状态直接分流）
                        let files: Vec<UploadRequestMeta> = state
                            .tasks
                            .values()
                            .filter(|t| t.batch_id.as_deref() == Some(batch_id.as_str()))
                            .map(|t| UploadRequestMeta {
                                relative_path: t.remote_path.clone(),
                                size: t.size,
                            })
                            .collect();
                        let total_size: u64 = files.iter().map(|f| f.size).sum();
                        let outcome = request_transfer(
                            host,
                            &base,
                            &auth,
                            &batch_id,
                            &files,
                            total_size,
                        );
                        match outcome {
                            Ok(TransferRequestOutcome::Approved) => {
                                state.batches.insert(
                                    batch_id.clone(),
                                    BatchRecord {
                                        batch_id: batch_id.clone(),
                                        peer_id: task_peer_id.clone(),
                                        state: BatchRecordState::Approved,
                                    },
                                );
                                host.log_info(&format!(
                                    "start_single_task: batch {} approved directly by hook",
                                    batch_id
                                ));
                            }
                            Ok(TransferRequestOutcome::Pending) => {
                                state.batches.insert(
                                    batch_id.clone(),
                                    BatchRecord {
                                        batch_id: batch_id.clone(),
                                        peer_id: task_peer_id.clone(),
                                        state: BatchRecordState::Pending,
                                    },
                                );
                                if let Some(task) = state.tasks.get_mut(task_id) {
                                    let _ = task.transition(TaskState::WaitingApproval);
                                }
                                state.queue.release(task_id);
                                host.log_info(&format!(
                                    "start_single_task: batch {} pending, task {} waiting approval",
                                    batch_id, task_id
                                ));
                                return Ok(());
                            }
                            Err(TransferRequestError::Denied(reason)) => {
                                // 策略拒绝（如 policy-denied）：批记录与任务都终态
                                state.batches.insert(
                                    batch_id.clone(),
                                    BatchRecord {
                                        batch_id: batch_id.clone(),
                                        peer_id: task_peer_id.clone(),
                                        state: BatchRecordState::Rejected { reason: reason.clone() },
                                    },
                                );
                                if let Some(task) = state.tasks.get_mut(task_id) {
                                    task.state = TaskState::Rejected;
                                    task.reason = Some(reason);
                                }
                                state.queue.release(task_id);
                                archive_task_if_terminal(state, host, task_id);
                                return Err("transfer request denied".to_string());
                            }
                            Err(TransferRequestError::Network(e)) => {
                                // 网络错误：不建批记录（任务 failed，reason 原文）
                                if let Some(task) = state.tasks.get_mut(task_id) {
                                    task.state = TaskState::Failed;
                                    task.reason = Some(e.clone());
                                }
                                state.queue.release(task_id);
                                archive_task_if_terminal(state, host, task_id);
                                return Err(e);
                            }
                        }
                    }
                }
            }

            // 续传握手（spec §7.4）
            let (session_id, received) = if let Some(ref sid) = upload_session_id {
                match handshake::query_session(host, &base, &auth, sid) {
                    Ok(received) => (sid.clone(), received),
                    Err(QuerySessionError::SessionLost) => {
                        // session 丢失 → 重建从头传（批准后免问重连续传，带批上下文）
                        let created = handshake::create_session(
                            host,
                            &base,
                            &auth,
                            &remote_path,
                            0,
                            session_batch_id.as_deref(),
                        )
                        .map_err(|e| format!("recreate session: {:?}", e))?;
                        (created.session_id, created.received)
                    }
                    Err(QuerySessionError::Other(e)) => return Err(e),
                }
            } else {
                // 新上传：创建 session（v2 批内任务带 batchId，免钩子）
                match handshake::create_session(
                    host,
                    &base,
                    &auth,
                    &remote_path,
                    0,
                    session_batch_id.as_deref(),
                ) {
                    Ok(created) => (created.session_id, created.received),
                    Err(CreateSessionError::DuplicateName) => {
                        if let Some(task) = state.tasks.get_mut(task_id) {
                            task.state = TaskState::Rejected;
                            task.reason = Some("duplicate-name".to_string());
                        }
                        state.queue.release(task_id);
                        archive_task_if_terminal(state, host, task_id);
                        return Err("duplicate-name".to_string());
                    }
                    Err(CreateSessionError::Other(e)) => return Err(e),
                }
            };

            if let Some(task) = state.tasks.get_mut(task_id) {
                task.upload_session_id = Some(session_id.clone());
                task.offset = received;
            }

            let request = TransferRequest {
                task_id: task_id.to_string(),
                direction: TransferDirection::Upload,
                url: format!("{}/upload/{}", base, session_id),
                headers: auth_headers(&auth),
                local_path: local_path.clone(),
                offset: received,
                expected_size: 0,
                final_path: None,
            };

            // 先订阅再启动（同 Download 分支，见上）
            let progress_topic = format!("transfer:{}", task_id);
            if let Err(e) = host.bus_subscribe(&progress_topic) {
                host.log_warn(&format!("bus_subscribe {} failed: {}", progress_topic, e));
            }
            let host_task_id = match host.transfer_start(&request) {
                Ok(id) => id,
                Err(e) => {
                    if let Err(uerr) = host.bus_unsubscribe(&progress_topic) {
                        host.log_warn(&format!("bus_unsubscribe {} failed: {}", progress_topic, uerr));
                    }
                    return Err(format!("transfer_start failed: {}", e));
                }
            };

            if let Some(task) = state.tasks.get_mut(task_id) {
                task.host_task_id = Some(host_task_id);
                task.state = TaskState::Transferring;
            }
        }
    }

    Ok(())
}

// ==================== 消息处理 ====================

/// 处理传输进度消息（on_message `transfer:{task_id}`）
pub fn handle_transfer_progress(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog + HostTransfer + HostFs + HostHttp + HostFileService + HostConfig + HostBus),
    progress: &TransferProgress,
) {
    let task_id = match state.tasks.find_by_host_task_id(&progress.task_id) {
        Some(id) => id,
        None => return, // 未知任务
    };

    let task = match state.tasks.get_mut(&task_id) {
        Some(t) => t,
        None => return,
    };

    // 终态幂等守卫：宿主终态事件可能迟到/重复（取消清理等），任务已终态时
    // 直接忽略——否则 Cancelled 分支兜底会把终态任务拉回活跃循环，
    // 前端每轮「失败→复活→再失败」重发一次通知
    if task.state.is_terminal() {
        return;
    }

    // 更新偏移
    task.offset = progress.transferred;
    if progress.total > 0 {
        task.size = progress.total;
    }

    // 终态处理
    match &progress.state {
        TransferState::Completed => {
            task.state = TaskState::Completed;
            task.offset = task.size;
            state.queue.release(&task_id);

            // 上传完成：通知远端 complete（失败记日志，不阻塞终态）
            if task.direction == Direction::Upload {
                if let Some(ref sid) = task.upload_session_id.clone() {
                    if let Ok((base, auth)) = state.peer.base_and_auth_for(&task.peer.device_id) {
                        if let Err(e) = handshake::complete_session(host, &base, &auth, sid) {
                            host.log_error(&format!(
                                "upload complete_session failed for task {}: {}",
                                task_id, e
                            ));
                        }
                    }
                }
            }
        }
        TransferState::Failed(reason) => {
            // 用户已取消（cancel() 先置 Cancelled）：取消竞态中宿主回报的
            // 失败（如远端已删 session 致 PUT 404）不覆写取消终态，
            // 否则用户看到「已取消」又跳回「失败」，表现为取消无效
            if task.state == TaskState::Cancelled {
                // 保持 cancelled，不进入 failed
            }
            // 对端下线已置恢复态（handle_peer_changed）→ 保持不覆写，
            // 续传握手会重校验文件指纹，避免把可恢复任务误判为终态
            else if task.state == TaskState::Resumable && task.auto_resumable {
                // 保持 resumable，不进入终态
            } else if reason == "duplicate-name" {
                task.state = TaskState::Rejected;
                task.reason = Some("duplicate-name".to_string());
            } else {
                task.state = TaskState::Failed;
                task.reason = Some(reason.clone());
                // 失败终态：清除断线自动续传标记，防止后续事件路径再复活
                task.auto_resumable = false;
            }
            state.queue.release(&task_id);
        }
        TransferState::Cancelled => {
            // 宿主回推的 Cancelled 终态可能来自多条路径：
            // 1. 用户取消（cancel() 已先置 Cancelled）
            // 2. 用户暂停（pause() 已先置 Paused）
            // 3. 对端下线（handle_peer_changed 已先置 Resumable + auto_resumable）
            // 只有真正用户取消才进入终态；恢复态不能被覆写，否则自动续传失效
            match task.state {
                TaskState::Cancelled => {
                    // 用户已取消 → 幂等清理 .part（文件可能已不存在）
                    if task.direction == Direction::Download {
                        let lp = task.local_path.clone();
                        delete_part_file(host, &lp);
                    }
                }
                TaskState::Paused | TaskState::Resumable | TaskState::Queued => {
                    // 暂停/恢复/排队中收到 cancel 回报 → 保持原状态不覆写
                }
                _ => {
                    // 异常路径：降级为 resumable 保数据，不丢 .part
                    task.state = TaskState::Resumable;
                    task.auto_resumable = true;
                }
            }
            state.queue.release(&task_id);
        }
        TransferState::Running => {
            // 进度更新，不改变状态
        }
    }

    // 持久化策略（spec §7.3）：终态立即写，Running 进度按 1s 节流；
    // emit_tasks_changed 每消息照发，保证 UI 实时进度。
    // 时间经宿主获取（wasm32-unknown-unknown 无系统时钟，SystemTime 会 panic）
    let now = now_ms(host);

    let is_terminal = matches!(
        &progress.state,
        TransferState::Completed | TransferState::Failed(_) | TransferState::Cancelled
    );

    if is_terminal {
        // v2：终态任务归档进历史并从当前队列移除（终态不留在当前列表）
        archive_task_if_terminal(state, host, &task_id);
        state.tasks.save(host);
        emit_tasks_changed(host, &state.tasks);
        // 终态已到，取消进度 topic 订阅（避免 topic 泄漏）；失败仅告警，不影响任务收尾
        if let Err(e) = host.bus_unsubscribe(&format!("transfer:{}", progress.task_id)) {
            host.log_warn(&format!(
                "bus_unsubscribe transfer:{} failed: {}",
                progress.task_id, e
            ));
        }
    } else if let Some(task) = state.tasks.get_mut(&task_id) {
        if task.should_flush(now) {
            task.mark_flushed(now);
            state.tasks.save(host);
        }
    }

    emit_tasks_changed(host, &state.tasks);
}

/// 处理对端上下线消息（on_message `filesrv:peer_changed`）
///
/// 多对端语义：任务与对端强绑定（task.peer.device_id），仅受影响对端的任务
/// 暂停/自动恢复；激活对端自动管理（首台自动激活、下线自动切换）在 PeerStore 内。
pub fn handle_peer_changed(
    state: &mut PluginState,
    host: &(impl HostFileService + HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostBus),
    peer_id: &str,
    online: bool,
) {
    let peers_changed = if online {
        state.peer.on_peer_online(host, peer_id)
    } else {
        state.peer.on_peer_offline(peer_id)
    };

    host.log_info(&format!(
        "peer_changed: peer_id={} online={} changed={}",
        peer_id, online, peers_changed
    ));
    if online {
        // 打印当前激活对端连接信息（token 只打长度，不打本体）
        match state.peer.active() {
            Some(ep) => {
                host.log_info(&format!(
                    "peer_changed: active peer ip={} port={} token_len={} mounts={}",
                    ep.ip,
                    ep.port,
                    ep.token.len(),
                    ep.mounts.len()
                ));
                if ep.token.is_empty() {
                    host.log_warn("peer_changed: active peer token is EMPTY — remote HTTP calls will lack Authorization (401)");
                }
            }
            None => {
                host.log_warn(&format!(
                    "peer_changed: online=true but no active peer (peer_id={})",
                    peer_id
                ));
            }
        }
    }

    if !online {
        // 该对端下线：其 transferring/queued 任务 → resumable（auto_resumable=true）；
        // v2：WaitingApproval 任务 → rejected(timeout)——等待同意期间断线不重发
        // （spec 14.2 边界 1），批记录保留 Pending（接收端自然超时）；
        // 其他对端的任务不受影响。queued 不摘除会留待后续调度周期被
        // start_single_task 误判 Failed（peer not online），且上线后无法自动恢复
        let affected_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                (t.state == TaskState::Transferring || t.state == TaskState::Queued)
                    && t.peer.device_id == peer_id
            })
            .map(|t| t.id.clone())
            .collect();

        for id in &affected_ids {
            if let Some(task) = state.tasks.get_mut(id) {
                let htid = task.host_task_id.clone();
                let _ = task.transition(TaskState::Resumable);
                task.auto_resumable = true;
                // 取消宿主传输 + 摘除队列（状态已置 resumable，排队语义失效）
                if let Some(ref h) = htid {
                    let _ = host.transfer_cancel(h);
                }
                state.queue.release(id);
                state.queue.remove(id);
            }
        }
        // v2：等待同意期间对端下线 → 任务直接 rejected(timeout)，不重发
        let waiting_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| t.state == TaskState::WaitingApproval && t.peer.device_id == peer_id)
            .map(|t| t.id.clone())
            .collect();
        for id in &waiting_ids {
            if let Some(task) = state.tasks.get_mut(&id) {
                task.state = TaskState::Rejected;
                task.reason = Some("timeout".to_string());
            }
            archive_task_if_terminal(state, host, &id);
        }
        if !affected_ids.is_empty() || !waiting_ids.is_empty() {
            state.tasks.save(host);
            emit_tasks_changed(host, &state.tasks);
        }
    } else if peers_changed {
        // 该对端上线（仅上下线边沿触发一次）：其 auto_resumable 的 resumable
        // 任务自动重新调度（spec §7.2）。重复公告（changed=false）不触发恢复
        // ——否则对端反复重连时任务被反复复活重启，每轮失败触发一次通知
        let auto_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                t.state == TaskState::Resumable
                    && t.auto_resumable
                    && t.peer.device_id == peer_id
            })
            .map(|t| t.id.clone())
            .collect();

        for id in &auto_ids {
            if let Some(task) = state.tasks.get_mut(id) {
                let _ = task.transition(TaskState::Queued);
                task.auto_resumable = false;
                state.queue.enqueue(id);
            }
        }
        if !auto_ids.is_empty() {
            state.tasks.save(host);
            emit_tasks_changed(host, &state.tasks);
            schedule_and_start(state, host);
        }
    }

    if peers_changed {
        emit_peers_changed(host, &state.peer);
    }
}

// ==================== v2 接收端（批应答 / 接收任务） ====================

/// 处理批量传输请求事件（`filesrv:transfer_request`，ask 分流建 pending 批时）
///
/// 接收端：建 PendingBatch（应答卡数据源）+ 推送 batches-changed；
/// ask 模式不发 toast（等待应答，spec §9.3）
pub fn handle_transfer_request(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostConfig),
    payload: &serde_json::Value,
) {
    let batch_id = payload.get("batchId").and_then(|v| v.as_str()).unwrap_or("");
    if batch_id.is_empty() {
        return;
    }
    let files: Vec<UploadRequestMeta> = payload
        .get("files")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let total_size = payload.get("totalSize").and_then(|v| v.as_u64()).unwrap_or(0);

    // 发送方 = 当前激活对端（接收端插件只服务一个激活对端）；
    // 设备名插件侧无缓存，前端按 peer_id 从设备列表补全展示名
    let peer_id = state.peer.active_id().unwrap_or("").to_string();
    let batch = PendingBatch {
        batch_id: batch_id.to_string(),
        peer_id,
        peer_name: String::new(),
        files,
        total_size,
        created_at: now_ms(host),
    };
    state.pending_batches.insert(batch_id.to_string(), batch);
    emit_batches_changed(host, state);
}

/// 处理批量传输请求已解决事件（`filesrv:transfer_resolved`，approve/reject 命令、TTL 超时）
///
/// 移除 PendingBatch（应答卡消失）；approved → 批级 toast（前端展示）
pub fn handle_transfer_resolved(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog),
    payload: &serde_json::Value,
) {
    let batch_id = payload.get("batchId").and_then(|v| v.as_str()).unwrap_or("");
    let decision = payload.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    let removed = state.pending_batches.remove(batch_id);
    if let Some(batch) = removed {
        emit_batches_changed(host, state);
        if decision == "approved" {
            // 批准后批级一条 toast（spec §9.3 / §12.4：mode=batch 立即弹）
            host.emit_event(
                "plugin:file-transfer:toast",
                &serde_json::json!({
                    "name": batch.peer_name,
                    "count": batch.files.len(),
                    "totalSize": batch.total_size,
                    "mode": "batch",
                }),
            );
        }
    }
}

/// 处理接收开始事件（`filesrv:receiving_started`，session 创建成功时）
///
/// 接收端：建 ReceivingTask（Transferring）+ 推送 receiving-changed；
/// accept 模式 → per-file toast（前端 3s 窗口合并去重）
pub fn handle_receiving_started(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostConfig),
    payload: &serde_json::Value,
) {
    let session_id = payload.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    if session_id.is_empty() {
        return;
    }
    let batch_id = payload
        .get("batchId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let remote_path = payload.get("relativePath").and_then(|v| v.as_str()).unwrap_or("");
    let size = payload.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    let now = now_ms(host);
    let task = ReceivingTask {
        session_id: session_id.to_string(),
        batch_id,
        remote_path: remote_path.to_string(),
        size,
        state: TaskState::Transferring,
        reason: None,
        peer_id: state.peer.active_id().unwrap_or("").to_string(),
        created_at: now,
        updated_at: now,
    };
    state.receiving.insert(session_id.to_string(), task);
    emit_receiving_changed(host, state);

    // accept 模式：传输开始时发 toast（3s 窗口合并去重由前端做）。
    // 发送方 v2 一律带 batchId（含 accept 模式），不能以 batchId 为空作为
    // 判断——否则 accept 模式 toast 永不触发（批级 toast 只在 ask 批准时经
    // transfer_resolved 发，此处 per-file 不会与批级重复）
    if state.settings.receiving_policy == "accept" {
        host.emit_event(
            "plugin:file-transfer:toast",
            &serde_json::json!({
                "name": active_peer_name(state),
                "count": 1,
                "mode": "per-file",
            }),
        );
    }
}

/// 处理接收结束事件（`filesrv:receiving_done`，complete / 409 / cancel）
///
/// ReceivingTask 终态 + 归档历史（initiator=peer）+ 推送 receiving-changed /
/// history-changed；409 竞态（duplicate-name）→ rejected 展示语义
pub fn handle_receiving_done(
    state: &mut PluginState,
    host: &(impl HostEvents + HostStorage + HostLog + HostConfig),
    payload: &serde_json::Value,
) {
    let session_id = payload.get("sessionId").and_then(|v| v.as_str()).unwrap_or("");
    let done_state = payload.get("state").and_then(|v| v.as_str()).unwrap_or("failed");
    let reason = payload
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let now = now_ms(host);

    let Some(mut task) = state.receiving.remove(session_id) else {
        return;
    };
    // 状态映射：completed / cancelled 直映；failed + duplicate-name → rejected
    // （批内同名即拒展示语义，spec 14.2 边界 5），其余 failed
    let (state_kind, final_reason) = match (done_state, reason.as_deref()) {
        ("completed", _) => (TaskState::Completed, None),
        ("cancelled", _) => (TaskState::Cancelled, None),
        ("failed", Some("duplicate-name")) => {
            (TaskState::Rejected, Some("duplicate-name".to_string()))
        }
        ("failed", r) => (TaskState::Failed, r.map(|s| s.to_string())),
        _ => (TaskState::Failed, reason.clone()),
    };
    task.state = state_kind;
    task.reason = final_reason.clone();
    task.updated_at = now;

    // 归档历史：接收任务（initiator=peer）；接收落点在私有下载目录，
    // 插件侧无路径语义，localPath 留空（spec §10 字段说明）
    let entry = HistoryEntry {
        id: task.session_id.clone(),
        // 接收方向（对端发起）文件流向 = 对端 → 本地，历史方向显示 ↓，
        // 与移动端归档一致（initiator=peer 时记 download）
        direction: Direction::Download,
        initiator: "peer".to_string(),
        file_name: task
            .remote_path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string(),
        size: task.size,
        state: state_kind,
        reason: final_reason,
        peer_name: active_peer_name(state),
        local_path: None,
        created_at: task.created_at,
        updated_at: now,
    };
    state.history.insert(host, entry);
    emit_receiving_changed(host, state);
    emit_history_changed(host, state);
}

/// 处理传输批应答事件（`filesrv:transfer_approval`，接收端批准/拒绝/超时 → 发送端）
///
/// approved → 批记录 Approved，批内 WaitingApproval 任务 → Queued + 重新调度；
/// rejected → 批记录 Rejected，批内 WaitingApproval 任务 → 终态归档（reason 映射）
pub fn handle_transfer_approval(
    state: &mut PluginState,
    host: &(impl HostEvents + HostStorage + HostLog + HostHttp + HostFs + HostConfig + HostTransfer + HostFileService + HostBus),
    payload: &serde_json::Value,
) {
    let batch_id = payload.get("batchId").and_then(|v| v.as_str()).unwrap_or("");
    let decision = payload.get("decision").and_then(|v| v.as_str()).unwrap_or("");
    let reason = payload.get("reason").and_then(|v| v.as_str()).unwrap_or("");
    if batch_id.is_empty() {
        return;
    }

    match decision {
        "approved" => {
            if let Some(rec) = state.batches.get_mut(batch_id) {
                rec.state = BatchRecordState::Approved;
            }
            host.log_info(&format!("transfer approval: batch {} approved", batch_id));
            // 批内 WaitingApproval 任务 → queued + 入队调度
            let task_ids: Vec<String> = state
                .tasks
                .values()
                .filter(|t| {
                    t.state == TaskState::WaitingApproval
                        && t.batch_id.as_deref() == Some(batch_id)
                })
                .map(|t| t.id.clone())
                .collect();
            for id in &task_ids {
                if let Some(task) = state.tasks.get_mut(id) {
                    let _ = task.transition(TaskState::Queued);
                    state.queue.enqueue(id);
                }
            }
            if !task_ids.is_empty() {
                state.tasks.save(host);
                emit_tasks_changed(host, &state.tasks);
            }
        }
        "rejected" => {
            let reason_owned = reason.to_string();
            if let Some(rec) = state.batches.get_mut(batch_id) {
                rec.state = BatchRecordState::Rejected {
                    reason: reason_owned.clone(),
                };
            }
            host.log_info(&format!(
                "transfer approval: batch {} rejected (reason={})",
                batch_id, reason
            ));
            // 批内 WaitingApproval 任务 → rejected（reason 映射：user-rejected / timeout）
            let task_ids: Vec<String> = state
                .tasks
                .values()
                .filter(|t| {
                    t.state == TaskState::WaitingApproval
                        && t.batch_id.as_deref() == Some(batch_id)
                })
                .map(|t| t.id.clone())
                .collect();
            for id in &task_ids {
                if let Some(task) = state.tasks.get_mut(&id) {
                    task.state = TaskState::Rejected;
                    task.reason = Some(reason_owned.clone());
                }
                archive_task_if_terminal(state, host, &id);
            }
            if !task_ids.is_empty() {
                state.tasks.save(host);
                emit_tasks_changed(host, &state.tasks);
            }
        }
        _ => {
            host.log_warn(&format!(
                "transfer approval: unknown decision '{}' for batch {}",
                decision, batch_id
            ));
        }
    }
}

// ==================== v2 接收端/历史命令 ====================

/// list-batches：pending 批快照（前端应答卡数据源）
pub fn list_batches(state: &PluginState) -> serde_json::Value {
    let mut list: Vec<&PendingBatch> = state.pending_batches.values().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    serde_json::to_value(list).unwrap_or(serde_json::Value::Array(vec![]))
}

/// approve-batch：批准传输批（接收端应答「接受全部」）
pub fn approve_batch(
    _state: &mut PluginState,
    host: &(impl HostFileService + HostLog),
    batch_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_approve_transfer(batch_id)
        .map_err(|e| anyhow::anyhow!("approve-batch failed: {}", e))?;
    host.log_info(&format!("approve-batch: {} approved", batch_id));
    Ok(serde_json::json!({ "ok": true }))
}

/// reject-batch：拒绝传输批（接收端应答「拒绝全部」）
pub fn reject_batch(
    _state: &mut PluginState,
    host: &(impl HostFileService + HostLog),
    batch_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_reject_transfer(batch_id)
        .map_err(|e| anyhow::anyhow!("reject-batch failed: {}", e))?;
    host.log_info(&format!("reject-batch: {} rejected", batch_id));
    Ok(serde_json::json!({ "ok": true }))
}

/// list-receiving：接收中任务快照
pub fn list_receiving(state: &PluginState) -> serde_json::Value {
    let mut list: Vec<&ReceivingTask> = state.receiving.values().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    serde_json::to_value(list).unwrap_or(serde_json::Value::Array(vec![]))
}

/// cancel-receiving：取消接收中的上传会话（接收端本地取消，session 级）
pub fn cancel_receiving(
    _state: &mut PluginState,
    host: &(impl HostFileService + HostLog),
    session_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_cancel_receiving(session_id)
        .map_err(|e| anyhow::anyhow!("cancel-receiving failed: {}", e))?;
    host.log_info(&format!("cancel-receiving: {} cancelled", session_id));
    Ok(serde_json::json!({ "ok": true }))
}

/// list-history：传输历史快照（终态归档，最新在前）
pub fn list_history(state: &PluginState) -> serde_json::Value {
    serde_json::to_value(state.history.snapshot()).unwrap_or(serde_json::Value::Array(vec![]))
}

/// clear-history：清空传输历史
pub fn clear_history(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog),
) -> anyhow::Result<serde_json::Value> {
    state.history.clear(host);
    emit_history_changed(host, state);
    Ok(serde_json::json!({ "ok": true }))
}

// ==================== v2 事件推送 helper ====================

/// 推送 pending 批快照事件
fn emit_batches_changed(host: &(impl HostEvents + HostLog), state: &PluginState) {
    let snapshot = list_batches(state);
    host.emit_event("plugin:file-transfer:batches-changed", &snapshot);
}

/// 推送接收任务快照事件
fn emit_receiving_changed(host: &(impl HostEvents + HostLog), state: &PluginState) {
    let snapshot = list_receiving(state);
    host.emit_event("plugin:file-transfer:receiving-changed", &snapshot);
}

/// 推送历史快照事件
fn emit_history_changed(host: &(impl HostEvents + HostLog), state: &PluginState) {
    let snapshot = list_history(state);
    host.emit_event("plugin:file-transfer:history-changed", &snapshot);
}

/// 激活对端设备名（toast 展示用；插件侧无设备名缓存，
/// 回退到绑定该对端的任务名，再回退空串由前端兜底）
fn active_peer_name(state: &PluginState) -> String {
    let Some(active_id) = state.peer.active_id() else {
        return String::new();
    };
    state
        .tasks
        .values()
        .find(|t| t.peer.device_id == active_id && !t.peer.name.is_empty())
        .map(|t| t.peer.name.clone())
        .unwrap_or_default()
}

/// 终态任务归档：插入历史并从当前任务列表移除（v2 终态不留在当前队列）
///
/// 幂等：非终态 / 已不在列表时静默跳过。接收任务经 receiving_done 归档，
/// 不走此路径（数据结构不同）
fn archive_task_if_terminal(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog + HostConfig),
    task_id: &str,
) {
    let Some(task) = state.tasks.get(task_id).cloned() else {
        return;
    };
    if !task.state.is_terminal() {
        return;
    }
    let file_name = task
        .remote_path
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    let local_path = if task.state == TaskState::Completed {
        // 下载方向 local_path 为 .part 临时名，文件完成后已 rename 到最终路径
        // （去后缀，只剥一次防 `x.part.part` 错位）；上传方向为源文件路径无需
        // 处理。历史条目保存最终路径，使「打开所在文件夹」可直接定位
        Some(
            task.local_path
                .strip_suffix(".part")
                .unwrap_or(&task.local_path)
                .to_string(),
        )
    } else {
        None
    };
    let entry = HistoryEntry {
        id: task.id.clone(),
        direction: task.direction,
        initiator: task.initiator.clone(),
        file_name,
        size: task.size,
        state: task.state,
        reason: task.reason.clone(),
        peer_name: task.peer.name.clone(),
        local_path,
        created_at: task.created_at,
        updated_at: now_ms(host),
    };
    state.history.insert(host, entry);
    emit_history_changed(host, state);
    // 终态任务保留在 TaskStore（当次会话可见，供 retry/remove），与移动端一致；
    // 历史为只读归档副本（跨重启可见，spec 14.5「终态即归档替代重启清除」）
}

/// list-peers：返回在线对端列表与激活对端（前端设备列表/切换数据源）
pub fn list_peers(state: &PluginState) -> serde_json::Value {
    peers_snapshot(&state.peer)
}

/// set-active-peer：切换激活对端（前端设备列表点击调用）
///
/// 对端必须在线（列表来自 list-peers）；切换后推送 peers-changed，
/// 前端据此重载目录。传输中任务不受影响（启动时已捕获 endpoint）。
pub fn set_active_peer(
    state: &mut PluginState,
    host: &(impl HostFileService + HostEvents + HostLog),
    peer_id: &str,
) -> anyhow::Result<serde_json::Value> {
    state
        .peer
        .set_active(host, peer_id)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    if let Some(ep) = state.peer.active() {
        host.log_info(&format!(
            "set-active-peer: peer_id={} ip={} port={} token_len={} mounts={}",
            peer_id, ep.ip, ep.port, ep.token.len(), ep.mounts.len()
        ));
    }
    emit_peers_changed(host, &state.peer);
    Ok(serde_json::json!({ "ok": true, "activePeerId": peer_id }))
}

/// 对端列表快照（list-peers 命令 / peers-changed 事件共用载荷）
pub fn peers_snapshot(peer: &PeerStore) -> serde_json::Value {
    let peers: Vec<serde_json::Value> = peer
        .peers()
        .into_iter()
        .map(|id| serde_json::json!({ "peerId": id }))
        .collect();
    serde_json::json!({
        "peers": peers,
        "activePeerId": peer.active_id(),
    })
}

/// 推送对端列表变更事件（列表/激活变化时调用）
fn emit_peers_changed(host: &(impl HostEvents + HostLog), peer: &PeerStore) {
    let snapshot = peers_snapshot(peer);
    host.emit_event("plugin:file-transfer:peers-changed", &snapshot);
}

// ==================== 上传钩子 ====================

/// 上传请求策略钩子（on_upload_request）
///
/// 解析 meta.relativePath 到 roots 下的绝对路径，对每个 root 拼出目标绝对路径，
/// 用 host.fs_exists 检查目标是否已存在（wasm 环境 std::fs 全部 stub false，不可用）。
/// 宿主沙箱已在上传创建前完成路径合法性校验，插件只需同名即拒。
///
/// v2：ask 策略下无批上下文的上传一律返回 ask（宿主 → 403 batch-context-required，
/// 防绕过 /upload）；accept 策略走 v1 同名即拒；reject 策略直接 deny(policy-denied)
pub fn handle_upload_request(
    state: &PluginState,
    host: &(impl HostFs + HostConfig),
    meta: &UploadRequestMeta,
) -> UploadHookDecision {
    match state.settings.receiving_policy.as_str() {
        "accept" => handle_upload_request_accept(state, host, meta),
        "reject" => UploadHookDecision::deny("policy-denied"),
        // ask（默认）：无批上下文的上传必须经批批准（spec 14.2 防绕过）
        _ => UploadHookDecision::ask(),
    }
}

/// accept 策略：v1 语义（同名即拒，其余放行）
fn handle_upload_request_accept(
    state: &PluginState,
    host: &(impl HostFs + HostConfig),
    meta: &UploadRequestMeta,
) -> UploadHookDecision {
    let rel = meta.relative_path.trim_matches('/');

    // 清洗相对路径（复刻 sandbox::clean_relative_parts，拒绝 ..、绝对路径、:）
    let parts = match clean_relative_parts(rel) {
        Ok(p) => p,
        Err(_) => return UploadHookDecision::deny("invalid-path"),
    };
    if parts.is_empty() {
        return UploadHookDecision::deny("invalid-path");
    }

    // 接收落点固定为下载目录（与 enqueue_download 同源），与移动端 MediaStore.Downloads
    // 对称——spec §8.5 方向模型：接收统一落下载目录，不落共享 roots。下载目录未配置
    // 则拒绝，避免落到宿主默认或不可预期位置。
    let download_dir = match resolve_download_dir(state, host) {
        Ok(d) => d,
        Err(e) => return UploadHookDecision::deny(&format!("no-download-dir: {}", e)),
    };
    let target = join_download_path(&download_dir, &parts.join("/"));
    if let Ok(true) = host.fs_exists(&target) {
        return UploadHookDecision::deny("duplicate-name");
    }

    UploadHookDecision::allow()
}

/// v2 批量传输请求钩子（on_transfer_request）：按接收策略分流（spec §9.1）
///
/// accept → allow（直接放行）；reject → deny("policy-denied")（零打扰）；
/// ask（默认）→ ask（批置 pending 等待用户应答）。
/// 钩子函数不可异步：策略是同步读 settings，无 IO，满足
pub fn handle_transfer_request_hook(state: &PluginState) -> UploadHookDecision {
    match state.settings.receiving_policy.as_str() {
        "accept" => UploadHookDecision::allow(),
        "reject" => UploadHookDecision::deny("policy-denied"),
        _ => UploadHookDecision::ask(),
    }
}

/// 清洗相对路径为安全分量列表（复刻 sandbox::clean_relative_parts）
fn clean_relative_parts(rel: &str) -> Result<Vec<String>, ()> {
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err(());
    }
    let mut parts = Vec::new();
    for part in rel.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(());
        }
        if part.contains(':') {
            return Err(());
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}

// ==================== 辅助函数 ====================

/// 构造新任务
fn make_task(
    direction: Direction,
    peer_id: &str,
    peer_name: &str,
    remote_path: &str,
    local_path: &str,
    size: u64,
    now: u64,
) -> Task {
    Task {
        id: generate_id(now),
        direction,
        peer: PeerInfo {
            device_id: peer_id.to_string(),
            name: peer_name.to_string(),
        },
        remote_path: remote_path.to_string(),
        local_path: local_path.to_string(),
        size,
        offset: 0,
        upload_session_id: None,
        fingerprint: None,
        state: TaskState::Queued,
        reason: None,
        created_at: now,
        updated_at: now,
        batch_id: None,
        initiator: "me".to_string(),
        host_task_id: None,
        auto_resumable: false,
        last_flush: 0,
    }
}

/// 生成唯一任务 ID
///
/// wasm32-unknown-unknown 无系统时钟/随机源（SystemTime::now() 会 panic），
/// 时间戳由宿主提供（now_ms），单调计数器保证同毫秒内不冲突；
/// 宿主时间不可用（now=0）时仅计数器兜底，仍保持进程内唯一。
fn generate_id(now: u64) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("ft-{:x}-{:x}", now, n)
}

/// 获取宿主当前时间（Unix 毫秒）；宿主不可用/解析失败降级为 0
///
/// wasm32-unknown-unknown 无系统时钟（SystemTime::now()/Instant::now() 均 panic
/// 触发 unreachable trap——移动端 aarch64 上 SIGILL 未被 wasmtime trap handler
/// 捕获会直接闪退），插件一律经 host.config_get(ConfigKey::CurrentTimeMs) 取时间。
fn now_ms(host: &impl HostConfig) -> u64 {
    host.config_get(ConfigKey::CurrentTimeMs)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// 归一化 Windows 目录路径（wasm 侧路径拼接专用）
///
/// wasm32-unknown-unknown 的 `std::path::PathBuf` 是 POSIX 语义
/// （MAIN_SEP='/'）：`Path::new(r"C:\Users\x").join("Downloads")` 会把
/// `C:\Users\x` 视为单组件并在其后追加 `/`，产出 `C:\Users\x/Downloads`
/// 混合分隔符路径。宿主 Windows fs API（CreateFileW 等）接受正斜杠，但
/// pickDirectory 返回的 `\\?\D:\下载` verbatim 前缀 + 混合分隔符会直接
/// 失败（os error 123 语法不正确）。统一剥 verbatim 前缀并把反斜杠转
/// 正斜杠，产出宿主全 API（fs_exists / rename / canonicalize）接受的
/// 纯正斜杠绝对路径；宿主侧 canonicalize 会还原为原生分隔符。
fn normalize_win_dir(dir: &str) -> String {
    dir.strip_prefix(r"\\?\")
        .unwrap_or(dir)
        .replace('\\', "/")
}

/// 拼接下载路径（目录 + 文件名/相对分量），规避 wasm32 POSIX PathBuf 陷阱
///
/// 目录尾部分隔符（`\` 或 `/`）先剥除再拼接，避免 `dir/name` 双斜杠；
/// 产出纯正斜杠路径，见 [`normalize_win_dir`]。
fn join_download_path(dir: &str, name: &str) -> String {
    format!("{}/{}", normalize_win_dir(dir).trim_end_matches('/'), name)
}

/// 解析下载目录
///
/// 两个用法：下载任务 `.part` 落点（[`enqueue_download`]）；以及文件服务挂载的
/// 接收落点（[`build_mount_options`] → `MountOptions.downloads_dir`，使对端 upload
/// 接收按 spec“下载目录 = 接收落点”语义落在下载目录而非共享 roots）。
pub fn resolve_download_dir(
    state: &PluginState,
    host: &impl HostConfig,
) -> anyhow::Result<String> {
    // 优先使用 settings 中的 downloadDir
    if !state.settings.download_dir.is_empty() {
        return Ok(state.settings.download_dir.clone());
    }

    // 桌面端：尝试 HostConfig::HomeDir + /Downloads（join_download_path 统一
    // 正斜杠，理由见其文档；宿主 canonicalize 时还原原生分隔符）
    if let Ok(Some(home)) = host.config_get(bedcode_plugin_api::host::ConfigKey::HomeDir) {
        return Ok(join_download_path(&home, "Downloads"));
    }

    Err(anyhow::anyhow!(
        "download directory not configured; use set-settings to set downloadDir"
    ))
}

/// 构造文件服务挂载选项
///
/// 三处调用点（activate / set-settings / mount-local）共用。`downloads_dir`
/// 为 `resolve_download_dir` 结果（失败则为 None：接收 upload 落点回退到共享
/// roots 语义，不中断挂载），与供对端浏览的共享 roots 分离，对齐 spec“下载
/// 目录 = 接收落点”方向模型。
pub fn build_mount_options(roots: &[String], downloads_dir: &Option<String>) -> MountOptions {
    MountOptions {
        mount_path: MOUNT_PATH.to_string(),
        roots: roots.to_vec(),
        operations: vec![
            FileOperation::List,
            FileOperation::Download,
            FileOperation::Upload,
        ],
        downloads_dir: downloads_dir.clone().filter(|s| !s.is_empty()),
    }
}

/// 构造 Authorization headers
fn auth_headers(auth: &str) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    if !auth.is_empty() {
        headers.insert("Authorization".to_string(), format!("Bearer {}", auth));
    }
    headers
}

/// URL 编码（最小实现）
fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
        .replace('=', "%3D")
}

/// 删除 .part 临时文件（幂等）
///
/// 桌面端 SDK HostFs 提供 fs_delete，经宿主沙箱删除；
/// 幂等场景文件可能已不存在，失败记 debug 日志即可。
fn delete_part_file(host: &(impl HostFs + HostLog), path: &str) {
    if let Err(e) = host.fs_delete(path) {
        // 幂等场景文件可能已不存在，debug 级即可
        host.log_debug(&format!("delete .part {} failed (ignored): {}", path, e));
    }
}

/// 保存设置到 storage
fn save_settings(host: &impl HostStorage, settings: &Settings) {
    if let Ok(json) = serde_json::to_value(settings) {
        let _ = host.storage_set(SETTINGS_KEY, &json);
    }
}

/// 同步批准超时到宿主（v2，仅已挂载时有效；失败仅告警不阻塞设置保存）
///
/// 宿主以 per-(plugin, mount) 配置驱动 pending 批 TTL 扫描；插件侧每次
/// 设置变化/挂载后调用，保证两侧配置一致
pub fn sync_approval_timeout(state: &PluginState, host: &(impl HostFileService + HostLog)) {
    if !state.mounted {
        return;
    }
    if let Err(e) = host.filesrv_set_approval_timeout(
        MOUNT_PATH,
        state.settings.approval_timeout_sec,
    ) {
        host.log_warn(&format!(
            "sync_approval_timeout FAILED (will retry on next set/mount): {}",
            e
        ));
    }
}

/// 加载设置从 storage
pub fn load_settings(host: &impl HostStorage) -> Settings {
    match host.storage_get(SETTINGS_KEY) {
        Ok(Some(value)) => serde_json::from_value(value).unwrap_or_default(),
        _ => Settings::default(),
    }
}

/// 向前端发射任务变更事件
fn emit_tasks_changed(host: &(impl HostEvents + HostLog), tasks: &TaskStore) {
    let snapshot = serde_json::to_value(tasks.snapshot()).unwrap_or(serde_json::Value::Array(vec![]));
    host.emit_event("plugin:file-transfer:tasks-changed", &snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_win_dir_strips_verbatim_prefix_and_backslashes() {
        // 对话框返回的 verbatim 前缀路径（logs 中 roots 形态）
        assert_eq!(normalize_win_dir(r"\\?\D:\小说"), "D:/小说");
        // 无前缀原生反斜杠
        assert_eq!(normalize_win_dir(r"C:\Users\x\Downloads"), "C:/Users/x/Downloads");
        // 已是正斜杠
        assert_eq!(normalize_win_dir("C:/Users/x/Downloads"), "C:/Users/x/Downloads");
    }

    #[test]
    fn join_download_path_never_emits_mixed_or_double_separators() {
        // 反斜杠目录 + 文件名 → 纯正斜杠（无 `\\?\`，宿主全 API 可接受）
        assert_eq!(
            join_download_path(r"C:\Users\x\Downloads", "a.mkv.part"),
            "C:/Users/x/Downloads/a.mkv.part"
        );
        // verbatim 前缀剥除
        assert_eq!(
            join_download_path(r"\\?\D:\下载", "a.mkv.part"),
            "D:/下载/a.mkv.part"
        );
        // 目录尾部分隔符不产生双斜杠
        assert_eq!(join_download_path("D:/下载/", "a.mkv"), "D:/下载/a.mkv");
        assert_eq!(join_download_path(r"D:\下载\", "a.mkv"), "D:/下载/a.mkv");
        // 多级相对分量
        assert_eq!(join_download_path(r"C:\Users\x", "小说/第一章.txt"), "C:/Users/x/小说/第一章.txt");
    }

    #[test]
    fn final_path_strips_part_once_only() {
        // 与前端 openInDir / remove_task 对齐：只剥一次后缀
        let local_path = join_download_path(r"C:\Users\x\Downloads", "movie.part.part");
        let final_path = local_path
            .strip_suffix(".part")
            .unwrap_or(&local_path)
            .to_string();
        assert_eq!(final_path, "C:/Users/x/Downloads/movie.part");
        // 普通文件名剥一次
        let local_path = join_download_path(r"C:\Users\x\Downloads", "movie.mkv.part");
        let final_path = local_path
            .strip_suffix(".part")
            .unwrap_or(&local_path)
            .to_string();
        assert_eq!(final_path, "C:/Users/x/Downloads/movie.mkv");
    }
}
