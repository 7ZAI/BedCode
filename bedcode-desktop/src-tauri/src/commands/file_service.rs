//! File Service Commands
//!
//! 插件文件服务的 Tauri 命令桥接 —— ts-only 插件的 TS 通道（规格 4.1）。
//! WASM 插件经 host functions（host_filesrv_*）直达注册表；ts-only 插件无法
//! 调用 host function，其挂载/目录更新/摘除/上传钩子回填经此转发，
//! 挂载的上传策略钩子目标为 [`HookTarget::Webview`]（事件往返见 registry）。
//!
//! 所有命令双重校验（Rust 端为最终仲裁）：插件处于 Activated 状态 +
//! manifest 声明了 fileservice 权限。

use crate::plugin::file_service::HookTarget;
use crate::plugin::host::PluginHost;
use bedcode_plugin_api::permission::PERMISSION_FILESERVICE;
use bedcode_plugin_api::{MountOptions, MountResult, PeerFileService, UploadHookDecision};
use std::sync::Arc;
use tauri::State;

// ==================== 内部工具 ====================

/// 校验插件身份与 fileservice 权限
///
/// 插件必须处于 Activated 状态且已声明 fileservice 权限，
/// 否则拒绝（与 api_bridge 中 storage/terminal 命令同模式）
async fn require_fileservice(
    plugin_host: &PluginHost,
    plugin_id: &str,
    op: &str,
) -> crate::Result<()> {
    if !plugin_host.is_activated(plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' is not activated",
            op, plugin_id
        )));
    }
    if !plugin_host
        .permission()
        .check(plugin_id, PERMISSION_FILESERVICE)
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no fileservice permission",
            op, plugin_id
        )));
    }
    Ok(())
}

// ==================== 挂载生命周期 ====================

