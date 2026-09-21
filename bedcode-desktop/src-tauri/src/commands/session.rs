//! Session Commands
//!
//! 宿主命令面**只保留终端渲染管道与引擎事实**（ADR 0022 裁剪线 + 终端红线）：
//! - `list_sessions` / `get_session`：引擎记录（+ 注解槽任务字段），终端窗口与
//!   通知种子化的读取面
//! - `resize_session`：尺寸裁决（插件裁决 + 内核登记/执行，保留宿主降级执行器）
//!
//! 会话**编排**命令（`start_session` / `create_session_no_start` /
//! `start_existing_session` / `kill_session` / `delete_session` / `restart_session`）
//! 已按 2026-09-21 命令面收敛注销（`.scratch/2026-09-21-host-rust-residue/issues/05`）：
//! 创建/停止/移除/重启的业务面归 `com.bedcode.session` 插件命令面
//! （`session.create` / `session.close` / `session.action.*`），宿主只经
//! `host-session` 原语执行（见 `plugin/manager/wasm_runtime/host_impl/session.rs`）。
//! 两阶段启动（建而不启 + 后续 `start_existing_session`）随之退役——v21 起唯一
//! 生产者（会话中心插件与移动端 HTTP/WS 线）一律 `start = true`。

use crate::session::{RendererSource, ResizeOutcome, SessionManager};
use crate::Result;
use std::sync::Arc;
use tauri::State;

/// 列出会话（对外视图：引擎记录 + 注解槽任务字段，票 12）
///
/// 返回类型 `SessionInfoView` 的 JSON 形状与迁移前 `SessionInfo` 逐字段一致
/// （记录字段 + `taskStatus` / `taskReason` / `taskUpdatedAt` / `taskQuestions`，
/// 缺省不出现）——前端契约不变，变的是取值来源（注解槽）。
#[tauri::command]
pub async fn list_sessions(
    session_manager: State<'_, Arc<SessionManager>>,
) -> Result<Vec<crate::session::SessionInfoView>> {
    Ok(session_manager.session_views().await)
}

/// 获取单个会话（对外视图，同 `list_sessions`）
#[tauri::command]
pub async fn get_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<Option<crate::session::SessionInfoView>> {
    Ok(session_manager.session_view(&session_id).await)
}

/// 调整会话终端大小（桌面本地路径，正统渲染端身份恒为 Desktop）
///
/// force 置位表示覆盖确认已通过（前端弹窗确认后重发）；返回 ResizeOutcome
/// 供前端判断是否需要弹窗确认（NeedsConfirmation 时未应用任何改动）。
///
/// 票 10：**裁决规则**下沉会话中心插件（正统端判定 + 覆盖确认策略在插件侧），
/// 内核只提供登记与执行；插件不可用时降级内核执行器（含内核裁决分支），
/// 对外行为（返回形状与 NeedsConfirmation 语义）不变。
#[tauri::command]
pub async fn resize_session(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: u16,
    rows: u16,
    force: Option<bool>,
) -> Result<ResizeOutcome> {
    let force = force.unwrap_or(false);
    if let Some(outcome) = crate::utils::session_action_bridge::resize_session_via_plugin(
        host.wasm_host_ctx(),
        &session_id,
        cols,
        rows,
        &RendererSource::Desktop,
        force,
    )
    .await?
    {
        return Ok(outcome);
    }
    session_manager
        .resize_session(&session_id, cols, rows, RendererSource::Desktop, force)
        .await
}
