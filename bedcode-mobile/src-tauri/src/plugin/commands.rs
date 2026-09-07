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
pub async fn plugin_list_loaded(app_handle: tauri::AppHandle) -> Result<Vec<MobilePluginInfo>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.list_loaded().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn plugin_get_info(app_handle: tauri::AppHandle, plugin_id: String) -> Result<Option<MobilePluginInfo>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.get_info(&plugin_id).await)
}

/// 预授权（启用前置，独立命令供前端先行调用）
///
/// 前端 toggle 时序：先调本命令（此阶段**不显示** LoadingDialog，授权弹窗
/// 可正常交互）→ 通过后再显示 loading 调 `plugin_activate`；拒绝则直接失败，
/// 不进入激活流程。`activate` 内部的 preauthorize 保留为兜底（启动
/// auto-activate 无头场景 + 已授权路径短路无二次弹窗）。
#[tauri::command]
pub async fn plugin_preauthorize(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.preauthorize_plugin(&plugin_id).await
}

/// 激活插件
#[tauri::command]
pub async fn plugin_activate(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.activate(&plugin_id).await
}

/// 停用插件
#[tauri::command]
pub async fn plugin_deactivate(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.deactivate(&plugin_id).await
}

// ==================== Plugin State Commands ====================

/// 查询插件启用状态
#[tauri::command]
pub async fn plugin_is_enabled(app_handle: tauri::AppHandle, plugin_id: String) -> Result<bool> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    Ok(manager.is_enabled(&plugin_id).await)
}

/// 设置插件启用状态
#[tauri::command]
pub async fn plugin_set_enabled(app_handle: tauri::AppHandle, plugin_id: String, enabled: bool) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.set_enabled(&plugin_id, enabled).await
}

/// 标记插件错误
#[tauri::command]
pub async fn plugin_mark_error(app_handle: tauri::AppHandle, plugin_id: String, error: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.mark_error(&plugin_id, error).await;
    Ok(())
}

/// 插件显式上报启动成功（Error → Activated 自愈）
#[tauri::command]
pub async fn plugin_report_ready(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.report_ready(&plugin_id).await
}

/// 批准插件权限（人工审批：记录权限清单 + 目录内容哈希钉扎）
///
/// 仅用户安装插件（file-install / remote-download）需要审批；
/// 内置插件（apk-asset）调用返回错误。批准成功后状态 NeedsApproval → Loaded。
#[tauri::command]
pub async fn plugin_approve(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.approve(&plugin_id).await
}

// ==================== Plugin Storage Commands ====================

/// 获取插件存储值
#[tauri::command]
pub async fn plugin_storage_get(app_handle: tauri::AppHandle, plugin_id: String, key: String) -> Result<Option<Value>> {
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
pub async fn plugin_storage_delete(app_handle: tauri::AppHandle, plugin_id: String, key: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.storage().delete(&plugin_id, &key).await
}

// ==================== Plugin Download & Install Commands ====================

/// 下载并安装远程 zip 插件包
#[tauri::command]
pub async fn plugin_download(app_handle: tauri::AppHandle, zip_url: String) -> Result<String> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    let plugins_dir = manager.plugins_dir().clone();

    let plugin_id = crate::plugin::downloader::PluginDownloader::download_and_install(&zip_url, &plugins_dir).await?;

    // 重新扫描并加载
    manager.scan_and_load().await;

    Ok(plugin_id)
}

/// 从本地 zip 插件包安装
#[tauri::command]
pub async fn plugin_install_from_file(app_handle: tauri::AppHandle, path: String) -> Result<String> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    let plugins_dir = manager.plugins_dir().clone();

    let plugin_id = crate::plugin::downloader::PluginDownloader::install_from_file(&path, &plugins_dir).await?;

    // 重新扫描并加载
    manager.scan_and_load().await;

    Ok(plugin_id)
}

/// 卸载插件（仅用户安装的插件；内置插件拒绝）
#[tauri::command]
pub async fn plugin_uninstall(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.uninstall(&plugin_id).await
}

/// 重新加载 WASM 插件（热重载）
#[tauri::command]
pub async fn reload_wasm_plugin(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();

    // 先停用
    manager.deactivate(&plugin_id).await?;

    // 重新扫描
    manager.scan_and_load().await;

    // 重新激活
    manager.activate(&plugin_id).await
}

