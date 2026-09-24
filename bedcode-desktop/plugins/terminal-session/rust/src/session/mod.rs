//! 会话登记域（会话引擎整体下沉 P1）——**会话真源 = 本插件**（P1-b 起）
//!
//! 宿主 `session/`（3759 行）的会话真源职责已迁入本域：登记 / 状态机 /
//! 生命周期分发 / 输出消费 / 输入转发。宿主只剩 PTY 引擎 + `host-pty` 原语。
//! 分阶段实施（spec `.scratch/2026-09-23-session-engine-downsink`）：
//!
//! - **P1-a**：登记 / 状态机 / 私有库真源落地 + 双写（宿主仍是权威）——已 land；
//! - **P1-b（本阶段）**：创建改走 `host-pty.spawn`（会话 id 由本插件自产、
//!   `BEDCODE_SESSION_ID` 由本插件注入、`pty_id` 落库），本域转权威；
//!   宿主窄转发层改经互调 api（`session-list` / `session-get` / `session-create` /
//!   `session-close` / `session-input` / `session-remove` / `session-rename` /
//!   `session-resize`）读会话事实；
//! - P1-c / P4：宿主 `session/` 整体退役（目录删除、host-session 12 原语退役）。
//!
//! ## P1-b 的诚实记账（不再有「镜像」）
//!
//! 双写期的 note_* 门面语义升级：从「按宿主事实记账的镜像」变为「真源写入」——
//! 宿主不再持有会话登记，本域记录是唯一事实；`sessions` / `session_annotations`
//! 两张私有库表是落盘真源，内存注册表是它的镜像（写库先行）。
//!
//! ## 生命周期驱动（P1-b 起由本域自持）
//!
//! 宿主 `SessionManager` 不再产生会话生命周期事件；本域改为自驱动：
//!
//! | 事实来源 | 覆盖 |
//! | --- | --- |
//! | 创建编排（`launch::create_via_host`） | Creating（agent 集成先于 spawn）/ Created / 广播 SessionCreated |
//! | 停止编排（[`close_via_pty`]） | Stopping（同步，kill 已发起） |
//! | `<owner>::pty:exit` 总线事件（[`on_pty_exit`]） | Stopped / Error 终态 + 广播 SessionStopped + 任务域收尾 |
//! | 动作编排（`actions`） | 移除 / 改名 / 归属登记 / 重启 |
//!
//! **终态单一发布者不变量**：`SessionStopped` 广播只在 [`on_pty_exit`] 一处发出
//! （自然退出与 kill 同路径），避免「kill 发起即广播 + 退出事件再广播」的双发竞态。
//!
//! ## 失败口径
//!
//! 真源写入失败**必须**显性可见（不再是双写期的 warn 降级）：会话面不可用时
//! 创建 / 停止 / 输入都应报错回给调用方；读取失败同样显性。只有**广播**是
//! 尽力投递（移动端同步通道故障不阻断会话面，warn 留痕）。

pub mod events;
pub mod input_line;
pub mod model;
pub mod ops;
pub mod registry;
pub mod store;
pub mod view;

use crate::actions::RendererSource;
use model::{SessionRecord, SessionStatus};

#[cfg(target_arch = "wasm32")]
use std::sync::Mutex;

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::bus::owned_topic;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostPty};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;
#[cfg(target_arch = "wasm32")]
use registry::Registry;
#[cfg(target_arch = "wasm32")]
use store::SessionStore;

/// 进程级注册表实例（wasm 侧全插件共用一份；同实例串行访问，无需额外并发协议）
#[cfg(target_arch = "wasm32")]
static REGISTRY: Registry = Registry::new();

/// 提交行重建器（进程级；`session-input` 写入管线与停会话清理共用）
#[cfg(target_arch = "wasm32")]
static LINE_TRACKER: std::sync::OnceLock<input_line::SubmittedLineTracker> =
    std::sync::OnceLock::new();

/// 提交行重建器访问（懒初始化；同实例串行访问）
#[cfg(target_arch = "wasm32")]
fn line_tracker() -> &'static input_line::SubmittedLineTracker {
    LINE_TRACKER.get_or_init(input_line::SubmittedLineTracker::new)
}

// ==================== 激活期装配（建表 + 进程域对账） ====================