/// 挂载文件服务（TS 通道，hook=Webview）
///
/// options_json 为 SDK `MountOptions` 的 camelCase JSON；
/// 返回 `MountResult`（mount_path + base_path，与 WASM host fn 版本一致）
#[tauri::command]
pub async fn plugin_filesrv_mount(
    plugin_id: String,
    options_json: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<MountResult> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_mount").await?;
    let options: MountOptions = serde_json::from_str(&options_json).map_err(|e| {
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
    let entry = plugin_host
        .file_service()
        .mount(&plugin_id, options, HookTarget::Webview)
        .await?;
    Ok(MountResult {
        mount_path: entry.mount_path.clone(),
        base_path: format!("/api/plugins/{}/{}", plugin_id, entry.mount_path),
    })
}

/// 更新挂载点的允许目录根（roots_json 为字符串数组 JSON，目录变更即时生效）
#[tauri::command]
pub async fn plugin_filesrv_update_roots(
    plugin_id: String,
    mount_path: String,
    roots_json: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_update_roots").await?;
    let roots: Vec<String> = serde_json::from_str(&roots_json).map_err(|e| {
        crate::AppError::InvalidInput(format!(
            "plugin_filesrv_update_roots: invalid roots JSON for plugin '{}': {}",
            plugin_id, e
        ))
    })?;
    plugin_host
        .file_service()
        .update_roots(&plugin_id, &mount_path, roots)
        .await
}

/// 摘除挂载点（对应 TS SDK `mount.dispose()`；插件 deactivate 时前端调用）
#[tauri::command]
pub async fn plugin_filesrv_dispose(
    plugin_id: String,
    mount_path: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_dispose").await?;
    tracing::info!(
        plugin_id = %plugin_id,
        mount = %mount_path,
        "plugin_filesrv_dispose (TS channel)"
    );
    plugin_host
        .file_service()
        .unmount(&plugin_id, &mount_path)
        .await
}

// ==================== 上传策略钩子 ====================

/// 回填 Webview 上传策略钩子的决定
///
/// 宿主在上传会话创建时 emit `filesrv:upload_request` 事件，前端插件回调
/// 后经本命令回填；request 已超时/不存在时返回错误（fail-closed 已由宿主兜底）
#[tauri::command]
pub async fn plugin_filesrv_respond_upload_request(
    plugin_id: String,
    request_id: String,
    allow: bool,
    reason: Option<String>,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_respond_upload_request").await?;
    let decision = if allow {
        UploadHookDecision::allow()
    } else {
        UploadHookDecision::deny(reason.unwrap_or_else(|| "upload denied by plugin without reason".to_string()))
    };
    let matched = plugin_host
        .file_service()
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

// ==================== v2 传输批命令 ====================

/// 批准传输批（接收端用户应答「接受全部」）
///
/// 批 pending → approved + 本地 `filesrv:transfer_resolved` 事件 +
/// 跨端 WS 推送 TransferApproval（发送方任务转传输中）
#[tauri::command]
pub async fn plugin_filesrv_approve_transfer(
    plugin_id: String,
    batch_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_approve_transfer").await?;
    plugin_host
        .file_service()
        .approve_transfer(&plugin_id, &batch_id)
        .await
        .map_err(map_batch_error)?;
    plugin_host
        .file_service()
        .publish_batch_resolved(&batch_id, "approved", "")
        .await;
    Ok(())
}

/// 拒绝传输批（接收端用户应答「拒绝全部」）
///
/// 批 pending → rejected(user-rejected) + resolved 事件 + 跨端推送
#[tauri::command]
pub async fn plugin_filesrv_reject_transfer(
    plugin_id: String,
    batch_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_reject_transfer").await?;
    plugin_host
        .file_service()
        .reject_transfer(&plugin_id, &batch_id)
        .await
        .map_err(map_batch_error)?;
    plugin_host
        .file_service()
        .publish_batch_resolved(&batch_id, "rejected", "user-rejected")
        .await;
    Ok(())
}

/// 设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
#[tauri::command]
pub async fn plugin_filesrv_set_approval_timeout(
    plugin_id: String,
    mount_path: String,
    seconds: u64,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_set_approval_timeout").await?;
    plugin_host
        .file_service()
        .set_approval_timeout(&plugin_id, &mount_path, seconds)
        .await
        .map_err(map_batch_error)
}

/// 取消接收中的上传会话（接收端本地取消，session 级）
///
/// 清理 .part + 推送 `filesrv:receiving_done(cancelled)`；
/// 发送方 session 丢失后自动重建从头传（v1 语义兜底）
#[tauri::command]
pub async fn plugin_filesrv_cancel_receiving(
    plugin_id: String,
    session_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_cancel_receiving").await?;
    plugin_host
        .file_service()
        .cancel_receiving_session(&plugin_id, &session_id)
        .await
        .map_err(|e| crate::AppError::NotFound(format!("cancel receiving failed: {}", e)))?;
    Ok(())
}

/// 回填 Webview 批量传输请求钩子的决定（v2，decision_json 为 UploadHookDecision JSON）
///
/// 宿主在 POST /transfer-request 时 emit `filesrv:transfer_request_hook` 事件，
/// 前端插件回调后经本命令回填；request 已超时/不存在时返回错误（fail-closed 已由宿主兜底）
#[tauri::command]
pub async fn plugin_filesrv_respond_transfer_request(
    plugin_id: String,
    request_id: String,
    decision_json: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_respond_transfer_request").await?;
    let decision: UploadHookDecision = serde_json::from_str(&decision_json).map_err(|e| {
        crate::AppError::InvalidInput(format!(
            "plugin_filesrv_respond_transfer_request: invalid decision JSON for plugin '{}': {}",
            plugin_id, e
        ))
    })?;
    let matched = plugin_host
        .file_service()
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

/// BatchError → AppError（spec §3.3：批不存在 → NotFound；非 pending → InvalidInput）
fn map_batch_error(e: crate::plugin::file_service::transfer::BatchError) -> crate::AppError {
    use crate::plugin::file_service::transfer::BatchError;
    match e {
        BatchError::NotFound(msg) => crate::AppError::NotFound(msg),
        BatchError::NotPending(msg) => crate::AppError::InvalidInput(msg),
        // 命令路径不产生 gating/策略拒绝（那是 HTTP 端点语义），按插件错误兜底
        other => crate::AppError::Plugin(other.to_string()),
    }
}

// ==================== 对端信息与目录选择 ====================

/// 获取对端文件服务信息（peers 表由 WS 控制面填充；未公告返回 null）
#[tauri::command]
pub async fn plugin_filesrv_get_peer(
    plugin_id: String,
    peer_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<Option<PeerFileService>> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_filesrv_get_peer").await?;
    Ok(plugin_host.file_service().get_peer(&peer_id).await)
}

/// 弹出系统目录选择对话框（插件设置页选择允许目录用）
///
/// 用户取消返回 null；同样要求 fileservice 权限，避免未授权插件探测本地路径
#[tauri::command]
pub async fn plugin_pick_directory(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
    app_handle: tauri::AppHandle,
) -> crate::Result<Option<String>> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_pick_directory").await?;
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

/// 弹出系统多文件选择对话框（上传方向“发送到手机”用）
///
/// 返回所选文件的绝对路径列表（用户取消返回空数组）；
/// 同样要求 fileservice 权限，避免未授权插件探测本地路径
#[tauri::command]
pub async fn plugin_pick_files(
    plugin_id: String,
    plugin_host: State<'_, Arc<PluginHost>>,
    app_handle: tauri::AppHandle,
) -> crate::Result<Vec<String>> {
    require_fileservice(&plugin_host, &plugin_id, "plugin_pick_files").await?;
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app_handle.dialog().file().pick_files(move |selection| {
        if tx.send(selection).is_err() {
            tracing::debug!("plugin_pick_files: receiver dropped before dialog completed");
        }
    });
    match rx.await {
        Ok(Some(paths)) => {
            let mut result = Vec::with_capacity(paths.len());
            for file_path in paths {
                let path = file_path.into_path().map_err(|e| {
                    crate::AppError::InvalidInput(format!(
                        "plugin_pick_files: failed to convert selected path for plugin '{}': {}",
                        plugin_id, e
                    ))
                })?;
                let s = path.to_str().ok_or_else(|| {
                    crate::AppError::InvalidInput(format!(
                        "plugin_pick_files: selected path is not valid UTF-8 for plugin '{}'",
                        plugin_id
                    ))
                })?;
                result.push(s.to_string());
            }
            Ok(result)
        }
        // 用户取消选择
        Ok(None) => Ok(Vec::new()),
        Err(e) => Err(crate::AppError::Plugin(format!(
            "plugin_pick_files: dialog channel closed for plugin '{}': {}",
            plugin_id, e
        ))),
    }
}
