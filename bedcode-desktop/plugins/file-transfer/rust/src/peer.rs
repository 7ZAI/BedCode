//! 数据面命令编排与存储运行时（Desktop）
//!
//! 设备缓存自持后，本模块只保留两类逻辑：
//! 1. 目标解析：显式 endpoint 参数 > endpoint memo；session 句柄优先、缺失时
//!    静默重拨（endpoint 已知场景），旧式 node-id 直呼作为最后兜底；
//! 2. 存储运行时：transfer_store 纯函数之上的装载/持久化/视图派发，
//!    拉取 retryMeta 的挂载匹配，以及设置/注册表命令的宿主 I/O 编排。

use crate::device_bridge::{self, DialEndpoint};
use crate::roots_registry::{self, SharedRoot};
use crate::settings_store::{self, TransferSettings};
use crate::transfer_store::{self, RetryMeta, PullFileSpec, TransferEntry};
use bedcode_plugin_api::host::{
    HostEvents, HostLog, HostPeer, HostPlatform, HostPluginDatabase, HostStorage,
};
use bedcode_plugin_api::wasm_host::WasmHost;
use std::sync::Mutex;
use std::sync::OnceLock;

type Result<T> = anyhow::Result<T>;

pub(crate) const PLUGIN_ID: &str = "com.bedcode.file-transfer";

/// 预授权 storage key：宿主 `preauthorize_plugin` 读取（启用插件时收集路径并合并
/// 弹窗授权）。与共享目录生命周期同步：mount-local 追加、update-roots 剔除——
/// 缺失该写入方时 file-transfer 启用永远命中「请先配置共享目录」门禁（Bug A）。
pub(crate) const PREAUTH_PATHS_KEY: &str = "preauth_paths";

/// 读取预授权路径数组（storage 缺失/损坏视为空数组，幂等）
fn load_preauth_paths(h: &impl HostStorage) -> anyhow::Result<Vec<String>> {
    let Some(v) = h.storage_get(PREAUTH_PATHS_KEY)? else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_value::<Vec<String>>(v)
        .unwrap_or_default()
        .into_iter()
        .filter(|s: &String| !s.trim().is_empty())
        .collect())
}

/// 追加预授权路径（去重后写回；已存在时无操作）
pub(crate) fn push_preauth_path(h: &impl HostStorage, path: &str) -> anyhow::Result<()> {
    let mut paths = load_preauth_paths(h)?;
    if !paths.iter().any(|p| p == path) {
        paths.push(path.to_string());
        h.storage_set(PREAUTH_PATHS_KEY, &serde_json::to_value(&paths)?)?;
    }
    Ok(())
}

/// 按路径剔除预授权（不存在时无操作）
pub(crate) fn remove_preauth_path(h: &impl HostStorage, path: &str) -> anyhow::Result<()> {
    let mut paths = load_preauth_paths(h)?;
    let before = paths.len();
    paths.retain(|p| p != path);
    if paths.len() != before {
        h.storage_set(PREAUTH_PATHS_KEY, &serde_json::to_value(&paths)?)?;
    }
    Ok(())
}

// ==================== 任务存储运行时 ====================

static STORE: OnceLock<Mutex<Vec<TransferEntry>>> = OnceLock::new();
static STORE_LOADED: OnceLock<()> = OnceLock::new();

/// 待挂载拉取元数据队列：(nodeId, spec)。pull-files 入队时压入，新接收条目
/// 首次出现在快照中且文件集匹配时消费挂载（引擎按文件逐批铸造 batchId，
/// 调用点拿不到 id，只能事后匹配）。
static PENDING_PULLS: OnceLock<Mutex<Vec<(String, RetryMeta)>>> = OnceLock::new();

const PENDING_PULLS_CAP: usize = 8;

fn store() -> &'static Mutex<Vec<TransferEntry>> {
    STORE.get_or_init(|| Mutex::new(Vec::new()))
}

fn pending_pulls() -> &'static Mutex<Vec<(String, RetryMeta)>> {
    PENDING_PULLS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 装载持久层（进程内一次）：载入 → 重启恢复标注 → 回写标注结果。
/// 表不存在等持久化异常降级为内存空表起步（如实记日志）。
fn ensure_loaded(h: &WasmHost) -> std::sync::MutexGuard<'static, Vec<TransferEntry>> {
    let mut guard = store().lock().expect("transfer store lock");
    if STORE_LOADED.set(()).is_ok() {
        match load_entries(h) {
            Ok(mut entries) => {
                let marked = transfer_store::mark_interrupted_on_load(&mut entries);
                if marked > 0 {
                    h.log_info(&format!("restored {} transfers, {} marked interrupted", entries.len(), marked));
                    persist_entries(h, &entries);
                } else {
                    h.log_info(&format!("restored {} transfers", entries.len()));
                }
                *guard = entries;
            }
            Err(e) => h.log_info(&format!("transfer store load deferred: {e}")),
        }
    }
    guard
}