/// 建表（幂等）+ 进程启动对账（清空上一进程遗留的会话行）
///
/// 对账理由：会话与其 PTY 同生命周期（P1-b 起 PTY 在宿主引擎注册表，同为进程
/// 内存），进程重启后旧会话记录不可回收，留着只会让读取面看到幽灵会话。清表后
/// 本域记录集合与当次进程的会话集合同域。
///
/// **P1-b 起失败必须阻断激活**（不再是双写期的「降级镜像」）：本域已是会话真源，
/// 建表失败 = 会话面不可用，继续激活只会让创建/停止/输入全线报「存储不可用」的
/// 半生不熟状态——显性失败让宿主把插件标记为不可用，用户路径立刻可见。
#[cfg(target_arch = "wasm32")]
pub fn ensure_schema_via_host() -> Result<(), String> {
    SessionStore::ensure_schema(&WasmHost)?;
    let stale = REGISTRY.clear(&WasmHost)?;
    if stale > 0 {
        WasmHost.log_warn(&format!(
            "session registry: dropped {stale} stale session row(s) from previous process (sessions are process-scoped)"
        ));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_schema_via_host() -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

// ==================== 真源写入（P1-b 起：本域即权威，失败显性） ====================

/// 会话概要（**类型化真源**）
///
/// 专项票 02 起 SDK `SyncEvent::SessionCreated.session` 就是这个类型本身
/// （`SyncEvent` 与宿主 `SyncPayload` 同 wire），不再有「宿主拿 Value 试着解析
/// 一下」的中间态——字段名与本域视图的对齐由编译期保证。
///
/// - `status` 是**展示字符串**（[`SessionStatus::wire_name`]）：状态机在本域，
///   折算成移动端要看的字符串是生产者的活，宿主不解读、也不兜底成空值。
///   `Error` 的描述文本不进 `status`（该字段形状是 String，塞对象会让整条载荷
///   解析失败）；错误详情留在本域记录与 `session-get` 视图。
/// - `task_status` / `task_reason` 取自注解槽（空串 = 缺失，与视图同判据）
#[cfg(target_arch = "wasm32")]
fn summary_view(record: &SessionRecord) -> bedcode_plugin_api::wire::SessionSummary {
    let annotations = REGISTRY
        .annotations(&WasmHost, &record.id)
        .unwrap_or_default();
    let slot = |key: &str| {
        annotations
            .get(key)
            .filter(|v| !v.is_empty())
            .cloned()
    };
    bedcode_plugin_api::wire::SessionSummary {
        id: record.id.clone(),
        name: record.name.clone(),
        status: record.status.wire_name(),
        created_at: record.created_at.clone(),
        started_at: record.started_at.clone(),
        session_type: Some("pty".to_string()),
        config_id: (!record.config_id.is_empty()).then(|| record.config_id.clone()),
        task_status: slot("taskStatus"),
        task_reason: slot("taskReason"),
    }
}

/// 互调 api / WS 控制面 `session_list` 的逐条形状
///
/// = [`summary_view`] 的 JSON 投影**去掉 null 键**：「缺值不出现键」是这一路的
/// 历史形状（同步载荷那一路走类型化形状，`started_at` 会出 null），两条路的差异
/// 是既存的，本票按字节保持不变。
#[cfg(target_arch = "wasm32")]
fn summary_json(record: &SessionRecord) -> serde_json::Value {
    let mut value = serde_json::to_value(summary_view(record)).expect("SessionSummary 可序列化");
    if let Some(obj) = value.as_object_mut() {
        obj.retain(|_, v| !v.is_null());
    }
    value
}

/// 供编排方（launch）取某会话的创建广播概要
///
/// 不在册 / 读失败 → 显性 `Err`：创建刚刚登记过却读不到概要属真源断链，
/// 发一条只有 id 的空概比对移动端不发的后果更坏（前端按它渲染出无名会话条目）。
#[cfg(target_arch = "wasm32")]
pub fn summary_for(
    session_id: &str,
) -> Result<bedcode_plugin_api::wire::SessionSummary, String> {
    match REGISTRY.get(&WasmHost, session_id) {
        Ok(Some(record)) => Ok(summary_view(&record)),
        Ok(None) => Err(format!("会话不在册（session_id={session_id}）")),
        Err(e) => Err(format!("session summary read failed (session_id={session_id}): {e}")),
    }
}

/// 会话创建登记（`launch::create_via_host` 调用；**真源写入**）
///
/// `start` / `source_device` 与宿主 `create_session_from_spec` 同参：决定初始
/// 状态（P1-b 起 `host-pty.spawn` 即起进程，恒 `Running`）与正统渲染端初始
/// 归属。`pty_id` 由调用方传入（spawn 回执）。
#[cfg(target_arch = "wasm32")]
pub fn note_created(
    session_id: &str,
    pty_id: &str,
    config_id: &str,
    name: &str,
    start: bool,
    source_device: Option<&str>,
) -> Result<(), String> {
    let record = ops::new_record(
        session_id,
        config_id,
        name,
        start,
        source_device,
        Some("com.bedcode.terminal-session"),
        &crate::config::model::now_rfc3339(),
    );
    let mut record = record;
    record.pty_id = Some(pty_id.to_string());
    REGISTRY.record(&WasmHost, &record)
}

/// 状态迁移（真源写入；非法迁移显性报错）
#[cfg(target_arch = "wasm32")]
pub fn note_status(session_id: &str, to: SessionStatus) -> Result<(), String> {
    REGISTRY
        .set_status(
            &WasmHost,
            session_id,
            to,
            &crate::config::model::now_rfc3339(),
        )
        .map(|_| ())
}

/// 会话移除（连带清注解槽）
#[cfg(target_arch = "wasm32")]
pub fn note_removed(session_id: &str) -> Result<bool, String> {
    REGISTRY.remove(&WasmHost, session_id)
}

/// 改名
#[cfg(target_arch = "wasm32")]
pub fn note_renamed(session_id: &str, name: &str) -> Result<(), String> {
    REGISTRY.set_name(
        &WasmHost,
        session_id,
        name,
        &crate::config::model::now_rfc3339(),
    )
    .map(|_| ())
}

/// 正统渲染端归属登记（尺寸裁决的登记事实）
#[cfg(target_arch = "wasm32")]
pub fn note_canonical(session_id: &str, source: &RendererSource) -> Result<(), String> {
    REGISTRY.set_canonical(
        &WasmHost,
        session_id,
        source,
        &crate::config::model::now_rfc3339(),
    )
    .map(|_| ())
}

/// 注解槽写入（会话不在册 → 显性报错，不写孤儿键）
#[cfg(target_arch = "wasm32")]
pub fn note_annotation(session_id: &str, key: &str, value: &str) -> Result<(), String> {
    match REGISTRY.annotate(&WasmHost, session_id, key, value)? {
        true => Ok(()),
        false => Err(format!("会话不存在：{session_id}")),
    }
}

/// 诊断快照（`session.status` 命令面）：`{count, active}`；读取失败只降级该字段
#[cfg(target_arch = "wasm32")]
pub fn diagnostics_via_host() -> serde_json::Value {
    match REGISTRY.all(&WasmHost) {
        Ok(records) => {
            let active = records.iter().filter(|r| ops::is_active(&r.status)).count();
            serde_json::json!({ "count": records.len(), "active": active })
        }
        Err(e) => {
            WasmHost.log_warn(&format!("session registry diagnostics degraded: {e}"));
            serde_json::json!({ "error": e })
        }
    }
}

// ==================== 读取面（互调 api `session-list` / `session-get` 的实现） ====================

/// 全部会话的对外视图（`SessionInfoView` 形状，见 [`view`]）→ `{sessions: [...]}`
#[cfg(target_arch = "wasm32")]
pub fn list_views_via_host() -> Result<serde_json::Value, String> {
    let records = REGISTRY.all(&WasmHost)?;
    let mut sessions = Vec::with_capacity(records.len());
    for record in records {
        sessions.push(view_of_record(&record)?);
    }
    Ok(serde_json::json!({ "sessions": sessions }))
}

/// 全部会话的 SessionSummary 形状（snake_case wire，票 09b/09c WS 控制面
/// `session_list` 载荷）→ `{sessions: [...]}`；逐条形状与 [`summary_json`]
/// 同源（id/name/status/created_at/started_at/session_type/config_id/
/// task_status/task_reason），供 [`crate::ws_control`] 直接回包。
#[cfg(target_arch = "wasm32")]
pub fn summaries_json() -> Result<serde_json::Value, String> {
    let records = REGISTRY.all(&WasmHost)?;
    let mut sessions = Vec::with_capacity(records.len());
    for record in records {
        sessions.push(summary_json(&record));
    }
    Ok(serde_json::json!({ "sessions": sessions }))
}

/// 单个会话的对外视图；不在册 → `Ok(None)`（调用方按「无此会话」分类，不产半成品）
#[cfg(target_arch = "wasm32")]
pub fn view_via_host(session_id: &str) -> Result<Option<serde_json::Value>, String> {
    match REGISTRY.get(&WasmHost, session_id)? {
        None => Ok(None),
        Some(record) => Ok(Some(view_of_record(&record)?)),
    }
}

/// 完整记录读取（含 `pty_id` / `canonical_renderer` / `name` 等内部事实；
/// 供动作编排域（actions）与内部消费方使用，不经互调面出网）
#[cfg(target_arch = "wasm32")]
pub fn record_via_host(session_id: &str) -> Result<Option<SessionRecord>, String> {
    REGISTRY.get(&WasmHost, session_id)
}

/// 记录 + 其注解槽 → 视图 JSON（告警在此落日志，视图函数保持纯逻辑）
#[cfg(target_arch = "wasm32")]
fn view_of_record(record: &model::SessionRecord) -> Result<serde_json::Value, String> {
    let annotations = REGISTRY.annotations(&WasmHost, &record.id)?;
    let (view, warning) = view::view_json(record, &annotations);
    if let Some(text) = warning {
        WasmHost.log_warn(&text);
    }
    Ok(view)
}

/// 内部消费方（本插件各域）读取会话**原始记录 + 注解槽**数组（含 `ptyId` /
/// `canonicalRenderer` / `owner` 等私有字段——只供插件内部派生，不经互调面出网）。
///
/// 与宿主旧 `host-session.list-sessions` 的数组形状同构（camelCase 记录 +
/// `annotations` 槽对象），供设备派生视图 / 任务域回填等域级消费方零改动迁移。
#[cfg(target_arch = "wasm32")]
pub fn internal_records_json() -> Result<serde_json::Value, String> {
    let records = REGISTRY.all(&WasmHost)?;
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let mut json = serde_json::to_value(&record)
            .map_err(|e| format!("session record serialize failed: {e}"))?;
        let annotations = REGISTRY.annotations(&WasmHost, &record.id)?;
        json["annotations"] = serde_json::json!(annotations);
        out.push(json);
    }
    Ok(serde_json::json!(out))
}

// ==================== 停止编排（互调 api `session-close` / 队列超时关闭共用） ====================

/// 待 pty:exit 到达时广播 `SessionStopped` 的来源设备名（kill 发起时登记、
/// 退出事件到达时消耗）——`SessionStopped` 广播只在 [`on_pty_exit`] 一处发出，
/// 但 `source_device` 只存在于 kill 请求侧，故经此表跨事件传递。
#[cfg(target_arch = "wasm32")]
static PENDING_STOP_SOURCE: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// 停止会话：登记 `Stopping`（同步）→ 发起 `host-pty.kill`（异步终止）→
/// 终态由 `<owner>::pty:exit` 事件驱动（[`on_pty_exit`]：翻 `Stopped` +
/// 广播 `SessionStopped` + 任务域收尾）。
///
/// 语义对齐内核 `kill_session_with_source`：
/// - 未知会话显性报错；
/// - 已终态（Stopped / Error）→ 幂等 `Ok`（**不**重复广播——比内核的重复
///   广播更干净，记账于 spec）；
/// - `Stopping`（并发停止在途）→ 幂等 `Ok`；
/// - kill 失败（句柄已摘除 = 进程已自然退出）→ `Ok`（退出事件已/将广播，
///   不做第二次终态处理）。
#[cfg(target_arch = "wasm32")]
pub fn close_via_pty(session_id: &str, source_device: Option<&str>) -> Result<(), String> {
    let record = REGISTRY
        .get(&WasmHost, session_id)?
        .ok_or_else(|| format!("会话不存在：{session_id}"))?;
    match &record.status {
        SessionStatus::Stopped | SessionStatus::Error(_) | SessionStatus::Stopping => {
            return Ok(());
        }
        _ => {}
    }
    let Some(pty_id) = record.pty_id.as_deref() else {
        return Err(format!("会话缺少 PTY 句柄，无法停止：{session_id}"));
    };
    // Stopping 先落（kill 与退出事件之间的过渡态；失败按在途处理——退出事件
    // 会把状态推到终态，这里不因状态机竞态阻塞停止请求）
    if let Err(e) = note_status(session_id, SessionStatus::Stopping) {
        WasmHost.log_warn(&format!(
            "close: status note to Stopping failed (session_id={session_id}): {e}"
        ));
    }
    PENDING_STOP_SOURCE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((session_id.to_string(), source_device.unwrap_or_default().to_string()));
    match WasmHost.pty_kill(pty_id) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("not owner") {
                // 句柄已摘除 = 进程已自然退出：退出事件已（或将）完成终态处理，
                // 这里只清掉待广播的来源登记，不重复处理
                WasmHost.log_debug(&format!(
                    "close: pty already gone, exit event will finalize (session_id={session_id}): {msg}"
                ));
                drain_pending_stop(session_id);
                Ok(())
            } else {
                drain_pending_stop(session_id);
                Err(format!("host pty kill failed: {msg}"))
            }
        }
    }
}

