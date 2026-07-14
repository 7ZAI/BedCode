//! Mobile Plugin Commands
//!
//! 暴露插件操作为 Tauri invoke 命令

use crate::plugin::manager::PluginManager;
use crate::plugin::types::MobilePluginInfo;
use crate::Result;
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

// ==================== Plugin Lifecycle Commands ====================

/// 获取所有已加载插件信息
#[tauri::command]
pub async fn plugin_list_loaded(
    app_handle: tauri::AppHandle,
) -> Result<Vec<MobilePluginInfo>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.list_loaded().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<MobilePluginInfo>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.get_info(&plugin_id).await)
}

/// 激活插件
#[tauri::command]
pub async fn plugin_activate(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.activate(&plugin_id, &app_handle).await
}

/// 停用插件
#[tauri::command]
pub async fn plugin_deactivate(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.deactivate(&plugin_id).await
}

// ==================== Plugin State Commands ====================

/// 查询插件启用状态
#[tauri::command]
pub async fn plugin_is_enabled(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<bool> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.is_enabled(&plugin_id).await)
}

/// 设置插件启用状态
#[tauri::command]
pub async fn plugin_set_enabled(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    enabled: bool,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.set_enabled(&plugin_id, enabled).await
}

/// 标记插件错误
#[tauri::command]
pub async fn plugin_mark_error(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    error: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.mark_error(&plugin_id, error).await;
    Ok(())
}

// ==================== Plugin Storage Commands ====================

/// 获取插件存储值
#[tauri::command]
pub async fn plugin_storage_get(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    key: String,
) -> Result<Option<Value>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.storage().get(&plugin_id, &key).await
}

/// 设置插件存储值
#[tauri::command]
pub async fn plugin_storage_set(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    key: String,
    value: Value,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.storage().set(&plugin_id, &key, value).await
}

/// 删除插件存储值
#[tauri::command]
pub async fn plugin_storage_delete(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    key: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.storage().delete(&plugin_id, &key).await
}
