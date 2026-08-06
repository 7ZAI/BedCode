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

/// 插件显式上报启动成功（Error → Activated 自愈）
#[tauri::command]
pub async fn plugin_report_ready(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.report_ready(&plugin_id).await
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

// ==================== Plugin Download & Install Commands ====================

/// 下载并安装远程 zip 插件包
#[tauri::command]
pub async fn plugin_download(
    app_handle: tauri::AppHandle,
    zip_url: String,
) -> Result<String> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    let plugins_dir = manager.plugins_dir().clone();

    let plugin_id = crate::plugin::downloader::PluginDownloader::download_and_install(
        &zip_url,
        &plugins_dir,
    )
    .await?;

    // 重新扫描并加载
    manager.scan_and_load().await;

    Ok(plugin_id)
}

/// 从本地 zip 插件包安装
#[tauri::command]
pub async fn plugin_install_from_file(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<String> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    let plugins_dir = manager.plugins_dir().clone();

    let plugin_id = crate::plugin::downloader::PluginDownloader::install_from_file(
        &path,
        &plugins_dir,
    )
    .await?;

    // 重新扫描并加载
    manager.scan_and_load().await;

    Ok(plugin_id)
}

/// 卸载插件（仅用户安装的插件；内置插件拒绝）
#[tauri::command]
pub async fn plugin_uninstall(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.uninstall(&plugin_id).await
}

/// 重新加载 WASM 插件（热重载）
#[tauri::command]
pub async fn reload_wasm_plugin(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();

    // 先停用
    manager.deactivate(&plugin_id).await?;

    // 重新扫描
    manager.scan_and_load().await;

    // 重新激活
    manager.activate(&plugin_id, &app_handle).await
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
pub async fn plugin_fs_add_path_whitelist(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().add_path_whitelist(&path).await.map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 移除路径白名单
#[tauri::command]
pub async fn plugin_fs_remove_path_whitelist(
    app_handle: tauri::AppHandle,
    path: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().remove_path_whitelist(&path).await.map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 获取路径白名单
#[tauri::command]
pub async fn plugin_fs_get_path_whitelist(
    app_handle: tauri::AppHandle,
) -> Result<Vec<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().get_path_whitelist().await.map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 添加插件白名单
#[tauri::command]
pub async fn plugin_fs_add_plugin_whitelist(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().add_plugin_whitelist(&plugin_id).await.map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 移除插件白名单
#[tauri::command]
pub async fn plugin_fs_remove_plugin_whitelist(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().remove_plugin_whitelist(&plugin_id).await.map_err(|e| crate::AppError::Plugin(e.to_string()))
}

/// 获取插件白名单
#[tauri::command]
pub async fn plugin_fs_get_plugin_whitelist(
    app_handle: tauri::AppHandle,
) -> Result<Vec<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.fs_auth().get_plugin_whitelist().await.map_err(|e| crate::AppError::Plugin(e.to_string()))
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

// ==================== File Service Commands（TS 通道） ====================
//
// ts-only 插件的 TS 通道，与 WASM host functions（host_filesrv_*）同构；
// 经 Tauri command 的挂载以 Webview 钩子目标注册，上传策略决定经
// filesrv:upload_request 事件往返（registry.call_webview_hook）。

/// 身份 + fileservice 权限校验（Rust 端为最终仲裁）
async fn require_fileservice(
    manager: &PluginManager,
    plugin_id: &str,
    op: &str,
) -> Result<()> {
    if !manager.is_activated(plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' is not activated",
            op, plugin_id
        )));
    }
    if !manager
        .has_permission(
            plugin_id,
            bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE,
        )
        .await
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no fileservice permission",
            op, plugin_id
        )));
    }
    Ok(())
}

