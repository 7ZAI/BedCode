//! host_fs_* — 文件系统（含 SAF 授权）（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：读取文件（SDK HostFs 契约：不存在返回 Ok(None)）
pub(crate) fn fs_read(state: &WasmPluginState, path: &str) -> Result<Option<String>, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
    {
        return Err("permission denied: fs:read".to_string());
    }

    // 访问校验（路径白名单弹窗/持久授权）
    let fs_auth = state.host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_read", false, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(fs_auth.check(&state.plugin_id, path, crate::plugin::fs_auth::FsOp::Read))
        })
    });
    if !allowed {
        return Err(format!("access denied by fs_auth: {}", path));
    }

    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("file read failed: {}", e)),
    }
}

/// 逻辑层：写入文件（自动创建父目录）
pub(crate) fn fs_write(state: &WasmPluginState, path: &str, data: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        return Err("permission denied: fs:write".to_string());
    }

    let fs_auth = state.host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_write", false, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(fs_auth.check(&state.plugin_id, path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        return Err(format!("access denied by fs_auth: {}", path));
    }

    // 自动创建父目录
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent directory: {}", e))?;
        }
    }

    std::fs::write(path, data).map_err(|e| format!("file write failed: {}", e))
}

/// 逻辑层：复制文件（需要读+写权限，自动创建目标父目录）
pub(crate) fn fs_copy(state: &WasmPluginState, src: &str, dst: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
        || !state
            .granted_permissions
            .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        return Err("permission denied: fs:read+fs:write".to_string());
    }

    // 复制需要读+写权限
    let fs_auth = state.host_ctx.fs_auth.clone();
    let plugin_id = state.plugin_id.clone();
    let src_clone = src.to_string();
    let dst_clone = dst.to_string();
    let rt = state.runtime_handle.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_copy", false, || {
        tokio::task::block_in_place(|| {
            rt.block_on(async move {
                let read_ok = fs_auth
                    .check(&plugin_id, &src_clone, crate::plugin::fs_auth::FsOp::Read)
                    .await;
                if !read_ok {
                    return false;
                }
                fs_auth
                    .check(&plugin_id, &dst_clone, crate::plugin::fs_auth::FsOp::Write)
                    .await
            })
        })
    });
    if !allowed {
        return Err(format!("access denied by fs_auth: {} -> {}", src, dst));
    }

    // 自动创建目标父目录
    if let Some(parent) = std::path::Path::new(dst).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent directory: {}", e))?;
        }
    }

    std::fs::copy(src, dst).map(|_| ()).map_err(|e| format!("file copy failed: {}", e))
}

/// 逻辑层：检查文件是否存在
pub(crate) fn fs_exists(state: &WasmPluginState, path: &str) -> Result<bool, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
    {
        return Err("permission denied: fs:read".to_string());
    }

    // 访问校验
    let fs_auth = state.host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_exists", false, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(fs_auth.check(&state.plugin_id, path, crate::plugin::fs_auth::FsOp::Read))
        })
    });
    if !allowed {
        return Err(format!("access denied by fs_auth: {}", path));
    }

    let exists = std::path::Path::new(path).exists();
    tracing::debug!(plugin_id = %state.plugin_id, path = %path, exists = %exists, "host_fs_exists");
    Ok(exists)
}

/// 逻辑层：批量请求目录授权（paths 为 JSON 字符串数组；
/// 全部同意 true，拒绝/超时 false——与 core ABI 0/1 语义一一对应）
pub(crate) fn fs_request_auth(state: &WasmPluginState, paths_json: &str) -> Result<bool, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
    {
        return Err("permission denied: fs:read".to_string());
    }

    let paths: Vec<String> = serde_json::from_str(paths_json)
        .map_err(|e| format!("invalid paths json: {}", e))?;

    if paths.is_empty() {
        return Ok(true);
    }

    // 访问校验（批量弹窗）；用户拒绝/超时 = Ok(false)（core ABI 返回 0），
    // 与”失败“（Err → -1）严格区分（语义不变量）
    let fs_auth = state.host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_request_auth", false, || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                fs_auth.check_batch(&state.plugin_id, &paths, crate::plugin::fs_auth::FsOp::Read),
            )
        })
    });
    Ok(allowed)
}

