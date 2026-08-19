//! host_filesrv_* / host_transfer_* / host_mark_plugin_error — 文件服务与传输（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use std::sync::Arc;

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

// ==================== v2.1 服务器归零：手机自主 HTTP 传输（client 栈） ====================

/// 逻辑层：手机自主发起下载（v2.1 client 栈，GET+Range 落本地/SAF 下载目录）
///
/// 返回 task_id；宿主后台执行下载（HEAD 指纹比对 + 断点续传 + 连续 3 次网络
/// 失败转移失败终态），进度/终态经 `plugin:transfer:progress` + `transfer:{task_id}`
/// 双通道回报插件（与 host_transfer_start 同一通道，task_id 命名空间一致）。
pub(crate) fn filesrv_download(
    state: &WasmPluginState,
    req_json: &str,
) -> Result<String, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let req: bedcode_plugin_api_mobile::FileTransferRequest = serde_json::from_str(req_json)
        .map_err(|e| format!("invalid FileTransferRequest JSON: {}", e))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let app_handle = state
        .host_ctx
        .app_handle
        .clone()
        .ok_or_else(|| "app_handle unavailable, rejected".to_string())?;
    let bus = state.host_ctx.message_bus.clone();

    // 同步解析（端点 + 落点），后台执行实际字节流
    let (base, auth, dest_path) = guarded_host_call(
        &state.plugin_id,
        "host_filesrv_download",
        Err(format!("host_filesrv_download panicked for {}", task_id)),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(async {
                    let (base, auth) =
                        crate::file_service::client::desktop_http_endpoint().await?;
                    // 下载落点：显式 dest_path 或 app 下载目录按文件名
                    let fs = crate::state::get_file_service();
                    let dest = match req.dest_path.clone() {
                        Some(p) => std::path::PathBuf::from(p),
                        None => {
                            let dir = fs.registry.downloads_dir().await.ok_or_else(|| {
                                "file service client: downloads dir unavailable".to_string()
                            })?;
                            let fname = std::path::Path::new(&req.relative_path)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "download".to_string());
                            dir.join(fname)
                        }
                    };
                    Ok::<_, String>((base, auth, dest))
                })
            })
        },
    )?;

    let url = crate::file_service::client::endpoint(
        &base,
        &req.plugin_id,
        &req.mount_path,
        &format!(
            "file?path={}",
            crate::file_service::client::urlencode_path(&req.relative_path)
        ),
    );
    let part_path = std::path::PathBuf::from(format!("{}.part", dest_path.display()));
    let dreq = crate::file_service::client::download::DownloadRequest {
        url,
        auth,
        dest_path: part_path,
        final_path: dest_path,
        total: req.size,
        media: None,
    };

    let handle = crate::file_service::client::TransferHandle::new(task_id.clone());
    crate::plugin::transfer::register_cancel_token(&task_id, handle.token());
    let store = crate::file_service::client::client_cursor_store();
    let app_for_task = app_handle.clone();
    let bus_for_task = bus.clone();
    let task_id_for_task = task_id.clone();

    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_host_download",
        async move {
            let client = crate::file_service::client::DownloadClient::new();
            // 进度 reporter：每 500ms 推送 Running（含瞬时速率）
            let reporter_stop = handle.token().child_token();
            {
                let app = app_for_task.clone();
                let bus = bus_for_task.clone();
                let handle_cl = handle.clone();
                let task_id_inner = task_id_for_task.clone();
                spawn_report_progress(app, bus, handle_cl, task_id_inner, reporter_stop.clone());
            }

            let result = crate::file_service::client::download_with_retry(
                &client, &dreq, store, &handle, 3,
            )
            .await;
            reporter_stop.cancel();
            crate::plugin::transfer::unregister_cancel_token(&task_id_for_task);
            let (transferred, total) = handle.progress();
            let state = match result {
                Ok(_) => bedcode_plugin_api_mobile::TransferState::Completed,
                Err(_) if handle.is_cancelled() => {
                    bedcode_plugin_api_mobile::TransferState::Cancelled
                }
                Err(e) => bedcode_plugin_api_mobile::TransferState::Failed(e.to_string()),
            };
            crate::plugin::transfer::emit_progress(
                &app_for_task,
                &bus_for_task,
                &task_id_for_task,
                transferred,
                total,
                0,
                state,
            );
        },
    );

    tracing::info!(
        plugin_id = %state.plugin_id,
        task_id = %task_id,
        path = %req.relative_path,
        "host_filesrv_download spawned"
    );
    Ok(task_id)
}

