//! host_filesrv_* / host_transfer_* / host_mark_plugin_error — 文件服务与传输

use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

/// 插件状态上报：标记插件为错误状态
///
/// 插件自检失败（如 API 配置无效）时调用。宿主置 Error 状态、
/// 持久化启用状态为 false，并通知前端。
pub(crate) fn host_mark_plugin_error(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    msg_ptr: u32,
    msg_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let msg = match read_wasm_string(&mut caller, msg_ptr, msg_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_mark_plugin_error: failed to read message");
            return;
        }
    };

    (host_ctx.status_reporter)(&plugin_id, &msg);
}

// ==================== File Service & Transfer Host Functions（ABI v4） ====================
//
// 内网文件传输插件规格阶段 2：文件服务挂载注册 + 传输引擎。
// 与桌面端 host_functions/file_service.rs + transfer.rs 同语义（移动端独立实现）。


/// 文件服务：挂载
///
/// 参数：(opts_ptr, opts_len, out_ptr) — opts 为 MountOptions JSON
/// 返回：0 成功（MountResult JSON 写入 out_ptr），-1 失败（权限/fs 授权/参数错误）
pub(crate) fn host_filesrv_mount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    opts_ptr: u32,
    opts_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_mount: permission denied (fileservice)");
        return -1;
    }

    let opts_str = match read_wasm_string(&mut caller, opts_ptr, opts_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_mount: failed to read options");
            return -1;
        }
    };

    let options: bedcode_plugin_api_mobile::MountOptions = match serde_json::from_str(&opts_str) {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: invalid MountOptions JSON");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let mount_path = options.mount_path.clone();
    let handle = caller.data().runtime_handle.clone();
    let mount_result = guarded_host_call(
        &plugin_id,
        "host_filesrv_mount",
        Err(crate::AppError::Internal("host_filesrv_mount panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                handle.block_on(fs.registry.mount(
                    &plugin_id,
                    options,
                    crate::file_service::registry::HookTarget::Wasm,
                ))
            })
        },
    );

    match mount_result {
        Ok(entry) => {
            let result = bedcode_plugin_api_mobile::MountResult {
                mount_path: entry.mount_path.clone(),
                // 移动端无 /api 前缀：/{plugin_id}/{mount}/**
                base_path: format!("/{}/{}", entry.plugin_id, entry.mount_path),
            };
            let result_json = match serde_json::to_string(&result) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_mount: serialize MountResult failed");
                    return -1;
                }
            };

            // 首个挂载会启动 HTTP 服务；挂载变更后立即公告（异步，不阻塞 WASM 调用；
            // 错误边界包装：announce/ensure_started panic 不致 release 构建闪退）
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_mount",
                async move {
                    fs.after_mount_changed().await;
                },
            );

            match write_wasm_string(&mut caller, &result_json) {
                Some((ptr, len)) => {
                    if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                        0
                    } else {
                        -1
                    }
                }
                None => {
                    tracing::error!(plugin_id = %plugin_id, mount = %mount_path, "host_filesrv_mount: failed to write result");
                    -1
                }
            }
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host_filesrv_mount: mount failed");
            -1
        }
    }
}


/// 文件服务：卸载挂载点
///
/// 参数：(mp_ptr, mp_len)
/// 返回：0 成功，-1 失败（权限/挂载不存在）
pub(crate) fn host_filesrv_unmount(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mp_ptr: u32,
    mp_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_unmount: permission denied (fileservice)");
        return -1;
    }

    let mount_path = match read_wasm_string(&mut caller, mp_ptr, mp_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_unmount: failed to read mount path");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_unmount",
        Err(crate::AppError::Internal("host_filesrv_unmount panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(fs.registry.unmount(&plugin_id, &mount_path))),
    );

    match result {
        Ok(()) => {
            // 末个挂载摘除时停服务 + Withdraw，否则重新公告
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_unmount",
                async move {
                    fs.after_unmount().await;
                },
            );
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, mount = %mount_path, error = %e, "host_filesrv_unmount: unmount failed");
            -1
        }
    }
}


