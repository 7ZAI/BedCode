//! 命令处理
//!
//! 16 个命令的实现（plugin.json 声明），由 lib.rs invoke_command 路由。
//! 每个命令接收 PluginState 引用和参数 JSON，返回结果 JSON。
//!
//! 宿主调用（transfer_start 等）在释放状态锁后执行，
//! 避免 on_bus_message 回调死锁。

use crate::handshake::{self, CompleteSessionError, CreateSessionError, QuerySessionError};
use crate::peer::{PeerStore, MOUNT_PATH};
use crate::queue::{Queue, DEFAULT_CONCURRENCY};
use crate::shared::{self, SharedRoot};
use crate::state::{
    Direction, Fingerprint, HistoryEntry, HistoryStore, PeerInfo, Task, TaskState, TaskStore,
};
use bedcode_plugin_api_mobile::host::{
    ConfigKey, HostBus, HostConfig, HostEvents, HostFileService, HostFs, HostHttp, HostLog,
    HostStorage, HostTransfer,
};
use bedcode_plugin_api_mobile::types::{
    FileOperation, MountOptions, TransferDirection, TransferProgress, TransferRequest,
    TransferState, UploadHookDecision, UploadRequestMeta,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 设置 storage key
const SETTINGS_KEY: &str = "file-transfer-settings";

/// SAF pipe 流 not-seekable-resume 重建上限（超过视为异常置失败，
/// 防止宿主异常持续回报时无限重建循环）
const MAX_RESUME_RETRIES: u32 = 2;

/// 插件设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// 共享目录条目（SAF URI 存储；私有下载目录免授权特殊条目不入库，读取时派生）
    /// 字段级容错：旧格式字符串数组解析失败时返回空，不拖垮整个 Settings
    #[serde(default, deserialize_with = "crate::shared::deserialize_roots")]
    pub roots: Vec<SharedRoot>,
    /// 下载目录（绝对路径，移动端必须配置）
    #[serde(default)]
    pub download_dir: String,
    /// 并发数（1..=8）
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// v2 接收策略：ask（默认，每次询问）| accept（直接接收）| reject（直接拒绝）
    /// 接收端本地生效、发送方不感知（发送方一律发请求，接收端钩子分流）
    #[serde(default = "default_receiving_policy")]
    pub receiving_policy: String,
    /// v2 同意超时秒（10–600，仅 ask 策略生效，默认 60；宿主 TTL 扫描用）
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_sec: u64,
}

fn default_concurrency() -> usize {
    DEFAULT_CONCURRENCY
}

fn default_receiving_policy() -> String {
    "ask".to_string()
}

fn default_approval_timeout() -> u64 {
    60
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            download_dir: String::new(),
            concurrency: DEFAULT_CONCURRENCY,
            receiving_policy: default_receiving_policy(),
            approval_timeout_sec: default_approval_timeout(),
        }
    }
}

/// 接收策略取值（wire 常量）
pub const POLICY_ASK: &str = "ask";
pub const POLICY_ACCEPT: &str = "accept";
pub const POLICY_REJECT: &str = "reject";

/// 发送方批记录状态（内存态，不持久化；批上下文不可跨重启恢复）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchRecordState {
    /// 已发起 POST /transfer-request 且接收端 ask：等待应答
    Pending,
    /// 已批准（批内任务可调度；session 创建带批 ID 免钩子）
    Approved,
    /// 已拒绝/策略拒绝（批内任务终态；retry 会清批上下文重新询问）
    Rejected { reason: String },
}

/// 发送方批记录（v2，PluginState.batches；不持久化）
#[derive(Debug, Clone)]
pub struct BatchRecord {
    /// 批 ID
    pub batch_id: String,
    /// 对端 ID（批请求发往的对端）
    pub peer_id: String,
    /// 当前状态
    pub state: BatchRecordState,
}

/// 接收端 pending 批（v2 应答卡数据源；内存态，不跨重启持久化）
#[derive(Debug, Clone)]
pub struct PendingBatch {
    /// 批 ID
    pub batch_id: String,
    /// 对端 ID（= 激活对端）
    pub peer_id: String,
    /// 批内文件清单
    pub files: Vec<bedcode_plugin_api_mobile::UploadRequestMeta>,
    /// 批内文件总大小
    pub total_size: u64,
    /// 创建时间（Unix 毫秒）
    pub created_at: u64,
}

