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
use bedcode_plugin_api_mobile::host::{HostEvents, HostLog, HostPeer, HostPlatform, HostStorage};
use bedcode_plugin_api_mobile::wasm_host::WasmHost;
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

/// 待挂载拉取元数据队列：(nodeId, spec)。`pull-files` **入队在前**、调
/// `peer-pull-files` 在后（票 08：引擎铸造 batchId 后`pull-started` 事件即
/// 回流，事后入队会错过挂载窗口）；新接收条目按 node × rel_path 匹配消费。
static PENDING_PULLS: OnceLock<Mutex<Vec<(String, RetryMeta)>>> = OnceLock::new();

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
                let restored = entries.len();
                let marked = restore_entries(h, &mut entries);
                h.log_info(&format!(
                    "restored {restored} transfers, {marked} marked interrupted"
                ));
                *guard = entries;
            }
            Err(e) => h.log_info(&format!("transfer store load deferred: {e}")),
        }
    }
    guard
}

/// 移动端持久层：host-storage 单键 JSON 数组（无 plugin-database 是现实约束；
/// 历史封顶 200 + 设备缓存 ≤50，整读改整写规模可控）
const ENTRIES_KEY: &str = "transfer_entries";

fn load_entries(h: &impl HostStorage) -> Result<Vec<TransferEntry>> {
    let Some(v) = h.storage_get(ENTRIES_KEY)? else {
        return Ok(vec![]);
    };
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// 全量回写（封顶 200 条，KV 整读改整写）。失败只记日志不向上抛：持久化
/// 退化不应打断数据面编排（下次变更会整读改整写重试）。
fn persist_entries(h: &(impl HostStorage + HostLog), entries: &[TransferEntry]) {
    let value = match serde_json::to_value(entries) {
        Ok(v) => v,
        Err(e) => {
            h.log_info(&format!("transfer entries serialize failed: {e}"));
            return;
        }
    };
    if let Err(e) = h.storage_set(ENTRIES_KEY, &value) {
        h.log_info(&format!("transfer entries persist failed: {e}"));
    }
}

/// 重启恢复：把在册进行中条目标注 `interrupted` 并回写（票 08 抽出为可测
/// seam——「载入 → 标注 → 落盘」三步此前只存在于 `OnceLock` 守卫内，
/// cargo test 无法驱动）。
///
/// 标注是**如实呈现**：原批随宿主进程消亡，保留 running 会让历史里永远挂着
/// 「正在传」的死条目。返回标注数量。
fn restore_entries(h: &(impl HostStorage + HostLog), entries: &mut [TransferEntry]) -> usize {
    let marked = transfer_store::mark_active_interrupted(entries);
    if marked > 0 {
        persist_entries(h, entries);
    }
    marked
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

/// 展示名解析（票 06）：设备快照（前端落盘的 device_snapshot）广播名优先，
/// 短指纹兜底——宿主不再解析展示名（发送侧 peerName 派生随任务表退役）
fn resolve_peer_name(h: &WasmHost, node_id: &str) -> String {
    let fallback = node_id.get(..8).unwrap_or(node_id).to_string();
    device_bridge::load_snapshot(h)
        .ok()
        .and_then(|entries| {
            entries
                .iter()
                .find(|d| d.node_id == node_id)
                .map(|d| d.device_name.clone())
                .filter(|n| !n.is_empty())
        })
        .unwrap_or(fallback)
}

/// 引擎原始事件归约入口（`peer:transfer-event` / `peer:receive-event`）：
/// 事件 → store 状态推进（建行/进度/终态/暂停同步），变更后统一 flush。建行
/// 锚点事件的展示名在此解析（I/O 锁外）。store 自此为双方向唯一任务真源。
pub(crate) fn reduce_and_emit(h: &WasmHost, direction: &str, event: &serde_json::Value) {
    let peer_name = event
        .get("nodeId")
        .and_then(|v| v.as_str())
        .map(|nid| resolve_peer_name(h, nid))
        .unwrap_or_default();
    let is_pull_started = event.get("kind").and_then(|v| v.as_str()) == Some("pull-started");
    let changed = {
        let mut guard = ensure_loaded(h);
        let changed = transfer_store::reduce_event(&mut guard, direction, event, &peer_name);
        // 拉取批次挂重试元数据：batch_id 由引擎铸造（pull-files 调用点拿不到），
        // 按 rel_path 匹配本插件发起的拉取意图
        changed | (is_pull_started && attach_pull_meta(&mut guard, event))
    };
    if !changed {
        return;
    }
    // send 方向终态/暂停会空出发送槽位：放行插件侧排队批（并发闸门自控）
    dispatch_pending_sends(h);
    let guard = ensure_loaded(h);
    flush(h, guard, true);
}

/// 接收方向事件入口（`peer:receive-event`，票 07）：引擎原始事件归约 +
/// auto 策略即时应答 + 节点停止的在册中断标注
///
/// `node-stopped` 额外清空 session 句柄表（票 08）：引擎通道关闭即节点下线，
/// 陈旧句柄会让后续数据面命令全打在死会话上（重拨路径 `ensure_target` 因此
/// 永远走不到）；endpoint memo 保留 —— 重启后按memo 重拨即可。
pub(crate) fn on_receive_event(h: &WasmHost, event: &serde_json::Value) {
    match event.get("kind").and_then(|v| v.as_str()) {
        Some("node-stopped") => {
            let marked = {
                let mut guard = ensure_loaded(h);
                transfer_store::mark_active_interrupted(&mut guard)
            };
            let stale: Vec<String> = device_bridge::drain_sessions();
            if marked > 0 || !stale.is_empty() {
                h.log_info(&format!(
                    "peer-net stopped: {marked} in-flight transfers marked interrupted, \
                     {} stale session handles dropped (endpoints kept for redial)",
                    stale.len()
                ));
            }
            let guard = ensure_loaded(h);
            flush(h, guard, marked > 0);
        }
        Some("offer-pending") => {
            // 先归约建行（视图出现待应答批），再按策略自动应答
            reduce_and_emit(h, "receive", event);
            auto_answer_offer(h, event);
        }
        _ => reduce_and_emit(h, "receive", event),
    }
}

/// 首屏兜底（票 06）：激活晚于事件时经 active-transfers 原语查询宿主在册
/// 活跃批，补最小占位行（peer_name/files 缺失，progress 事件随后补全）。
/// 原语不可用（节点未起）降级日志不阻断激活。
pub(crate) fn rebuild_from_active_transfers(h: &WasmHost) {
    let rows = match h.peer_active_transfers() {
        Ok(v) => v,
        Err(e) => {
            h.log_info(&format!("active-transfers bootstrap deferred (non-fatal): {e}"));
            return;
        }
    };
    let Some(arr) = rows.as_array() else { return };
    let inserted = {
        let mut guard = ensure_loaded(h);
        transfer_store::insert_active_projections(&mut guard, arr, "")
    };
    if inserted > 0 {
        h.log_info(&format!("active-transfers bootstrap: +{inserted} rows"));
        let guard = ensure_loaded(h);
        flush(h, guard, true);
    }
}

/// 拉取批次的重试元数据挂载（票 07 建锚点，票 08 收窄为「凭证取出」）：
/// `pull-started` 事件的 rel_path × 待挂载意图队列匹配——命中即把该单文件
/// 规格与共享根 id 写入 retryMeta 并消费队列项（多选拉取逐文件成批，故按
/// 单文件匹配而非整批文件集相等）
fn attach_pull_meta(store: &mut [TransferEntry], event: &serde_json::Value) -> bool {
    let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
    let Some(node_id) = event.get("nodeId").and_then(|v| v.as_str()) else { return false };
    let rel_path = event
        .get("files")
        .and_then(|v| v.as_array())
        .and_then(|files| files.first())
        .and_then(|f| f.get("path"))
        .and_then(|v| v.as_str());
    let Some(rel_path) = rel_path else { return false };
    let Some(entry) = store
        .iter_mut()
        .find(|e| e.batch_id == batch_id && e.retry_meta.is_none())
    else {
        return false;
    };
    let intent = transfer_store::take_pull_intent(
        &mut pending_pulls().lock().expect("pending pulls lock"),
        node_id,
        rel_path,
    );
    match intent {
        Some(meta) => {
            entry.retry_meta = Some(meta);
            true
        }
        // 未命中：引擎铸造的批不属于本插件（如对端发起的入站供流），如实不挂
        None => false,
    }
}

/// auto 分支自动应答（accept/reject 策略）：入站询问批建行后立即
/// respond-transfer。ask 分支不动作（弹窗编排在 UI 层，倒计时到点由前端
/// 主动应答；宿主闸门 sweeper 兜底不变）。
fn auto_answer_offer(h: &WasmHost, event: &serde_json::Value) {
    let policy = match settings_store::load(h) {
        Ok(p) => p,
        Err(_) => return,
    };
    let Some(answer) = settings_store::auto_answer(&policy) else {
        return;
    };
    let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return };
    if h.peer_respond_transfer(batch_id, answer).is_ok() {
        h.log_info(&format!("auto-answer ({answer}) batch {batch_id}"));
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

/// 元素级载荷构造：`{ path, encrypt }`（加密默认值取插件设置）。
/// `concurrency` 并发脉冲字段已随宿主闸门退役（票 06：载荷携带即被宿主
/// 显性拒绝）——发送节流完全由插件侧闸门自控
fn send_payload(settings: &TransferSettings, paths: &[String]) -> Vec<serde_json::Value> {
    paths
        .iter()
        .map(|p| serde_json::json!({ "path": p, "encrypt": settings.encryption }))
        .collect()
}

/// 发送批发起（闸门放行后的公共路径）：解析目标 → send-files → 入店占位
fn launch_send(
    h: &WasmHost,
    node_id: &str,
    endpoint: Option<DialEndpoint>,
    paths: Vec<String>,
) -> Result<serde_json::Value> {
    let target = ensure_target(h, node_id, endpoint)?;
    let settings = settings_store::load(h)?;
    let batch_id = h.peer_send_files(&target, &send_payload(&settings, &paths))?;
    // 返回已收窄为传输句柄；先入店最小条目占位，引擎事件随即补全明细
    insert_send_entry(h, node_id, &batch_id, RetryMeta::Send { paths })
}

/// 发送方向 running 批数（插件并发闸门的槽位口径；与宿主闸门同语义：
/// 只数本端发起的 running，serve 供流记账行不计——它无 retryMeta 且属
/// 响应式供流）
fn running_send_count(guard: &[TransferEntry]) -> usize {
    guard
        .iter()
        .filter(|e| e.direction == "send" && e.status == "running" && e.retry_meta.is_some())
        .count()
}

/// 插件侧排队中的发送批（并发槽位满时压入；终态/暂停归约空出槽位后放行）
struct PendingSend {
    node_id: String,
    endpoint: Option<DialEndpoint>,
    paths: Vec<String>,
    /// 已派发失败次数（票 08：失败不再静默丢弃，见 [`MAX_SEND_ATTEMPTS`]）
    attempts: u8,
}

static PENDING_SENDS: OnceLock<Mutex<Vec<PendingSend>>> = OnceLock::new();

fn pending_sends() -> &'static Mutex<Vec<PendingSend>> {
    PENDING_SENDS.get_or_init(|| Mutex::new(Vec::new()))
}

/// 排队上限（防失控积累；超出即拒绝新任务——与 `PULL_INTENT_CAP` 同风格）
const PENDING_SENDS_CAP: usize = 32;

/// 排队批最大派发尝试次数（票 08）
///
/// 排队批在 store 里**没有行**（引擎未铸造 batchId），失败即静默丢用户意图。
/// 故给有限次重试（对端临时不可达 / 句柄陈旧时重拨可能成功），用尽后落一条
/// 带 `retry_meta` 的终态失败行——用户从历史看得见、能重试。
const MAX_SEND_ATTEMPTS: u8 = 2;

/// 发送槽位是否空出（插件闸门唯一判据；设置真源 = 插件 storage 的
/// `concurrency`，缺省 3）
fn send_slot_available(h: &WasmHost) -> bool {
    let guard = ensure_loaded(h);
    let limit = settings_store::load(h)
        .map(|s| settings_store::clamp_concurrency(s.concurrency))
        .unwrap_or(settings_store::DEFAULT_CONCURRENCY);
    transfer_store::send_slot_open(running_send_count(&guard), limit)
}

/// 并发闸门放行：running < 设置并发上限时逐个出队发起（每次发起后槽位
/// 再检查）。派发失败按 [`MAX_SEND_ATTEMPTS`] 重排队，用尽后落终态失败行
/// （票 08：排队批不再静默消失）。
fn dispatch_pending_sends(h: &WasmHost) {
    loop {
        if !send_slot_available(h) {
            return;
        }
        let Some(job) = pending_sends().lock().expect("pending sends lock").pop() else {
            return;
        };
        match launch_send(h, &job.node_id, job.endpoint.clone(), job.paths.clone()) {
            Ok(_) => h.log_info("pending send dispatched (plugin-side gate)"),
            Err(e) => {
                let detail = e.to_string();
                if job.attempts + 1 < MAX_SEND_ATTEMPTS {
                    h.log_error(&format!(
                        "pending send dispatch failed (attempt {}/{}), requeued: {detail}",
                        job.attempts + 1,
                        MAX_SEND_ATTEMPTS
                    ));
                    let mut queue = pending_sends().lock().expect("pending sends lock");
                    queue.push(PendingSend {
                        attempts: job.attempts + 1,
                        ..job
                    });
                    return;
                }
                // 用尽尝试：落终态行（带回放凭证），用户从历史可见可重试
                h.log_error(&format!(
                    "pending send dispatch abandoned after {} attempts: {detail}",
                    MAX_SEND_ATTEMPTS
                ));
                if let Err(write_err) =
                    insert_failed_send_entry(h, &job.node_id, job.paths, &detail)
                {
                    h.log_error(&format!(
                        "pending send failure row persist failed: {write_err}"
                    ));
                }
                // 本批已落终态，槽位仍空 → 继续放行下一批
            }
        }
    }
}

/// 清空进程内意图（插件下线，票 08）：排队发送批与待挂载拉取意图随停用作废
///
/// 两者都只存在于内存：无行可查、用户无从重试，留到下次激活就是「凭空冒出
/// 的旧任务」。任务真源（store）不在此列——它由持久层承接，停用不丢历史。
pub(crate) fn reset_volatile_intents() -> (usize, usize) {
    let queued = {
        let mut queue = pending_sends().lock().expect("pending sends lock");
        let n = queue.len();
        queue.clear();
        n
    };
    let intents = {
        let mut pending = pending_pulls().lock().expect("pending pulls lock");
        let n = pending.len();
        pending.clear();
        n
    };
    (queued, intents)
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

    // 并发闸门自控（票 06）：发送编排归插件——running 批满额时新任务进
    // 本地队列（不调 send-files），槽位空出后 dispatch 放行
    if send_slot_available(h) {
        launch_send(h, &node_id, endpoint, paths)
    } else {
        let mut queue = pending_sends().lock().expect("pending sends lock");
        if queue.len() >= PENDING_SENDS_CAP {
            anyhow::bail!("send queue full ({} pending): raise concurrency or retry later", PENDING_SENDS_CAP);
        }
        queue.push(PendingSend {
            node_id,
            endpoint,
            paths,
            attempts: 0,
        });
        h.log_info("send queued by plugin-side concurrency gate");
        Ok(serde_json::json!({ "queued": true }))
    }
}

/// 发送入店（宿主只回传输句柄）——最小条目占位 + retryMeta，
/// 引擎事件到达后按 batchId 补全文件清单/大小/时间戳
fn insert_send_entry(
    h: &WasmHost,
    node_id: &str,
    batch_id: &str,
    meta: RetryMeta,
) -> Result<serde_json::Value> {
    insert_send_row(h, send_row(node_id, batch_id, "running", meta, None))
}

/// 本端发起的 send 行构造（发起与「排队批派发失败落终态」共用一个形状单点）
fn send_row(
    node_id: &str,
    batch_id: &str,
    status: &str,
    meta: RetryMeta,
    detail: Option<String>,
) -> TransferEntry {
    TransferEntry {
        batch_id: batch_id.to_string(),
        node_id: node_id.to_string(),
        peer_name: String::new(),
        direction: "send".to_string(),
        status: status.to_string(),
        files: vec![],
        total_bytes: 0,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail,
        reject_reason: None,
        // wasm32 无本地时钟（禁std::time）：时间戳由引擎事件补全；
        // 本地终态行以 0 落历史末尾（排序口径见 history_view）
        created_at_ms: 0,
        updated_at_ms: 0,
        retry_meta: Some(meta),
        local_path: None,
    }
}

/// 排队发送批派发失败后的终态行（票 08）
///
/// batchId 由本地序号铸造（引擎从未见过这批——它连`send-files` 都没调通）；
/// 携带 retryMeta 故用户可从历史直接重试。返回落库后的行 JSON。
fn insert_failed_send_entry(
    h: &WasmHost,
    node_id: &str,
    paths: Vec<String>,
    detail: &str,
) -> Result<serde_json::Value> {
    let batch_id = next_local_failure_id();
    insert_send_row(
        h,
        send_row(
            node_id,
            &batch_id,
            "failed",
            RetryMeta::Send { paths },
            Some(format!("send dispatch failed: {detail}")),
        ),
    )
}

/// 本地终态行序号（wasm32 无时钟，用进程内自增序号保证 batchId 唯一）
fn next_local_failure_id() -> String {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    format!("local-fail-{n}")
}

/// 行入店统一出口（幂等 upsert + 封顶 + 持久化 + 视图派发）
fn insert_send_row(h: &WasmHost, entry: TransferEntry) -> Result<serde_json::Value> {
    let out = serde_json::to_value(&entry)?;
    let mut guard = ensure_loaded(h);
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

/// 显式暂停：宿主中断会话（任务保留含已传字节、不落历史），本地乐观标记
/// paused 供前端即时反馈；引擎快照随后以 paused 状态合并确认。
pub(crate) fn pause_task(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let batch_id =
        args.get("taskId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    h.peer_pause_transfer(&batch_id)?;
    let mut guard = ensure_loaded(h);
    let changed = transfer_store::mark_paused(&mut guard, &batch_id);
    flush(h, guard, changed);
    Ok(serde_json::json!({ "ok": true }))
}

/// 恢复单个暂停任务：宿主按句柄表续流或重拨（接收端按已写偏移续传）。
pub(crate) fn resume_task(h: &WasmHost, args: &serde_json::Value) -> Result<serde_json::Value> {
    let batch_id =
        args.get("taskId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    h.peer_resume_transfer(&batch_id)?;
    // 状态推进由引擎事件归约接管，本地无需乐观改写
    Ok(serde_json::json!({ "ok": true }))
}

/// 恢复全部暂停任务（票 06：批量恢复编排归插件——遍历自身暂停批逐个调
/// resume-transfer；宿主 `resume-all-transfers` 原语已退役，并发节流由
/// 插件侧闸门自控）。返回实际恢复数。
pub(crate) fn resume_all_tasks(h: &WasmHost) -> Result<serde_json::Value> {
    let paused_ids: Vec<String> = {
        let guard = ensure_loaded(h);
        guard
            .iter()
            .filter(|e| e.direction == "send" && e.status == "paused")
            .map(|e| e.batch_id.clone())
            .collect()
    };
    let mut resumed = 0usize;
    for batch_id in paused_ids {
        if h.peer_resume_transfer(&batch_id).is_ok() {
            resumed += 1;
        }
    }
    Ok(serde_json::json!({ "resumed": resumed }))
}

/// 重试 = 批元数据回放重调原语：
/// - send 条目：回放 send-files，新 batchId 顶替原条目（同一条历史）；
/// - pull 条目：回放 pull-files（逐文件新批自然入账），原失败记录保留为历史。
///
/// 票 08 重排了次序——**判据与闸门前置到调引擎之前**：
/// ① [`transfer_store::retry_source`] 先判可重试（终态 + 有回放凭证）；
/// ② send 方向先过插件并发闸门。原实现先 `send-files` 再 `apply_retry`，
/// 对不可重试条目会铸出无主会话：send 方向没有引擎建行事件（行由插件发起
/// 时自建），该会话永不入店、进度事件打不中行，并永久占死一个发送槽位。
pub(crate) fn retry_task(
    h: &WasmHost,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let task_id =
        args.get("taskId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let endpoint = parse_endpoint(args.get("endpoint"))?;

    let (node_id, meta) = {
        let guard = ensure_loaded(h);
        transfer_store::retry_source(&guard, &task_id)
            .map_err(|why| anyhow::anyhow!(why.message(&task_id)))?
    };
    // send 方向回放也占发送槽位：闸门满时显性拒绝（不排队——重试是用户对
    // 特定历史行的显式操作，排队会让「点了没反应」无从追因）
    if matches!(meta, RetryMeta::Send { .. }) && !send_slot_available(h) {
        anyhow::bail!(
            "retry rejected: send concurrency full, wait for a running transfer to finish"
        );
    }
    let target = ensure_target(h, &node_id, endpoint)?;

    match meta {
        RetryMeta::Send { paths } => {
            let settings = settings_store::load(h)?;
            let new_batch = h.peer_send_files(&target, &send_payload(&settings, &paths))?;
            let now = 0u64; // 时间戳由引擎事件补全（updated_at_ms 单调性由 merge 保证）
            let mut guard = ensure_loaded(h);
            // retryMeta 由 apply_retry 保留（新批仍是本端发起，仍可再重试）
            let changed = transfer_store::apply_retry(&mut guard, &task_id, &new_batch, now);
            if !changed {
                // 判据已前置，理论上不可达；显性报错而非静默留无主会话
                anyhow::bail!("retry replay lost its entry (concurrent mutation?): {task_id}");
            }
            flush(h, guard, true);
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
            // 意图先于引擎调用入队（票 08）：`pull-started` 事件随引擎铸造
            // batchId 即回流，事后入队会错过挂载窗口 → 新行永不可重试
            transfer_store::push_pull_intent(
                &mut pending_pulls().lock().expect("pending pulls lock"),
                &node_id,
                RetryMeta::Pull {
                    dir_id: dir_id.clone(),
                    files: files.clone(),
                },
            );
            let n = h.peer_pull_files(&target, &dir_id, &values)?;
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

/// 历史列表命令：与 history-changed 事件同形状（前端 refreshReceiving 初始快照）
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
    // 破坏性操作留痕：真机「点了没反应」需要能区分「命令没到」与「到了没清」
    h.log_info(&format!("clear-history cleared {cleared} terminal entries"));
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
    Ok(listing)
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

    // 意图先于引擎调用入队（票 08）：`pull-started` 事件随引擎铸造 batchId
    // 即回流，事后入队会错过挂载窗口 → 新行永不可重试
    transfer_store::push_pull_intent(
        &mut pending_pulls().lock().expect("pending pulls lock"),
        &node_id,
        RetryMeta::Pull {
            dir_id: dir_id.clone(),
            files: specs.clone(),
        },
    );
    let n = h.peer_pull_files(&target, &dir_id, &values)?;
    Ok(serde_json::json!({ "count": n }))
}

// ==================== 设置 / 注册表命令 ====================

/// SAF 树 URI → 展示名：末段 URL 解码 + 去卷前缀
///
/// `content://com.android.externalstorage.documents/tree/primary%3ADownload`
/// → 末段 `primary%3ADownload` → 解码 `primary:Download` → 剥离 `primary:`
/// → `Download`（SD 卡卷号 `ABCD-1234:Music` 同理）。解码/剥离失败回退原文。
fn tree_uri_display_name(uri: &str) -> String {
    let last = uri
        .trim_end_matches('/')
        .rsplit('/')
        .find(|seg| !seg.is_empty())
        .unwrap_or("");
    let decoded = percent_decode(last);
    match decoded.split_once(':') {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => decoded,
    }
}

/// 最简 percent-decode（%XX → 字节；UTF-8 校验失败回退原文）
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn get_settings(h: &WasmHost) -> Result<serde_json::Value> {
    let s = settings_store::load_or_migrate(h)?;
    let roots = roots_registry::load_all(h)?;
    // 内置条目 local-downloads 由引擎注入、不进注册表；此处以只读形状合并展示
    // （前端按 builtin 标记渲染「私有下载」分区，不可移除）
    let builtin = serde_json::json!({
        "id": "local-downloads", "name": "下载", "builtin": true, "treeUri": "",
    });
    let registry: Vec<serde_json::Value> = roots
        .iter()
        .map(|r| serde_json::json!({
            "id": r.id, "name": r.name, "builtin": false, "treeUri": r.path,
        }))
        .collect();
    let mut all_roots = vec![builtin];
    all_roots.extend(registry);
    Ok(serde_json::json!({
        "roots": all_roots,
        "policy_mode": settings_store::policy_to_host(&s.receiving_policy),
        "ask_timeout_sec": settings_store::clamp_timeout(s.approval_timeout_sec),
        // 移动端固定落点 MediaStore.Downloads（只读展示，不支持自定义）
        "download_dir": "MediaStore/Downloads",
        "encryption": s.encryption,
        "concurrency": s.concurrency,
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
    // downloadDir 移动端不支持（固定 MediaStore.Downloads），静默忽略
    if let Some(enabled) = args.get("encryption").and_then(|v| v.as_bool()) {
        s.encryption = enabled;
    }
    if let Some(n) = args.get("concurrency").and_then(|v| v.as_u64()) {
        s.concurrency = settings_store::clamp_concurrency(n as u8);
    }
    settings_store::save_and_push(h, &s)?;
    h.log_info("set-settings: policy/encryption applied (plugin-sourced, mobile)");
    Ok(serde_json::json!({ "ok": true }))
}

/// 添加共享目录（Mobile）：弹 SAF 目录树选择器（host-platform.pick-folder），
/// 授权与注册表均由本插件持有（id=URI 哈希，同根天然去重）；推送失败自动回滚。
/// 用户取消以错误上抛（调用方按 'cancelled' 字样分流）。
///
/// 宿主返回 SAF 树 URI（content://.../tree/...，WIT 契约）——引擎共享目录
/// 注册表校验 SAF 根为 content://，真实路径会被整批拒绝导致「多次选择只显
/// 示一条」；展示名由树 URI 末段解码（卷前缀剥离）派生。
pub(crate) fn mount_local(
    h: &(impl HostPlatform + HostStorage + HostPeer),
    _args: &serde_json::Value,
) -> Result<serde_json::Value> {
    let uri = h.platform_pick_folder()?;
    if uri.is_empty() {
        anyhow::bail!("cancelled");
    }
    // 展示名：树 URI 末段 URL 解码 + 去卷前缀（primary:Download → Download）；
    // 空则固定占位
    let name = tree_uri_display_name(&uri);
    let name = if name.is_empty() { "共享目录".to_string() } else { name };
    let entry = SharedRoot { id: roots_registry::root_id(&uri), name, path: uri.clone() };
    let next = roots_registry::apply_and_push(h, |list| {
        roots_registry::upsert(list, entry.clone());
    })?;
    let added = next.iter().find(|r| r.id == entry.id);
    if let Some(r) = &added {
        // 预授权路径与共享目录同步：追加后宿主启用插件时才能收集到（缺失时
        // 启用命中「请先配置共享目录」门禁）。storage 写失败如实上抛——共享目录
        // 已落库但预授权缺失会再次锁死启用，宁可让用户看到错误重试
        push_preauth_path(h, &r.path)?;
    }
    Ok(added
        .map(|r| serde_json::json!({ "id": r.id, "name": r.name, "treeUri": r.path }))
        .unwrap_or_default())
}

/// 移除共享目录：args.remove = 条目 id；推送失败自动回滚
pub(crate) fn update_roots(
    h: &(impl HostStorage + HostPeer),
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
    use super::roots_registry::{self, SharedRoot};
    use super::{load_preauth_paths, percent_decode, push_preauth_path, tree_uri_display_name, update_roots};
    use bedcode_plugin_api_mobile::host::{HostError, HostPeer, HostPlatform, HostStorage};
    use std::cell::RefCell;
    // 票 08 新增分组（redial 场景链 / 终态历史持久化往返）拆至`peer/tests/`
    mod redial_and_history;

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

    /// 内存版宿主 mock：移动端 roots_registry 走 HostStorage 键值 + HostPeer 推送
    struct MockHost {
        kv: RefCell<std::collections::HashMap<String, serde_json::Value>>,
        pushed_roots: RefCell<Vec<serde_json::Value>>,
        logs: RefCell<Vec<String>>,
    }

    impl MockHost {
        fn new() -> Self {
            MockHost {
                kv: RefCell::new(std::collections::HashMap::new()),
                pushed_roots: RefCell::new(vec![]),
                logs: RefCell::new(vec![]),
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
        fn peer_pause_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_resume_transfer(&self, _batch_id: &str) -> Result<(), HostError> {
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
        // 票 04 新增 5 原语（票 06 补齐 mock：trait 扩容后 mock 必须同步，
        // 否则测试编译不过——缺方法的 mock 会把真实调用点挡在编译期之外）
        fn peer_set_download_dir(&self, _path: &str) -> Result<(), HostError> {
            unimplemented!()
        }
        fn peer_start_node(&self) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_stop_node(&self) -> Result<bool, HostError> {
            unimplemented!()
        }
        fn peer_active_transfers(&self) -> Result<serde_json::Value, HostError> {
            unimplemented!()
        }
        fn peer_collect_outgoing(&self, _paths: &[serde_json::Value]) -> Result<serde_json::Value, HostError> {
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
    }

    /// 票 08：持久化 seam（`persist_entries` / `restore_entries`）要求
    /// `HostStorage + HostLog`——恢复路径要落盘也要记「标注了几条」。
    /// 日志收进`logs` 供断言（如实呈现而非静默）。
    impl bedcode_plugin_api_mobile::host::HostLog for MockHost {
        fn log_info(&self, message: &str) {
            self.logs.borrow_mut().push(message.to_string());
        }
        fn log_debug(&self, _message: &str) {}
        fn log_warn(&self, message: &str) {
            self.logs.borrow_mut().push(message.to_string());
        }
        fn log_error(&self, message: &str) {
            self.logs.borrow_mut().push(message.to_string());
        }
        fn mark_plugin_error(&self, _error: &str) {}
    }

    /// mount-local（SAF URI）追加预授权路径；重复挂载幂等
    #[test]
    fn mount_local_appends_preauth_paths_and_dedupes() {
        let h = MockHost::new();
        let uri = "content://com.android.externalstorage.documents/tree/primary%3AShareX";
        // 移动端 mount_local 走系统选择器（mock 不支持），改为直接验证
        // preauth 读写原语与 update_roots 组合；mount 侧用共享根注入模拟
        roots_registry::apply_and_push(&h, |list| {
            roots_registry::upsert(
                list,
                SharedRoot { id: roots_registry::root_id(uri), name: "ShareX".into(), path: uri.into() },
            );
        })
        .unwrap();
        push_preauth_path(&h, uri).unwrap();
        push_preauth_path(&h, uri).unwrap();
        assert_eq!(h.preauth(), vec![uri.to_string()]);

        // update-roots 移除：预授权同步剔除
        let out = update_roots(&h, &serde_json::json!({ "remove": roots_registry::root_id(uri) })).unwrap();
        assert_eq!(out["removed"], true);
        assert!(h.preauth().is_empty());
    }

    /// 移除不存在 id：幂等 no-op
    #[test]
    fn update_roots_unknown_id_is_noop() {
        let h = MockHost::new();
        let uri = "content://com.android.externalstorage.documents/tree/primary%3AShareZ";
        roots_registry::apply_and_push(&h, |list| {
            roots_registry::upsert(
                list,
                SharedRoot { id: roots_registry::root_id(uri), name: "ShareZ".into(), path: uri.into() },
            );
        })
        .unwrap();
        push_preauth_path(&h, uri).unwrap();
        let out = update_roots(&h, &serde_json::json!({ "remove": "root-deadbeef" })).unwrap();
        assert_eq!(out["removed"], false);
        assert_eq!(h.preauth(), vec![uri.to_string()]);
    }

    /// SAF 树 URI → 展示名：URL 解码 + 卷前缀剥离
    #[test]
    fn tree_uri_display_name_decodes_and_strips_volume() {
        assert_eq!(
            tree_uri_display_name(
                "content://com.android.externalstorage.documents/tree/primary%3ADownload"
            ),
            "Download"
        );
        // 嵌套目录：末段解码后含路径分隔，保留路径部分
        assert_eq!(
            tree_uri_display_name(
                "content://com.android.externalstorage.documents/tree/primary%3ADownload%2FFoo"
            ),
            "Download/Foo"
        );
        // SD 卡卷号
        assert_eq!(
            tree_uri_display_name(
                "content://com.android.externalstorage.documents/tree/ABCD-1234%3AMusic"
            ),
            "Music"
        );
        // 已解码段（无 %）直接剥离卷前缀；无卷前缀原样返回
        assert_eq!(tree_uri_display_name("content://x/tree/primary:DCIM"), "DCIM");
        assert_eq!(tree_uri_display_name("content://x/tree/plain"), "plain");
        // 空/尾斜杠 → 空（尾斜杠被 trim 后取末段，非空段即其名）
        assert_eq!(tree_uri_display_name(""), "");
        assert_eq!(tree_uri_display_name("content://x/tree/"), "tree");
    }

    /// percent-decode：合法 %XX 解码，非法/孤立 % 原样保留，非 UTF-8 回退原文
    #[test]
    fn percent_decode_handles_valid_and_broken_escapes() {
        assert_eq!(percent_decode("primary%3ADownload"), "primary:Download");
        assert_eq!(percent_decode("a%2Fb%20c"), "a/b c");
        assert_eq!(percent_decode("%GG"), "%GG");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%E4%B8%AD%E6%96%87"), "中文");
    }

    /// 插件并发闸门的槽位口径（票 06）：只数「本端发起且 running 且带
    /// retryMeta」的批——serve 供流记账行（无 retryMeta）、paused/pending/
    /// 终态行、以及接收方向都不占发送槽
    #[test]
    fn running_send_count_counts_only_initiated_running_sends() {
        let mk = |id: &str, direction: &str, status: &str, retry: bool| {
            let mut e: super::TransferEntry = serde_json::from_value(serde_json::json!({
                "batchId": id, "direction": direction, "status": status,
            }))
            .unwrap();
            if retry {
                e.retry_meta = Some(super::RetryMeta::Send { paths: vec!["C:/a.txt".into()] });
            }
            e
        };
        let store = vec![
            mk("run-1", "send", "running", true),
            mk("run-2", "send", "running", true),
            // serve 供流记账行：无 retryMeta（响应式供流，不占发起方向槽位）
            mk("serve", "send", "running", false),
            mk("paused", "send", "paused", true),
            mk("pend", "send", "pending", true),
            mk("done", "send", "completed", true),
            mk("recv", "receive", "running", true),
        ];
        assert_eq!(super::running_send_count(&store), 2);
    }
}
