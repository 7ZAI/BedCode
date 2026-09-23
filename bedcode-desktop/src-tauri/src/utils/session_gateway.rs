//! 会话窄转发层（会话引擎整体下沉 P1-b，宿主侧单点）——**纯插件互调 api**
//!
//! spec：`.scratch/2026-09-23-session-engine-downsink/spec.md`（P1-b 真源切换）。
//!
//! ## 为什么要有这一层
//!
//! 会话操作在宿主侧原本散在三线各自直连 `SessionManager`（桌面 Tauri 命令 /
//! 移动端 HTTP 控制器 / 移动端 WS 服务），同一条规则被写多遍。本模块是**宿主侧
//! 调用会话的唯一收口点**：消费面只调这里的函数，不再直接碰 `SessionManager` /
//! `session_*_bridge`。P1-b 起会话真源已在 `com.bedcode.terminal-session` 插件
//! 登记域，本层全部实现 = 插件互调 api 调用（无内核执行器、无内核裁决副本）。
//!
//! ## 今日策略（P1-b：真源切换完成，插件必需）
//!
//! | 操作 | 实现 | 插件互调 api |
//! | --- | --- | --- |
//! | 查询（list / get） | 插件登记域视图（`SessionInfoView` 形状） | `session-list` / `session-get` |
//! | 创建（start） | 插件编排 + `host-pty.spawn`（插件自产 id） | `session-create` |
//! | 停止 | 插件登记 Stopping + `host-pty.kill`（终态由 pty:exit 收尾） | `session-close` |
//! | 移除 | 插件摘记录 + kill + 广播 SessionRemoved | `session-remove` |
//! | 尺寸（桌面 + 移动端信号路径统一） | 插件裁决（正统端判定 / 覆盖确认） + `host-pty.resize` | `session-resize` |
//! | 输入（普通 / 特殊键） | 插件提交行重建 + `host-pty.write`（特殊键绕过重建） | `session-input` |
//! | 历史快照 / 输出存在性 | 宿主 `GlobalOutputManager`（P3 形态 B 改直读同进程 `PtyRing`） | —（宿主直读） |
//!
//! **插件未激活 / 互调失败一律显性报错**（会话真源已不在宿主，无降级轨）——
//! 双写期的 `session_{create,action}_bridge.rs`（含 `Ok(None)` 降级轨）已随
//! 真源切换退役。
//!
//! ## 不属于本层
//!
//! 会话**事件的形状与广播**（`events/sync_handler.rs` / `events/forwarder.rs`）与
//! WS 终端通道的状态订阅属事件面，随 P4 收口（P1-b 起由插件经 `host-events`
//! 广播 `SyncEvent` 会话变体，宿主只做转发）。

use crate::protocol::{RendererSource, ResizeOutcome, SessionInfoView};
use crate::utils::auth::auth_center::call_api;
use crate::wasm_core::host_api::pty::broadcast_handle_for_session;
use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_LIST: &str = "com.bedcode.terminal-session.session-list";
const API_GET: &str = "com.bedcode.terminal-session.session-get";
const API_CREATE: &str = "com.bedcode.terminal-session.session-create";
const API_CLOSE: &str = "com.bedcode.terminal-session.session-close";
const API_REMOVE: &str = "com.bedcode.terminal-session.session-remove";
const API_RESIZE: &str = "com.bedcode.terminal-session.session-resize";
const API_INPUT: &str = "com.bedcode.terminal-session.session-input";

/// 插件不可用时的显性错误（无降级；会话真源已不在宿主）
fn plugin_required_error(what: &str) -> AppError {
    AppError::Plugin(format!(
        "session plugin not active: {what} requires com.bedcode.terminal-session"
    ))
}

/// 插件互调统一包装：未激活显性报错 + 失败留痕
///
/// `call_api` 是同步阻塞调用（宿主桥接等待插件回复；调用方按 async 约定，
/// 此处保持同步——与旧 `session_create_bridge` 同构）。
fn call_session_api(
    host_ctx: &WasmHostContext,
    api: &str,
    what: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    if !crate::utils::auth::auth_center::session_active(host_ctx) {
        tracing::warn!(api = %api, "{what} refused: session plugin not active");
        return Err(plugin_required_error(what));
    }
    call_api(host_ctx, api, params).map_err(|e| {
        tracing::error!(api = %api, error = %e, "{what} failed via plugin");
        AppError::Plugin(format!("{what} failed (plugin error): {e}"))
    })
}