/// 取出并移除某会话的待广播来源设备名
#[cfg(target_arch = "wasm32")]
fn drain_pending_stop(session_id: &str) -> Option<String> {
    let mut pending = PENDING_STOP_SOURCE.lock().unwrap_or_else(|e| e.into_inner());
    let idx = pending.iter().position(|(id, _)| id == session_id)?;
    Some(pending.remove(idx).1)
}

// ==================== 输入转发（互调 api `session-input`） ====================

/// 普通输入写入：提交行重建 → 任务域分发 → `host-pty.write`
///
/// 语义对齐内核 `SessionManager::write_input` 的观察部分（ADR 0001）：
/// - 提交行重建（[`input_line`]）在**写入前**完成，任务域只看到已提交行；
/// - 重建为纯观察：失败只降级任务域，输入照写（D7）；
/// - 写入后的状态恒 `Running`（P1-b 起会话创建即 Running，无需迁移）。
///
/// 特殊键（票 06 下沉）：`special_key` 为按键组合串（如 `"ctrl+c"`）时，
/// 本插件自译（[`crate::keys`]）成 ANSI/ASCII 转义字节后**直写**——绕过提交行
/// 重建与任务域观察（用户裁定统一直写语义：Ctrl+C 就是 `\x03` 直接进 pty），
/// 宿主不再消费按键组合类型。`data` 仅在普通输入时使用。
#[cfg(target_arch = "wasm32")]
pub fn input_via_pty(
    session_id: &str,
    data: &str,
    special_key: Option<&str>,
) -> Result<(), String> {
    let record = REGISTRY
        .get(&WasmHost, session_id)?
        .ok_or_else(|| format!("会话不存在：{session_id}"))?;
    let Some(pty_id) = record.pty_id.as_deref() else {
        return Err(format!("会话缺少 PTY 句柄，无法写入：{session_id}"));
    };
    // 特殊键：本插件自译自写（绕过提交行重建与任务域观察）；未知名/不支持显式报错
    if let Some(combo) = special_key {
        let bytes = crate::keys::special_key_to_pty_bytes(combo)
            .ok_or_else(|| format!("unsupported special key: {combo}"))?;
        return WasmHost
            .pty_write(pty_id, &bytes)
            .map_err(|e| format!("host pty write failed: {}", e.message));
    }
    // 普通输入：提交行重建 → 任务域分发 → 直写
    let submitted = line_tracker().feed(session_id, data);
    for line in submitted {
        crate::task::state::handle_submitted_input(&WasmHost, session_id, &line);
    }
    WasmHost
        .pty_write(pty_id, data.as_bytes())
        .map_err(|e| format!("host pty write failed: {}", e.message))
}

