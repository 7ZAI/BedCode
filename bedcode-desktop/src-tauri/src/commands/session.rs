//! Session Commands

use crate::session::{RendererSource, ResizeOutcome, SessionManager};
use crate::Result;
use std::sync::Arc;
use tauri::State;

/// 解析启动端终端组件默认网格（>0 才生效；None 时由内核/插件兜底默认尺寸）
fn initial_size(cols: Option<u16>, rows: Option<u16>) -> Option<(u16, u16)> {
    match (cols, rows) {
        (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)),
        _ => None,
    }
}

#[tauri::command]
pub async fn start_session(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<String> {
    tracing::info!(config_id = %config_id, "start_session called");
    // 票 09：创建编排下沉会话中心插件（命名唯一化 / config→launch spec 映射 /
    // 创建即启动）。插件可用 → 由插件编排并经 `create-with-spec` 执行；不可用 →
    // 降级宿主旧路径（读主库投影 + 命名服务 + 配置映射服务），行为逐字等价。
    if let Some(session_id) = crate::utils::session_create_bridge::create_session_via_plugin(
        host.wasm_host_ctx(),
        &config_id,
        cols,
        rows,
        true,
    )
    .await?
    {
        return Ok(session_id);
    }
    // 降级：宿主旧路径（README 保持迁移前行为；双轨期未激活是常态）
    let result = session_manager
        .create_session_with_source(&config_id, None, initial_size(cols, rows))
        .await;
    match result {
        Ok(id) => {
            tracing::info!(session_id = %id, "Session created successfully (host fallback)");
            Ok(id)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create session (host fallback)");
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn create_session_no_start(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    config_id: String,
) -> Result<String> {
    tracing::info!(config_id = %config_id, "create_session_no_start called");
    // 票 09：两阶段第一阶段（只创建不启动）经插件编排；插件不可用降级宿主旧路径
    if let Some(session_id) = crate::utils::session_create_bridge::create_session_via_plugin(
        host.wasm_host_ctx(),
        &config_id,
        None,
        None,
        false,
    )
    .await?
    {
        return Ok(session_id);
    }
    let result = session_manager.create_session_no_start(&config_id).await;
    match result {
        Ok(id) => {
            tracing::info!(session_id = %id, "Session created (not started) successfully (host fallback)");
            Ok(id)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to create session (not started) (host fallback)");
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn start_existing_session(
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<()> {
    tracing::info!(session_id = %session_id, "start_existing_session called");
    // 两阶段启动第二阶段：spawn 前按请求端尺寸调整 PTY
    let initial_size = match (cols, rows) {
        (Some(c), Some(r)) if c > 0 && r > 0 => Some((c, r)),
        _ => None,
    };
    let result = session_manager.start_existing_session(&session_id, initial_size).await;
    match result {
        Ok(_) => {
            tracing::info!(session_id = %session_id, "Session started successfully");
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to start session: {}", e);
            Err(e)
        }
    }
}

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

#[tauri::command]
pub async fn kill_session(session_manager: State<'_, Arc<SessionManager>>, session_id: String) -> Result<()> {
    session_manager.kill_session(&session_id).await
}

/// 删除（移除）会话
///
/// 票 10：移除编排下沉会话中心插件（存在性预检 + 失败可见），宿主执行器保留为
/// 降级路径（插件未激活 / 互调失败时行为与迁移前逐字一致）。
#[tauri::command]
pub async fn delete_session(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<()> {
    if crate::utils::session_action_bridge::remove_session_via_plugin(host.wasm_host_ctx(), &session_id)
        .await?
        .is_some()
    {
        return Ok(());
    }
    session_manager.remove_session(&session_id).await
}

/// 重启会话（同一 session id 重建并启动）
///
/// 票 10：重启编排下沉会话中心插件（存在性预检给出同步可见的失败）；插件不可用时
/// 降级宿主执行器（`SessionManager::restart_session`，即插件侧调用的同一执行端）。
#[tauri::command]
pub async fn restart_session(
    host: State<'_, Arc<crate::plugin::PluginHost>>,
    session_manager: State<'_, Arc<SessionManager>>,
    session_id: String,
) -> Result<String> {
    if let Some(session_id) =
        crate::utils::session_action_bridge::restart_session_via_plugin(host.wasm_host_ctx(), &session_id).await?
    {
        return Ok(session_id);
    }
    session_manager.restart_session(&session_id).await
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