// ==================== File System Auth Commands ====================

/// 回复文件访问授权请求
#[tauri::command]
pub async fn plugin_fs_auth_respond(
    app_handle: tauri::AppHandle,
    request_id: String,
    allowed: bool,
    remember: bool,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().respond(&request_id, allowed, remember).await;
    Ok(())
}

/// 添加路径白名单
#[tauri::command]
pub async fn plugin_fs_add_path_whitelist(app_handle: tauri::AppHandle, path: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .add_path_whitelist(&path)
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 移除路径白名单
#[tauri::command]
pub async fn plugin_fs_remove_path_whitelist(app_handle: tauri::AppHandle, path: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .remove_path_whitelist(&path)
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 获取路径白名单
#[tauri::command]
pub async fn plugin_fs_get_path_whitelist(app_handle: tauri::AppHandle) -> Result<Vec<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .get_path_whitelist()
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 添加插件白名单
#[tauri::command]
pub async fn plugin_fs_add_plugin_whitelist(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .add_plugin_whitelist(&plugin_id)
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 移除插件白名单
#[tauri::command]
pub async fn plugin_fs_remove_plugin_whitelist(app_handle: tauri::AppHandle, plugin_id: String) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .remove_plugin_whitelist(&plugin_id)
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 获取插件白名单
#[tauri::command]
pub async fn plugin_fs_get_plugin_whitelist(app_handle: tauri::AppHandle) -> Result<Vec<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager
        .fs_auth()
        .get_plugin_whitelist()
        .await
        .map_err(|e| crate::AppError::Plugin(e.to_string()))
}

// ==================== Plugin Logging Commands ====================

/// 插件日志输出（TS SDK 调用，统一到宿主 tracing）
#[tauri::command]
pub fn plugin_log(plugin_id: String, level: String, message: String) {
    match level.as_str() {
        "debug" => tracing::debug!("[plugin:{}] {}", plugin_id, message),
        "warn" => tracing::warn!("[plugin:{}] {}", plugin_id, message),
        "error" => tracing::error!("[plugin:{}] {}", plugin_id, message),
        _ => tracing::info!("[plugin:{}] {}", plugin_id, message),
    }
}

// ==================== Plugin Command Invoke ====================

/// 调用 WASM 插件命令（前端 context.commands.execute 的回退桥）
#[tauri::command]
pub async fn plugin_invoke(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    command: String,
    args: Value,
) -> Result<Value> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    // WASM 命令接受字符串参数（JSON 序列化），invoke_command 返回的是命令结果字符串
    let args_str = args.to_string();
    let result = manager.invoke_command(&plugin_id, &command, &args_str).await?;
    // 还原为 JSON 对象返回前端（任务数组 / {ok:true} 等）
    let value: Value = serde_json::from_str(&result).map_err(|e| {
        crate::AppError::Plugin(format!(
            "plugin_invoke: invalid result JSON from plugin {}: {}",
            plugin_id, e
        ))
    })?;
    // 插件 invoke_command 的 Err 经 SDK 宏序列化为 {"error": "..."} 的**成功** JSON，
    // 此处还原为真正错误（与桌面端同口径，见 host/commands.rs）
    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        if !err.is_empty() {
            return Err(crate::AppError::Plugin(err.to_string()));
        }
    }
    Ok(value)
}

// ==================== System Open（历史「打开所在文件夹」真机路径） ====================

/// 按文件名打开接收文件所在目录（历史记录「打开所在文件夹」真实设备路径）
///
/// wire 不携带接收落盘路径，仅凭文件名经 Kotlin DownloadsDirPlugin
/// openFileLocationByName（MediaStore 公共下载按名命中 → primary:Download；
/// 未命中回退 app 私有下载目录）。需 system:open 权限，前端
/// requireSystemOpenPermission 已校验（与 plugin_open_file* 回调链同模式）。
#[tauri::command]
pub async fn plugin_reveal_received_file(
    plugin_id: String,
    file_name: String,
) -> Result<()> {
    if file_name.trim().is_empty() {
        return Err(crate::AppError::Plugin(
            "plugin_reveal_received_file: file_name is required".to_string(),
        ));
    }
    tracing::debug!(plugin_id = %plugin_id, file_name = %file_name, "reveal received file location");
    crate::plugin::android_plugins::open_download_file_location_by_name(&file_name)
        .await
}
