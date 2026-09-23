//! 会话登记域（会话引擎整体下沉 P1）——**会话真源从宿主 `session/` 挪到本插件**
//!
//! 宿主今天持有会话登记 / 状态机 / 生命周期分发（`src-tauri/src/session/`，
//! 3759 行），本域是它们迁入插件后的落点；宿主侧只留 PTY 引擎 + `host-pty`
//! 原语。分阶段实施（spec `.scratch/2026-09-23-session-engine-downsink`）：
//!
//! - **P1-a（本模块）**：登记 / 状态机 / 私有库真源落地 + **双写**
//!   （宿主仍是权威，本域按同一事实记账）——行为零变化；
//! - P1-b：创建改走 `host-pty.spawn`（会话 id 由本插件自产），本域转权威，
//!   宿主经互调 api 读会话事实；
//! - P1-c：宿主窄转发层（server HTTP/WS + 命令面）改读本域，宿主 `session/` 收窄。
//!
//! ## 双写期的诚实记账（不静默）
//!
//! 迁移期本域是**镜像**，以下事实驱动它：
//!
//! | 事实来源 | 覆盖 |
//! | --- | --- |
//! | 本插件自身的创建 / 动作编排（`launch` / `actions`） | 创建、移除、改名、归属登记 |
//! | `host-session` 生命周期回调（`Created` / `Stopping` / `Stopped`） | 状态迁移与启动时间 |
//! | 注解写面（`devices::annotate_via_host`） | 注解槽 |
//!
//! **已知缺口（P1-b 收口）**：宿主侧不经插件的路径（移动端 HTTP/WS 的
//! `remove` 直连 `SessionManager::remove_session_with_source` 不派发生命周期事件）
//! 不会进入本域镜像——真源切换后这类路径必须改经插件，届时缺口自然闭合。
//!
//! ## 双写失败口径
//!
//! 镜像写入失败**不得**影响用户操作路径（宿主仍持有权威事实），但也不许静默：
//! 门面函数把错误记 `warn` 留痕并继续（D7 故障隔离）；状态机拒绝的非法迁移
//! 同样是 `warn`（见 [`registry::Registry::set_status`]）。
//!
//! 模块构成：
//! - [`model`]：wire / 存储模型（`SessionStatus` 与宿主 enum 逐字同形）
//! - [`store`]：私有库端口（wasm = 插件私有库；native 单测注入内存实现）
//! - [`ops`]：纯逻辑（状态机 / 记录构造 / 判据）
//! - [`registry`]：内存镜像 + 写穿 + 惰性载入

pub mod model;
pub mod ops;
pub mod registry;
pub mod store;
pub mod view;

use crate::actions::RendererSource;
use model::SessionStatus;

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::HostLog;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm::WasmPlugin;
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;
#[cfg(target_arch = "wasm32")]
use registry::Registry;
#[cfg(target_arch = "wasm32")]
use store::SessionStore;

/// 进程级注册表实例（wasm 侧全插件共用一份；同实例串行访问，无需额外并发协议）
#[cfg(target_arch = "wasm32")]
static REGISTRY: Registry = Registry::new();

// ==================== 激活期装配（建表 + 进程域对账） ====================

/// 建表（幂等）+ 进程启动对账（清空上一进程遗留的会话行）
///
/// 对账理由：会话与其 PTY 同生命周期，宿主真源同样是**进程内存**——进程重启后
/// 旧会话记录不可回收，留着只会让后续读取面（P1-b 起）看到幽灵会话。清表后
/// 本域记录集合与宿主当次进程的会话集合同域。
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

// ==================== 双写入口（P1 迁移期；宿主仍是权威） ====================

/// 镜像写入降级留痕（结构化字段不适用：插件日志面是单串，宿主加 `[plugin:ID]` 前缀）
#[cfg(target_arch = "wasm32")]
fn warn_registry(op: &str, session_id: &str, error: &str) {
    WasmHost.log_warn(&format!(
        "session registry {op} degraded (session_id={session_id}): {error}"
    ));
}

/// 会话创建成功后的登记（`launch::create_via_host` 调用）
///
/// `start` / `source_device` 与宿主 `create_session_from_spec` 同参：决定初始
/// 状态（`Running` / `Starting`）与正统渲染端初始归属。
#[cfg(target_arch = "wasm32")]
pub fn note_created_via_host(
    session_id: &str,
    config_id: &str,
    name: &str,
    start: bool,
    source_device: Option<&str>,
) {
    let record = ops::new_record(
        session_id,
        config_id,
        name,
        start,
        source_device,
        Some(crate::SessionPlugin::ID),
        &crate::config::model::now_rfc3339(),
    );
    if let Err(e) = REGISTRY.record(&WasmHost, &record) {
        warn_registry("register", session_id, &e);
    }
}