/// 插件视图 JSON（camelCase）→ 宿主 `SessionInfoView`（仅 Serialize 的宿主类型，
/// 拆两个半场：`SessionInfo`（约定 camelCase，可反序列化）+ 任务字段从 raw 取）
fn parse_view(raw: serde_json::Value) -> Result<SessionInfoView> {
    let info = serde_json::from_value::<crate::protocol::SessionInfo>(raw.clone()).map_err(|e| {
        AppError::Plugin(format!("session row is not a SessionInfo: {e} (row: {raw})"))
    })?;
    let get = |key: &str| raw.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
    Ok(SessionInfoView {
        info,
        task_status: get("taskStatus"),
        task_reason: get("taskReason"),
        task_updated_at: get("taskUpdatedAt"),
        task_questions: raw
            .get("taskQuestions")
            .filter(|v| !v.is_null())
            .cloned(),
    })
}

// ==================== 查询（插件登记域真源） ====================

/// 全部会话的对外视图（`SessionInfoView`，记录 + 注解槽任务字段；
/// 形状与迁移前逐字段一致，取值来源 = 插件登记域）
pub async fn list_views(host_ctx: &WasmHostContext) -> Result<Vec<SessionInfoView>> {
    let v = call_session_api(host_ctx, API_LIST, "session list", serde_json::json!({}))?;
    let sessions = v
        .get("sessions")
        .and_then(|s| s.as_array())
        .ok_or_else(|| AppError::Plugin(format!("session-list reply missing sessions: {v}")))?;
    sessions.iter().cloned().map(parse_view).collect()
}

/// 单个会话的对外视图；不在册 → `Ok(None)`（无此会话不是错误）
pub async fn view(host_ctx: &WasmHostContext, session_id: &str) -> Result<Option<SessionInfoView>> {
    let v = call_session_api(
        host_ctx,
        API_GET,
        "session get",
        serde_json::json!({ "sessionId": session_id }),
    )?;
    if v.is_null() {
        return Ok(None);
    }
    parse_view(v).map(Some)
}

// ==================== 创建（插件必需，无宿主降级） ====================

/// 经会话中心插件编排创建会话（插件自产 id + `host-pty.spawn`；
/// 回执即会话 id，创建已同步完成——旧 create-with-spec 的异步半程不再存在）
pub async fn start(
    host_ctx: &WasmHostContext,
    config_id: &str,
    cols: Option<u16>,
    rows: Option<u16>,
    start: bool,
    source_device: Option<&str>,
) -> Result<String> {
    let v = call_session_api(
        host_ctx,
        API_CREATE,
        "session create",
        serde_json::json!({
            "configId": config_id,
            "cols": cols,
            "rows": rows,
            "start": start,
            // 启动端事实透传（移动端 HTTP/WS 启动携带设备名 → 正统端初始归属该端）
            "sourceDevice": source_device,
        }),
    )?;
    let sid = v
        .get("sessionId")
        .and_then(|s| s.as_str())
        .ok_or_else(|| AppError::Plugin(format!("session-create reply missing sessionId: {v}")))?
        .to_string();
    tracing::info!(config_id = %config_id, session_id = %sid, start, "session created via plugin");
    Ok(sid)
}

// ==================== 生命周期动作 ====================

/// 停止会话（登记 Stopping + `host-pty.kill`；终态由插件 pty:exit 收尾并广播）
pub async fn stop(host_ctx: &WasmHostContext, session_id: &str, _source_device: Option<String>) -> Result<()> {
    let v = call_session_api(
        host_ctx,
        API_CLOSE,
        "session stop",
        serde_json::json!({ "sessionId": session_id }),
    )?;
    tracing::info!(session_id = %session_id, "session stop requested via plugin");
    let _ = v;
    Ok(())
}

/// 移除会话（摘记录 + kill + 广播 SessionRemoved；source_device 参与广播排除语义）
pub async fn remove(host_ctx: &WasmHostContext, session_id: &str, source_device: Option<String>) -> Result<()> {
    call_session_api(
        host_ctx,
        API_REMOVE,
        "session remove",
        serde_json::json!({ "sessionId": session_id, "sourceDevice": source_device }),
    )?;
    tracing::info!(session_id = %session_id, "session removed via plugin");
    Ok(())
}