fn load_entries(h: &WasmHost) -> Result<Vec<TransferEntry>> {
    h.plugin_db_execute(
        "CREATE TABLE IF NOT EXISTS transfer_entries (
            batch_id TEXT PRIMARY KEY,
            direction TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            payload TEXT NOT NULL
        )",
    )?;
    let Some(rows) = h.plugin_db_query("SELECT payload FROM transfer_entries ORDER BY rowid")? else {
        return Ok(vec![]);
    };
    Ok(rows
        .as_array()
        .map(|a| a.iter())
        .unwrap_or_default()
        .filter_map(|r| r.get("payload").and_then(|p| p.as_str()))
        .filter_map(|p| serde_json::from_str::<TransferEntry>(p).ok())
        .collect())
}

/// 全量回写（快照规模 ≤200 + 进行中条目，SQLite 本地写开销可接受）
fn persist_entries(h: &WasmHost, entries: &[TransferEntry]) {
    let _ = h.plugin_db_execute("DELETE FROM transfer_entries");
    for e in entries {
        let payload = serde_json::to_string(e).unwrap_or_default();
        let _ = h.plugin_db_execute_params(
            "INSERT INTO transfer_entries (batch_id, direction, status, payload) VALUES (?1, ?2, ?3, ?4)",
            &[
                serde_json::Value::String(e.batch_id.clone()),
                serde_json::Value::String(e.direction.clone()),
                serde_json::Value::String(e.status.clone()),
                serde_json::Value::String(payload),
            ],
        );
    }
}

/// 变更后统一出口：持久化 + 四路视图派发（tasks/receiving 仅进行中，
/// 终态条目由 history 视图承接——「完成/失败自动归档历史」）
fn flush(h: &WasmHost, mut guard: std::sync::MutexGuard<'static, Vec<TransferEntry>>, changed: bool) {
    if changed {
        transfer_store::evict_overflow(&mut guard);
        persist_entries(h, &guard);
    }
    let tasks: Vec<&TransferEntry> = transfer_store::active_send_entries(&guard);
    let batches: Vec<&TransferEntry> = guard
        .iter()
        .filter(|e| e.direction == "receive" && e.status == "pending")
        .collect();
    let receiving: Vec<&TransferEntry> = transfer_store::active_receive_entries(&guard);
    let history = history_view(&guard);

    let dump = |list: Vec<&TransferEntry>| {
        list.iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect::<Vec<_>>()
    };
    h.emit_event("plugin:file-transfer:tasks-changed", &serde_json::Value::Array(dump(tasks)));
    h.emit_event("plugin:file-transfer:batches-changed", &serde_json::Value::Array(dump(batches)));
    h.emit_event("plugin:file-transfer:receiving-changed", &serde_json::Value::Array(dump(receiving)));
    h.emit_event("plugin:file-transfer:history-changed", &serde_json::Value::Array(dump(history)));
}

// ==================== 事件编排 ====================