/// 逻辑层：删除文件（不存在视为成功；Android 经 Kotlin FileDeletePlugin）
pub(crate) fn fs_delete(state: &WasmPluginState, path: &str) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        return Err("permission denied: fs:write".to_string());
    }

    let fs_auth = state.host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_delete", false, || {
        tokio::task::block_in_place(|| {
            state
                .runtime_handle
                .block_on(fs_auth.check(&state.plugin_id, path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        return Err(format!("access denied by fs_auth: {}", path));
    }

    // 幂等：不存在视为成功（与桌面端 host_fs_delete 语义一致）
    if !std::path::Path::new(path).exists() {
        return Ok(());
    }

    #[cfg(target_os = "android")]
    {
        let path_clone = path.to_string();
        guarded_host_call(
            &state.plugin_id,
            "host_fs_delete(android)",
            Err(crate::AppError::Internal("host_fs_delete(android) panicked".to_string())),
            || {
                tokio::task::block_in_place(|| {
                    state.runtime_handle.block_on(
                        crate::plugin::android_plugins::delete_file(&path_clone),
                    )
                })
            },
        )
        .map_err(|e| format!("android delete failed: {}", e))
    }

    #[cfg(not(target_os = "android"))]
    {
        std::fs::remove_file(path).map_err(|e| format!("file delete failed: {}", e))
    }
}

/// 逻辑层：写入 MediaStore 公共下载目录（M2，接收方向统一落点）
///
/// 落点写公共存储不经 fs_auth 路径白名单（MediaStore 零权限写入，非路径 IO）；
/// 入参 src 校验：必须是宿主解析的 app 下载目录内文件。失败由调用方回退私有目录。
pub(crate) fn fs_write_media_downloads(
    state: &WasmPluginState,
    src_path: &str,
    display_name: &str,
    mime_type: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        return Err("permission denied: fs:write".to_string());
    }
    let host_ctx = &state.host_ctx;
    let Some(app) = &host_ctx.app_handle else {
        return Err("app_handle unavailable, rejected".to_string());
    };
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_write_media_downloads", false, || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                crate::plugin::android_plugins::is_within_app_downloads_dir(app, src_path),
            )
        })
    });
    if !allowed {
        return Err(format!("src outside app downloads dir, rejected: {}", src_path));
    }

    let saf_io = {
        use tauri::Manager;
        app.state::<crate::plugin::saf_io::SafIoState>().inner().0.clone()
    };
    let result = guarded_host_call(
        &state.plugin_id,
        "host_fs_write_media_downloads(saf)",
        Err(crate::AppError::Internal(
            "host_fs_write_media_downloads panicked".to_string(),
        )),
        || saf_io.write_media_downloads(src_path, display_name, mime_type),
    );
    match result {
        Ok(()) => {
            tracing::info!(
                plugin_id = %state.plugin_id,
                src = %src_path,
                display_name = %display_name,
                "host_fs_write_media_downloads: ok"
            );
            Ok(())
        }
        // 失败不视为异常（回退私有目录是正常分支），warn 级记录原因供排查
        Err(e) => {
            tracing::warn!(
                error = %e,
                plugin_id = %state.plugin_id,
                src = %src_path,
                "host_fs_write_media_downloads failed, caller falls back to private dir"
            );
            Err(format!("write media downloads failed: {}", e))
        }
    }
}

/// 逻辑层：「保存到…」（M3）弹系统保存对话框并流拷贝到用户选择的位置
///
/// 失败/用户取消：调用方保留私有副本（回退语义）
pub(crate) fn fs_save_to_document(
    state: &WasmPluginState,
    src_path: &str,
    suggested_name: &str,
    mime_type: &str,
) -> Result<(), String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        return Err("permission denied: fs:write".to_string());
    }
    let host_ctx = &state.host_ctx;
    let Some(app) = &host_ctx.app_handle else {
        return Err("app_handle unavailable, rejected".to_string());
    };
    let allowed = guarded_host_call(&state.plugin_id, "host_fs_save_to_document", false, || {
        tokio::task::block_in_place(|| {
            state.runtime_handle.block_on(
                crate::plugin::android_plugins::is_within_app_downloads_dir(app, src_path),
            )
        })
    });
    if !allowed {
        return Err(format!("src outside app downloads dir, rejected: {}", src_path));
    }

    let saf_io = {
        use tauri::Manager;
        app.state::<crate::plugin::saf_io::SafIoState>().inner().0.clone()
    };
    let result = guarded_host_call(
        &state.plugin_id,
        "host_fs_save_to_document(saf)",
        Err(crate::AppError::Internal(
            "host_fs_save_to_document panicked".to_string(),
        )),
        || saf_io.save_to_document(src_path, suggested_name, mime_type),
    );
    match result {
        Ok(()) => {
            tracing::info!(
                plugin_id = %state.plugin_id,
                src = %src_path,
                suggested_name = %suggested_name,
                "host_fs_save_to_document: ok"
            );
            Ok(())
        }
        // 失败/用户取消：保留私有副本（回退语义），warn 级记录原因
        Err(e) => {
            tracing::warn!(
                error = %e,
                plugin_id = %state.plugin_id,
                src = %src_path,
                "host_fs_save_to_document failed/cancelled, private copy kept"
            );
            Err(format!("save to document failed/cancelled: {}", e))
        }
    }
}

