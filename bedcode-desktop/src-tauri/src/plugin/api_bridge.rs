//! Plugin API Bridge
//!
//! Tauri commands — 前端 PluginContext 的每个 API 调用通过 Tauri invoke 到达此桥接层
//! Rust 端做权限校验后执行操作

use crate::plugin::fs_auth::FsAuthChecker;
use crate::plugin::host::PluginHost;
use crate::plugin::types::DesktopPluginInfo;
use std::sync::Arc;
use tauri::State;

// ==================== Plugin Lifecycle ====================

/// 获取所有已加载插件列表
#[tauri::command]
pub async fn plugin_list_loaded(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<DesktopPluginInfo>> {
    tracing::debug!("[API] plugin_list_loaded called");
    let result = plugin_host.list_plugins().await;
    tracing::debug!("[API] plugin_list_loaded returning {} plugin(s)", result.len());
    Ok(result)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<DesktopPluginInfo>> {
    tracing::debug!("[API] plugin_get_info({})", plugin_id);
    Ok(plugin_host.get_plugin(&plugin_id).await)
}

/// 激活插件（用户操作，持久化状态）
#[tauri::command]
pub async fn plugin_activate(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    tracing::info!("[API] plugin_activate({})", plugin_id);
    let result = plugin_host.activate_plugin(&plugin_id, true).await;
    if let Err(ref e) = result {
        tracing::error!("[API] plugin_activate({}) failed: {}", plugin_id, e);
    }
    result
}

/// 停用插件（用户操作，持久化状态）
#[tauri::command]
pub async fn plugin_deactivate(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    tracing::info!("[API] plugin_deactivate({})", plugin_id);
    let result = plugin_host.deactivate_plugin(&plugin_id, true).await;
    if let Err(ref e) = result {
        tracing::error!("[API] plugin_deactivate({}) failed: {}", plugin_id, e);
    }
    result
}

/// 标记插件错误
#[tauri::command]
pub async fn plugin_mark_error(
    plugin_id: String,
    error: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    plugin_host.mark_error(&plugin_id, error).await;
    Ok(())
}

/// 获取插件激活状态映射（plugin_id → is_activated）
#[tauri::command]
pub async fn plugin_get_activated_state(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<std::collections::HashMap<String, bool>> {
    Ok(plugin_host.get_activated_state().await)
}

// ==================== Plugin Storage ====================

/// 插件存储：获取值
///
/// 校验调用者身份：plugin_id 对应的插件必须处于 Activated 状态
#[tauri::command]
pub async fn plugin_storage_get(
    plugin_id: String,
    key: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<serde_json::Value>> {
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated", plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission", plugin_id
        )));
    }
    plugin_host.storage().get(&plugin_id, &key).await
}

/// 插件存储：设置值
#[tauri::command]
pub async fn plugin_storage_set(
    plugin_id: String,
    key: String,
    value: serde_json::Value,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated", plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission", plugin_id
        )));
    }
    plugin_host.storage().set(&plugin_id, &key, value).await
}

/// 插件存储：删除值
#[tauri::command]
pub async fn plugin_storage_delete(
    plugin_id: String,
    key: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated", plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "storage") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no storage permission", plugin_id
        )));
    }
    plugin_host.storage().delete(&plugin_id, &key).await
}

// ==================== Plugin Terminal ====================

/// 插件终端：发送输入
#[tauri::command]
pub async fn plugin_terminal_send_input(
    plugin_id: String,
    session_id: String,
    text: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    if !plugin_host.is_activated(&plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} is not activated", plugin_id
        )));
    }
    if !plugin_host.permission().check(&plugin_id, "terminal:input") {
        return Err(crate::AppError::Plugin(format!(
            "Plugin {} has no terminal:input permission", plugin_id
        )));
    }
    let ctx = crate::system::app_context::AppContext::global();
    ctx.session_manager().write_input(&session_id, &text).await
}

// ==================== Plugin Registry Queries ====================

/// 获取所有命令
#[tauri::command]
pub async fn plugin_list_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::plugin::registry::CommandEntry>> {
    Ok(plugin_host.registry().list_commands().await)
}