// ==================== pty:exit 事件 → 终态收尾（唯一 SessionStopped 广播点） ====================

/// 进程退出事件（任意原因：自然退出 / kill / 启动错误）→ 终态收尾
///
/// 载荷 `{ ptyId, reason, exitCode? }`（camelCase；reason =
/// `"stopped" | "killed" | "error"`）。按 `ptyId` 反查会话记录：
/// - 不在册（已移除 / 非本插件句柄）→ 忽略（不产生孤儿终态）；
/// - 已终态（重复事件）→ 幂等忽略（单一发布者不变量）；
/// - 否则：翻 `Stopped` / `Error` + 广播 `SessionStopped`（kill 的来源设备名
///   经 [`PENDING_STOP_SOURCE`] 跨事件传递）+ 任务域会话结束收尾 +
///   提交行缓冲清理（残余内容不补发）。
#[cfg(target_arch = "wasm32")]
pub fn on_pty_exit(pty_id: &str, reason: &str, exit_code: Option<i32>) {
    let record = match find_by_pty(pty_id) {
        Ok(Some(r)) => r,
        Ok(None) => {
            WasmHost.log_debug(&format!(
                "pty exit: no tracked session for pty (pty_id={pty_id}, reason={reason})"
            ));
            return;
        }
        Err(e) => {
            WasmHost.log_warn(&format!("pty exit: registry read failed: {e}"));
            return;
        }
    };
    let session_id = record.id.clone();
    if record.status.is_terminal() {
        WasmHost.log_debug(&format!(
            "pty exit: session already terminal, idempotent skip (session_id={session_id})"
        ));
        return;
    }
    let to = if reason == "error" {
        SessionStatus::Error(exit_code.map(|c| format!("pty exited with error code {c}")))
    } else {
        SessionStatus::Stopped
    };
    match note_status(&session_id, to) {
        Ok(()) => {}
        Err(e) => {
            // 状态机拒绝（如已由并发路径置 Stopped）→ 幂等忽略
            WasmHost.log_debug(&format!(
                "pty exit: status transition skipped (session_id={session_id}): {e}"
            ));
            return;
        }
    }
    // 广播 SessionStopped（kill 请求携带的来源设备名；自然退出为空串）
    let source_device = drain_pending_stop(&session_id).unwrap_or_default();
    events::publish_stopped(&session_id, &record.name, &source_device);
    // 任务域会话结束收尾（agent Stop hook 没机会推送终态时兜底）
    crate::task::state::interrupt_running_tasks_on_session_end(&WasmHost, &session_id);
    // 提交行缓冲清理（残余内容不补发，见 ADR 0001）
    line_tracker().remove_session(&session_id);
    WasmHost.log_info(&format!(
        "session terminated via pty exit (session_id={session_id}, pty_id={pty_id}, reason={reason}, exit_code={exit_code:?})"
    ));
}

