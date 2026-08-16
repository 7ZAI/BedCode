//! host_filesrv_* / host_transfer_* / host_mark_plugin_error — 文件服务与传输（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：标记插件为错误状态（WIT host-log.mark-plugin-error）
///
/// 插件自检失败（如 API 配置无效）时调用。宿主置 Error 状态、
/// 持久化启用状态为 false，并通知前端。
pub(crate) fn mark_plugin_error(state: &WasmPluginState, msg: &str) {
    (state.host_ctx.status_reporter)(&state.plugin_id, msg);
}

// ==================== 逻辑层（文件服务与传输） ====================
//
// 与桌面端 host_functions/file_service.rs + transfer.rs 同语义（移动端独立实现）。

/// 逻辑层：挂载（返回 MountResult JSON）
pub(crate) fn filesrv_mount(
    state: &WasmPluginState,
    opts_json: &str,
) -> Result<String, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let options: bedcode_plugin_api_mobile::MountOptions = serde_json::from_str(opts_json)
        .map_err(|e| format!("invalid MountOptions JSON: {}", e))?;

    let fs = crate::state::get_file_service();
    let mount_entry = guarded_host_call(
        &state.plugin_id,
        "host_filesrv_mount",
        Err(crate::AppError::Internal("host_filesrv_mount panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(fs.registry.mount(
                        &state.plugin_id,
                        options,
                        crate::file_service::registry::HookTarget::Wasm,
                    ))
            })
        },
    )
    .map_err(|e| format!("mount failed: {}", e))?;

    let result = bedcode_plugin_api_mobile::MountResult {
        mount_path: mount_entry.mount_path.clone(),
        // 移动端无 /api 前缀：/{plugin_id}/{mount}/**
        base_path: format!("/{}/{}", mount_entry.plugin_id, mount_entry.mount_path),
    };
    let result_json = serde_json::to_string(&result)
        .map_err(|e| format!("serialize MountResult failed: {}", e))?;

    // 首个挂载会启动 HTTP 服务；挂载变更后立即公告（异步，不阻塞 WASM 调用；
    // 错误边界包装：announce/ensure_started panic 不致 release 构建闪退）
    crate::system::error_boundary::spawn_with_error_boundary("filesrv_wasm_after_mount", async move {
        fs.after_mount_changed().await;
    });

    Ok(result_json)
}

/// 逻辑层：卸载挂载点
pub(crate) fn filesrv_unmount(state: &WasmPluginState, mount_path: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_unmount",
        Err(crate::AppError::Internal("host_filesrv_unmount panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(fs.registry.unmount(&state.plugin_id, mount_path))
            })
        },
    )
    .map_err(|e| format!("unmount failed: {}", e))?;

    // 末个挂载摘除时停服务 + Withdraw，否则重新公告
    crate::system::error_boundary::spawn_with_error_boundary("filesrv_wasm_after_unmount", async move {
        fs.after_unmount().await;
    });
    Ok(())
}

/// 逻辑层：更新挂载点允许目录根（roots 为 JSON 数组字符串）
pub(crate) fn filesrv_update_roots(
    state: &WasmPluginState,
    mount_path: &str,
    roots_json: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let roots: Vec<String> = serde_json::from_str(roots_json)
        .map_err(|e| format!("invalid roots JSON: {}", e))?;

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_update_roots",
        Err(crate::AppError::Internal("host_filesrv_update_roots panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(fs.registry.update_roots(&state.plugin_id, mount_path, roots))
            })
        },
    )
    .map_err(|e| format!("update failed: {}", e))?;

    // 目录变更即时生效：重新公告（挂载集合未变，公告幂等）
    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_wasm_after_update_roots",
        async move {
            fs.after_mount_changed().await;
        },
    );
    Ok(())
}

/// 逻辑层：获取对端文件服务信息（未公告返回 Ok(None)）
pub(crate) fn filesrv_get_peer(state: &WasmPluginState, peer_id: &str) -> Result<Option<String>, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    let peer = guarded_host_call(&state.plugin_id, "host_filesrv_get_peer", None, || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(fs.registry.get_peer(peer_id))
        })
    });

    let Some(peer) = peer else {
        // 未公告：插件侧 SDK 映射为 Ok(None)
        return Ok(None);
    };
    serde_json::to_string(&peer)
        .map(Some)
        .map_err(|e| format!("serialize failed: {}", e))
}

/// 逻辑层：主动询问对端状态（经 WS 控制面发送 Query）
pub(crate) fn filesrv_query_peer(state: &WasmPluginState, peer_id: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let conn = crate::state::get_connection_manager();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_query_peer",
        Err(crate::AppError::WebSocket("host_filesrv_query_peer panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(async {
                    if !conn.is_connected().await {
                        return Err(crate::AppError::WebSocket("not connected".to_string()));
                    }
                    conn.send(&crate::model::message::Message::file_service(
                        crate::enums::file_service::FileServicePayload::Query {},
                    ))
                    .await
                })
            })
        },
    )
    .map_err(|e| format!("send failed: {}", e))?;

    tracing::debug!(plugin_id = %state.plugin_id, peer_id = %peer_id, "file service query sent");
    Ok(())
}

