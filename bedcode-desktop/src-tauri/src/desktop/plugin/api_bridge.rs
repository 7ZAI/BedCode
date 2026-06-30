//! Plugin API Bridge
//!
//! Tauri commands — 前端 PluginContext 的每个 API 调用通过 Tauri invoke 到达此桥接层
//! Rust 端做权限校验后执行操作

use crate::desktop::plugin::host::PluginHost;
use crate::desktop::plugin::types::PluginInfo;
use std::sync::Arc;
use tauri::State;

// ==================== Plugin Lifecycle ====================

/// 获取所有已加载插件列表
#[tauri::command]
pub async fn plugin_list_loaded(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<PluginInfo>> {
    Ok(plugin_host.list_plugins().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<PluginInfo>> {
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
///
/// 校验调用者身份：plugin_id 对应的插件必须处于 Activated 状态
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
    let ctx = crate::desktop::app_context::AppContext::global();
    ctx.session_manager().write_input(&session_id, &text).await
}

// ==================== Plugin Registry Queries ====================

/// 获取所有命令
#[tauri::command]
pub async fn plugin_list_commands(
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::desktop::plugin::registry::CommandEntry>> {
    Ok(plugin_host.registry().list_commands().await)
}

/// 获取指定类型的视图
#[tauri::command]
pub async fn plugin_list_views(
    view_type: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Vec<crate::desktop::plugin::registry::ViewEntry>> {
    Ok(plugin_host.registry().get_views_by_type(&view_type).await)
}

/// 查找文件处理器
#[tauri::command]
pub async fn plugin_find_file_handler(
    extension: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<crate::desktop::plugin::registry::FileHandlerEntry>> {
    Ok(plugin_host.registry().find_file_handler(&extension).await)
}