/// 尺寸调整（**桌面本地与移动端信号路径统一**）：插件裁决（正统端判定 /
/// 覆盖确认策略，登记事实在本域记录）→ 仅可应用时 `host-pty.resize`。
///
/// 请求方身份由调用方决定：桌面本地恒 `Desktop`；移动端按 JWT claims（无
/// claims 回退 `Desktop`，仍受 `NeedsConfirmation` 门控）。
pub async fn resize(
    host_ctx: &WasmHostContext,
    session_id: &str,
    cols: u16,
    rows: u16,
    requester: RendererSource,
    force: bool,
) -> Result<ResizeOutcome> {
    let v = call_session_api(
        host_ctx,
        API_RESIZE,
        "session resize",
        serde_json::json!({
            "sessionId": session_id,
            "cols": cols,
            "rows": rows,
            "requester": requester,
            "force": force,
        }),
    )?;
    serde_json::from_value::<ResizeOutcome>(v).map_err(|e| {
        AppError::Plugin(format!("session-resize reply is not a ResizeOutcome: {e}"))
    })
}

// ==================== 输入 ====================

/// 写入普通输入（插件提交行重建 + 任务域观察 + `host-pty.write`）
pub async fn input(host_ctx: &WasmHostContext, session_id: &str, data: &str) -> Result<()> {
    call_session_api(
        host_ctx,
        API_INPUT,
        "session input",
        serde_json::json!({ "sessionId": session_id, "data": data }),
    )?;
    Ok(())
}

/// 写入特殊键（工具栏停止/复制等）：宿主 `KeyCombo` 翻译为转义字节（引擎级
/// 终端转义表，随 pty 引擎留宿主），经插件 `session-input` 的 `special` 标记
/// 直写——对齐内核 `send_special_key` 的「绕过提交行重建」不对称，否则特殊键
/// 会污染任务域的提交行观察。
pub async fn special_key(host_ctx: &WasmHostContext, session_id: &str, key: &str) -> Result<()> {
    let combo = crate::enums::KeyCombo::parse(key)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown special key: {key}")))?;
    let bytes = combo
        .to_pty_bytes()
        .ok_or_else(|| AppError::InvalidInput(format!("Unsupported key combo: {key}")))?;
    let data = String::from_utf8(bytes)
        .map_err(|_| AppError::InvalidInput(format!("special key '{key}' is not UTF-8")))?;
    call_session_api(
        host_ctx,
        API_INPUT,
        "session special key",
        serde_json::json!({ "sessionId": session_id, "data": data, "special": true }),
    )?;
    Ok(())
}

// ==================== 输出面（P3 形态 B：直读同进程 `PtyRing`） ====================

/// 一次性历史快照：`(data, min_offset, snapshot_offset, history_bytes)`
///
/// **引擎优先（票 06，M7 恢复）**：插件会话的输出环在宿主 PTY 引擎（票 05 广播
/// 声明），经 [`broadcast_handle_for_session`] 直读同进程 `PtyRing`（零跨 WASM
/// 边界）——`from` 旧于 `min_offset` 时如实返回驻留起点（缺口由 `min_offset`
/// 显式上报，客户端据此判定截断，不假装连续）。旧内核会话（无广播声明）保持
/// `GlobalOutputManager::snapshot_bytes` 兑底。
pub async fn history_snapshot(session_id: &str, from: u64) -> Option<(Vec<u8>, u64, u64, u64)> {
    if let Some(handle) = broadcast_handle_for_session(session_id) {
        let (data, min_offset, snapshot_offset, history_bytes) = {
            let ring = handle.ring.lock().unwrap_or_else(|e| e.into_inner());
            let (min, max) = ring.watermarks();
            // 宿主直读不受 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（那是 WASM 边界限额）；
            // 从 `from.max(min)` 起拉取到产出端，一次取净驻留历史
            let start = from.max(min);
            let fetched = ring.fetch(start, max.saturating_sub(start) as usize);
            (fetched.data, min, max, max.saturating_sub(min))
        };
        return Some((data, min_offset, snapshot_offset, history_bytes));
    }
    crate::session::GlobalOutputManager::global()
        .snapshot_bytes(session_id, from)
        .await
}

/// 取消某订阅者对该会话的输出订阅（WS 控制面停止 / 移除动作之后调用；
/// 对插件会话为幂等 no-op——输出订阅面随 P3 恢复）
pub async fn unsubscribe_output(session_id: &str, subscriber: &str) {
    crate::session::GlobalOutputManager::global()
        .unsubscribe(session_id, subscriber)
        .await;
}