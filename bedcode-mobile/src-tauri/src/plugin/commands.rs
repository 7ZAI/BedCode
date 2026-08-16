//! Mobile Plugin Commands
//!
//! 暴露插件操作为 Tauri invoke 命令

use crate::file_service::registry::BatchError;
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

/// 批准插件权限（人工审批：记录权限清单 + 目录内容哈希钉扎）
///
/// 仅用户安装插件（file-install / remote-download）需要审批；
/// 内置插件（apk-asset）调用返回错误。批准成功后状态 NeedsApproval → Loaded。
#[tauri::command]
pub async fn plugin_approve(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    manager.approve(&plugin_id).await
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

/// 身份 + system:open 权限校验（Rust 端为最终仲裁）
async fn require_system_open(
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
            bedcode_plugin_api_mobile::permission::PERMISSION_SYSTEM_OPEN,
        )
        .await
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no system:open permission",
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

// ==================== Transfer Batch Commands（v2 接收策略 / 批量批准） ====================
//
// 接收端用户应答（approve/reject）、批准超时配置、接收中任务取消、
// Webview 批钩子回填。与 WASM host functions（host_filesrv_*）同构，
// 均过 require_fileservice 门控。

/// 批准传输批（接收端用户应答「接受全部」）
///
/// 批 pending → approved；随后宿主发本地 resolved 事件 + 跨端推送发送方
#[tauri::command]
pub async fn plugin_filesrv_approve_transfer(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    batch_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_approve_transfer").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        "plugin_filesrv_approve_transfer"
    );
    let fs = crate::state::get_file_service();
    fs.registry
        .approve_transfer(&plugin_id, &batch_id)
        .await
        .map_err(BatchError::into_app_error)
}

/// 拒绝传输批（接收端用户应答「拒绝全部」）
///
/// 批 pending → rejected(user-rejected)；随后宿主发 resolved 事件 + 跨端推送
#[tauri::command]
pub async fn plugin_filesrv_reject_transfer(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    batch_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_reject_transfer").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        batch_id = %batch_id,
        "plugin_filesrv_reject_transfer"
    );
    let fs = crate::state::get_file_service();
    fs.registry
        .reject_transfer(&plugin_id, &batch_id)
        .await
        .map_err(BatchError::into_app_error)
}

/// 设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
#[tauri::command]
pub async fn plugin_filesrv_set_approval_timeout(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    mount_path: String,
    seconds: u64,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_set_approval_timeout").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        mount = %mount_path,
        seconds = seconds,
        "plugin_filesrv_set_approval_timeout"
    );
    let fs = crate::state::get_file_service();
    fs.registry
        .set_approval_timeout(&plugin_id, &mount_path, seconds)
        .await
        .map_err(BatchError::into_app_error)
}

/// 取消接收中的上传会话（接收端本地取消，session 级）
///
/// 取消后删除 .part 临时文件并发出 `filesrv:receiving_done`(cancelled)
#[tauri::command]
pub async fn plugin_filesrv_cancel_receiving(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    session_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_cancel_receiving").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        session_id = %session_id,
        "plugin_filesrv_cancel_receiving"
    );
    let fs = crate::state::get_file_service();
    fs.registry
        .cancel_receiving_session(&plugin_id, &session_id)
        .await
        .map_err(BatchError::into_app_error)
}

