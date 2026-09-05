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

// ==================== OCR Commands（spec §4.2，插件 com.bedcode.ocr 宿主侧）====================

/// 身份 + ocr 权限校验（Rust 端为最终仲裁，仿 require_fileservice）
async fn require_ocr(manager: &PluginManager, plugin_id: &str, op: &str) -> Result<()> {
    if !manager.is_activated(plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' is not activated",
            op, plugin_id
        )));
    }
    if !manager
        .has_permission(plugin_id, bedcode_plugin_api_mobile::permission::PERMISSION_OCR)
        .await
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no ocr permission",
            op, plugin_id
        )));
    }
    Ok(())
}

/// 识别图片：RGBA 由 Kotlin 桥产出，engine 字段 v1 固定 offline（接缝路由见 §7）
#[tauri::command]
pub async fn plugin_ocr_recognize(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    input: crate::ocr::OcrRecognizeInput,
) -> Result<crate::ocr::OcrOutput> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_ocr_recognize").await?;
    let result = crate::ocr::engine::recognize(&app_handle, &input).await;
    // spec §4.4：识别完成后宿主清理 Kotlin 桥产出的临时 RGBA 文件（成败均清理）
    cleanup_ocr_temp_rgba(&app_handle, &input.image.rgba_path);
    result
}

/// 识别完成后清理 Kotlin 桥产出的 RGBA 临时文件（spec §4.4）
///
/// 仅清理 app cache/ocr 目录下的文件（防误删用户指定路径）；失败仅告警不阻断。
fn cleanup_ocr_temp_rgba(app_handle: &tauri::AppHandle, rgba_path: &str) {
    let Ok(cache_dir) = app_handle.path().app_cache_dir() else {
        return;
    };
    if !is_ocr_temp_path(&cache_dir, rgba_path) {
        return;
    }
    let path = std::path::Path::new(rgba_path);
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("plugin_ocr_recognize: failed to remove temp RGBA {}: {}", rgba_path, e);
        }
    }
}

/// RGBA 临时文件判定：位于 app cache/ocr 目录下（组件级路径比较，防 ocr2 误命中）
fn is_ocr_temp_path(cache_dir: &std::path::Path, rgba_path: &str) -> bool {
    std::path::Path::new(rgba_path).starts_with(cache_dir.join("ocr"))
}

/// 引擎状态：模型是否就位/占用字节/引擎加载态/支持引擎列表
/// available = onnxruntime .so 打包在位（Android 经 Kotlin 桥探测）
#[tauri::command]
pub async fn plugin_ocr_engine_status(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<crate::ocr::OcrEngineStatus> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_ocr_engine_status").await?;
    let data_dir = app_handle.path().app_data_dir()?;
    let onnx_so = crate::ocr::engine::probe_onnxruntime_so(&app_handle).await;
    Ok(crate::ocr::engine::engine_status(&data_dir, onnx_so.as_deref()).await)
}

/// 删除已解压模型（释放空间；先释放常驻引擎 session 再删，见 spec §4.2/§4.3）
#[tauri::command]
pub async fn plugin_ocr_delete_models(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<crate::ocr::OcrDeleteModelsOutput> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_ocr_delete_models").await?;
    // 先释放引擎（session 持有模型文件句柄），再删目录
    crate::ocr::ppocr::reset_resident();
    let data_dir = app_handle.path().app_data_dir()?;
    crate::ocr::models::delete_models(&data_dir)
        .map(|(deleted, freed_bytes)| crate::ocr::OcrDeleteModelsOutput { deleted, freed_bytes })
}

/// 从 APK assets 恢复模型（幂等；Android 经 Kotlin 桥惰性解压，见 spec §4.5）
#[tauri::command]
pub async fn plugin_ocr_restore_models(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<crate::ocr::OcrRestoreModelsOutput> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_ocr_restore_models").await?;
    let data_dir = app_handle.path().app_data_dir()?;
    let app_version = app_handle.package_info().version.to_string();
    crate::ocr::models::restore_models(&data_dir, &app_version)
        .await
        .map(|restored| crate::ocr::OcrRestoreModelsOutput { restored })
}

/// 相册选图：SAF image/*（零权限）→ Kotlin 解码降采样 → RGBA8 临时文件
/// （spec §4.4/§5.1）；返回 None 表示用户取消。
#[tauri::command]
pub async fn plugin_pick_image(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<crate::ocr::OcrImageSource>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_pick_image").await?;
    crate::plugin::android_plugins::pick_image_android().await
}

/// 拍照：ACTION_IMAGE_CAPTURE + CAMERA 运行时权限（拒绝返回明确错误）→
/// 同一解码链路 → RGBA8 临时文件（spec §4.4/§5.2）；返回 None 表示用户取消。
#[tauri::command]
pub async fn plugin_camera_capture(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<crate::ocr::OcrImageSource>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_ocr(&manager, &plugin_id, "plugin_camera_capture").await?;
    crate::plugin::android_plugins::camera_capture_android().await
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
    serde_json::from_str(&result).map_err(|e| {
        crate::AppError::Plugin(format!(
            "plugin_invoke: invalid result JSON from plugin {}: {}",
            plugin_id, e
        ))
    })
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// RGBA 临时文件清理守卫：仅命中 app cache/ocr 目录（组件级，ocr2 不误命中）
    #[test]
    fn ocr_temp_cleanup_guard_matches_only_cache_ocr() {
        let cache = std::path::Path::new("/data/user/0/com.bedcode.mobile/cache");
        assert!(is_ocr_temp_path(
            cache,
            "/data/user/0/com.bedcode.mobile/cache/ocr/ocr_1.rgba"
        ));
        // 同名前缀目录不误命中（组件级比较）
        assert!(!is_ocr_temp_path(
            cache,
            "/data/user/0/com.bedcode.mobile/cache/ocr2/x.rgba"
        ));
        // cache 之外的路径不清理（防误删用户指定文件）
        assert!(!is_ocr_temp_path(
            cache,
            "/data/user/0/com.bedcode.mobile/files/ocr/ocr_1.rgba"
        ));
        assert!(!is_ocr_temp_path(
            cache,
            "/data/user/0/com.bedcode.mobile/cache/other/x.rgba"
        ));
    }
}