/// 文件服务：更新挂载点允许目录根（roots 为 JSON 数组字符串）
///
/// 参数：(mp_ptr, mp_len, roots_ptr, roots_len)
/// 返回：0 成功，-1 失败（权限/挂载不存在/fs 授权/参数错误）
pub(crate) fn host_filesrv_update_roots(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mp_ptr: u32,
    mp_len: u32,
    roots_ptr: u32,
    roots_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_update_roots: permission denied (fileservice)");
        return -1;
    }

    let mount_path = match read_wasm_string(&mut caller, mp_ptr, mp_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_update_roots: failed to read mount path");
            return -1;
        }
    };
    let roots_str = match read_wasm_string(&mut caller, roots_ptr, roots_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_update_roots: failed to read roots");
            return -1;
        }
    };
    let roots: Vec<String> = match serde_json::from_str(&roots_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_filesrv_update_roots: invalid roots JSON");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_update_roots",
        Err(crate::AppError::Internal("host_filesrv_update_roots panicked".to_string())),
        || tokio::task::block_in_place(|| handle.block_on(fs.registry.update_roots(&plugin_id, &mount_path, roots))),
    );

    match result {
        Ok(()) => {
            // 目录变更即时生效：重新公告（挂载集合未变，公告幂等）
            crate::system::error_boundary::spawn_with_error_boundary(
                "filesrv_wasm_after_update_roots",
                async move {
                    fs.after_mount_changed().await;
                },
            );
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, mount = %mount_path, error = %e, "host_filesrv_update_roots: update failed");
            -1
        }
    }
}


/// 文件服务：获取对端文件服务信息
///
/// 参数：(peer_ptr, peer_len, out_ptr)
/// 返回：0 成功（PeerFileService JSON 写入 out_ptr；(0,0) 表示未公告），-1 失败
pub(crate) fn host_filesrv_get_peer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    peer_ptr: u32,
    peer_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_get_peer: permission denied (fileservice)");
        return -1;
    }

    let peer_id = match read_wasm_string(&mut caller, peer_ptr, peer_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_get_peer: failed to read peer id");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let peer = guarded_host_call(&plugin_id, "host_filesrv_get_peer", None, || {
        tokio::task::block_in_place(|| handle.block_on(fs.registry.get_peer(&peer_id)))
    });

    let Some(peer) = peer else {
        // 未公告：out_ptr 写 (0,0)，插件侧 SDK 映射为 Ok(None)
        return if write_result_to_out_ptr(&mut caller, out_ptr, 0, 0) { 0 } else { -1 };
    };

    let json = match serde_json::to_string(&peer) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, peer_id = %peer_id, "host_filesrv_get_peer: serialize failed");
            return -1;
        }
    };
    match write_wasm_string(&mut caller, &json) {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, peer_id = %peer_id, "host_filesrv_get_peer: failed to write result");
            -1
        }
    }
}