/// 回填 Webview 批量传输钩子的决定
///
/// 宿主在 POST /transfer-request 时 emit `filesrv:transfer_request_hook` 事件，
/// 前端插件回调后经本命令回填；decision_json 为 `UploadHookDecision` 的
/// camelCase JSON（allow/ask/reason）。request 已超时/不存在时返回错误
#[tauri::command]
pub async fn plugin_filesrv_respond_transfer_request(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    request_id: String,
    decision_json: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_filesrv_respond_transfer_request").await?;
    let decision: bedcode_plugin_api_mobile::UploadHookDecision =
        serde_json::from_str(&decision_json).map_err(|e| {
            crate::AppError::InvalidInput(format!(
                "plugin_filesrv_respond_transfer_request: invalid decision JSON for plugin '{}': {}",
                plugin_id, e
            ))
        })?;
    let fs = crate::state::get_file_service();
    let matched = fs
        .registry
        .respond_transfer_hook(&request_id, decision)
        .await;
    if matched {
        Ok(())
    } else {
        Err(crate::AppError::InvalidInput(format!(
            "plugin_filesrv_respond_transfer_request: request '{}' not pending (timed out or unknown) for plugin '{}'",
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

/// 用系统查看器打开已下载文件（传输完成「打开本地文件」）
///
/// 经 Kotlin DownloadsDirPlugin.openFile：MediaStore 公共下载按名命中优先，
/// 未命中回退 FileProvider。需 system:open 权限。
#[tauri::command]
pub async fn plugin_open_file(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    path: String,
    display_name: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_system_open(&manager, &plugin_id, "plugin_open_file").await?;
    crate::plugin::android_plugins::open_download_file(&path, &display_name).await
}

/// 打开文件所在目录（历史记录「打开所在文件夹」；FileProvider + ACTION_VIEW）。
/// 需 system:open 权限。
#[tauri::command]
pub async fn plugin_open_file_location(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    path: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_system_open(&manager, &plugin_id, "plugin_open_file_location").await?;
    crate::plugin::android_plugins::open_download_file_location(&path).await
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


// ==================== All Files Access（分区存储授权引导） ====================

/// 查询/引导「所有文件访问权限」（Android 11+ 分区存储）
///
/// 非媒体集合的顶层自定义目录（如存储根目录下的自定义文件夹）read_dir 被
/// FUSE 过滤为空，需用户手动在系统设置授予 MANAGE_EXTERNAL_STORAGE
/// （无运行时弹窗机制）。此命令跳转系统授权页并返回跳转前是否已授权；
/// 非 Android 平台（桌面 dev 窗口 / iOS）返回明确错误。
#[tauri::command]
pub async fn open_all_files_settings(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<bool> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "open_all_files_settings").await?;
    crate::plugin::android_plugins::open_all_files_settings_android().await
}


// ==================== SAF 存储访问（SafIo 主 seam，共享目录/上传页用） ====================

/// SAF：列出目录树子条目（共享目录 App 内遍历，免系统选择器）
///
/// 权限经 require_fileservice 门控；实现经 Tauri state 的 SafIoState
/// 注入（Android = KotlinSafIo 转发 SafTransferPlugin；测试注入 fake）。
#[tauri::command]
pub async fn plugin_saf_list_tree(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    tree_uri: String,
    document_id: String,
) -> Result<Vec<crate::plugin::saf_io::SafEntry>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_list_tree").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_list_tree_impl(saf.inner().0.as_ref(), &tree_uri, &document_id)
}

/// SAF：启动中转复制（SAF 源 → app 私有 cache），返回 {copyId, destPath}
#[tauri::command]
pub async fn plugin_saf_copy_start(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    uri: String,
    dest_name: String,
) -> Result<crate::plugin::saf_io::SafCopyHandle> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_copy_start").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_copy_start_impl(saf.inner().0.as_ref(), &uri, &dest_name)
}

/// SAF：轮询中转复制进度（「准备中」进度条数据源）
#[tauri::command]
pub async fn plugin_saf_copy_status(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    copy_id: String,
) -> Result<crate::plugin::saf_io::SafCopyStatus> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_copy_status").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_copy_status_impl(saf.inner().0.as_ref(), &copy_id)
}

/// SAF：取消中转复制（复制方删除半成品后结束，无残留）
#[tauri::command]
pub async fn plugin_saf_copy_cancel(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    copy_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_copy_cancel").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_copy_cancel_impl(saf.inner().0.as_ref(), &copy_id)
}

/// SAF：清扫中转复制残留（file-transfer 插件激活时调用）
#[tauri::command]
pub async fn plugin_saf_cleanup_stale_copies(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_cleanup_stale_copies").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_cleanup_stale_copies_impl(saf.inner().0.as_ref())
}

/// SAF：检测树授权是否仍有效（失效标记 → 前端提示重新授权）
#[tauri::command]
pub async fn plugin_saf_check_authorized(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    tree_uri: String,
) -> Result<bool> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_check_authorized").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_check_authorized_impl(saf.inner().0.as_ref(), &tree_uri)
}

/// SAF：写入 MediaStore 公共下载目录（接收方向统一落点，M2）
///
/// src_path 为 app 私有下载目录中的最终文件；mime_type 为空串时由
/// Kotlin 按扩展名推断。失败（含 API<29 设备不支持）由调用方回退私有目录。
#[tauri::command]
pub async fn plugin_saf_write_media_downloads(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    src_path: String,
    display_name: String,
    mime_type: String,
) -> Result<()> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_write_media_downloads").await?;
    let saf = app_handle.state::<crate::plugin::saf_io::SafIoState>();
    saf_write_media_downloads_impl(
        saf.inner().0.as_ref(),
        &src_path,
        &display_name,
        &mime_type,
    )
}