/// 状态迁移镜像（`lib.rs` 生命周期回调调用：`Created` / `Stopping` / `Stopped`）
#[cfg(target_arch = "wasm32")]
pub fn note_status_via_host(session_id: &str, to: SessionStatus) {
    match REGISTRY.set_status(
        &WasmHost,
        session_id,
        to,
        &crate::config::model::now_rfc3339(),
    ) {
        Ok(Some(_)) => {}
        // 会话不在册：宿主自建（无插件登记）或 P1-b 前未登记的会话，属正常分支
        Ok(None) => WasmHost.log_debug(&format!(
            "session registry: session not tracked, status note skipped (session_id={session_id})"
        )),
        Err(e) => warn_registry("status", session_id, &e),
    }
}

/// 会话移除镜像（连带清注解槽）
#[cfg(target_arch = "wasm32")]
pub fn note_removed_via_host(session_id: &str) {
    match REGISTRY.remove(&WasmHost, session_id) {
        Ok(true) => {}
        Ok(false) => WasmHost.log_debug(&format!(
            "session registry: removing untracked session (session_id={session_id})"
        )),
        Err(e) => warn_registry("remove", session_id, &e),
    }
}

/// 改名镜像
#[cfg(target_arch = "wasm32")]
pub fn note_renamed_via_host(session_id: &str, name: &str) {
    match REGISTRY.set_name(
        &WasmHost,
        session_id,
        name,
        &crate::config::model::now_rfc3339(),
    ) {
        Ok(Some(_)) => {}
        Ok(None) => WasmHost.log_debug(&format!(
            "session registry: renaming untracked session (session_id={session_id})"
        )),
        Err(e) => warn_registry("rename", session_id, &e),
    }
}

/// 正统渲染端归属镜像（尺寸裁决的登记事实）
#[cfg(target_arch = "wasm32")]
pub fn note_canonical_via_host(session_id: &str, source: &RendererSource) {
    match REGISTRY.set_canonical(
        &WasmHost,
        session_id,
        source,
        &crate::config::model::now_rfc3339(),
    ) {
        Ok(Some(_)) => {}
        Ok(None) => WasmHost.log_debug(&format!(
            "session registry: canonical note for untracked session (session_id={session_id})"
        )),
        Err(e) => warn_registry("canonical", session_id, &e),
    }
}

/// 注解槽镜像（宿主 `annotate` 原语的同批写入）
#[cfg(target_arch = "wasm32")]
pub fn note_annotation_via_host(session_id: &str, key: &str, value: &str) {
    match REGISTRY.annotate(&WasmHost, session_id, key, value) {
        Ok(true) => {}
        Ok(false) => WasmHost.log_debug(&format!(
            "session registry: annotation for untracked session skipped (session_id={session_id})"
        )),
        Err(e) => warn_registry("annotation", session_id, &e),
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
///
/// 与诊断面的区别要说清：`session.status` 的 `sessionRegistry` 是**自省字段**（运维看
/// 登记规模），本函数是**给消费方的事实**。宿主窄转发层改读本域时走的是这里。
#[cfg(target_arch = "wasm32")]
pub fn list_views_via_host() -> Result<serde_json::Value, String> {
    let records = REGISTRY.all(&WasmHost)?;
    let mut sessions = Vec::with_capacity(records.len());
    for record in records {
        sessions.push(view_of_record(&record)?);
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

// ==================== native：无私有库，双写入口为空实现 ====================
//
// native（cargo test）下 `WasmHost` 没有 `SessionStore` impl（wasm 专属 import
// 符号不在 native 链接），故镜像入口是空实现——与 `actions::flush_pending_restart`
// 的 native 分档同模式。native 单测覆盖的是 [`ops`] / [`registry`] / [`store`]
// 的纯逻辑与端口语义。

#[cfg(not(target_arch = "wasm32"))]
pub fn note_created_via_host(
    _session_id: &str,
    _config_id: &str,
    _name: &str,
    _start: bool,
    _source_device: Option<&str>,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_status_via_host(_session_id: &str, _to: SessionStatus) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_removed_via_host(_session_id: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_renamed_via_host(_session_id: &str, _name: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_canonical_via_host(_session_id: &str, _source: &RendererSource) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn note_annotation_via_host(_session_id: &str, _key: &str, _value: &str) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn diagnostics_via_host() -> serde_json::Value {
    serde_json::json!({ "available": false })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_views_via_host() -> Result<serde_json::Value, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn view_via_host(_session_id: &str) -> Result<Option<serde_json::Value>, String> {
    Err("session registry store unavailable outside wasm runtime".to_string())
}