/// 文件服务：主动询问对端状态（经 WS 控制面发送 Query）
///
/// 参数：(peer_ptr, peer_len) — 单连接场景忽略 peer_id，向当前连接发送；
/// 对端回复 Announce/Withdraw 后由注册表推送 `filesrv:peer_changed`。
/// 返回：0 成功（已发送），-1 失败（权限/未连接/发送失败）
pub(crate) fn host_filesrv_query_peer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    peer_ptr: u32,
    peer_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_query_peer: permission denied (fileservice)");
        return -1;
    }

    let peer_id = match read_wasm_string(&mut caller, peer_ptr, peer_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_query_peer: failed to read peer id");
            return -1;
        }
    };

    let conn = crate::state::get_connection_manager();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_query_peer",
        Err(crate::AppError::WebSocket("host_filesrv_query_peer panicked".to_string())),
        || {
            tokio::task::block_in_place(|| {
                handle.block_on(async {
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
    );

    match result {
        Ok(_) => {
            tracing::debug!(plugin_id = %plugin_id, peer_id = %peer_id, "file service query sent");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, error = %e, "host_filesrv_query_peer: send failed");
            -1
        }
    }
}


/// 传输引擎：启动传输任务
///
/// 参数：(req_ptr, req_len, out_ptr) — req 为 TransferRequest JSON
/// 返回：0 成功（task_id 写入 out_ptr），-1 失败（权限/fs 授权/参数错误）
pub(crate) fn host_transfer_start(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER) {
        tracing::warn!(plugin_id = %plugin_id, "host_transfer_start: permission denied (transfer)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let req_str = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to read request");
            return -1;
        }
    };

    let request: bedcode_plugin_api_mobile::TransferRequest = match serde_json::from_str(&req_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_transfer_start: invalid TransferRequest JSON");
            return -1;
        }
    };

    // 本地路径 fs 授权：下载 = 写授权，上传 = 读授权
    // （panic guard：授权流程异常不崩溃，fail-closed 拒绝并回报插件）
    let handle = caller.data().runtime_handle.clone();
    let authorized = guarded_host_call(&plugin_id, "host_transfer_start", false, || {
        tokio::task::block_in_place(|| {
            handle.block_on(crate::plugin::transfer::check_local_path_authorized(
                &plugin_id, &request,
            ))
        })
    });
    if !authorized {
        tracing::error!(
            plugin_id = %plugin_id,
            local_path = %request.local_path,
            "host_transfer_start: local path not authorized by user"
        );
        return -1;
    }

    tracing::info!(
        plugin_id = %plugin_id,
        direction = ?request.direction,
        url = %request.url,
        local_path = %request.local_path,
        "host_transfer_start: spawning transfer"
    );
    let task_id = crate::plugin::transfer::spawn_transfer(
        request,
        host_ctx.app_handle.clone(),
        host_ctx.message_bus.clone(),
    );
    tracing::info!(
        plugin_id = %plugin_id,
        task_id = %task_id,
        "host_transfer_start: transfer spawned"
    );

    match write_wasm_string(&mut caller, &task_id) {
        Some((ptr, len)) => {
            if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                0
            } else {
                -1
            }
        }
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_start: failed to write task_id");
            -1
        }
    }
}


/// 传输引擎：取消传输任务
///
/// 参数：(task_ptr, task_len)
/// 返回：0 成功；任务不存在（已完成/未知）也返回 0（幂等），记录 debug 日志
pub(crate) fn host_transfer_cancel(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    task_ptr: u32,
    task_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_TRANSFER) {
        tracing::warn!(plugin_id = %plugin_id, "host_transfer_cancel: permission denied (transfer)");
        return -1;
    }

    let task_id = match read_wasm_string(&mut caller, task_ptr, task_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_transfer_cancel: failed to read task id");
            return -1;
        }
    };

    let handle = caller.data().runtime_handle.clone();
    let cancelled = guarded_host_call(&plugin_id, "host_transfer_cancel", false, || {
        tokio::task::block_in_place(|| handle.block_on(crate::plugin::transfer::cancel_transfer(&task_id)))
    });
    if cancelled {
        tracing::info!(plugin_id = %plugin_id, task_id = %task_id, "transfer cancel requested");
    } else {
        tracing::debug!(
            plugin_id = %plugin_id,
            task_id = %task_id,
            "host_transfer_cancel: task not active (already finished or unknown)"
        );
    }
    0
}

// ==================== Batch Transfer Host Functions（ABI v6） ====================
//
// 批量传输批准协议（v2 接收策略）：接收端用户应答「接受全部/拒绝全部」、
// 设置批准超时、取消接收中的上传会话。全部经 fileservice 权限门控，
// 与 host_filesrv_* 同模式（block_in_place + handle.block_on 执行异步 registry）。

/// 批准传输批（接收端用户应答「接受全部」）
///
/// 参数：(batch_ptr, batch_len)
/// 返回：0 成功，-1 失败（权限/批不存在/批非 pending）
pub(crate) fn host_filesrv_approve_transfer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    batch_ptr: u32,
    batch_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_approve_transfer: permission denied (fileservice)");
        return -1;
    }

    let batch_id = match read_wasm_string(&mut caller, batch_ptr, batch_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_approve_transfer: failed to read batch id");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_approve_transfer",
        Err(crate::AppError::Internal("host_filesrv_approve_transfer panicked".to_string())),
        || tokio::task::block_in_place(|| {
            handle
                .block_on(fs.registry.approve_transfer(&plugin_id, &batch_id))
                .map_err(crate::file_service::registry::BatchError::into_app_error)
        }),
    );

    match result {
        Ok(()) => {
            tracing::info!(plugin_id = %plugin_id, batch_id = %batch_id, "transfer batch approved");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, batch_id = %batch_id, error = %e, "host_filesrv_approve_transfer failed");
            -1
        }
    }
}

