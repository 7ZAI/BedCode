//! Plugin API Bridge
//!
//! Tauri commands — 前端 PluginContext 的每个 API 调用通过 Tauri invoke 到达此桥接层
//! Rust 端做权限校验后执行操作

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
    Ok(plugin_host.list_plugins().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<DesktopPluginInfo>> {
    Ok(plugin_host.get_plugin(&plugin_id).await)
}

/// 激活插件
#[tauri::command]
pub async fn plugin_activate(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    plugin_host.activate_plugin(&plugin_id).await
}

/// 停用插件
#[tauri::command]
pub async fn plugin_deactivate(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    plugin_host.deactivate_plugin(&plugin_id).await
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
