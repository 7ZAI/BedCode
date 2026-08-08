//! 命令处理
//!
//! 14 个命令的实现（plugin.json 声明），由 lib.rs invoke_command 路由。
//! 每个命令接收 PluginState 引用和参数 JSON，返回结果 JSON。
//!
//! 宿主调用（transfer_start 等）在释放状态锁后执行，
//! 避免 on_message 回调死锁。

use crate::handshake::{self, CreateSessionError, QuerySessionError};
use crate::peer::{PeerCache, MOUNT_PATH};
use crate::queue::{Queue, DEFAULT_CONCURRENCY};
use crate::state::{Direction, Fingerprint, PeerInfo, Task, TaskState, TaskStore};
use bedcode_plugin_api::host::{
    HostConfig, HostEvents, HostFileService, HostFs, HostHttp, HostLog, HostStorage, HostTransfer,
};
use bedcode_plugin_api::types::{
    FileOperation, MountOptions, TransferDirection, TransferProgress, TransferRequest,
    TransferState, UploadHookDecision, UploadRequestMeta,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 设置 storage key
const SETTINGS_KEY: &str = "file-transfer-settings";

/// 插件设置
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

fn default_concurrency() -> usize {
    DEFAULT_CONCURRENCY
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            download_dir: String::new(),
            concurrency: DEFAULT_CONCURRENCY,
        }
    }
}

/// 插件全局状态（Mutex 保护，WASM 单线程）
pub struct PluginState {
    /// 任务存储
    pub tasks: TaskStore,
    /// 传输队列
    pub queue: Queue,
    /// 插件设置
    pub settings: Settings,
    /// 对端缓存
    pub peer: PeerCache,
    /// 是否已挂载
    pub mounted: bool,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            tasks: TaskStore::new(),
            queue: Queue::new(DEFAULT_CONCURRENCY),
            settings: Settings::default(),
            peer: PeerCache::new(),
            mounted: false,
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
pub fn query_peer(host: &impl HostFileService) -> anyhow::Result<serde_json::Value> {
    host.filesrv_query_peer("")
        .map_err(|e| anyhow::anyhow!("query-peer: {}", e))?;
    Ok(serde_json::json!({ "ok": true }))
}

/// list-remote：列举对端目录
pub fn list_remote(
    state: &PluginState,
    host: &impl HostHttp,
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (base, auth) = state.peer.base_and_auth()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let entries = handshake::list_remote(host, &base, &auth, path)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(serde_json::to_value(entries)?)
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
    _args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    // 确定下载目录
    let download_dir = resolve_download_dir(state, host)?;

    // 文件名
    let file_name = remote_path
        .rsplit('/')
        .next()
        .unwrap_or(remote_path);

    let local_path = format!("{}/{}.part", download_dir, file_name);
    let final_path = format!("{}/{}", download_dir, file_name);

    // 目标存在性预检（spec §7.4：目标已存在 → rejected duplicate-name）
    if let Ok(true) = host.fs_exists(&final_path) {
        let mut task = make_task(
            Direction::Download,
            peer_id,
            peer_name,
            remote_path,
            &local_path,
            0,
        );
        task.state = TaskState::Rejected;
        task.reason = Some("duplicate-name".to_string());
        let task_json = serde_json::to_value(&task)?;
        state.tasks.insert(task);
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

    let task = make_task(
        Direction::Upload,
        peer_id,
        peer_name,
        remote_path,
        local_path,
        0,
    );
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
    let (host_task_id, direction, upload_session_id, local_path) = {
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
        )
    };

    // 取消宿主传输
    if let Some(ref htid) = host_task_id {
        let _ = host.transfer_cancel(htid);
    }

    // 上传：取消远端 session（失败记日志，不阻塞本地终态）
    if direction == Direction::Upload {
        if let Some(ref sid) = upload_session_id {
            if let Ok((base, auth)) = state.peer.base_and_auth() {
                if let Err(e) = handshake::cancel_session(host, &base, &auth, sid) {
                    host.log_error(&format!(
                        "upload cancel_session failed for task {}: {}",
                        task_id, e
                    ));
                }
            }
        }
    }

    // 下载：删除 .part 文件（桌面端有 fs_delete；移动端没有，跳过）
    if direction == Direction::Download {
        delete_part_file(host, &local_path);
    }

    state.queue.release(task_id);
    state.queue.remove(task_id);
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
    host: &(impl HostStorage + HostEvents + HostLog + HostFs),
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
    task.transition(TaskState::Queued)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    task.reason = None;
    task.offset = 0;
    task.host_task_id = None;
    task.upload_session_id = None;
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
    host: &(impl HostStorage + HostLog + HostFileService),
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
            let options = MountOptions {
                mount_path: MOUNT_PATH.to_string(),
                roots: roots.clone(),
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
    }
    if let Some(dir) = args.get("downloadDir").and_then(|v| v.as_str()) {
        state.settings.download_dir = dir.to_string();
    }
    if let Some(n) = args.get("concurrency").and_then(|v| v.as_u64()) {
        state.queue.set_concurrency(n as usize);
        state.settings.concurrency = state.queue.concurrency();
    }
    save_settings(host, &state.settings);
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
    host: &(impl HostFileService + HostLog),
    args: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let roots = args
        .get("roots")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_else(|| state.settings.roots.clone());

    if roots.is_empty() {
        return Err(anyhow::anyhow!("no roots to mount"));
    }

    let options = MountOptions {
        mount_path: MOUNT_PATH.to_string(),
        roots: roots.clone(),
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
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents + HostFileService),
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
    host: &(impl HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostFileService),
    task_id: &str,
) -> Result<(), String> {
    let (base, auth) = state.peer.base_and_auth()?;
    let task = state.tasks.get(task_id).ok_or("task not found")?;
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
                direction: TransferDirection::Download,
                url: format!("{}/file?path={}", base, urlencoded(&remote_path)),
                headers: auth_headers(&auth),
                local_path: local_path.clone(),
                offset,
                expected_size: task.size,
                final_path: Some(final_path),
            };