/// SAF：弹系统目录树选择器，返回 SAF 树元数据（添加共享目录条目用）
///
/// 返回 (uri, documentId, displayName)；用户取消返回 None。
/// 持久化授权（takePersistableUriPermission）由 Kotlin SafPickerPlugin 完成。
#[tauri::command]
pub async fn plugin_pick_shared_directory(
    app_handle: tauri::AppHandle,
    plugin_id: String,
) -> Result<Option<(String, String, String)>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_pick_shared_directory").await?;
    crate::plugin::android_plugins::pick_shared_directory_android()
        .await
        .map_err(|e| crate::AppError::Plugin(format!("{}: {}", plugin_id, e)))
}

/// 列出真实路径目录条目（免授权特殊条目「app 私有下载目录」浏览用）
///
/// 仅允许 AppDownloadsDir（Android 外部私有下载目录）及其子目录：
/// 特殊条目是唯一保留的真实路径共享条目（CONTEXT.md「文件传输」术语），
/// 白名单校验防任意路径探测。基址解析与 WASM host_config_get 共用同一
/// 函数（resolve_app_downloads_dir），保证外部存储不可用时特殊条目的
/// 派生路径与浏览白名单一致。
#[tauri::command]
pub async fn plugin_saf_list_dir(
    app_handle: tauri::AppHandle,
    plugin_id: String,
    path: String,
) -> Result<Vec<crate::plugin::saf_io::SafEntry>> {
    let manager = app_handle.state::<Arc<PluginManager>>();
    require_fileservice(&manager, &plugin_id, "plugin_saf_list_dir").await?;
    list_private_downloads_dir(&app_handle, &path).await
}

/// 命令内部实现（与 Tauri 解耦，供单测注入 fake SafIo）
fn saf_list_tree_impl(
    saf: &dyn crate::plugin::saf_io::SafIo,
    tree_uri: &str,
    document_id: &str,
) -> Result<Vec<crate::plugin::saf_io::SafEntry>> {
    saf.list_tree(tree_uri, document_id).map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_list_tree({}): {}", tree_uri, e))
    })
}

fn saf_copy_start_impl(
    saf: &dyn crate::plugin::saf_io::SafIo,
    uri: &str,
    dest_name: &str,
) -> Result<crate::plugin::saf_io::SafCopyHandle> {
    saf.read_to_cache(uri, dest_name).map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_copy_start({}): {}", uri, e))
    })
}

fn saf_copy_status_impl(
    saf: &dyn crate::plugin::saf_io::SafIo,
    copy_id: &str,
) -> Result<crate::plugin::saf_io::SafCopyStatus> {
    saf.copy_status(copy_id).map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_copy_status({}): {}", copy_id, e))
    })
}

fn saf_copy_cancel_impl(saf: &dyn crate::plugin::saf_io::SafIo, copy_id: &str) -> Result<()> {
    saf.cancel_copy(copy_id).map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_copy_cancel({}): {}", copy_id, e))
    })
}

fn saf_cleanup_stale_copies_impl(saf: &dyn crate::plugin::saf_io::SafIo) -> Result<()> {
    saf.cleanup_stale_copies().map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_cleanup_stale_copies: {}", e))
    })
}