/// 按 pty_id 反查会话记录（退出事件回寻；同 pty 多条记录取其一——记录是
/// 唯一 pty 的镜像，正常不重复）
#[cfg(target_arch = "wasm32")]
fn find_by_pty(pty_id: &str) -> Result<Option<SessionRecord>, String> {
    Ok(REGISTRY
        .all(&WasmHost)?
        .into_iter()
        .find(|r| r.pty_id.as_deref() == Some(pty_id)))
}

/// 按 pty_id 取会话 id（票 04：WS 终端连接在 pty:exit 后按会话收尾停止帧；
/// 读失败 / 不在册 → `None`，调用方跳过终端收尾不阻断终态处理）
#[cfg(target_arch = "wasm32")]
pub fn session_id_by_pty(pty_id: &str) -> Option<String> {
    match find_by_pty(pty_id) {
        Ok(Some(record)) => Some(record.id),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn session_id_by_pty(_pty_id: &str) -> Option<String> {
    None
}

// ==================== native：无私有库，编排入口为空实现 ====================
//
// native（cargo test）下 `WasmHost` 没有 `SessionStore` / `HostPty` impl
// （wasm 专属 import 符号不在 native 链接），故真源写入与编排入口是空实现——
// 与 `actions::flush_pending_restart` 的 native 分档同模式。native 单测覆盖的
// 是 [`ops`] / [`registry`] / [`store`] / [`input_line`] 的纯逻辑与端口语义。

#[cfg(not(target_arch = "wasm32"))]
pub fn note_created(
    _session_id: &str,
    _pty_id: &str,
    _config_id: &str,
    _name: &str,
    _start: bool,
    _source_device: Option<&str>,
) -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_status(_session_id: &str, _to: SessionStatus) -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_removed(_session_id: &str) -> Result<bool, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_renamed(_session_id: &str, _name: &str) -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_canonical(_session_id: &str, _source: &RendererSource) -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_annotation(_session_id: &str, _key: &str, _value: &str) -> Result<(), String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn diagnostics_via_host() -> serde_json::Value {
    serde_json::json!({ "available": false })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_views_via_host() -> Result<serde_json::Value, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn summaries_json() -> Result<serde_json::Value, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn view_via_host(_session_id: &str) -> Result<Option<serde_json::Value>, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn record_via_host(_session_id: &str) -> Result<Option<SessionRecord>, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn internal_records_json() -> Result<serde_json::Value, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn close_via_pty(_session_id: &str, _source_device: Option<&str>) -> Result<(), String> {
    Err("session close unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn input_via_pty(
    _session_id: &str,
    _data: &str,
    _special_key: Option<&str>,
) -> Result<(), String> {
    Err("session input unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn on_pty_exit(_pty_id: &str, _reason: &str, _exit_code: Option<i32>) {}