/// 拒绝传输批（接收端用户应答「拒绝全部」）
///
/// 参数：(batch_ptr, batch_len)
/// 返回：0 成功，-1 失败（权限/批不存在/批非 pending）
pub(crate) fn host_filesrv_reject_transfer(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    batch_ptr: u32,
    batch_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_reject_transfer: permission denied (fileservice)");
        return -1;
    }

    let batch_id = match read_wasm_string(&mut caller, batch_ptr, batch_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_reject_transfer: failed to read batch id");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_reject_transfer",
        Err(crate::AppError::Internal("host_filesrv_reject_transfer panicked".to_string())),
        || tokio::task::block_in_place(|| {
            handle
                .block_on(fs.registry.reject_transfer(&plugin_id, &batch_id))
                .map_err(crate::file_service::registry::BatchError::into_app_error)
        }),
    );

    match result {
        Ok(()) => {
            tracing::info!(plugin_id = %plugin_id, batch_id = %batch_id, "transfer batch rejected");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, batch_id = %batch_id, error = %e, "host_filesrv_reject_transfer failed");
            -1
        }
    }
}

/// 设置批准超时（秒，10–600；仅 ask 策略生效，宿主 TTL 扫描用）
///
/// 参数：(mount_ptr, mount_len, seconds: i64)
/// 返回：0 成功，-1 失败（权限/超时值越界）
pub(crate) fn host_filesrv_set_approval_timeout(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    mount_ptr: u32,
    mount_len: u32,
    seconds: i64,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_set_approval_timeout: permission denied (fileservice)");
        return -1;
    }

    let mount_path = match read_wasm_string(&mut caller, mount_ptr, mount_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_set_approval_timeout: failed to read mount path");
            return -1;
        }
    };
    if seconds < 0 {
        tracing::warn!(plugin_id = %plugin_id, seconds, "host_filesrv_set_approval_timeout: negative seconds");
        return -1;
    }

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_set_approval_timeout",
        Err(crate::AppError::Internal("host_filesrv_set_approval_timeout panicked".to_string())),
        || tokio::task::block_in_place(|| {
            handle
                .block_on(fs.registry.set_approval_timeout(
                    &plugin_id,
                    &mount_path,
                    seconds as u64,
                ))
                .map_err(crate::file_service::registry::BatchError::into_app_error)
        }),
    );

    match result {
        Ok(()) => {
            tracing::info!(plugin_id = %plugin_id, mount = %mount_path, seconds, "approval timeout set");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, mount = %mount_path, error = %e, "host_filesrv_set_approval_timeout failed");
            -1
        }
    }
}

/// 取消接收中的上传会话（接收端本地取消，session 级）
///
/// 参数：(sid_ptr, sid_len)
/// 返回：0 成功，-1 失败（权限/session 不存在）
pub(crate) fn host_filesrv_cancel_receiving(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    sid_ptr: u32,
    sid_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FILESERVICE) {
        tracing::warn!(plugin_id = %plugin_id, "host_filesrv_cancel_receiving: permission denied (fileservice)");
        return -1;
    }

    let session_id = match read_wasm_string(&mut caller, sid_ptr, sid_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_filesrv_cancel_receiving: failed to read session id");
            return -1;
        }
    };

    let fs = crate::state::get_file_service();
    let handle = caller.data().runtime_handle.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_filesrv_cancel_receiving",
        Err(crate::AppError::Internal("host_filesrv_cancel_receiving panicked".to_string())),
        || tokio::task::block_in_place(|| {
            handle
                .block_on(fs.registry.cancel_receiving_session(&plugin_id, &session_id))
                .map_err(crate::file_service::registry::BatchError::into_app_error)
        }),
    );

    match result {
        Ok(()) => {
            tracing::info!(plugin_id = %plugin_id, session_id = %session_id, "receiving session cancelled");
            0
        }
        Err(e) => {
            tracing::warn!(plugin_id = %plugin_id, session_id = %session_id, error = %e, "host_filesrv_cancel_receiving failed");
            -1
        }
    }
}

// ==================== Config Host Function（ABI v5） ====================