/// 挂载文件服务（TS 通道，hook=Webview）
///
/// options_json 为 SDK `MountOptions` 的 camelCase JSON；返回 `MountResult`
/// （mount_path + base_path，与 WASM host fn 版本同构）
#[tauri::command]
pub async fn plugin_filesrv_mount(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    options_json: String,
) -> Result<bedcode_plugin_api_mobile::MountResult> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_mount").await?;
    let options: bedcode_plugin_api_mobile::MountOptions = serde_json::from_str(&options_json)
        .map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "plugin_filesrv_mount: invalid MountOptions JSON for plugin '{}': {}",
                plugin_id, e
            ))
        })?;
    tracing::info!(
        plugin_id = %plugin_id,
        mount = %options.mount_path,
        "plugin_filesrv_mount (TS channel)"
    );
    let fs = crate::state::get_file_service();
    // 幂等注入 AppHandle：Webview 钩子经它 emit 上传请求事件
    fs.registry.set_app_handle(app_handle.clone()).await;
    let entry = fs
        .registry
        .mount(
            &plugin_id,
            options,
            crate::file_service::registry::HookTarget::Webview,
        )
        .await?;
    let result = bedcode_plugin_api_mobile::MountResult {
        mount_path: entry.mount_path.clone(),
        // 移动端无 /api 前缀：/{plugin_id}/{mount}/**（与 WASM host fn 同构）
        base_path: format!("/{}/{}", entry.plugin_id, entry.mount_path),
    };
    // 首个挂载会启动 HTTP 服务；挂载变更后立即公告（异步，不阻塞命令；
    // 错误边界包装：announce/ensure_started panic 不致 release 构建闪退）
    let fs_announce = fs.clone();
    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_after_mount_changed",
        async move {
            fs_announce.after_mount_changed().await;
        },
    );
    Ok(result)
}

/// 更新挂载点的允许目录根（roots_json 为字符串数组 JSON，目录变更即时生效）
#[tauri::command]
pub async fn plugin_filesrv_update_roots(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    mount_path: String,
    roots_json: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_update_roots").await?;
    let roots: Vec<String> = serde_json::from_str(&roots_json).map_err(|e| {
        crate::AppError::InvalidInput(format!(
            "plugin_filesrv_update_roots: invalid roots JSON for plugin '{}': {}",
            plugin_id, e
        ))
    })?;
    let fs = crate::state::get_file_service();
    fs.registry
        .update_roots(&plugin_id, &mount_path, roots)
        .await?;
    // 目录变更即时生效：重新公告（挂载集合未变，公告幂等）
    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_after_update_roots",
        async move {
            fs.after_mount_changed().await;
        },
    );
    Ok(())
}

/// 摘除挂载点（对应 TS SDK `mount.dispose()`）
#[tauri::command]
pub async fn plugin_filesrv_dispose(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    mount_path: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_dispose").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        mount = %mount_path,
        "plugin_filesrv_dispose (TS channel)"
    );
    let fs = crate::state::get_file_service();
    fs.registry.unmount(&plugin_id, &mount_path).await?;
    // 末个挂载摘除时停服务并 Withdraw，否则重新公告
    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_after_unmount",
        async move {
            fs.after_unmount().await;
        },
    );
    Ok(())
}

/// 回填 Webview 上传策略钩子的决定
///
/// 宿主在上传会话创建时 emit `filesrv:upload_request` 事件，前端插件回调
/// 后经本命令回填；request 已超时/不存在时返回错误（fail-closed 已由宿主兜底）
#[tauri::command]
pub async fn plugin_filesrv_respond_upload_request(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    request_id: String,
    allow: bool,
    reason: Option<String>,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_respond_upload_request").await?;
    let decision = if allow {
        bedcode_plugin_api_mobile::UploadHookDecision::allow()
    } else {
        bedcode_plugin_api_mobile::UploadHookDecision::deny(
            reason.unwrap_or_else(|| "upload denied by plugin without reason".to_string()),
        )
    };
    let fs = crate::state::get_file_service();
    let matched = fs
        .registry
        .respond_upload_hook(&request_id, decision)
        .await;
    if matched {
        Ok(())
    } else {
        Err(crate::AppError::InvalidInput(format!(
            "plugin_filesrv_respond_upload_request: request '{}' not pending (timed out or unknown) for plugin '{}'",
            request_id, plugin_id
        )))
    }
}

/// 获取对端文件服务信息（peers 表由桌面端 sync 推送填充；未公告返回 null）
#[tauri::command]
pub async fn plugin_filesrv_get_peer(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    peer_id: String,
) -> Result<Option<bedcode_plugin_api_mobile::PeerFileService>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_get_peer").await?;
    let fs = crate::state::get_file_service();
    Ok(fs.registry.get_peer(&peer_id).await)
}

/// 弹出系统目录选择对话框（插件设置页选择允许目录用）
///
/// 用户取消返回 null。Android/iOS 无目录选择能力（tauri-plugin-dialog 的
/// pick_folder 仅桌面可用）：返回明确错误，插件可在 catch 中改用手动路径
/// 输入（如 `context.dialogs.showPrompt`）
#[tauri::command]
pub async fn plugin_pick_directory(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_pick_directory").await?;
    pick_directory_native(&plugin_id, &app_handle).await
}