            let host_task_id = host.transfer_start(&request)
                .map_err(|e| format!("transfer_start failed: {}", e))?;

            if let Some(task) = state.tasks.get_mut(task_id) {
                task.host_task_id = Some(host_task_id);
                task.state = TaskState::Transferring;
            }
        }
        Direction::Upload => {
            // 续传握手（spec §7.4）
            let (session_id, received) = if let Some(ref sid) = upload_session_id {
                match handshake::query_session(host, &base, &auth, sid) {
                    Ok(received) => (sid.clone(), received),
                    Err(QuerySessionError::SessionLost) => {
                        // session 丢失 → 重建从头传
                        let created = handshake::create_session(host, &base, &auth, &remote_path, 0)
                            .map_err(|e| format!("recreate session: {:?}", e))?;
                        (created.session_id, created.received)
                    }
                    Err(QuerySessionError::Other(e)) => return Err(e),
                }
            } else {
                // 新上传：创建 session
                match handshake::create_session(host, &base, &auth, &remote_path, 0) {
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
                direction: TransferDirection::Upload,
                url: format!("{}/upload/{}", base, session_id),
                headers: auth_headers(&auth),
                local_path: local_path.clone(),
                offset: received,
                expected_size: 0,
                final_path: None,
            };

            let host_task_id = host.transfer_start(&request)
                .map_err(|e| format!("transfer_start failed: {}", e))?;

            if let Some(task) = state.tasks.get_mut(task_id) {
                task.host_task_id = Some(host_task_id);
                task.state = TaskState::Transferring;
            }
        }
    }

    Ok(())
}

// ==================== 消息处理 ====================

