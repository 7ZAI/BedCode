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
//! ## 今日策略（P1-b 真源切换完成 + 2026-09-25 零业务类型收口）
//!
//! | 操作 | 实现 | 插件互调 api |
//! | --- | --- | --- |
//! | 查询（list / get） | 插件登记域视图 | `session-list` / `session-get` |
//! | 创建（start） | 插件编排 + `host-pty.spawn`（插件自产 id） | `session-create` |
//! | 停止 | 插件登记 Stopping + `host-pty.kill`（终态由 pty:exit 收尾） | `session-close` |
//! | 移除 | 插件摘记录 + kill + 广播 SessionRemoved | `session-remove` |
//! | 尺寸（桌面 + 移动端信号路径统一） | 插件裁决（正统端判定 / 覆盖确认）+ `host-pty.resize` | `session-resize` |
//! | 输入（普通 / 特殊键） | 插件提交行重建 + `host-pty.write`（特殊键绕过重建） | `session-input` |
//! | 历史快照 | 插件经 `host-pty.ring-fetch` 拉净驻留历史 | `session-history` |
//!
//! **插件未激活 / 互调失败一律显性报错**（会话真源已不在宿主，无降级轨）——
//! 双写期的 `session_{create,action}_bridge.rs`（含 `Ok(None)` 降级轨）已随
//! 真源切换退役。
//!
//! ## 零业务类型（2026-09-25）
//!
//! 本层接口**不再持有任何会话业务类型**（`SessionInfo/View/Status/ResizeOutcome/...`
//! 已随 `protocol/` 会话域整体退役）：查询/创建/尺寸/历史一律 `serde_json::Value`
//! 原样透传插件 reply，宿主不解析、不解释、不校验业务字段——字段形状契约归插件
//! 产出口（`com.bedcode.terminal-session` 的 `session/view.rs` 形状锁 + 插件集成
//! 测试），宿主只是互调 api 的机械转发面。
//!
//! ## 不属于本层
//!
//! 会话**事件的形状与广播**（`events/sync_handler.rs`）与 WS 终端通道的状态订阅
//! 属事件面：P1-b 起由插件经 `host-events` 广播 `SyncEvent` 会话变体、宿主只做
//! 转发；票 09 已把处理器的内核回查兜底与状态订阅转接通道（`events/forwarder.rs`）
//! 一并删除（载荷必须自携带，缺失即 `warn` + 不广播）。

use crate::utils::auth::auth_center::call_api;
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
const API_HISTORY: &str = "com.bedcode.terminal-session.session-history";

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

// ==================== 查询（插件登记域真源） ====================

/// 全部会话视图（插件 reply 原样透传：`{sessions: [...]}`）。
/// 宿主不解析任何字段（形状契约归插件产出口，2026-09-25）。
pub async fn list_views(host_ctx: &WasmHostContext) -> Result<serde_json::Value> {
    call_session_api(host_ctx, API_LIST, "session list", serde_json::json!({}))
}

/// 运行中会话视图（**关窗守卫专用**）：插件按会话语义过滤
/// （`ops::needs_close_confirmation`：Running / Starting / WaitingInput）——
/// 「哪些状态算运行中需要确认」的判据在插件会话域（2026-09-25 下沉），
/// 宿主不持有状态集合判断、不解析 return 字段。
pub async fn running_views(host_ctx: &WasmHostContext) -> Result<serde_json::Value> {
    call_session_api(
        host_ctx,
        API_LIST,
        "session list (running)",
        serde_json::json!({ "filter": "running" }),
    )
}

/// 单个会话视图（插件 reply 原样透传；不在册 → `null`）。
pub async fn view(host_ctx: &WasmHostContext, session_id: &str) -> Result<serde_json::Value> {
    call_session_api(
        host_ctx,
        API_GET,
        "session get",
        serde_json::json!({ "sessionId": session_id }),
    )
}

// ==================== 创建（插件必需，无宿主降级） ====================

/// 经会话中心插件编排创建会话（插件自产 id + `host-pty.spawn`；
/// 回执 `{sessionId: ...}` 原样透传，创建已同步完成——
/// 旧 create-with-spec 的异步半程不再存在）。宿主不解析回执字段。
pub async fn start(
    host_ctx: &WasmHostContext,
    config_id: &str,
    cols: Option<u16>,
    rows: Option<u16>,
    start: bool,
    source_device: Option<&str>,
) -> Result<serde_json::Value> {
    call_session_api(
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
    )
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
/// `requester` 为调用方构造的 JSON（桌面恒 `{"kind":"desktop"}`；移动端按
/// JWT claims 构造），宿主原样透传不解释——请求方身份形状属插件契约。
pub async fn resize(
    host_ctx: &WasmHostContext,
    session_id: &str,
    cols: u16,
    rows: u16,
    requester: serde_json::Value,
    force: bool,
) -> Result<serde_json::Value> {
    call_session_api(
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
    )
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

/// 写入特殊键（工具栏停止/复制等）：**票 06 下沉**——按键组合 → 转义字节的
/// 翻译已移至插件（`com.bedcode.terminal-session` 读 `specialKey` 自译自写，
/// 统一直写语义，绕过提交行重建）。宿主此处只转发组合串，不再消费
/// `KeyCombo`/`to_pty_bytes`（pte 输入路径触点清零）。
pub async fn special_key(host_ctx: &WasmHostContext, session_id: &str, key: &str) -> Result<()> {
    if key.trim().is_empty() {
        return Err(AppError::InvalidInput("special key 不能为空".to_string()));
    }
    call_session_api(
        host_ctx,
        API_INPUT,
        "session special key",
        serde_json::json!({ "sessionId": session_id, "data": "", "specialKey": key }),
    )?;
    Ok(())
}

// ==================== 输出面（websocket 业务下沉票 08：插件互调，宿主不再直读环） ====================

/// 一次性历史快照（插件 reply 原样透传：`{data, minOffset, snapshotOffset,
/// historyBytes}`）。
///
/// **插件必需（websocket 业务下沉票 08）**：宿主不再持有「会话 id → pty 句柄」的
/// 广播映射（`hostBroadcastSessionId` / `broadcast_handle_for_session` 已退役，PTY
/// 引擎不再知道 session id）；历史快照改经插件互调 api——插件用自己的
/// `session record.pty_id` 调 `host-pty.ring-fetch` 拉净驻留历史
/// （spec §4.3：from 旧于环驻留起点时 minOffset 如实上报缺口，客户端据此判定
/// 截断，不假装连续）。
///
/// 会话不存在 / 插件未激活 → 显性 `Err`（fail-visible，不静默当「无数据」）。
pub async fn history_snapshot(host_ctx: &WasmHostContext, session_id: &str, from: u64) -> Result<serde_json::Value> {
    call_session_api(
        host_ctx,
        API_HISTORY,
        "session history",
        serde_json::json!({ "sessionId": session_id, "from": from }),
    )
}

// 票 11：`unsubscribe_output`（向内核输出环退订）随 `session/` 目录删除。
// 票 08：`history_snapshot` 不再直读宿主 `PtyRing`——会话输出环读取归插件
// （`session-history` 互调 api），宿主不再持有会话级环句柄。