/// 快照合并入口（peer:transfer / peer:receive 共用）：缺席剪枝 → 拉取元数据
/// 挂载 → 合并 → 持久化派发
pub(crate) fn merge_and_emit(h: &WasmHost, snapshot: &[serde_json::Value], direction: &str) {
    let mut guard = ensure_loaded(h);
    let ids: Vec<String> = snapshot
        .iter()
        .filter_map(|d| d.get("batchId").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();
    let mut changed = transfer_store::prune_absent(&mut guard, &ids, direction) > 0;
    let known_before: std::collections::HashSet<String> =
        guard.iter().map(|e| e.batch_id.clone()).collect();
    changed |= transfer_store::merge_snapshot(&mut guard, snapshot);
    changed |= attach_pending_pull_meta(&mut guard, &known_before);
    flush(h, guard, changed);
}

/// 新出现条目 × 待挂载队列的文件集匹配
fn attach_pending_pull_meta(
    store: &mut [TransferEntry],
    known_before: &std::collections::HashSet<String>,
) -> bool {
    let mut attached = false;
    let mut consumed: Vec<usize> = vec![];
    {
        let mut pending = pending_pulls().lock().expect("pending pulls lock");
        for (idx, (node_id, meta)) in pending.iter().enumerate() {
            let RetryMeta::Pull { files, .. } = meta else { continue };
            let spec_paths: std::collections::HashSet<&str> =
                files.iter().map(|f| f.rel_path.as_str()).collect();
            for entry in store.iter_mut() {
                if known_before.contains(&entry.batch_id)
                    || entry.node_id != *node_id
                    || entry.retry_meta.is_some()
                {
                    continue;
                }
                let entry_paths: std::collections::HashSet<&str> = entry
                    .files
                    .iter()
                    .filter_map(|f| f.get("path").and_then(|v| v.as_str()))
                    .collect();
                if !spec_paths.is_empty() && entry_paths == spec_paths {
                    entry.retry_meta = Some(meta.clone());
                    attached = true;
                    consumed.push(idx);
                    break;
                }
            }
        }
        // 后进先出地移除已消费项（索引从大到小删避免位移）
        consumed.sort_unstable();
        consumed.dedup();
        for idx in consumed.into_iter().rev() {
            pending.remove(idx);
        }
        // 队列封顶：最旧先出
        while pending.len() > PENDING_PULLS_CAP {
            pending.remove(0);
        }
    }
    attached
}

/// auto 分支自动应答（accept/reject 策略）：对快照中的 pending 批立即
/// respond-transfer。ask 分支不动作（弹窗编排在 UI 层，倒计时到点由前端
/// 主动应答；宿主闸门 sweeper 兜底不变）。
pub(crate) fn auto_answer_pending(h: &WasmHost, snapshot: &[serde_json::Value]) {
    let policy = match settings_store::load(h) {
        Ok(p) => p,
        Err(_) => return,
    };
    let Some(answer) = settings_store::auto_answer(&policy) else {
        return;
    };
    for dto in snapshot {
        let is_pending = dto.get("status").and_then(|v| v.as_str()) == Some("pending");
        if !is_pending {
            continue;
        }
        if let Some(batch_id) = dto.get("batchId").and_then(|v| v.as_str()) {
            if h.peer_respond_transfer(batch_id, answer).is_ok() {
                h.log_info(&format!("auto-answer ({answer}) batch {batch_id}"));
            }
        }
    }
}

// ==================== 目标解析 ====================

fn parse_endpoint(v: Option<&serde_json::Value>) -> Result<Option<DialEndpoint>> {
    match v {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => Ok(Some(
            serde_json::from_value(v.clone())
                .map_err(|e| anyhow::anyhow!("invalid endpoint: {e}"))?,
        )),
    }
}

/// 目标三元组解析：peerId 显式参数 > ACTIVE_NODE
fn resolve_node(
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Option<String> {
    args.get("peerId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| Some(active_node.lock().expect("active node lock").clone()))
        .filter(|s| !s.is_empty())
}

/// 连接保障：已有活跃会话直接用句柄；endpoint 可知则静默重拨铸新句柄；
/// 都不行退回 node-id 直呼（过渡期兜底）。返回传给数据面原语的首参。
fn ensure_target(h: &WasmHost, node_id: &str, endpoint: Option<DialEndpoint>) -> Result<String> {
    if let Some(handle) = device_bridge::session_of(node_id) {
        return Ok(handle);
    }
    let ep = device_bridge::resolve_endpoint(endpoint, node_id)
        .ok_or_else(|| anyhow::anyhow!("no endpoint known for peer {node_id}"))?;
    let handle = h.peer_dial(&serde_json::to_value(&ep)?)?;
    device_bridge::remember_session(&ep, handle);
    Ok(device_bridge::session_of(node_id).unwrap_or_else(|| node_id.to_string()))
}

// ==================== 连接命令 ====================

pub(crate) fn dial_peer(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    // 允许两种传参形状：{ endpoint: {...} } 或直接顶层 { nodeId, addr, port }
    let endpoint_value = args.get("endpoint").cloned().unwrap_or_else(|| args.clone());
    let endpoint: DialEndpoint = serde_json::from_value(endpoint_value)
        .map_err(|e| anyhow::anyhow!("invalid endpoint: {e}"))?;
    let handle = h.peer_dial(&serde_json::to_value(&endpoint)?)?;
    device_bridge::remember_session(&endpoint, handle);
    Ok(serde_json::json!({ "status": "connected" }))
}

pub(crate) fn disconnect_peer(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let node_id = args
        .get("nodeId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    // 仅会话句柄路径（Phase 4 收紧）：无句柄 = 本就未连接
    let existed = match device_bridge::session_of(&node_id) {
        Some(handle) => h.peer_close(&handle)?,
        None => false,
    };
    device_bridge::forget_session(&node_id);
    Ok(serde_json::json!({ "existed": existed }))
}

// ==================== 发送命令 ====================

/// 元素级载荷构造：`{ path, encrypt }`（加密默认值取插件设置）
fn send_payload(settings: &TransferSettings, paths: &[String]) -> Vec<serde_json::Value> {
    paths
        .iter()
        .map(|p| serde_json::json!({ "path": p, "encrypt": settings.encryption }))
        .collect()
}

pub(crate) fn enqueue(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let node_id = resolve_node(args, active_node).ok_or_else(|| anyhow::anyhow!("no active peer"))?;

    let mut paths: Vec<String> = args
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    if let Some(single) = args.get("localPath").and_then(|v| v.as_str()) {
        if !single.is_empty() {
            paths.push(single.to_string());
        }
    }
    if paths.is_empty() {
        anyhow::bail!("enqueue: no files to send");
    }

    let endpoint = parse_endpoint(args.get("endpoint"))?;
    let target = ensure_target(h, &node_id, endpoint)?;
    let settings = settings_store::load(h)?;
    let batch_id = h.peer_send_files(&target, &send_payload(&settings, &paths))?;
    // 返回已收窄为传输句柄；先入店最小条目占位，引擎快照事件随即补全明细
    insert_send_entry(
        h,
        &node_id,
        &batch_id,
        RetryMeta::Send { paths },
    )
}

/// 发送入店（Phase 4：宿主只回传输句柄）——最小条目占位 + retryMeta，
/// 引擎快照事件到达后按 batchId 合并补全文件清单/大小/时间戳
fn insert_send_entry(
    h: &WasmHost,
    node_id: &str,
    batch_id: &str,
    meta: RetryMeta,
) -> Result<serde_json::Value> {
    let entry = TransferEntry {
        batch_id: batch_id.to_string(),
        node_id: node_id.to_string(),
        peer_name: String::new(),
        direction: "send".to_string(),
        status: "running".to_string(),
        files: vec![],
        total_bytes: 0,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: 0,
        updated_at_ms: 0,
        retry_meta: Some(meta),
    };
    let out = serde_json::to_value(&entry)?;
    let mut guard = ensure_loaded(h);
    let known_before: std::collections::HashSet<String> =
        guard.iter().map(|e| e.batch_id.clone()).collect();
    let _ = known_before;
    let changed = transfer_store::merge_snapshot(&mut guard, &[out.clone()]);
    flush(h, guard, changed);
    Ok(out)
}

pub(crate) fn list_tasks(h: &WasmHost) -> Result<serde_json::Value> {
    let guard = ensure_loaded(h);
    let tasks: Vec<serde_json::Value> = transfer_store::active_send_entries(&guard)
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(serde_json::Value::Array(tasks))
}

pub(crate) fn cancel_task(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let batch_id =
        args.get("taskId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let hit = h.peer_close(&batch_id)?;
    let mut guard = ensure_loaded(h);
    let changed = transfer_store::mark_cancelled(&mut guard, &batch_id);
    flush(h, guard, changed);
    Ok(serde_json::json!({ "ok": hit || changed }))
}

/// 重试 = 批元数据回放重调原语（issue 13 步骤 3）：
/// - send 条目：回放 send-files，返回 DTO 的新 batchId 回填原条目（同一条历史）；
/// - pull 条目：回放 pull-files（逐文件新批自然入账），原失败记录保留为历史。
pub(crate) fn retry_task(
    h: &WasmHost,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let task_id =
        args.get("taskId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let endpoint = parse_endpoint(args.get("endpoint"))?;

    let (node_id, meta) = {
        let guard = ensure_loaded(h);
        let entry = guard
            .iter()
            .find(|e| e.batch_id == task_id)
            .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
        match &entry.retry_meta {
            Some(m) => (entry.node_id.clone(), m.clone()),
            None => anyhow::bail!("task not retryable (initiator metadata missing): {task_id}"),
        }
    };
    let target = ensure_target(h, &node_id, endpoint)?;

    match meta {
        RetryMeta::Send { paths } => {
            let settings = settings_store::load(h)?;
            let new_batch = h.peer_send_files(&target, &send_payload(&settings, &paths))?;
            let now = 0u64; // 时间戳由引擎快照事件补全（updated_at_ms 单调性由 merge 保证）
            let mut guard = ensure_loaded(h);
            let changed = transfer_store::apply_retry(&mut guard, &task_id, &new_batch, now);
            if let Some(slot) = guard.iter_mut().find(|e| e.batch_id == new_batch) {
                slot.retry_meta = Some(RetryMeta::Send { paths });
            }
            flush(h, guard, changed);
            let guard = ensure_loaded(h);
            let updated = guard
                .iter()
                .find(|e| e.batch_id == new_batch)
                .map(|e| serde_json::to_value(e).unwrap_or_default())
                .unwrap_or_default();
            Ok(updated)
        }
        RetryMeta::Pull { dir_id, files } => {
            let values: Vec<serde_json::Value> = files
                .iter()
                .map(|f| serde_json::json!({ "relPath": f.rel_path, "size": f.size }))
                .collect();
            let n = h.peer_pull_files(&target, &dir_id, &values)?;
            pending_pulls().lock().expect("pending pulls lock").push((
                node_id,
                RetryMeta::Pull { dir_id, files },
            ));
            Ok(serde_json::json!({ "requeued": n }))
        }
    }
}

// ==================== 接收侧视图 / 操作 ====================

pub(crate) fn list_batches(h: &WasmHost) -> Result<serde_json::Value> {
    let guard = ensure_loaded(h);
    Ok(serde_json::Value::Array(
        guard
            .iter()
            .filter(|e| e.direction == "receive" && e.status == "pending")
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
    ))
}

pub(crate) fn list_receiving(h: &WasmHost) -> Result<serde_json::Value> {
    let guard = ensure_loaded(h);
    Ok(serde_json::Value::Array(
        transfer_store::active_receive_entries(&guard)
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
    ))
}

/// 历史视图：终态条目按 updatedAtMs 降序（flush 的 history-changed 派发与
/// list-history 初始快照命令共用同一口径）
fn history_view(guard: &[TransferEntry]) -> Vec<&TransferEntry> {
    let mut history: Vec<&TransferEntry> = guard.iter().filter(|e| e.is_terminal()).collect();
    history.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    history
}

/// 历史列表命令：与 history-changed 事件同形状（前端 refresh 初始快照）
pub(crate) fn list_history(h: &WasmHost) -> Result<serde_json::Value> {
    let guard = ensure_loaded(h);
    Ok(serde_json::Value::Array(
        history_view(&guard)
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
    ))
}

pub(crate) fn cancel_receiving(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let batch_id = args
        .get("sessionId")
        .or_else(|| args.get("batchId"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let hit = h.peer_close(&batch_id)?;
    let mut guard = ensure_loaded(h);
    let changed = transfer_store::mark_cancelled(&mut guard, &batch_id);
    flush(h, guard, changed);
    Ok(serde_json::json!({ "ok": hit || changed }))
}

pub(crate) fn clear_history(h: &WasmHost) -> Result<serde_json::Value> {
    let mut guard = ensure_loaded(h);
    let cleared = transfer_store::clear_terminal(&mut guard);
    flush(h, guard, cleared > 0);
    Ok(serde_json::json!({ "cleared": cleared }))
}

// ==================== 远端浏览 / 拉取 ====================

/// list-remote 目标解析：dirId 非空即权威根（path = 根内相对路径，两端前端
/// 目录导航维护的契约）；缺失 dirId 的旧形态（path 首段即根）从 path 推根。
/// 返回 (root, rel)，rel 已去首尾斜杠
fn split_root_rel<'a>(dir_id: &'a str, path: &'a str) -> (&'a str, &'a str) {
    if dir_id.is_empty() {
        match path.split_once('/') {
            Some((head, rest)) => (head, rest.trim_matches('/')),
            None => (path, ""),
        }
    } else {
        (dir_id, path.trim_matches('/'))
    }
}

pub(crate) fn list_remote(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let dir_id = args.get("dirId").and_then(|v| v.as_str()).unwrap_or("");
    let node_id =
        resolve_node(args, active_node).ok_or_else(|| anyhow::anyhow!("no active peer"))?;
    let endpoint = parse_endpoint(args.get("endpoint"))?;
    let target = ensure_target(h, &node_id, endpoint)?;

    if path.is_empty() && dir_id.is_empty() {
        let roots = h.peer_list_shared_roots(&target)?;
        return Ok(serde_json::json!({ "roots": roots }));
    }

    // dirId 非空即权威根，path = 根内相对路径（前端目录导航维护）。旧实现按
    // root 前缀剥离 path——根内子目录名与根 id 同名时误剥离、列错目录
    let (root, rel) = split_root_rel(dir_id, path);

    let listing = h.peer_browse_directory(&target, root, rel)?;
    // 引擎 RemoteBrowseDto.filtered → 旧 notice 提示字段（前端 useRemoteFs 契约）
    let filtered = listing.get("filtered").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(serde_json::json!({
        "entries": listing.get("entries").cloned().unwrap_or_default(),
        "notice": if filtered { serde_json::json!("filtered") } else { serde_json::Value::Null },
    }))
}

pub(crate) fn pull_files(
    h: &WasmHost,
    args: &serde_json::Value,
    active_node: &'static Mutex<String>,
) -> Result<serde_json::Value> {
    let dir_id = args
        .get("dirId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing dirId"))?
        .to_string();
    let base = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let names: Vec<String> = args
        .get("files")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .ok_or_else(|| anyhow::anyhow!("missing files"))?;
    if names.is_empty() {
        anyhow::bail!("pull-files: no files selected");
    }
    let node_id =
        resolve_node(args, active_node).ok_or_else(|| anyhow::anyhow!("no active peer"))?;
    let endpoint = parse_endpoint(args.get("endpoint"))?;
    let target = ensure_target(h, &node_id, endpoint)?;

    let specs: Vec<PullFileSpec> = names
        .iter()
        .map(|name| {
            let rel = if base.is_empty() { name.clone() } else { format!("{base}/{name}") };
            PullFileSpec { rel_path: rel, size: 0 }
        })
        .collect();
    let values: Vec<serde_json::Value> = specs
        .iter()
        .map(|f| serde_json::json!({ "relPath": f.rel_path, "size": f.size }))
        .collect();

    let n = h.peer_pull_files(&target, &dir_id, &values)?;
    pending_pulls().lock().expect("pending pulls lock").push((
        node_id,
        RetryMeta::Pull { dir_id, files: specs },
    ));
    Ok(serde_json::json!({ "count": n }))
}

// ==================== 设置 / 注册表命令 ====================

pub(crate) fn get_settings(h: &WasmHost) -> Result<serde_json::Value> {
    let s = settings_store::load_or_migrate(h)?;
    let roots = roots_registry::load_all(h)?;
    Ok(serde_json::json!({
        "roots": roots.iter().map(|r| serde_json::json!({
            "id": r.id, "name": r.name, "path": r.path,
        })).collect::<Vec<_>>(),
        "policy_mode": settings_store::policy_to_host(&s.receiving_policy),
        "ask_timeout_sec": settings_store::clamp_timeout(s.approval_timeout_sec),
        "download_dir": s.download_dir.unwrap_or_default(),
        "encryption": s.encryption,
        "concurrency": 1,
    }))
}

/// 设置写入：未指定的键沿用现值；storage 真源 + 宿主配置原语推送
pub(crate) fn set_settings(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let mut s = settings_store::load_or_migrate(h)?;
    if let Some(policy) = args.get("receivingPolicy").and_then(|v| v.as_str()) {
        s.receiving_policy = policy.to_string();
    }
    if let Some(t) = args.get("approvalTimeoutSec").and_then(|v| v.as_u64()) {
        s.approval_timeout_sec = settings_store::clamp_timeout(t);
    }
    if let Some(dir) = args.get("downloadDir").and_then(|v| v.as_str()) {
        s.download_dir = if dir.is_empty() { None } else { Some(dir.to_string()) };
    }
    if let Some(enabled) = args.get("encryption").and_then(|v| v.as_bool()) {
        s.encryption = enabled;
    }
    settings_store::save_and_push(h, &s)?;
    h.log_info("set-settings: policy/download-dir applied (plugin-sourced)");
    Ok(serde_json::json!({ "ok": true }))
}

pub(crate) fn pick_download_dir(h: &WasmHost) -> Result<serde_json::Value> {
    let folder = h.platform_pick_folder()?;
    if folder.is_empty() {
        return Ok(serde_json::json!({ "cancelled": true }));
    }
    set_settings(
        h,
        &serde_json::json!({ "downloadDir": folder }),
    )?;
    Ok(serde_json::json!({ "path": folder }))
}

/// 添加共享目录：无 path 参数则弹系统多目录选择器（host-platform，一次可选
/// 多个）；同根去重在注册表层完成（id=路径哈希）；推送失败自动回滚
pub(crate) fn mount_local(
    h: &(impl HostPlatform + HostPluginDatabase + HostPeer + HostStorage),
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let paths =
        match args.get("path").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            Some(p) => vec![p.to_string()],
            None => {
                let picked = h.platform_pick_folders()?;
                if picked.is_empty() {
                    anyhow::bail!("cancelled");
                }
                picked
            }
        };
    // 显式 name 仅单路径注册（args.path + args.name）时生效；选择器多选按
    // 各自路径末段取默认名
    let explicit_name = args
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let entries: Vec<SharedRoot> = paths
        .into_iter()
        .map(|path| {
            let name =
                explicit_name.clone().unwrap_or_else(|| default_root_name(&path));
            SharedRoot { id: roots_registry::root_id(&path), name, path }
        })
        .collect();
    let next = roots_registry::apply_and_push(h, |list| {
        for entry in &entries {
            roots_registry::upsert(list, entry.clone());
        }
    })?;
    let mut added = Vec::with_capacity(entries.len());
    for entry in &entries {
        if let Some(r) = next.iter().find(|r| r.id == entry.id) {
            // 预授权路径与共享目录同步：追加后宿主启用插件时才能收集到（缺失时
            // 启用命中「请先配置共享目录」门禁）。storage 写失败如实上抛——共享目录
            // 已落库但预授权缺失会再次锁死启用，宁可让用户看到错误重试
            push_preauth_path(h, &r.path)?;
            added.push(serde_json::json!({ "id": r.id, "name": r.name, "path": r.path }));
        }
    }
    Ok(serde_json::json!({ "added": added }))
}

/// 路径默认展示名：去尾分隔符后的最后一段（Windows / Unix 分隔符均适配）
fn default_root_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}

/// 移除共享目录：args.remove = 条目 id；推送失败自动回滚
pub(crate) fn update_roots(
    h: &(impl HostPluginDatabase + HostPeer + HostStorage),
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let id = args
        .get("remove")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing remove id"))?
        .to_string();
    let (existed, removed_path) = {
        let list = roots_registry::load_all(h)?;
        let removed_path = list.iter().find(|r| r.id == id).map(|r| r.path.clone());
        (list.iter().any(|r| r.id == id), removed_path)
    };
    roots_registry::apply_and_push(h, |list| {
        roots_registry::remove(list, &id);
    })?;
    if let Some(path) = removed_path {
        // 预授权路径同步剔除：共享目录移除后启用插件不再收集该路径
        remove_preauth_path(h, &path)?;
    }
    Ok(serde_json::json!({ "removed": existed }))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::{load_preauth_paths, mount_local, roots_registry, update_roots};
    use bedcode_plugin_api::host::{HostError, HostPeer, HostPlatform, HostPluginDatabase, HostStorage};
    use std::cell::RefCell;

    /// 历史视图口径：仅终态条目、按 updatedAtMs 降序（list-history 初始快照与
    /// flush 的 history-changed 派发共用本函数，防两路口径漂移）
    #[test]
    fn history_view_returns_terminal_entries_sorted_desc() {
        let mk = |id: &str, status: &str, updated: u64| {
            serde_json::from_value::<super::TransferEntry>(serde_json::json!({
                "batchId": id, "direction": "send", "status": status, "updatedAtMs": updated,
            }))
            .unwrap()
        };
        let store = vec![
            mk("run", "running", 9),
            mk("b-old", "completed", 1),
            mk("pend", "pending", 8),
            mk("b-new", "failed", 5),
        ];
        let ids: Vec<&str> =
            super::history_view(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(ids, vec!["b-new", "b-old"]);
    }

    /// list-remote 根/相对路径解析：dirId 权威、根内同名子目录不误剥离、
    /// 旧形态回落（回归：strip_prefix 误命中致列错目录）
    #[test]
    fn split_root_rel_prefers_dir_id_and_keeps_root_relative_path() {
        // 根内子目录与根 id 同名：path 原样保留（旧实现误剥离成 ""，列出根目录）
        assert_eq!(super::split_root_rel("docs", "docs"), ("docs", "docs"));
        assert_eq!(super::split_root_rel("docs", "docs/sub"), ("docs", "docs/sub"));
        assert_eq!(
            super::split_root_rel("local-downloads", "a/b.txt"),
            ("local-downloads", "a/b.txt")
        );
        assert_eq!(super::split_root_rel("r", "/a/"), ("r", "a"));
        // 旧形态：缺失 dirId，path 首段即根
        assert_eq!(super::split_root_rel("", "r/sub"), ("r", "sub"));
        assert_eq!(super::split_root_rel("", "solo"), ("solo", ""));
    }

    /// 内存版宿主 mock：覆盖 mount_local / update_roots 所需 trait
    /// （storage 键值 + shared_roots 行集 + set-shared-roots 推送记录）
    struct MockHost {
        kv: RefCell<std::collections::HashMap<String, serde_json::Value>>,
        rows: RefCell<Vec<serde_json::Value>>,
        pushed_roots: RefCell<Vec<serde_json::Value>>,
        /// platform_pick_folders 预设返回值（多选目录对话框桩）
        picked: RefCell<Vec<String>>,
    }

    impl MockHost {
        fn new() -> Self {
            MockHost {
                kv: RefCell::new(std::collections::HashMap::new()),
                rows: RefCell::new(vec![]),
                pushed_roots: RefCell::new(vec![]),
                picked: RefCell::new(vec![]),
            }
        }

        fn preauth(&self) -> Vec<String> {
            load_preauth_paths(self).unwrap()
        }
    }

    impl HostStorage for MockHost {
        fn storage_get(&self, key: &str) -> Result<Option<serde_json::Value>, HostError> {
            Ok(self.kv.borrow().get(key).cloned())
        }
        fn storage_set(&self, key: &str, value: &serde_json::Value) -> Result<(), HostError> {
            self.kv.borrow_mut().insert(key.to_string(), value.clone());
            Ok(())
        }
        fn storage_delete(&self, key: &str) -> Result<(), HostError> {
            self.kv.borrow_mut().remove(key);
            Ok(())
        }
    }

    impl HostPluginDatabase for MockHost {
        fn plugin_db_execute(&self, sql: &str) -> Result<i32, HostError> {
            if sql.contains("CREATE TABLE IF NOT EXISTS shared_roots") {
                // 建表幂等：不触碰已有行（load_all 每次都会调 ensure_table）
                return Ok(0);
            }
            if sql == "DELETE FROM shared_roots" {
                self.rows.borrow_mut().clear();
                return Ok(0);
            }
            Ok(0)
        }
        fn plugin_db_query(&self, sql: &str) -> Result<Option<serde_json::Value>, HostError> {
            if sql.contains("SELECT id, name, path FROM shared_roots") {
                return Ok(Some(serde_json::Value::Array(self.rows.borrow().clone())));
            }
            Ok(None)
        }
        fn plugin_db_execute_params(&self, sql: &str, params: &[serde_json::Value]) -> Result<i32, HostError> {
            if sql.contains("INSERT INTO shared_roots") {
                self.rows.borrow_mut().push(serde_json::json!({
                    "id": params[0],
                    "name": params[1],
                    "path": params[2],
                }));
                return Ok(1);
            }
            Ok(0)
        }
        fn plugin_db_query_params(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Option<serde_json::Value>, HostError> {
            Ok(None)
        }
    }

    impl HostPeer for MockHost {
        fn peer_dial(&self, _endpoint: &serde_json::Value) -> Result<String, HostError> {
            unimplemented!()
        }
        fn peer_close(&self, _handle: &str) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_respond_consent(&self, _request_id: &str, _accepted: bool) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_list_trusted(&self) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_revoke_trusted(&self, _node_id: &str) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_send_files(&self, _session: &str, _paths: &[serde_json::Value]) -> Result<String, HostError> {
            unimplemented!()
        }
        fn peer_respond_transfer(&self, _batch_id: &str, _accept: bool) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_set_receive_policy(&self, _mode: &str, _timeout_secs: u64) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_set_shared_roots(&self, dirs: &[serde_json::Value]) -> Result<(), HostError> {
            self.pushed_roots.borrow_mut().clear();
            self.pushed_roots.borrow_mut().extend_from_slice(dirs);
            Ok(())
        }
        fn peer_list_shared_roots(&self, _session: &str) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_browse_directory(&self, _session: &str, _dir_id: &str, _rel_path: &str) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_pull_files(&self, _session: &str, _dir_id: &str, _files: &[serde_json::Value]) -> Result<u32, HostError> {
            unimplemented!()
        }
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), HostError> {
            unimplemented!()
        }
    }

    impl HostPlatform for MockHost {
        fn platform_pick_files(&self) -> Result<Vec<String>, HostError> {
            unimplemented!()
        }
        fn platform_pick_folder(&self) -> Result<String, HostError> {
            unimplemented!()
        }
        fn platform_pick_folders(&self) -> Result<Vec<String>, HostError> {
            Ok(self.picked.borrow().clone())
        }
    }

    /// mount-local 追加预授权路径：新增目录落 preauth_paths（宿主启用门禁读取）；
    /// 同路径重复挂载幂等（注册表 upsert + 预授权去重，均不重复）
    #[test]
    fn mount_local_appends_preauth_paths_and_dedupes() {
        let h = MockHost::new();
        let r1 = mount_local(&h, &serde_json::json!({ "path": "/tmp/share-x", "name": "x" })).unwrap();
        assert_eq!(r1["added"][0]["path"], "/tmp/share-x");
        assert_eq!(h.preauth(), vec!["/tmp/share-x".to_string()]);

        // 同路径重复挂载：预授权不重复、注册表不重复
        let r2 = mount_local(&h, &serde_json::json!({ "path": "/tmp/share-x", "name": "x" })).unwrap();
        assert_eq!(r2["added"][0]["id"], r1["added"][0]["id"]);
        assert_eq!(h.preauth(), vec!["/tmp/share-x".to_string()]);
        assert_eq!(roots_registry::load_all(&h).unwrap().len(), 1);

        // 第二个目录追加
        mount_local(&h, &serde_json::json!({ "path": "/tmp/share-y", "name": "y" })).unwrap();
        assert_eq!(h.preauth(), vec!["/tmp/share-x".to_string(), "/tmp/share-y".to_string()]);
        // 注册表与预授权保持一致（宿主收集的就是共享目录）
        let roots: Vec<String> = roots_registry::load_all(&h).unwrap().into_iter().map(|r| r.path).collect();
        assert_eq!(roots, h.preauth());
    }

    /// mount-local 多选：一次注册多个目录，注册表 + 预授权同步追加；
    /// 默认名取各路径末段（显式 name 仅单路径注册生效）
    #[test]
    fn mount_local_multi_pick_registers_all() {
        let h = MockHost::new();
        *h.picked.borrow_mut() =
            vec!["/tmp/multi-a".to_string(), "/tmp/media/multi-b".to_string()];
        let out = mount_local(&h, &serde_json::json!({})).unwrap();
        let added = out["added"].as_array().unwrap();
        assert_eq!(added.len(), 2);
        assert_eq!(added[0]["path"], "/tmp/multi-a");
        assert_eq!(added[0]["name"], "multi-a");
        assert_eq!(added[1]["name"], "multi-b");
        assert_eq!(
            h.preauth(),
            vec!["/tmp/multi-a".to_string(), "/tmp/media/multi-b".to_string()]
        );
        assert_eq!(roots_registry::load_all(&h).unwrap().len(), 2);
    }

    /// mount-local 取消选择：空数组视为用户取消，注册表与预授权不变
    #[test]
    fn mount_local_cancelled_when_no_folders_picked() {
        let h = MockHost::new();
        let err = mount_local(&h, &serde_json::json!({})).unwrap_err();
        assert!(err.to_string().contains("cancelled"));
        assert!(h.preauth().is_empty());
        assert!(roots_registry::load_all(&h).unwrap().is_empty());
    }

    /// update-roots 移除共享目录：预授权路径同步剔除；全删后回到空数组
    /// （宿主 file-transfer 门禁恢复「未配置共享目录」拒绝语义）
    #[test]
    fn update_roots_removes_preauth_path() {
        let h = MockHost::new();
        mount_local(&h, &serde_json::json!({ "path": "/tmp/share-a", "name": "a" })).unwrap();
        mount_local(&h, &serde_json::json!({ "path": "/tmp/share-b", "name": "b" })).unwrap();
        assert_eq!(h.preauth().len(), 2);

        let id_a = roots_registry::root_id("/tmp/share-a");
        let out = update_roots(&h, &serde_json::json!({ "remove": id_a })).unwrap();
        assert_eq!(out["removed"], true);
        assert_eq!(h.preauth(), vec!["/tmp/share-b".to_string()]);

        let id_b = roots_registry::root_id("/tmp/share-b");
        let out = update_roots(&h, &serde_json::json!({ "remove": id_b })).unwrap();
        assert_eq!(out["removed"], true);
        assert!(h.preauth().is_empty());
    }

    /// 移除不存在的 id：幂等，预授权不受影响
    #[test]
    fn update_roots_unknown_id_is_noop() {
        let h = MockHost::new();
        mount_local(&h, &serde_json::json!({ "path": "/tmp/share-c", "name": "c" })).unwrap();
        let out = update_roots(&h, &serde_json::json!({ "remove": "root-deadbeef" })).unwrap();
        assert_eq!(out["removed"], false);
        assert_eq!(h.preauth(), vec!["/tmp/share-c".to_string()]);
    }
}