/// 接收中任务（v2「正在接收」tab；仅 session 级取消，无暂停/恢复；不持久化）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivingTask {
    /// 宿主上传 session ID
    pub session_id: String,
    /// 所属批 ID（无批 = accept 模式 per-file）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// 远端相对路径（= 目标文件名）
    pub remote_path: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 状态（transferring/completed/failed/rejected/cancelled）
    pub state: String,
    /// 终态原因（如 duplicate-name）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 对端 ID
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
    /// v2 发送方批记录（batch_id → 批；内存态，不持久化）
    pub batches: HashMap<String, BatchRecord>,
    /// v2 接收端 pending 批（应答卡数据源；内存态，不持久化）
    pub pending_batches: Vec<PendingBatch>,
    /// v2 接收中任务（「正在接收」tab；内存态，不持久化）
    pub receiving_tasks: HashMap<String, ReceivingTask>,
    /// v2 传输历史（持久化，封顶 200 条）
    pub history: HistoryStore,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            tasks: TaskStore::new(),
            queue: Queue::new(DEFAULT_CONCURRENCY),
            settings: Settings::default(),
            // 移动插件对端恒为桌面端（activate 时按平台值重建，此处仅防呆）
            peer: PeerStore::new(true),
            mounted: false,
            batches: HashMap::new(),
            pending_batches: Vec::new(),
            receiving_tasks: HashMap::new(),
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
/// 经宿主 WS 控制面发送 Query；对端回复 Announce/Withdraw 后宿主注册表
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

    match direction {
        "download" => enqueue_download(state, host, remote_path, peer_id, peer_name, args),
        "upload" => enqueue_upload(state, host, remote_path, peer_id, peer_name, args),
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
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    // 确定下载目录
    let download_dir = resolve_download_dir(state, host)?;

    // 文件名
    let file_name = remote_path
        .rsplit('/')
        .next()
        .unwrap_or(remote_path);

    // 「保存到…」（M3）：下载完成后弹系统保存对话框（用户选位置）。中转文件
    // 名唯一化（前缀 .save-），避免与默认下载的同名文件冲突——保存到…的
    // 同名语义由系统对话框在用户选位置时裁决，不走 duplicate-name 预检
    let save_to = args
        .get("saveTo")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let local_path = if save_to {
        format!(
            "{}/.save-{}-{}.part",
            download_dir,
            now_ms(host),
            file_name
        )
    } else {
        format!("{}/{}.part", download_dir, file_name)
    };
    let final_path = local_path.trim_end_matches(".part").to_string();

    // 目标存在性预检（spec §7.4：目标已存在 → rejected duplicate-name）；
    // 保存到…的中转名唯一化天然不冲突，跳过预检
    if !save_to {
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
            state.tasks.insert(task);
            state.tasks.save(host);
            emit_tasks_changed(host, &state.tasks);
            return Ok(task_json);
        }
    }

    let mut task = make_task(
        Direction::Download,
        peer_id,
        peer_name,
        remote_path,
        &local_path,
        0,
        now_ms(host),
    );
    task.save_to = save_to;
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
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let local_path = args
        .get("localPath")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing localPath for upload"))?;
    // 中转复制（Relay Copy）cache 副本标记：上传完成后删除本地源文件
    // （SAF 共享目录 → cache 链路的副本生命周期，见 spec「复制桥语义」）
    let cleanup_local = args
        .get("cleanupLocal")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // 声明的文件大小（前端 SAF 条目元信息；批请求 totalSize 与进度展示用；0 = 未知）
    let size = args.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

    // 本地文件必须存在。SAF 流直传源（content://）无法经真实路径 fs_exists
    // 校验（分区存储 FUSE 过滤/云盘 provider 无路径），存在性由宿主 Kotlin
    // safOpen 打开时验证；仅真实路径源做预检
    if !local_path.starts_with("content://") {
        if let Ok(false) = host.fs_exists(local_path) {
            return Err(anyhow::anyhow!("local file not found: {}", local_path));
        }
    }

    let mut task = make_task(
        Direction::Upload,
        peer_id,
        peer_name,
        remote_path,
        local_path,
        size,
        now_ms(host),
    );
    // insert 前直接落标记，避免 insert → get_mut → 改 → 再 save 的迂回
    task.cleanup_local = cleanup_local;
    // v2 批上下文：上传恒走批流（发送方一律发请求，接收端钩子分流）；
    // 前端一次「发送」动作传同一 batchId，未传时自动生成（每任务一批）
    let batch_id = args
        .get("batchId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| generate_batch_id(now_ms(host)));
    task.batch_id = Some(batch_id);
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
    let task = state.tasks.get(task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
    if !matches!(task.state, TaskState::Paused | TaskState::Resumable) {
        return Err(anyhow::anyhow!("task not paused/resumable: {}", task_id));
    }
    state.tasks.get_mut(task_id)
        .unwrap()
        .transition(TaskState::Queued)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    // 手动恢复 = 全新机会：重置 pipe 流重建计数
    state.tasks.get_mut(task_id).unwrap().resume_retries = 0;
    state.queue.enqueue(task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
    Ok(serde_json::json!({"ok": true}))
}

/// cancel：取消任务
pub fn cancel(
    state: &mut PluginState,
    host: &(impl HostTransfer + HostFs + HostHttp + HostStorage + HostEvents + HostLog),
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

    // 下载：删除 .part 文件（移动端 HostFs 无 fs_delete，跳过）
    if direction == Direction::Download {
        delete_part_file(host, &local_path);
    }

    // 本地终态先落地并推送：UI 即时响应取消，不依赖对端可达性。
    // 远端 cancel_session 为同步 HTTP（宿主代理总超时 120s，对端失联时
    // 仍会阻塞较久），若在其后 emit，WASM 单线程被阻塞，前端表现为
    // 「取消无反应」
    state.queue.release(task_id);
    state.queue.remove(task_id);
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
    let (direction, reason, local_path) = {
        let task = state.tasks.get_mut(task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", task_id))?;
        if task.state != TaskState::Failed && task.state != TaskState::Rejected {
            return Err(anyhow::anyhow!("task not failed/rejected: {}", task_id));
        }
        (
            task.direction,
            task.reason.clone(),
            task.local_path.clone(),
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
    // v2 拒绝重试：approval 相关拒绝（user-rejected/timeout/policy-denied）重置批上下文，
    // 重新入队后按新批重新发起 transfer-request（即"重新询问"）；duplicate-name 保留
    // 批上下文（批已批准，重试免问直接传）
    if direction == Direction::Upload && task.batch_id.is_some() {
        if matches!(
            reason.as_deref(),
            Some("user-rejected" | "timeout" | "policy-denied")
        ) {
            if let Some(bid) = task.batch_id.clone() {
                state.batches.remove(&bid);
                // 新批上下文：旧批已终态（拒绝），复用同一批 ID 会命中 Rejected 记录；
                // 换新批 ID 使下次调度重新发起 transfer-request（重新询问）
                task.batch_id = Some(generate_batch_id(now_ms(host)));
                host.log_info(&format!(
                    "retry: reset batch context for task {} (approval rejected, re-ask on next start)",
                    task_id
                ));
            }
        }
    }
    task.transition(TaskState::Queued)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    task.reason = None;
    task.offset = 0;
    task.host_task_id = None;
    task.upload_session_id = None;
    // 手动重试 = 全新机会：重置 pipe 流重建计数
    task.resume_retries = 0;
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
///
/// roots 注入免授权特殊条目（app 私有下载目录）：不入库、读取时派生，
/// 始终置顶展示（特殊条目不可移除，前端按 kind 区分）。
/// download_dir 仅在未显式配置时用宿主默认值填充展示（保留既有语义：
/// 不覆盖存储值——前端 persist 会全量回传设置，若此处无条件覆写，首次
/// 保存即把存储中的自定义 download_dir 永久改写为 AppDownloadsDir）。
pub fn get_settings(state: &PluginState, host: &impl HostConfig) -> serde_json::Value {
    let mut settings = state.settings.clone();
    if let Ok(Some(dir)) = host
        .config_get(bedcode_plugin_api_mobile::host::ConfigKey::AppDownloadsDir)
    {
        // 派生免授权特殊条目（已有同 id 条目时不重复插入；与 download_dir
        // 填充独立——特殊条目恒以宿主配置为基址，不受存储值影响）
        let special = SharedRoot {
            id: dir.clone(),
            kind: shared::KIND_PRIVATE_DOWNLOADS.to_string(),
            name: std::path::Path::new(&dir)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Download".to_string()),
            document_id: String::new(),
            authorized: true,
        };
        if !shared::contains(&settings.roots, &special.id) {
            settings.roots.insert(0, special);
        }
        if settings.download_dir.is_empty() {
            settings.download_dir = dir;
        }
    }
    serde_json::to_value(&settings).unwrap_or_default()
}

/// set-settings：更新设置
///
/// roots 为结构化条目数组（SharedRoot）；入库时剔除免授权特殊条目
/// （派生数据不入库，避免与宿主配置漂移）。挂载含全部条目——SAF 树条目
/// 与真实路径条目均可挂载（M2：file_service 三端点已 SAF 化，宿主按
/// content:// 前缀分流）。
pub fn set_settings(
    state: &mut PluginState,
    host: &(impl HostStorage + HostLog + HostFileService + HostConfig),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    if let Some(roots) = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<SharedRoot>>(v.clone()).ok())
    {
        // 入库前剔除特殊条目（派生数据）+ 同 id 去重（防前端回传重复）
        let mut stored: Vec<SharedRoot> = Vec::new();
        for root in roots {
            if root.kind == shared::KIND_PRIVATE_DOWNLOADS {
                continue;
            }
            if !shared::contains(&stored, &root.id) {
                stored.push(root);
            }
        }
        state.settings.roots = stored;
        sync_mount(state, host)?;
    }
    if let Some(dir) = args.get("downloadDir").and_then(|v| v.as_str()) {
        state.settings.download_dir = dir.to_string();
    }
    if let Some(n) = args.get("concurrency").and_then(|v| v.as_u64()) {
        state.queue.set_concurrency(n as usize);
        state.settings.concurrency = state.queue.concurrency();
    }
    // v2 接收策略（ask/accept/reject；非法值忽略保持原值）
    if let Some(policy) = args.get("receivingPolicy").and_then(|v| v.as_str()) {
        if matches!(policy, POLICY_ASK | POLICY_ACCEPT | POLICY_REJECT) {
            state.settings.receiving_policy = policy.to_string();
        }
    }
    // v2 同意超时秒（前端限制 10–600；此处 clamp 防存储脏值）
    if let Some(secs) = args.get("approvalTimeoutSec").and_then(|v| v.as_u64()) {
        state.settings.approval_timeout_sec = secs.clamp(10, 600);
    }
    // 策略或超时变化（且已挂载）→ 同步宿主 per-mount 批准超时（宿主校验 10–600）
    if state.mounted {
        let _ = host.filesrv_set_approval_timeout(
            MOUNT_PATH,
            state.settings.approval_timeout_sec,
        );
    }
    save_settings(host, &state.settings);
    Ok(serde_json::json!({"ok": true}))
}

/// 计算生效挂载根：存储可挂载（真实路径）条目 + 免授权特殊条目去重
///
/// 特殊条目（app 私有下载目录）始终可挂载（story #11：免授权即可共享该
/// 目录）。activate 与 sync_mount 必须共用同一推导：否则 set-settings 后
/// 存储条目全为 SAF（不可挂载）时 sync_mount 会算出空根直接卸载整个
/// 文件服务，桌面端连免授权特殊条目也看不到了（挂载状态漂移）。
pub fn effective_mount_roots(state: &PluginState, host: &impl HostConfig) -> Vec<String> {
    let mut roots = shared::mountable_paths(&state.settings.roots);
    if let Ok(Some(dir)) = host.config_get(bedcode_plugin_api_mobile::host::ConfigKey::AppDownloadsDir)
    {
        if !roots.contains(&dir) {
            roots.push(dir);
        }
    }
    roots
}

/// 同步挂载状态：可挂载（真实路径）条目非空 → 挂载/更新；为空 → 卸载
///
/// 清空全部可挂载共享目录 = 停止共享：卸载挂载（宿主拒绝空 roots 挂载）。
fn sync_mount(
    state: &mut PluginState,
    host: &(impl HostStorage + HostLog + HostFileService + HostConfig),
) -> anyhow::Result<()> {
    let mount_roots = effective_mount_roots(state, host);
    if mount_roots.is_empty() {
        if state.mounted {
            let _ = host.filesrv_unmount(MOUNT_PATH);
            state.mounted = false;
            host.log_info("no mountable shared roots, file service unmounted");
        }
        return Ok(());
    }
    if state.mounted {
        let _ = host.filesrv_update_roots(MOUNT_PATH, &mount_roots);
    } else {
        // 之前未挂载（如清空后重配目录）：与激活逻辑一致重新挂载
        let options = MountOptions {
            mount_path: MOUNT_PATH.to_string(),
            roots: mount_roots,
            operations: vec![FileOperation::List, FileOperation::Download, FileOperation::Upload],
        };
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
    Ok(())
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
///
/// roots 参数为结构化条目数组（SharedRoot，与 set-settings 同格式）；
/// 挂载含全部条目（SAF 树条目与真实路径条目均可挂载，宿主按前缀分流）。
pub fn mount_local(
    state: &mut PluginState,
    host: &(impl HostFileService + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let roots: Vec<SharedRoot> = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<SharedRoot>>(v.clone()).ok())
        .unwrap_or_else(|| state.settings.roots.clone());
    let mount_roots = shared::mountable_paths(&roots);

    if mount_roots.is_empty() {
        return Err(anyhow::anyhow!("no mountable roots"));
    }

    let options = MountOptions {
        mount_path: MOUNT_PATH.to_string(),
        roots: mount_roots,
        operations: vec![FileOperation::List, FileOperation::Download, FileOperation::Upload],
    };

    match host.filesrv_mount(&options) {
        Ok(result) => {
            state.mounted = true;
            state.settings.roots = roots;
            host.log_info(&format!("mounted at {}", result.base_path));
            Ok(serde_json::json!({"ok": true, "basePath": result.base_path}))
        }
        Err(e) => Err(anyhow::anyhow!("mount failed: {}", e)),
    }
}

/// update-roots：更新挂载根（结构化条目数组；SAF 树条目与真实路径条目均可挂载）
pub fn update_roots(
    state: &mut PluginState,
    host: &(impl HostFileService + HostStorage + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let roots: Vec<SharedRoot> = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<SharedRoot>>(v.clone()).ok())
        .ok_or_else(|| anyhow::anyhow!("missing roots"))?;
    let mount_roots = shared::mountable_paths(&roots);

    if state.mounted {
        if mount_roots.is_empty() {
            // 清空全部可挂载共享目录 = 停止共享：卸载挂载（宿主拒绝空 roots 挂载）
            host.filesrv_unmount(MOUNT_PATH)
                .map_err(|e| anyhow::anyhow!("unmount failed: {}", e))?;
            state.mounted = false;
        } else {
            host.filesrv_update_roots(MOUNT_PATH, &mount_roots)
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
    for task_id in actions {
        if let Err(e) = start_single_task(state, host, &task_id) {
            host.log_error(&format!("start task {} failed: {}", task_id, e));
            if let Some(task) = state.tasks.get_mut(&task_id) {
                task.state = TaskState::Failed;
                task.reason = Some(e);
            }
            state.queue.release(&task_id);
        }
    }
    if state.tasks.is_dirty() {
        state.tasks.save(host);
    }
}

/// 启动单个任务传输
fn start_single_task(
    state: &mut PluginState,
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostFileService + HostBus + HostEvents),
    task_id: &str,
) -> Result<(), String> {
    let task = state.tasks.get(task_id).ok_or("task not found")?;
    // 任务从入队起绑定对端：调度用任务自己的 endpoint，切换激活对端不影响排队任务
    let (base, auth) = state.peer.base_and_auth_for(&task.peer.device_id)?;
    let direction = task.direction;
    let remote_path = task.remote_path.clone();
    let local_path = task.local_path.clone();
    let offset = task.offset;
    let upload_session_id = task.upload_session_id.clone();
    let fingerprint = task.fingerprint.clone();

    match direction {
        Direction::Download => {
            // 续传指纹校验（spec §7.4）
            let remote_fp = handshake::fingerprint(host, &base, &auth, &remote_path)?;

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
            let final_path = task.local_path.trim_end_matches(".part").to_string();

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
            // v2 批 gating：任务带 batch_id 时先确保批上下文（批内首个任务发起
            // transfer-request；pending → waiting-approval 不入队启动；已拒绝 → 终态）
            let batch_id = task.batch_id.clone();
            if let Some(ref bid) = batch_id {
                match ensure_batch_ready(state, host, task_id, bid) {
                    Ok(true) => {
                        // 批已批准：继续建 session（宿主免钩子）
                    }
                    Ok(false) => {
                        // 批 pending：任务已转 waiting-approval，等待应答事件重新调度
                        return Ok(());
                    }
                    Err(e) => {
                        // 批已拒绝/网络失败：任务已置终态并归档
                        state.queue.release(task_id);
                        return Err(e);
                    }
                }
            }

            // 续传握手（spec §7.4）
            let (session_id, received) = if let Some(ref sid) = upload_session_id {
                match handshake::query_session(host, &base, &auth, sid) {
                    Ok(received) => (sid.clone(), received),
                    Err(QuerySessionError::SessionLost) => {
                        // session 丢失 → 重建从头传（带批 ID，免钩子续传）
                        let created = handshake::create_session(
                            host, &base, &auth, &remote_path, 0, batch_id.as_deref(),
                        )
                        .map_err(|e| format!("recreate session: {:?}", e))?;
                        (created.session_id, created.received)
                    }
                    Err(QuerySessionError::Other(e)) => return Err(e),
                }
            } else {
                // 新上传：创建 session（v2 带批 ID 走批 gating，免钩子）
                match handshake::create_session(
                    host, &base, &auth, &remote_path, 0, batch_id.as_deref(),
                ) {
                    Ok(created) => (created.session_id, created.received),
                    Err(CreateSessionError::DuplicateName) => {
                        if let Some(task) = state.tasks.get_mut(task_id) {
                            task.state = TaskState::Rejected;
                            task.reason = Some("duplicate-name".to_string());
                        }
                        state.queue.release(task_id);
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

/// 处理传输进度消息（on_bus_message `transfer:{task_id}`）
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

    // 终态幂等守卫：宿主终态事件可能迟到/重复（取消清理、not-seekable-resume
    // 重建后的旧 host 回报等），任务已终态时直接忽略——否则 Cancelled 分支
    // 兜底 / not-seekable-resume 分支会把终态任务拉回活跃循环，前端每轮
    // 「失败→复活→再失败」重发一次通知
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

            // M2/M3 接收方向落位：下载完成 → 「保存到…」（用户指定位置）或
            // MediaStore 公共下载目录（默认）；失败回退私有目录（最终文件保留
            // 在原位）。task.place 标记落点，前端据此提示
            if task.direction == Direction::Download {
                if task.save_to {
                    place_saved_to_document(host, task, &task_id);
                } else {
                    place_downloaded_file(host, task, &task_id);
                }
            }

            // 上传完成：通知远端 complete（失败记日志，不阻塞终态）；
            // 409 duplicate-name = 落位竞态失败（该文件 rejected，批内其他不受影响）
            if task.direction == Direction::Upload {
                if let Some(ref sid) = task.upload_session_id.clone() {
                    if let Ok((base, auth)) = state.peer.base_and_auth_for(&task.peer.device_id) {
                        match handshake::complete_session(host, &base, &auth, sid) {
                            Ok(()) => {}
                            Err(CompleteSessionError::DuplicateName) => {
                                // 接收端目标已存在同名：引擎已完成字节流，但落位被拒——
                                // 覆写为 rejected(duplicate-name)（v1 同名即拒语义）
                                task.state = TaskState::Rejected;
                                task.reason = Some("duplicate-name".to_string());
                                host.log_warn(&format!(
                                    "upload complete rejected (duplicate-name) for task {}",
                                    task_id
                                ));
                            }
                            Err(CompleteSessionError::Other(e)) => {
                                host.log_error(&format!(
                                    "upload complete_session failed for task {}: {}",
                                    task_id, e
                                ));
                            }
                        }
                    }
                }
                // 中转复制 cache 副本：完成即删（生命周期「复制 → 上传 → 完成 → 删除」）；
                // 真实路径源（免授权特殊条目）不标记 cleanup_local，不会误删用户文件
                if task.cleanup_local {
                    let lp = task.local_path.clone();
                    if let Err(e) = host.fs_delete(&lp) {
                        host.log_warn(&format!(
                            "cleanup_local delete failed for task {} ({}): {}",
                            task_id, lp, e
                        ));
                    } else {
                        host.log_debug(&format!(
                            "cleanup_local deleted cache copy for task {}",
                            task_id
                        ));
                    }
                }
            }
            // v2 终态归档（传输历史）
            archive_terminal_task(state, host, &task_id);
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
            } else if reason == "not-seekable-resume" {
                // M3 SAF pipe 流（不可 seek）跨任务续传：宿主只能从头打开
                // （effective_offset=0 ≠ 请求 offset）→ 重建 session 全量重传
                // （Kotlin 侧 offset=0 重开会强制重开 fd 从头，见 spec M3
                // 续传策略）。直接重新入队，不置终态——对端 session 以旧
                // 偏移累计，必须废弃重建
                if task.resume_retries >= MAX_RESUME_RETRIES {
                    // 连续多次重建仍失败 → 宿主异常/对端持续不可用，落终态
                    // 终止循环（否则每次重建失败都触发一次前端失败通知）
                    task.state = TaskState::Failed;
                    task.reason = Some("resume-limit-exceeded".to_string());
                    task.auto_resumable = false;
                } else {
                    task.state = TaskState::Queued;
                    task.offset = 0;
                    task.upload_session_id = None;
                    task.reason = None;
                    task.resume_retries += 1;
                    let id = task.id.clone();
                    state.queue.enqueue(&id);
                }
            } else {
                task.state = TaskState::Failed;
                task.reason = Some(reason.clone());
                // 失败终态：清除断线自动续传标记，防止后续事件路径再复活
                task.auto_resumable = false;
            }
            state.queue.release(&task_id);
            // v2 终态归档（传输历史）
            archive_terminal_task(state, host, &task_id);
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
            // v2 终态归档（用户取消）
            archive_terminal_task(state, host, &task_id);
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
        state.tasks.save(host);
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

/// 下载完成后的 MediaStore 落位（M2 接收方向统一落下载目录）///
/// 引擎已完成 `.part` → 下载目录最终名 rename（私有副本在握，续传/跨重启
/// 可靠，现有机制零改动）；此处将其拷贝到系统公共下载目录：
/// - 成功：删除私有副本（落点唯一，重复下载同名文件不再被 duplicate-name
///   预检拦截，也不会产生两份内容）
/// - 失败：保留私有副本（回退），task.place 标记供前端提示
fn place_downloaded_file(host: &(impl HostFs + HostLog), task: &mut Task, task_id: &str) {
    let final_path = task.local_path.trim_end_matches(".part").to_string();
    // 展示名取远端路径 basename（远端文件名即用户期望的目标名）
    let display_name = task
        .remote_path
        .rsplit('/')
        .next()
        .unwrap_or(&task.remote_path)
        .to_string();
    match host.fs_write_media_downloads(&final_path, &display_name, "") {
        Ok(()) => {
            // 公共目录已持有副本：删除私有副本（落点唯一）。删失败仅告警——
            // 残留副本由用户自行清理，不影响任务终态
            if let Err(e) = host.fs_delete(&final_path) {
                host.log_warn(&format!(
                    "download placement: delete private copy failed for task {} ({}): {}",
                    task_id, final_path, e
                ));
            }
            task.place = Some("system".to_string());
            host.log_info(&format!(
                "download placement: saved to system Downloads for task {}",
                task_id
            ));
        }
        Err(e) => {
            // 回退私有目录：最终文件保留在原位（无需额外动作），仅标记供前端提示
            task.place = Some("private".to_string());
            host.log_warn(&format!(
                "download placement: MediaStore write failed for task {} ({}), kept in private dir: {}",
                task_id, final_path, e
            ));
        }
    }
}

/// 下载完成后的「保存到…」落位（M3 单文件目标）
///
/// 引擎已完成 `.part` → 下载目录最终名 rename（私有副本在握）；此处弹系统
/// 保存对话框（ACTION_CREATE_DOCUMENT，用户选位置）并流拷贝到所选位置
/// （写完即达）：
/// - 成功：删除私有副本（落点唯一，不留残余）
/// - 失败/用户取消：保留私有副本（回退），task.place 标记供前端提示
fn place_saved_to_document(host: &(impl HostFs + HostLog), task: &mut Task, task_id: &str) {
    let final_path = task.local_path.trim_end_matches(".part").to_string();
    // 展示名取远端路径 basename（用户期望的文件名，对话框默认名）
    let suggested_name = task
        .remote_path
        .rsplit('/')
        .next()
        .unwrap_or(&task.remote_path)
        .to_string();
    match host.fs_save_to_document(&final_path, &suggested_name, "") {
        Ok(()) => {
            // 用户位置已持有副本：删除私有副本（落点唯一）。删失败仅告警——
            // 残留副本由用户自行清理，不影响任务终态
            if let Err(e) = host.fs_delete(&final_path) {
                host.log_warn(&format!(
                    "save-to placement: delete private copy failed for task {} ({}): {}",
                    task_id, final_path, e
                ));
            }
            task.place = Some("saved-to".to_string());
            host.log_info(&format!(
                "save-to placement: saved to user-selected location for task {}",
                task_id
            ));
        }
        Err(e) => {
            // 失败/取消：保留私有副本（回退语义），标记供前端提示
            task.place = Some("save-failed".to_string());
            host.log_warn(&format!(
                "save-to placement: saveToDocument failed/cancelled for task {} ({}), kept in private dir: {}",
                task_id, final_path, e
            ));
        }
    }
}

/// 处理对端上下线消息（on_bus_message `filesrv:peer_changed`）
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

        // v2：等待同意期间断线不重发（spec 14.2 边界 1）——waiting-approval 任务
        // 直接 rejected(timeout)；批记录保留 Pending（接收端自然超时）
        let approval_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                t.state == TaskState::WaitingApproval && t.peer.device_id == peer_id
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
        if !affected_ids.is_empty() {
            state.tasks.save(host);
            emit_tasks_changed(host, &state.tasks);
        }

        for id in &approval_ids {
            if let Some(task) = state.tasks.get_mut(id) {
                let _ = task.transition(TaskState::Rejected);
                task.reason = Some("timeout".to_string());
                task.auto_resumable = false;
            }
            state.queue.release(id);
            state.queue.remove(id);
            archive_terminal_task(state, host, id);
        }
        if !approval_ids.is_empty() {
            state.tasks.save(host);
        }
    } else if peers_changed {
        // 该对端上线（仅上下线边沿触发一次）：其 auto_resumable 的 resumable
        // 任务自动重新调度（spec §7.2）。重复公告（changed=false，WS 控制面
        // 周期推送，实测约每秒一次）不触发恢复——否则对端网络抖动时任务被
        // 反复复活重启，每轮「复活→失败」都触发一次前端失败通知
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
/// 桌面端推送（upload session）的落点是**下载目录**（M2 方向模型：接收统一落
/// 下载目录，不落共享目录）。同名预检因此针对下载目录而非共享目录 roots：
/// 私有下载目录同名经 host.fs_exists 提前拒绝；公共 Download 目录同名由宿主
/// MediaStore 预检（writeMediaDownloads 返回 duplicate-name）兜底拒绝。
/// 宿主沙箱已在上传创建前完成路径合法性校验，插件只需同名即拒。
pub fn handle_upload_request(
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

    // 同名预检目标 = 下载目录 + 远端文件名（与落位 display_name 一致）
    let Ok(download_dir) = resolve_download_dir(state, host) else {
        // 下载目录未配置：宿主侧 create_upload 会拒绝，无需在此重复拒绝
        return UploadHookDecision::allow();
    };
    let file_name = parts.last().expect("parts non-empty checked above");
    let target = PathBuf::from(&download_dir).join(file_name);
    // host.fs_exists 缺 fs:read 权限时 fail-closed 返回 Err，同名预检静默失效
    // ——公共目录同名由宿主 MediaStore 预检兜底，不依赖本检查
    if let Ok(true) = host.fs_exists(target.to_string_lossy().as_ref()) {
        return UploadHookDecision::deny("duplicate-name");
    }

    UploadHookDecision::allow()
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
        initiator: "me".to_string(),
        batch_id: None,
        place: None,
        save_to: false,
        created_at: now,
        updated_at: now,
        host_task_id: None,
        cleanup_local: false,
        auto_resumable: false,
        resume_retries: 0,
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

/// 生成批 ID（v2：一次「发送」动作一匹；与任务 ID 同命名空间）
fn generate_batch_id(now: u64) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("batch-{:x}-{:x}", now, n)
}

// ==================== v2 批上下文（发送方） ====================

/// 确保批上下文就绪（批内首个任务启动时调用一次）
///
/// 返回：
/// - Ok(true)：批已批准，可继续建 session（免钩子）
/// - Ok(false)：批 pending，任务已转 waiting-approval（调用方停止启动）
/// - Err(reason)：批已拒绝/网络失败，任务已置终态（调用方停止）
///
/// 批记录不存在 → 发起 POST /transfer-request（HTTP，base+auth 同 handshake 模式）：
/// 200 → Approved；202 → Pending；403 → Rejected(policy-denied)；网络错误 → failed
fn ensure_batch_ready(
    state: &mut PluginState,
    host: &(impl HostHttp + HostEvents + HostLog + HostStorage + HostFs + HostConfig + HostTransfer + HostFileService + HostBus),
    task_id: &str,
    bid: &str,
) -> Result<bool, String> {
    // 已有批记录：按状态分流（Pending 任务入等待；Approved 直接继续；Rejected 落终态）
    if let Some(record) = state.batches.get(bid).cloned() {
        return match record.state {
            BatchRecordState::Approved => Ok(true),
            BatchRecordState::Pending => {
                to_waiting_approval(state, host, task_id);
                Ok(false)
            }
            BatchRecordState::Rejected { reason } => {
                to_rejected(state, host, task_id, &reason);
                Err(reason)
            }
        };
    }

    // 批记录不存在：发起 transfer-request（批内首个任务启动时）
    let peer_id = state
        .tasks
        .get(task_id)
        .map(|t| t.peer.device_id.clone())
        .ok_or_else(|| "task not found".to_string())?;
    let (base, auth) = state
        .peer
        .base_and_auth_for(&peer_id)
        .map_err(|e| e.to_string())?;
    // 批内文件清单 = 全部同批上传任务（批 ID 一次「发送」一匹）
    let files: Vec<bedcode_plugin_api_mobile::UploadRequestMeta> = state
        .tasks
        .values()
        .filter(|t| t.direction == Direction::Upload && t.batch_id.as_deref() == Some(bid))
        .map(|t| bedcode_plugin_api_mobile::UploadRequestMeta {
            relative_path: t.remote_path.clone(),
            size: t.size,
        })
        .collect();
    let total_size: u64 = files.iter().map(|f| f.size).sum();

    match handshake::request_transfer(host, &base, &auth, bid, &files, total_size) {
        Ok(handshake::TransferRequestOutcome::Approved) => {
            state.batches.insert(
                bid.to_string(),
                BatchRecord {
                    batch_id: bid.to_string(),
                    peer_id: peer_id.clone(),
                    state: BatchRecordState::Approved,
                },
            );
            Ok(true)
        }
        Ok(handshake::TransferRequestOutcome::Pending) => {
            state.batches.insert(
                bid.to_string(),
                BatchRecord {
                    batch_id: bid.to_string(),
                    peer_id: peer_id.clone(),
                    state: BatchRecordState::Pending,
                },
            );
            to_waiting_approval(state, host, task_id);
            Ok(false)
        }
        Err(handshake::TransferRequestError::Denied(reason)) => {
            // 403：策略拒绝（policy-denied / hook 不可用 / 超时 fail-closed）→ 批 Rejected
            state.batches.insert(
                bid.to_string(),
                BatchRecord {
                    batch_id: bid.to_string(),
                    peer_id: peer_id.clone(),
                    state: BatchRecordState::Rejected {
                        reason: reason.clone(),
                    },
                },
            );
            to_rejected(state, host, task_id, &reason);
            Err(reason)
        }
        Err(handshake::TransferRequestError::Network(e)) => {
            // 网络错误/超时/非预期：任务 failed（reason 原文），不建批（重试重新询问）
            to_failed(state, host, task_id, &e);
            Err(e)
        }
    }
}

/// 任务转 waiting-approval（v2）：释放队列槽位 + 状态迁移 + 推送
fn to_waiting_approval(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage),
    task_id: &str,
) {
    if let Some(task) = state.tasks.get_mut(task_id) {
        let _ = task.transition(TaskState::WaitingApproval);
    }
    state.queue.release(task_id);
    state.tasks.save(host);
    emit_tasks_changed(host, &state.tasks);
}

/// 任务转 rejected（v2）：释放槽位 + 终态 + 归档历史 + 推送
fn to_rejected(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage),
    task_id: &str,
    reason: &str,
) {
    if let Some(task) = state.tasks.get_mut(task_id) {
        let _ = task.transition(TaskState::Rejected);
        task.reason = Some(reason.to_string());
    }
    state.queue.release(task_id);
    archive_terminal_task(state, host, task_id);
}

/// 任务转 failed（网络/异常）：释放槽位 + 终态 + 归档历史 + 推送
fn to_failed(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage),
    task_id: &str,
    reason: &str,
) {
    if let Some(task) = state.tasks.get_mut(task_id) {
        let _ = task.transition(TaskState::Failed);
        task.reason = Some(reason.to_string());
        task.auto_resumable = false;
    }
    state.queue.release(task_id);
    archive_terminal_task(state, host, task_id);
}

/// 终态任务归档（v2 传输历史）：写历史存储 + 推送 history-changed
///
/// 任务**保留在 TaskStore**（终态仍留在队列列表，retry/remove 交互与 v1 一致；
/// 与方案 §10「从 TaskStore 移除」的偏离说明见报告）——历史是终态的快照归档，
/// 历史 tab 数据源为 list-history；批维度不记（per-file 记），封顶 200 滚动淘汰
fn archive_terminal_task(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage),
    task_id: &str,
) {
    let Some(task) = state.tasks.get(task_id).cloned() else {
        return;
    };
    let state_name = match task.state {
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Rejected => "rejected",
        TaskState::Cancelled => "cancelled",
        _ => return, // 非终态不归档
    }
    .to_string();
    let file_name = task
        .remote_path
        .rsplit('/')
        .next()
        .unwrap_or(&task.remote_path)
        .to_string();
    let entry = HistoryEntry {
        id: task.id.clone(),
        direction: match task.direction {
            Direction::Upload => "upload".to_string(),
            Direction::Download => "download".to_string(),
        },
        initiator: "me".to_string(),
        file_name,
        size: task.size,
        state: state_name,
        reason: task.reason.clone(),
        peer_name: task.peer.name.clone(),
        // 下载完成且落盘：保留本地路径供「打开所在文件夹」；上传方向无本地落点
        local_path: if task.direction == Direction::Download
            && task.state == TaskState::Completed
            && !task.local_path.is_empty()
        {
            Some(task.local_path.trim_end_matches(".part").to_string())
        } else {
            None
        },
        created_at: task.created_at,
        updated_at: task.updated_at,
    };
    state.history.insert(entry);
    state.history.save(host);
    emit_history_changed(host, &state.history);
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

/// 解析下载目录
fn resolve_download_dir(
    state: &PluginState,
    host: &impl HostConfig,
) -> anyhow::Result<String> {
    // 优先使用 settings 中的 downloadDir
    if !state.settings.download_dir.is_empty() {
        return Ok(state.settings.download_dir.clone());
    }

    // 移动端：尝试 HostConfig::AppDownloadsDir（Android 外部私有下载目录，免权限）
    if let Ok(Some(downloads)) = host.config_get(bedcode_plugin_api_mobile::host::ConfigKey::AppDownloadsDir) {
        return Ok(downloads);
    }

    Err(anyhow::anyhow!(
        "download directory not configured; use set-settings to set downloadDir"
    ))
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
/// 移动端 SDK HostFs 自 fs_delete 落地后已具备删除能力（Android 走
/// Kotlin FileDeletePlugin，非 Android 平台宿主 std::fs），与桌面端对齐。
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

// ==================== v2 批钩子与接收端处理 ====================

/// 批量传输请求钩子（on_transfer_request，v2 接收策略三路分流）
///
/// 同步读取 settings（无 IO，满足钩子不可异步约束）：
/// - "accept" → allow（批直接批准，接收端无 pending 卡）
/// - "reject" → deny("policy-denied")（宿主 403，发送方 rejected，零打扰）
/// - "ask"（默认）→ ask（批进入 pending，宿主发本地事件等用户应答）
pub fn handle_transfer_request(
    state: &PluginState,
    _host: &(impl HostLog),
    _meta: &bedcode_plugin_api_mobile::TransferRequestMeta,
) -> UploadHookDecision {
    match state.settings.receiving_policy.as_str() {
        POLICY_ACCEPT => UploadHookDecision::allow(),
        POLICY_REJECT => UploadHookDecision::deny("policy-denied"),
        _ => UploadHookDecision::ask(),
    }
}

/// 接收端：批请求事件（filesrv:transfer_request，宿主 ask 分流后发出）
///
/// 建 PendingBatch（应答卡数据源，peer = 激活对端）→ 全量快照推送。
/// ask 模式不发 toast（等待应答，避免打扰；批准后才有批级 toast）
pub fn handle_transfer_request_event(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostConfig),
    payload: &serde_json::Value,
) {
    let batch_id = payload
        .get("batchId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if batch_id.is_empty() {
        host.log_warn("filesrv:transfer_request: missing batchId, ignored");
        return;
    }
    let files: Vec<bedcode_plugin_api_mobile::UploadRequestMeta> =
        serde_json::from_value(payload.get("files").cloned().unwrap_or_default())
            .unwrap_or_default();
    let total_size = payload.get("totalSize").and_then(|v| v.as_u64()).unwrap_or(0);
    let peer_id = state.peer.active_id().unwrap_or("").to_string();
    let now = now_ms(host);
    // 同批重复事件（发送方重建批）覆盖旧卡
    state.pending_batches.retain(|b| b.batch_id != batch_id);
    state.pending_batches.push(PendingBatch {
        batch_id,
        peer_id,
        files,
        total_size,
        created_at: now,
    });
    emit_batches_changed(host, &state.pending_batches);
}

/// 接收端：批已解决事件（filesrv:transfer_resolved，approve/reject 命令与 TTL 超时共用）
///
/// 移除 PendingBatch → 全量快照推送；decision=approved → 批级 toast 一条
///（{ name, count, totalSize, mode: "batch" }，前端立即展示）
pub fn handle_transfer_resolved_event(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog),
    payload: &serde_json::Value,
) {
    let batch_id = payload
        .get("batchId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if batch_id.is_empty() {
        return;
    }
    let removed: Option<PendingBatch> = {
        let idx = state
            .pending_batches
            .iter()
            .position(|b| b.batch_id == batch_id);
        idx.map(|i| state.pending_batches.remove(i))
    };
    if removed.is_none() {
        return;
    }
    emit_batches_changed(host, &state.pending_batches);

    // approved → 批级 toast（ask 模式批准后一条；拒绝/超时无 toast）
    let decision = payload
        .get("decision")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if decision == "approved" {
        let batch = removed.expect("removed checked above");
        let peer_name = state
            .peer
            .endpoint(&batch.peer_id)
            .map(|_| batch.peer_id.clone())
            .unwrap_or_else(|| batch.peer_id.clone());
        emit_toast(
            host,
            serde_json::json!({
                "name": peer_name,
                "count": batch.files.len(),
                "totalSize": batch.total_size,
                "mode": "batch",
            }),
        );
    }
}

/// 接收端：正在接收任务开始事件（filesrv:receiving_started，session 创建成功后）
///
/// 建 ReceivingTask（transferring）→ 全量快照推送；accept 模式 → toast
///（per-file，前端 3s 窗口合并去重）；ask 模式不重复 toast（批准时已发批级）
pub fn handle_receiving_started_event(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostConfig),
    payload: &serde_json::Value,
) {
    let session_id = payload
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if session_id.is_empty() {
        host.log_warn("filesrv:receiving_started: missing sessionId, ignored");
        return;
    }
    let remote_path = payload
        .get("relativePath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let size = payload.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
    let batch_id = payload
        .get("batchId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let peer_id = state.peer.active_id().unwrap_or("").to_string();
    let now = now_ms(host);
    // 同名 session 重复事件（宿主重建）覆盖旧记录
    state.receiving_tasks.remove(&session_id);
    state.receiving_tasks.insert(
        session_id.clone(),
        ReceivingTask {
            session_id,
            batch_id,
            remote_path,
            size,
            state: "transferring".to_string(),
            reason: None,
            peer_id,
            created_at: now,
            updated_at: now,
        },
    );
    emit_receiving_changed(host, &state.receiving_tasks);

    // accept 模式：传输开始 toast（前端 3s 窗口合并去重，只更新计数）
    if state.settings.receiving_policy == POLICY_ACCEPT {
        let peer_name = state.peer.active_id().unwrap_or("").to_string();
        emit_toast(
            host,
            serde_json::json!({
                "name": peer_name,
                "count": 1,
                "mode": "per-file",
            }),
        );
    }
}

/// 接收端：接收任务终态事件（filesrv:receiving_done）
///
/// ReceivingTask 终态（completed/failed/cancelled；409 竞态 → rejected
/// duplicate-name）→ 归档历史（per-file）→ receiving-changed + history-changed
pub fn handle_receiving_done_event(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage + HostConfig),
    payload: &serde_json::Value,
) {
    let session_id = payload
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if session_id.is_empty() {
        return;
    }
    let Some(mut task) = state.receiving_tasks.remove(&session_id) else {
        host.log_debug(&format!(
            "filesrv:receiving_done: unknown session {}, ignored",
            session_id
        ));
        return;
    };
    let wire_state = payload
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("failed");
    let reason = payload
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // 409 竞态（complete duplicate-name）→ rejected（接收端历史记 rejected）
    let (state_name, state_reason) =
        if wire_state == "failed" && reason.as_deref() == Some("duplicate-name") {
            ("rejected".to_string(), reason)
        } else {
            (wire_state.to_string(), reason)
        };
    task.state = state_name.clone();
    task.reason = state_reason.clone();
    task.updated_at = now_ms(host);

    // 接收任务终态 → 归档历史（per-file 记；移动端接收无本地路径）
    let file_name = task
        .remote_path
        .rsplit('/')
        .next()
        .unwrap_or(&task.remote_path)
        .to_string();
    let entry = HistoryEntry {
        id: task.session_id.clone(),
        direction: "download".to_string(),
        initiator: "peer".to_string(),
        file_name,
        size: task.size,
        state: state_name,
        reason: state_reason,
        peer_name: task.peer_id.clone(),
        // 移动端接收任务无本地路径（MediaStore 场景无路径语义，spec 14.5）
        local_path: None,
        created_at: task.created_at,
        updated_at: task.updated_at,
    };
    state.history.insert(entry);
    state.history.save(host);
    emit_history_changed(host, &state.history);
    emit_receiving_changed(host, &state.receiving_tasks);
}

/// 发送方：批应答事件（filesrv:transfer_approval，接收端批准/拒绝/超时 → 发送端）
///
/// - approved：批记录 Approved；批内 waiting-approval 任务 → queued + 重新调度
/// - rejected：批记录 Rejected(reason)；批内 waiting-approval 任务 → rejected
///   （reason 映射：user-rejected / timeout）
pub fn handle_transfer_approval_event(
    state: &mut PluginState,
    host: &(impl HostEvents + HostLog + HostStorage + HostBus + HostHttp + HostFs + HostConfig + HostTransfer + HostFileService),
    payload: &serde_json::Value,
) {
    let batch_id = payload
        .get("batchId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if batch_id.is_empty() {
        host.log_warn("filesrv:transfer_approval: missing batchId, ignored");
        return;
    }
    let decision = payload
        .get("decision")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let reason = payload.get("reason").and_then(|v| v.as_str()).unwrap_or("");

    if decision == "approved" {
        let existed = state.batches.contains_key(&batch_id);
        state.batches.insert(
            batch_id.clone(),
            BatchRecord {
                batch_id: batch_id.clone(),
                peer_id: String::new(),
                state: BatchRecordState::Approved,
            },
        );
        // 批内 waiting-approval 任务 → queued + 重新调度（批记录可能已被
        // 断线路径清理，插入即可——任务调度时按批 ID 命中新记录）
        let wake_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                t.state == TaskState::WaitingApproval
                    && t.batch_id.as_deref() == Some(&batch_id)
            })
            .map(|t| t.id.clone())
            .collect();
        for id in &wake_ids {
            if let Some(task) = state.tasks.get_mut(id) {
                let _ = task.transition(TaskState::Queued);
            }
            state.queue.enqueue(id);
        }
        state.tasks.save(host);
        emit_tasks_changed(host, &state.tasks);
        host.log_info(&format!(
            "transfer approval: batch {} approved, {} task(s) released (existed={})",
            batch_id,
            wake_ids.len(),
            existed
        ));
        if !wake_ids.is_empty() {
            schedule_and_start(state, host);
        }
    } else {
        // rejected：批内 waiting-approval 任务 → rejected（reason 映射）
        state.batches.insert(
            batch_id.clone(),
            BatchRecord {
                batch_id: batch_id.clone(),
                peer_id: String::new(),
                state: BatchRecordState::Rejected {
                    reason: reason.to_string(),
                },
            },
        );
        let reject_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| {
                t.state == TaskState::WaitingApproval
                    && t.batch_id.as_deref() == Some(&batch_id)
            })
            .map(|t| t.id.clone())
            .collect();
        for id in &reject_ids {
            if let Some(task) = state.tasks.get_mut(&id) {
                let _ = task.transition(TaskState::Rejected);
                task.reason = Some(reason.to_string());
                task.auto_resumable = false;
            }
            state.queue.release(&id);
            archive_terminal_task(state, host, &id);
        }
        if !reject_ids.is_empty() {
            state.tasks.save(host);
        }
        host.log_info(&format!(
            "transfer approval: batch {} rejected (reason={}), {} task(s) rejected",
            batch_id,
            reason,
            reject_ids.len()
        ));
    }
}

/// PendingBatch 快照（list-batches 命令 / batches-changed 事件共用载荷）
fn batches_snapshot(pending: &[PendingBatch]) -> serde_json::Value {
    serde_json::json!(pending
        .iter()
        .map(|b| {
            serde_json::json!({
                "batchId": b.batch_id,
                "peerName": b.peer_id,
                "files": b.files,
                "totalSize": b.total_size,
                "createdAt": b.created_at,
            })
        })
        .collect::<Vec<_>>())
}

/// 接收任务快照（list-receiving 命令 / receiving-changed 事件共用载荷）
fn receiving_snapshot(tasks: &HashMap<String, ReceivingTask>) -> serde_json::Value {
    let mut list: Vec<&ReceivingTask> = tasks.values().collect();
    list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    serde_json::to_value(list).unwrap_or(serde_json::Value::Array(vec![]))
}

/// 历史快照（list-history 命令 / history-changed 事件共用载荷）
fn history_snapshot(history: &HistoryStore) -> serde_json::Value {
    serde_json::to_value(history.snapshot()).unwrap_or(serde_json::Value::Array(vec![]))
}

/// 推送接收批快照事件
fn emit_batches_changed(host: &(impl HostEvents + HostLog), pending: &[PendingBatch]) {
    let snapshot = batches_snapshot(pending);
    host.emit_event("plugin:file-transfer:batches-changed", &snapshot);
}

/// 推送接收任务快照事件
fn emit_receiving_changed(
    host: &(impl HostEvents + HostLog),
    tasks: &HashMap<String, ReceivingTask>,
) {
    let snapshot = receiving_snapshot(tasks);
    host.emit_event("plugin:file-transfer:receiving-changed", &snapshot);
}

/// 推送历史快照事件
fn emit_history_changed(host: &(impl HostEvents + HostLog), history: &HistoryStore) {
    let snapshot = history_snapshot(history);
    host.emit_event("plugin:file-transfer:history-changed", &snapshot);
}

/// 推送接收端 toast 请求（{ name, count, totalSize?, mode: "batch"|"per-file" }）
fn emit_toast(host: &(impl HostEvents + HostLog), payload: serde_json::Value) {
    host.emit_event("plugin:file-transfer:toast", &payload);
}

// ==================== v2 接收端命令 ====================

/// list-batches：pending 批快照（前端应答卡数据源）
pub fn list_batches(state: &PluginState) -> serde_json::Value {
    batches_snapshot(&state.pending_batches)
}

/// approve-batch：批准传输批（应答卡「接受全部」→ 宿主命令）
pub fn approve_batch(
    _state: &PluginState,
    host: &(impl HostFileService + HostLog),
    batch_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_approve_transfer(batch_id)
        .map_err(|e| anyhow::anyhow!("approve-batch {}: {}", batch_id, e))?;
    Ok(serde_json::json!({ "ok": true }))
}

/// reject-batch：拒绝传输批（应答卡「拒绝全部」→ 宿主命令）
pub fn reject_batch(
    _state: &PluginState,
    host: &(impl HostFileService + HostLog),
    batch_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_reject_transfer(batch_id)
        .map_err(|e| anyhow::anyhow!("reject-batch {}: {}", batch_id, e))?;
    Ok(serde_json::json!({ "ok": true }))
}

/// list-receiving：接收任务快照
pub fn list_receiving(state: &PluginState) -> serde_json::Value {
    receiving_snapshot(&state.receiving_tasks)
}

/// cancel-receiving：取消接收中的上传会话（本地取消，宿主删 .part + done 事件）
pub fn cancel_receiving(
    _state: &PluginState,
    host: &(impl HostFileService + HostLog),
    session_id: &str,
) -> anyhow::Result<serde_json::Value> {
    host.filesrv_cancel_receiving(session_id)
        .map_err(|e| anyhow::anyhow!("cancel-receiving {}: {}", session_id, e))?;
    Ok(serde_json::json!({ "ok": true }))
}

/// list-history：传输历史快照
pub fn list_history(state: &PluginState) -> serde_json::Value {
    history_snapshot(&state.history)
}

/// clear-history：清空传输历史（仅清插件侧归档，不影响任务列表）
pub fn clear_history(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog),
) -> anyhow::Result<serde_json::Value> {
    state.history.clear();
    state.history.save(host);
    emit_history_changed(host, &state.history);
    Ok(serde_json::json!({ "ok": true }))
}

/// 拒绝原因 wire → 前端文案 key 映射（§8.4；未知原因归 unknown 兜底）
///
/// 纯函数：前端按映射后的 key 取 i18n 文案（transfer.error.*）
pub fn map_reject_reason(reason: &str) -> &'static str {
    match reason {
        "duplicate-name" => "duplicateName",
        "user-rejected" => "rejectedByUser",
        "timeout" => "noResponse",
        "policy-denied" => "policyDenied",
        _ => "unknown",
    }
}

/// 批记录状态迁移校验（纯函数，安全关键路径）
///
/// 仅 pending → approved / rejected 合法（与宿主批状态机同语义）；
/// 已批准/已拒绝的批不再迁移（防止 approved 后被覆写为 rejected）
pub fn validate_batch_record_transition(
    from: &BatchRecordState,
    to: &BatchRecordState,
) -> bool {
    matches!(
        (from, to),
        (BatchRecordState::Pending, BatchRecordState::Approved)
            | (BatchRecordState::Pending, BatchRecordState::Rejected { .. })
    )
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_plugin_api_mobile::host::HostError;

    /// 落位测试用 fake 宿主：记录 fs_delete 调用，write_media_downloads 按
    /// 配置返回成功/失败（M2 完成钩子层编排测试，spec「主 seam fake 注入」）；
    /// fs_save_to_document 按 save_ok 返回（M3「保存到…」编排测试）
    struct FakePlaceHost {
        media_ok: bool,
        save_ok: bool,
        deleted: std::sync::Mutex<Vec<String>>,
    }

    impl HostFs for FakePlaceHost {
        fn fs_read(&self, _path: &str) -> Result<Option<String>, HostError> {
            Ok(None)
        }
        fn fs_write(&self, _path: &str, _data: &str) -> Result<(), HostError> {
            Ok(())
        }
        fn fs_copy(&self, _src: &str, _dst: &str) -> Result<(), HostError> {
            Ok(())
        }
        fn fs_exists(&self, _path: &str) -> Result<bool, HostError> {
            Ok(false)
        }
        fn fs_delete(&self, path: &str) -> Result<(), HostError> {
            self.deleted.lock().unwrap().push(path.to_string());
            Ok(())
        }
        fn fs_request_auth(&self, _paths: &[String]) -> Result<bool, HostError> {
            Ok(true)
        }
        fn fs_write_media_downloads(
            &self,
            _src_path: &str,
            _display_name: &str,
            _mime_type: &str,
        ) -> Result<(), HostError> {
            if self.media_ok {
                Ok(())
            } else {
                Err(HostError::custom(-1, "requires API 29+"))
            }
        }

        fn fs_save_to_document(
            &self,
            _src_path: &str,
            _suggested_name: &str,
            _mime_type: &str,
        ) -> Result<(), HostError> {
            if self.save_ok {
                Ok(())
            } else {
                Err(HostError::custom(-1, "cancelled by user"))
            }
        }
    }

    impl HostLog for FakePlaceHost {
        fn log_info(&self, _message: &str) {}
        fn log_debug(&self, _message: &str) {}
        fn log_warn(&self, _message: &str) {}
        fn log_error(&self, _message: &str) {}
        fn mark_plugin_error(&self, _error: &str) {}
    }

    fn download_task(local_path: &str, remote_path: &str) -> Task {
        let mut task = make_task(
            Direction::Download,
            "peer-1",
            "桌面",
            remote_path,
            local_path,
            100,
            0,
        );
        task.state = TaskState::Completed;
        task
    }

    #[test]
    fn place_downloaded_file_success_deletes_private_copy_and_marks_system() {
        let host = FakePlaceHost {
            media_ok: true,
            save_ok: false,
            deleted: std::sync::Mutex::new(Vec::new()),
        };
        let mut task = download_task("/data/dl/movie.mp4.part", "movie.mp4");
        place_downloaded_file(&host, &mut task, "t1");
        assert_eq!(task.place.as_deref(), Some("system"));
        // 私有副本已删（落点唯一）；展示名 = 远端 basename
        assert_eq!(
            host.deleted.lock().unwrap().as_slice(),
            &["/data/dl/movie.mp4".to_string()]
        );
    }

    #[test]
    fn place_downloaded_file_failure_keeps_private_copy_and_marks_private() {
        let host = FakePlaceHost {
            media_ok: false,
            save_ok: false,
            deleted: std::sync::Mutex::new(Vec::new()),
        };
        let mut task = download_task("/data/dl/movie.mp4.part", "dir/movie.mp4");
        place_downloaded_file(&host, &mut task, "t2");
        assert_eq!(task.place.as_deref(), Some("private"));
        // 回退私有目录：不删除最终文件
        assert!(host.deleted.lock().unwrap().is_empty());
    }

    // ==================== M3「保存到…」落位 ====================

    #[test]
    fn place_saved_to_document_success_deletes_private_copy_and_marks_saved() {
        let host = FakePlaceHost {
            media_ok: true,
            save_ok: true,
            deleted: std::sync::Mutex::new(Vec::new()),
        };
        let mut task = download_task("/data/dl/.save-1-movie.mp4.part", "dir/movie.mp4");
        place_saved_to_document(&host, &mut task, "t3");
        assert_eq!(task.place.as_deref(), Some("saved-to"));
        // 用户位置已持有副本：私有副本已删（落点唯一）
        assert_eq!(
            host.deleted.lock().unwrap().as_slice(),
            &["/data/dl/.save-1-movie.mp4".to_string()]
        );
    }

    #[test]
    fn place_saved_to_document_failure_keeps_private_copy_and_marks_save_failed() {
        let host = FakePlaceHost {
            media_ok: true,
            save_ok: false,
            deleted: std::sync::Mutex::new(Vec::new()),
        };
        let mut task = download_task("/data/dl/.save-2-movie.mp4.part", "movie.mp4");
        place_saved_to_document(&host, &mut task, "t4");
        assert_eq!(task.place.as_deref(), Some("save-failed"));
        // 失败/取消：保留私有副本（回退语义）
        assert!(host.deleted.lock().unwrap().is_empty());
    }

    // ==================== v2 批状态机与拒绝映射 ====================

    #[test]
    fn test_batch_record_transition_valid() {
        // pending → approved / rejected 合法（应答流）
        assert!(validate_batch_record_transition(
            &BatchRecordState::Pending,
            &BatchRecordState::Approved
        ));
        assert!(validate_batch_record_transition(
            &BatchRecordState::Pending,
            &BatchRecordState::Rejected { reason: "timeout".to_string() }
        ));
    }

    #[test]
    fn test_batch_record_transition_invalid() {
        // 终态不可再迁移（approved 后不能再 rejected；rejected 后不能再批准）
        assert!(!validate_batch_record_transition(
            &BatchRecordState::Approved,
            &BatchRecordState::Rejected { reason: "user-rejected".to_string() }
        ));
        assert!(!validate_batch_record_transition(
            &BatchRecordState::Rejected { reason: "timeout".to_string() },
            &BatchRecordState::Approved
        ));
        assert!(!validate_batch_record_transition(
            &BatchRecordState::Pending,
            &BatchRecordState::Pending
        ));
    }

    #[test]
    fn test_map_reject_reason() {
        // §8.4 拒绝文案映射：wire reason → 前端 key 后缀
        assert_eq!(map_reject_reason("duplicate-name"), "duplicateName");
        assert_eq!(map_reject_reason("user-rejected"), "rejectedByUser");
        assert_eq!(map_reject_reason("timeout"), "noResponse");
        assert_eq!(map_reject_reason("policy-denied"), "policyDenied");
        assert_eq!(map_reject_reason("something-else"), "unknown");
    }
}