/// 逻辑层：手机自主发起上传（v2.1 client 栈，POST/PUT session 编排）
///
/// 返回 task_id；断点真源 = 服务端 session received（重查 + 404 重建）。
/// 上传源 source_path 缺省取 relative_path（插件提供的本地路径）。
pub(crate) fn filesrv_upload(
    state: &WasmPluginState,
    req_json: &str,
) -> Result<String, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let req: bedcode_plugin_api_mobile::FileTransferRequest = serde_json::from_str(req_json)
        .map_err(|e| format!("invalid FileTransferRequest JSON: {}", e))?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let app_handle = state
        .host_ctx
        .app_handle
        .clone()
        .ok_or_else(|| "app_handle unavailable, rejected".to_string())?;
    let bus = state.host_ctx.message_bus.clone();

    let (base, auth) = guarded_host_call(
        &state.plugin_id,
        "host_filesrv_upload",
        Err(format!("host_filesrv_upload panicked for {}", task_id)),
        || {
            tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(crate::file_service::client::desktop_http_endpoint())
            })
        },
    )?;

    let create = crate::file_service::client::CreateUploadRequest {
        relative_path: req.relative_path.clone(),
        size: req.size,
        batch_id: req.batch_id.clone(),
    };
    let source_path = req
        .source_path
        .clone()
        .unwrap_or_else(|| req.relative_path.clone());

    let handle = crate::file_service::client::TransferHandle::new(task_id.clone());
    crate::plugin::transfer::register_cancel_token(&task_id, handle.token());
    let saf = guarded_host_call(
        &state.plugin_id,
        "host_filesrv_upload",
        None,
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(async {
                    let fs = crate::state::get_file_service();
                    fs.registry.saf_io().await
                })
            })
        },
    );
    let app_for_task = app_handle.clone();
    let bus_for_task = bus.clone();
    let task_id_for_task = task_id.clone();

    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_host_upload",
        async move {
            let reporter_stop = handle.token().child_token();
            {
                let app = app_for_task.clone();
                let bus = bus_for_task.clone();
                let handle_cl = handle.clone();
                let task_id_inner = task_id_for_task.clone();
                spawn_report_progress(app, bus, handle_cl, task_id_inner, reporter_stop.clone());
            }

            let client = crate::file_service::client::UploadClient::new();
            let result = client
                .upload_file(
                    &base,
                    &req.plugin_id,
                    &req.mount_path,
                    &create,
                    &auth,
                    &source_path,
                    saf,
                    &handle,
                )
                .await;
            reporter_stop.cancel();
            crate::plugin::transfer::unregister_cancel_token(&task_id_for_task);
            let (transferred, total) = handle.progress();
            let state = match &result {
                Ok(_) => bedcode_plugin_api_mobile::TransferState::Completed,
                Err(_) if handle.is_cancelled() => {
                    bedcode_plugin_api_mobile::TransferState::Cancelled
                }
                Err(e) => bedcode_plugin_api_mobile::TransferState::Failed(e.to_string()),
            };
            crate::plugin::transfer::emit_progress(
                &app_for_task,
                &bus_for_task,
                &task_id_for_task,
                transferred,
                total,
                0,
                state,
            );
        },
    );

    tracing::info!(
        plugin_id = %state.plugin_id,
        task_id = %task_id,
        path = %req.relative_path,
        "host_filesrv_upload spawned"
    );
    Ok(task_id)
}

/// 进度 reporter（复用插件传输通道：`plugin:transfer:progress` + bus topic）
fn spawn_report_progress(
    app: Arc<tauri::AppHandle>,
    bus: Arc<crate::plugin::message_bus::MessageBus>,
    handle: crate::file_service::client::TransferHandle,
    task_id: String,
    stop: tokio_util::sync::CancellationToken,
) {
    crate::system::error_boundary::spawn_with_error_boundary(
        "filesrv_host_progress",
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            interval.tick().await; // 首个 tick 立即完成，跳过避免启动即推
            let mut last_bytes = 0u64;
            let mut last_tick = tokio::time::Instant::now();
            loop {
                tokio::select! {
                    _ = stop.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let now = tokio::time::Instant::now();
                let (current, total) = handle.progress();
                let elapsed = now.duration_since(last_tick).as_secs_f64();
                let bps = if elapsed > 0.0 {
                    (current.saturating_sub(last_bytes) as f64 / elapsed) as u64
                } else {
                    0
                };
                last_bytes = current;
                last_tick = now;
                crate::plugin::transfer::emit_progress(
                    &app,
                    &bus,
                    &task_id,
                    current,
                    total,
                    bps,
                    bedcode_plugin_api_mobile::TransferState::Running,
                );
            }
        },
    );
}




/// 逻辑层：intent 应答（v2.1 push 审批门；decision = "accepted" | "rejected"）
///
/// 经响应器放行/拒绝 push intent：accepted → 手机回 IntentAck{accepted} 并执行
/// 下载；rejected → 回 IntentAck{rejected} 且不执行（防未授权数据流入本机）
pub(crate) fn filesrv_respond_intent(
    state: &WasmPluginState,
    intent_id: &str,
    decision: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE)
    {
        return Err("permission denied: fileservice".to_string());
    }

    let responder = crate::file_service::get_responder();
    match decision {
        "accepted" => guarded_host_call(
            &state.plugin_id,
            "host_filesrv_respond_intent",
            Err("host_filesrv_respond_intent panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    responder
                        .approve_intent(intent_id)
                        .map_err(|e| e.to_string())
                })
            },
        ),
        "rejected" => guarded_host_call(
            &state.plugin_id,
            "host_filesrv_respond_intent",
            Err("host_filesrv_respond_intent panicked".to_string()),
            || {
                tokio::task::block_in_place(|| {
                    responder
                        .reject_intent(intent_id)
                        .map_err(|e| e.to_string())
                })
            },
        ),
        other => Err(format!("invalid intent decision: {}", other)),
    }?;

    tracing::info!(
        plugin_id = %state.plugin_id,
        intent_id = %intent_id,
        decision = %decision,
        "intent responded"
    );
    Ok(())
}