/// 逻辑层：启动传输任务（返回 task_id；本地路径 fs 授权 fail-closed）
pub(crate) fn transfer_start(
    state: &WasmPluginState,
    req_json: &str,
) -> Result<String, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER)
    {
        return Err("permission denied: transfer".to_string());
    }

    let request: bedcode_plugin_api_mobile::TransferRequest = serde_json::from_str(req_json)
        .map_err(|e| format!("invalid TransferRequest JSON: {}", e))?;

    // 本地路径 fs 授权：下载 = 写授权，上传 = 读授权
    // （panic guard：授权流程异常不崩溃，fail-closed 拒绝并回报插件）
    let authorized = guarded_host_call(&state.plugin_id, "host_transfer_start", false, || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                crate::plugin::transfer::check_local_path_authorized(&state.plugin_id, &request),
            )
        })
    });
    if !authorized {
        return Err(format!(
            "local path not authorized by user: {}",
            request.local_path
        ));
    }

    tracing::info!(
        plugin_id = %state.plugin_id,
        direction = ?request.direction,
        url = %request.url,
        local_path = %request.local_path,
        "host_transfer_start: spawning transfer"
    );
    // 无头/测试上下文（app_handle 为 None）：传输引擎不可用，拒绝
    let Some(app_handle) = state.host_ctx.app_handle.clone() else {
        return Err("app_handle unavailable, rejected".to_string());
    };
    let task_id = crate::plugin::transfer::spawn_transfer(
        request,
        app_handle,
        state.host_ctx.message_bus.clone(),
    );
    tracing::info!(
        plugin_id = %state.plugin_id,
        task_id = %task_id,
        "host_transfer_start: transfer spawned"
    );
    Ok(task_id)
}

/// 逻辑层：取消传输任务（任务不存在也返回 Ok——幂等）
pub(crate) fn transfer_cancel(state: &WasmPluginState, task_id: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER)
    {
        return Err("permission denied: transfer".to_string());
    }

    let cancelled = guarded_host_call(&state.plugin_id, "host_transfer_cancel", false, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(crate::plugin::transfer::cancel_transfer(task_id))
        })
    });
    if cancelled {
        tracing::info!(plugin_id = %state.plugin_id, task_id = %task_id, "transfer cancel requested");
    } else {
        tracing::debug!(
            plugin_id = %state.plugin_id,
            task_id = %task_id,
            "host_transfer_cancel: task not active (already finished or unknown)"
        );
    }
    Ok(())
}

/// 逻辑层：批准传输批（接收端用户应答「接受全部」）
pub(crate) fn filesrv_approve_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_approve_transfer",
        Err(crate::AppError::Internal("host_filesrv_approve_transfer panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(fs.registry.approve_transfer(&state.plugin_id, batch_id))
                    .map_err(crate::file_service::registry::BatchError::into_app_error)
            })
        },
    )
    .map_err(|e| format!("approve failed: {}", e))?;

    tracing::info!(plugin_id = %state.plugin_id, batch_id = %batch_id, "transfer batch approved");
    Ok(())
}

/// 逻辑层：拒绝传输批（接收端用户应答「拒绝全部」）
pub(crate) fn filesrv_reject_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_reject_transfer",
        Err(crate::AppError::Internal("host_filesrv_reject_transfer panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(fs.registry.reject_transfer(&state.plugin_id, batch_id))
                    .map_err(crate::file_service::registry::BatchError::into_app_error)
            })
        },
    )
    .map_err(|e| format!("reject failed: {}", e))?;

    tracing::info!(plugin_id = %state.plugin_id, batch_id = %batch_id, "transfer batch rejected");
    Ok(())
}

/// 逻辑层：设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
pub(crate) fn filesrv_set_approval_timeout(
    state: &WasmPluginState,
    mount_path: &str,
    seconds: u64,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_set_approval_timeout",
        Err(crate::AppError::Internal("host_filesrv_set_approval_timeout panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(fs.registry.set_approval_timeout(
                    &state.plugin_id,
                    mount_path,
                    seconds,
                ))
                .map_err(crate::file_service::registry::BatchError::into_app_error)
            })
        },
    )
    .map_err(|e| format!("set approval timeout failed: {}", e))?;

    tracing::info!(plugin_id = %state.plugin_id, mount = %mount_path, seconds, "approval timeout set");
    Ok(())
}

/// 逻辑层：取消接收中的上传会话（接收端本地取消，session 级）
pub(crate) fn filesrv_cancel_receiving(state: &WasmPluginState, session_id: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let fs = crate::state::get_file_service();
    guarded_host_call(
        &state.plugin_id,
        "host_filesrv_cancel_receiving",
        Err(crate::AppError::Internal("host_filesrv_cancel_receiving panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(fs.registry.cancel_receiving_session(&state.plugin_id, session_id))
                    .map_err(crate::file_service::registry::BatchError::into_app_error)
            })
        },
    )
    .map_err(|e| format!("cancel receiving failed: {}", e))?;

    tracing::info!(plugin_id = %state.plugin_id, session_id = %session_id, "receiving session cancelled");
    Ok(())
}