/// 获取指定类型的视图
#[tauri::command]
pub async fn plugin_list_views(
    view_type: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::plugin::registry::ViewEntry>> {
    Ok(plugin_host.registry().get_views_by_type(&view_type).await)
}

/// 查找文件处理器
#[tauri::command]
pub async fn plugin_find_file_handler(
    extension: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<crate::plugin::registry::FileHandlerEntry>> {
    Ok(plugin_host.registry().find_file_handler(&extension).await)
}

// ==================== Rust Plugin Command Dispatch ====================

/// 调用 Rust 插件的自定义 command
///
/// 统一路由：前端通过 `invoke('plugin_invoke', { pluginId, command, args })` 调用
/// PluginHost 内部查找对应 handler 并执行，前端无法伪造 plugin_id
#[tauri::command]
pub async fn plugin_invoke(
    plugin_id: String,
    command: String,
    args: serde_json::Value,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<serde_json::Value> {
    plugin_host.invoke_rust_command(&plugin_id, &command, args).await
}

/// 获取所有 Rust 插件的 command 列表
#[tauri::command]
pub async fn plugin_list_rust_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<bedcode_plugin_api::PluginCommandEntry>> {
    Ok(plugin_host.list_rust_commands().await)
}

// ==================== Dev Mode ====================

/// 热重载 WASM 插件（仅开发模式可用）
///
/// 执行完整的卸载-重载-激活循环，用于开发期间快速迭代。
/// 生产构建中调用此命令返回错误
#[tauri::command]
pub async fn plugin_dev_reload(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    #[cfg(debug_assertions)]
    {
        plugin_host.reload_wasm_plugin(&plugin_id).await
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (plugin_host, plugin_id);
        Err(crate::AppError::Plugin("Hot reload only available in dev mode".to_string()))
    }
}

// ==================== File System Auth ====================

/// 回复文件系统授权请求（由前端弹窗调用）
#[tauri::command]
pub async fn plugin_fs_auth_respond(
    request_id: String,
    allowed: bool,
    remember: bool,
    fs_auth: State<'_, Arc<FsAuthChecker>>,
) -> crate::Result<()> {
    tracing::info!(
        "[API] plugin_fs_auth_respond: request_id={}, allowed={}, remember={}",
        request_id, allowed, remember
    );
    fs_auth.respond(&request_id, allowed, remember).await;
    Ok(())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    //! 本模块（Tauri commands 桥）不可单测的原因：
    //!
    //! 1. 所有 command 函数的第一个/最后一个参数均为
    //!    `State<'_, Arc<PluginHost>>`（或 `State<'_, Arc<FsAuthChecker>>`），
    //!    Tauri 的 `State` 不实现 `From<T>`，且其 `CommandArg` 实现需要
    //!    Tauri 运行时上下文（`StateManager`）才能构造 —— 单元测试无法
    //!    直接调用这些函数。
    //! 2. 启用 `tauri` 的 `test` feature（`tauri::test::mock_builder`）可
    //!    模拟运行时，但需要修改 Cargo.toml（本任务约束：只加测试模块），
    //!    且桥接函数体全部是「权限门禁 + 委托给 PluginHost / FsAuthChecker」
    //!    的薄封装，无独立纯逻辑可提取。
    //! 3. 门禁逻辑（`is_activated` / `permission().check`）与委托目标
    //!    （`list_plugins` / `activate_plugin` / `invoke_rust_command` /
    //!    `storage()` 等）均已在 `host.rs` 测试中直接覆盖（含错误分支的
    //!    错误字符串断言），桥接层只是透传。
    //!
    //! 结论：不硬造测试；桥接层行为由 host.rs 的宿主测试 + 前端集成测试
    //! 覆盖。`plugin_terminal_send_input` 的成功路径还依赖
    //! `AppContext::global()`（未初始化即 panic），同样无法在无头测试构造。
    //!
    //! 若未来启用 tauri test feature，可在此处为 `plugin_storage_*` /
    //! `plugin_terminal_send_input` 的门禁错误分支补测试。
}