fn saf_check_authorized_impl(saf: &dyn crate::plugin::saf_io::SafIo, tree_uri: &str) -> Result<bool> {
    saf.check_authorized(tree_uri).map_err(|e| {
        crate::AppError::Plugin(format!("plugin_saf_check_authorized({}): {}", tree_uri, e))
    })
}

fn saf_write_media_downloads_impl(
    saf: &dyn crate::plugin::saf_io::SafIo,
    src_path: &str,
    display_name: &str,
    mime_type: &str,
) -> Result<()> {
    saf.write_media_downloads(src_path, display_name, mime_type).map_err(|e| {
        crate::AppError::Plugin(format!(
            "plugin_saf_write_media_downloads({}): {}",
            src_path, e
        ))
    })
}

/// 列出 AppDownloadsDir 及其子目录（白名单校验 + std::fs::read_dir）
///
/// 目录不可访问（非 Android / 目录被清）返回明确错误；条目只含
/// name/isDir/size（真实路径条目，无 uri/documentId 语义）。
async fn list_private_downloads_dir(
    app_handle: &tauri::AppHandle,
    path: &str,
) -> Result<Vec<crate::plugin::saf_io::SafEntry>> {
    // 白名单：必须位于 AppDownloadsDir（免授权特殊条目）之下；基址解析与
    // WASM host config 共用（resolve_app_downloads_dir，含 app_data 回退）
    if !crate::plugin::android_plugins::is_within_app_downloads_dir(app_handle, path).await {
        return Err(crate::AppError::Auth(format!(
            "plugin_saf_list_dir: path '{}' is outside the private downloads dir",
            path
        )));
    }

    let entries = std::fs::read_dir(path).map_err(|e| {
        crate::AppError::Plugin(format!(
            "plugin_saf_list_dir: failed to read '{}': {}",
            path, e
        ))
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            // 单条目读取失败（并发删除等）跳过，不阻断整个列表
            Err(e) => {
                tracing::debug!(error = %e, path = %path, "plugin_saf_list_dir: skip unreadable entry");
                continue;
            }
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(crate::plugin::saf_io::SafEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: meta.len() as i64,
            mime: String::new(),
            // 真实路径条目以绝对路径承载 uri 字段（免授权特殊条目直读直传）
            uri: entry.path().to_string_lossy().into_owned(),
            document_id: String::new(),
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::saf_io::{SafCopyHandle, SafCopyStatus, SafEntry, SafIo, SafStreamHandle};

    /// 命令层 impl 包装测试用 fake：按配置返回成功或固定错误
    ///
    /// 覆盖 saf_*_impl 的错误上下文包装（AppError::Plugin 带命令名/参数）
    /// 与成功透传（spec「主 seam fake 注入覆盖编排」：命令入口薄包装
    /// 同样纳入，防错误上下文丢失）。
    struct FakeSaf {
        fail_with: Option<&'static str>,
    }

    impl SafIo for FakeSaf {
        fn list_tree(&self, _tree_uri: &str, _document_id: &str) -> Result<Vec<SafEntry>> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(vec![SafEntry {
                    name: "a.txt".to_string(),
                    is_dir: false,
                    size: 1,
                    mime: "text/plain".to_string(),
                    uri: "content://tree/root/document/f1".to_string(),
                    document_id: "f1".to_string(),
                }]),
            }
        }

        fn read_to_cache(&self, _uri: &str, _dest_name: &str) -> Result<SafCopyHandle> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(SafCopyHandle {
                    copy_id: "copy-1".to_string(),
                    dest_path: "/cache/a.txt".to_string(),
                }),
            }
        }

        fn copy_status(&self, _copy_id: &str) -> Result<SafCopyStatus> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(SafCopyStatus {
                    copy_id: "copy-1".to_string(),
                    done: 1,
                    total: 1,
                    finished: true,
                    cancelled: false,
                    error: None,
                    dest_path: "/cache/a.txt".to_string(),
                }),
            }
        }

        fn cancel_copy(&self, _copy_id: &str) -> Result<()> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }

        fn cleanup_stale_copies(&self) -> Result<()> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }

        fn check_authorized(&self, _tree_uri: &str) -> Result<bool> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(true),
            }
        }

        fn write_media_downloads(
            &self,
            src_path: &str,
            display_name: &str,
            _mime_type: &str,
        ) -> Result<()> {
            assert_eq!(src_path, "/data/downloads/a.txt");
            assert_eq!(display_name, "a.txt");
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }

        fn open_stream(&self, _uri: &str, offset: u64) -> Result<SafStreamHandle> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(SafStreamHandle {
                    handle_id: "stream-1".to_string(),
                    effective_offset: offset,
                    seekable: true,
                    size: 0,
                }),
            }
        }

        fn read_stream(&self, _handle_id: &str, _len: usize) -> Result<Vec<u8>> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(b"abc".to_vec()),
            }
        }

        fn seek_stream(&self, _handle_id: &str, _offset: u64) -> Result<()> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }

        fn close_stream(&self, _handle_id: &str) -> Result<()> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }

        fn stream_seekable(&self, _uri: &str) -> Result<bool> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(true),
            }
        }

        fn save_to_document(&self, _src: &str, _name: &str, _mime: &str) -> Result<()> {
            match self.fail_with {
                Some(msg) => Err(crate::AppError::Plugin(msg.to_string())),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn saf_list_tree_impl_wraps_error_with_command_context() {
        let fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_list_tree_impl(&fake, "content://tree/root", "d1").unwrap_err();
        // 错误必须带命令名与参数（调用方定位），而非裸底层错误
        assert!(err.to_string().contains("plugin_saf_list_tree"));
        assert!(err.to_string().contains("content://tree/root"));
    }

    #[test]
    fn saf_list_tree_impl_forwards_entries() {
        let fake = FakeSaf { fail_with: None };
        let entries = saf_list_tree_impl(&fake, "content://tree/root", "d1").expect("list should succeed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "a.txt");
    }

    #[test]
    fn saf_copy_start_impl_wraps_error_with_uri() {
        let fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_copy_start_impl(&fake, "content://tree/root/document/f1", "a.txt").unwrap_err();
        assert!(err.to_string().contains("plugin_saf_copy_start"));
        assert!(err.to_string().contains("content://tree/root/document/f1"));
    }

    #[test]
    fn saf_copy_status_impl_wraps_error_with_copy_id() {
        let fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_copy_status_impl(&fake, "copy-1").unwrap_err();
        assert!(err.to_string().contains("plugin_saf_copy_status"));
        assert!(err.to_string().contains("copy-1"));
    }

    #[test]
    fn saf_copy_cancel_impl_wraps_error_with_copy_id() {
        let fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_copy_cancel_impl(&fake, "copy-1").unwrap_err();
        assert!(err.to_string().contains("plugin_saf_copy_cancel"));
        assert!(err.to_string().contains("copy-1"));
    }

    #[test]
    fn saf_cleanup_stale_copies_impl_wraps_error() {
        let fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_cleanup_stale_copies_impl(&fake).unwrap_err();
        assert!(err.to_string().contains("plugin_saf_cleanup_stale_copies"));
    }

    #[test]
    fn saf_check_authorized_impl_forwards_and_wraps() {
        let ok_fake = FakeSaf { fail_with: None };
        assert!(saf_check_authorized_impl(&ok_fake, "content://tree/root").expect("check should succeed"));
        let err_fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_check_authorized_impl(&err_fake, "content://tree/root").unwrap_err();
        assert!(err.to_string().contains("plugin_saf_check_authorized"));
    }

    #[test]
    fn saf_write_media_downloads_impl_forwards_and_wraps() {
        let ok_fake = FakeSaf { fail_with: None };
        saf_write_media_downloads_impl(&ok_fake, "/data/downloads/a.txt", "a.txt", "")
            .expect("media write should succeed");
        let err_fake = FakeSaf { fail_with: Some("boom") };
        let err = saf_write_media_downloads_impl(&err_fake, "/data/downloads/a.txt", "a.txt", "")
            .unwrap_err();
        assert!(err.to_string().contains("plugin_saf_write_media_downloads"));
        assert!(err.to_string().contains("/data/downloads/a.txt"));
    }
}