/// 桌面：tauri-plugin-dialog 系统目录选择对话框
#[cfg(not(any(target_os = "android", target_os = "ios")))]
async fn pick_directory_native(
    plugin_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_folder(move |selection| {
        // 接收端仍在 await 时才有效；命令被取消时发送失败，记日志即可
        if tx.send(selection).is_err() {
            tracing::debug!("plugin_pick_directory: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(file_path)) => {
            let path = file_path.into_path().map_err(|e| {
                crate::AppError::InvalidInput(format!(
                    "plugin_pick_directory: failed to convert selected path for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;
            path.to_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                crate::AppError::InvalidInput(format!(
                    "plugin_pick_directory: selected path is not valid UTF-8 for plugin '{}'",
                    plugin_id
                ))
            })
        }
        // 用户取消选择
        Ok(None) => Ok(None),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "plugin_pick_directory: dialog channel closed for plugin '{}': {}",
            plugin_id, e
        ))),
    }
}

/// 移动端（Android）：经 Kotlin SafPickerPlugin 弹 SAF 目录树选择器，
/// 解析为真实路径（主存储/SD 卡/downloads raw:）；不支持的 provider 返回错误供插件降级
#[cfg(target_os = "android")]
async fn pick_directory_native(
    plugin_id: &str,
    _app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    crate::plugin::android_plugins::pick_directory_android()
        .await
        .map_err(|e| crate::AppError::Plugin(format!("{}: {}", plugin_id, e)))
}

/// 移动端（iOS）：系统选择器无目录选择能力，返回明确错误供插件降级
#[cfg(target_os = "ios")]
async fn pick_directory_native(
    plugin_id: &str,
    _app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    tracing::warn!(
        plugin_id = %plugin_id,
        "plugin_pick_directory: directory picker unavailable on iOS"
    );
    Err(crate::AppError::InvalidInput(format!(
        "plugin_pick_directory: directory picker is not available on this platform (plugin '{}'); fall back to manual path input",
        plugin_id
    )))
}

// ==================== Plugin File Picker ====================

/// 弹出系统文件选择对话框（插件上传本地文件用；用户取消返回 null）
#[tauri::command]
pub async fn plugin_pick_file(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<String>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_pick_file").await?;
    pick_file_native(&plugin_id, &app_handle).await
}

/// 桌面：tauri-plugin-dialog 系统文件选择对话框
#[cfg(not(any(target_os = "android", target_os = "ios")))]
async fn pick_file_native(
    plugin_id: &str,
    app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_file(move |selection| {
        // 接收端仍在 await 时才有效；命令被取消时发送失败，记日志即可
        if tx.send(selection).is_err() {
            tracing::debug!("plugin_pick_file: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(file_path)) => {
            let path = file_path.into_path().map_err(|e| {
                crate::AppError::InvalidInput(format!(
                    "plugin_pick_file: failed to convert selected path for plugin '{}': {}",
                    plugin_id, e
                ))
            })?;
            path.to_str().map(|s| Some(s.to_string())).ok_or_else(|| {
                crate::AppError::InvalidInput(format!(
                    "plugin_pick_file: selected path is not valid UTF-8 for plugin '{}'",
                    plugin_id
                ))
            })
        }
        // 用户取消选择
        Ok(None) => Ok(None),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "plugin_pick_file: dialog channel closed for plugin '{}': {}",
            plugin_id, e
        ))),
    }
}

/// 移动端（Android）：经 Kotlin SafPickerPlugin 弹 SAF 文件选择器，
/// 优先 _data 列直读真实路径，否则按 externalstorage/downloads raw: 解析
#[cfg(target_os = "android")]
async fn pick_file_native(
    plugin_id: &str,
    _app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    crate::plugin::android_plugins::pick_file_android()
        .await
        .map_err(|e| crate::AppError::Plugin(format!("{}: {}", plugin_id, e)))
}

/// 移动端（iOS）：系统文档选择器未接入，返回明确错误供插件降级
#[cfg(target_os = "ios")]
async fn pick_file_native(
    plugin_id: &str,
    _app_handle: &tauri::AppHandle,
) -> Result<Option<String>> {
    tracing::warn!(
        plugin_id = %plugin_id,
        "plugin_pick_file: file picker unavailable on iOS"
    );
    Err(crate::AppError::InvalidInput(format!(
        "plugin_pick_file: file picker is not available on this platform (plugin '{}'); fall back to manual path input",
        plugin_id
    )))
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