/// 处理传输进度消息（on_message `transfer:{host_task_id}`）
pub fn handle_transfer_progress(
    state: &mut PluginState,
    host: &(impl HostStorage + HostEvents + HostLog + HostTransfer + HostFs + HostHttp + HostFileService + HostConfig),
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
                    if let Ok((base, auth)) = state.peer.base_and_auth() {
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
            // 对端下线已置恢复态（handle_peer_changed）→ 保持不覆写，
            // 续传握手会重校验文件指纹，避免把可恢复任务误判为终态
            if task.state == TaskState::Resumable && task.auto_resumable {
                // 保持 resumable，不进入终态
            } else if reason == "duplicate-name" {
                task.state = TaskState::Rejected;
                task.reason = Some("duplicate-name".to_string());
            } else {
                task.state = TaskState::Failed;
                task.reason = Some(reason.clone());
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
    // emit_tasks_changed 每消息照发，保证 UI 实时进度
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let is_terminal = matches!(
        &progress.state,
        TransferState::Completed | TransferState::Failed(_) | TransferState::Cancelled
    );

    if is_terminal {
        state.tasks.save(host);
    } else if let Some(task) = state.tasks.get_mut(&task_id) {
        if task.should_flush(now) {
            task.mark_flushed(now);
            state.tasks.save(host);
        }
    }

    emit_tasks_changed(host, &state.tasks);
}

/// 处理对端上下线消息（on_message `filesrv:peer_changed`）
pub fn handle_peer_changed(
    state: &mut PluginState,
    host: &(impl HostFileService + HostHttp + HostFs + HostStorage + HostLog + HostConfig + HostTransfer + HostEvents),
    peer_id: &str,
    online: bool,
) {
    // 首次感知对端上线时自动采纳 peer_id（单对端场景，尚无对端 ID 时）
    if online && state.peer.peer_id().is_none() {
        state.peer.set_peer_id(peer_id);
    }

    state.peer.on_peer_changed(host, peer_id, online);

    if !online {
        // 对端下线：transferring → resumable（auto_resumable=true）
        let transferring_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| t.state == TaskState::Transferring)
            .map(|t| t.id.clone())
            .collect();

        for id in &transferring_ids {
            if let Some(task) = state.tasks.get_mut(id) {
                let htid = task.host_task_id.clone();
                let _ = task.transition(TaskState::Resumable);
                task.auto_resumable = true;
                // 取消宿主传输
                if let Some(ref h) = htid {
                    let _ = host.transfer_cancel(h);
                }
                state.queue.release(id);
            }
        }
        if !transferring_ids.is_empty() {
            state.tasks.save(host);
            emit_tasks_changed(host, &state.tasks);
        }
    } else {
        // 对端上线：auto_resumable 的 resumable 自动重新调度（spec §7.2）
        let auto_ids: Vec<String> = state
            .tasks
            .values()
            .filter(|t| t.state == TaskState::Resumable && t.auto_resumable)
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
}

// ==================== 上传钩子 ====================

/// 上传请求策略钩子（on_upload_request）
///
/// 解析 meta.relativePath 到 roots 下的绝对路径，对每个 root 拼出目标绝对路径，
/// 用 host.fs_exists 检查目标是否已存在（wasm 环境 std::fs 全部 stub false，不可用）。
/// 宿主沙箱已在上传创建前完成路径合法性校验，插件只需同名即拒。
pub fn handle_upload_request(
    state: &PluginState,
    host: &impl HostFs,
    meta: &UploadRequestMeta,
) -> UploadHookDecision {
    let rel = meta.relative_path.trim_matches('/');
    let roots: Vec<PathBuf> = state.settings.roots.iter().map(PathBuf::from).collect();

    if roots.is_empty() {
        return UploadHookDecision::deny("no-roots");
    }

    // 清洗相对路径（复刻 sandbox::clean_relative_parts，拒绝 ..、绝对路径、:）
    let parts = match clean_relative_parts(rel) {
        Ok(p) => p,
        Err(_) => return UploadHookDecision::deny("invalid-path"),
    };
    if parts.is_empty() {
        return UploadHookDecision::deny("invalid-path");
    }

    // 任一根下目标已存在 → 同名拒绝；全部不存在 → allow
    // （host.fs_exists 缺 fs:read 权限时 fail-closed 返回 Err，同名预检静默失效）
    for root in &roots {
        let mut target = root.clone();
        for part in &parts {
            target.push(part);
        }
        if let Ok(true) = host.fs_exists(target.to_string_lossy().as_ref()) {
            return UploadHookDecision::deny("duplicate-name");
        }
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
) -> Task {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    Task {
        id: generate_id(),
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
        host_task_id: None,
        auto_resumable: false,
        last_flush: 0,
    }
}

/// 生成唯一任务 ID
fn generate_id() -> String {
    // 简单实现：时间戳 + 随机后缀
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("ft-{:x}", now)
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

    // 桌面端：尝试 HostConfig::HomeDir + /Downloads
    if let Ok(Some(home)) = host.config_get(bedcode_plugin_api::host::ConfigKey::HomeDir) {
        let downloads = format!("{}/Downloads", home);
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
