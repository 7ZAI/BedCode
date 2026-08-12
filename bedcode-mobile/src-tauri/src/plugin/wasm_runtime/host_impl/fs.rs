//! host_fs_* — 文件系统（含 SAF 授权）

use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

/// 文件系统：读取文件
pub(crate) fn host_fs_read(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_read: permission denied (fs:read)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_read: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_read", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Read))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_read: access denied by fs_auth");
        return -1;
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match write_wasm_string(&mut caller, &content) {
            Some((ptr, len)) => {
                if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                    0
                } else {
                    -1
                }
            }
            None => {
                tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_read: failed to write result to WASM memory");
                -1
            }
        },
        // SDK HostFs 契约：文件不存在返回 Ok(None)（out=(0,0)，与空文件编码一致）
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if write_result_to_out_ptr(&mut caller, out_ptr, 0, 0) {
                0
            } else {
                -1
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_read: file read failed");
            -1
        }
    }
}


/// 文件系统：写入文件
pub(crate) fn host_fs_write(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_write: permission denied (fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write: failed to read path");
            return -1;
        }
    };

    let data = match read_wasm_string(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_write: failed to read data");
            return -1;
        }
    };

    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_write", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_write: access denied by fs_auth");
        return -1;
    }

    // 自动创建父目录
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_write: failed to create parent directory");
                return -1;
            }
        }
    }

    match std::fs::write(&path, &data) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_write: file write failed");
            -1
        }
    }
}


/// 文件系统：复制文件
pub(crate) fn host_fs_copy(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    dst_ptr: u32,
    dst_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ)
        || !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE)
    {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_copy: permission denied (fs:read+fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let src = match read_wasm_string(&mut caller, src_ptr, src_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read src path");
            return -1;
        }
    };

    let dst = match read_wasm_string(&mut caller, dst_ptr, dst_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read dst path");
            return -1;
        }
    };

    // 复制需要读+写权限
    let fs_auth = host_ctx.fs_auth.clone();
    let plugin_id_clone = plugin_id.clone();
    let src_clone = src.clone();
    let dst_clone = dst.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_copy", false, || {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let read_ok = fs_auth.check(&plugin_id_clone, &src_clone, crate::plugin::fs_auth::FsOp::Read).await;
                if !read_ok { return false; }
                fs_auth.check(&plugin_id_clone, &dst_clone, crate::plugin::fs_auth::FsOp::Write).await
            })
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, src = %src, dst = %dst, "host_fs_copy: access denied by fs_auth");
        return -1;
    }

    // 自动创建目标父目录
    if let Some(parent) = std::path::Path::new(&dst).parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!(error = %e, plugin_id = %plugin_id, dst = %dst, "host_fs_copy: failed to create parent directory");
                return -1;
            }
        }
    }

    match std::fs::copy(&src, &dst) {
        Ok(_) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, src = %src, dst = %dst, "host_fs_copy: file copy failed");
            -1
        }
    }
}


/// 文件系统：检查文件是否存在
///
/// 返回：1 存在，0 不存在，-1 错误
pub(crate) fn host_fs_exists(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_exists: permission denied (fs:read)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_exists: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_exists", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Read),
            )
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_exists: access denied by fs_auth");
        return -1;
    }

    let exists = std::path::Path::new(&path).exists();
    tracing::debug!(plugin_id = %plugin_id, path = %path, exists = %exists, "host_fs_exists");
    if exists { 1 } else { 0 }
}


/// 文件系统：批量请求目录授权
///
/// paths 参数为 JSON 字符串数组（未授权路径合并为一次弹窗询问）。
/// 返回：1 全部同意，0 拒绝/超时，-1 失败。
pub(crate) fn host_fs_request_auth(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    paths_ptr: u32,
    paths_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_READ) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_request_auth: permission denied (fs:read)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let paths_json = match read_wasm_string(&mut caller, paths_ptr, paths_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_request_auth: failed to read paths");
            return -1;
        }
    };

    let paths: Vec<String> = match serde_json::from_str(&paths_json) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_fs_request_auth: invalid paths json");
            return -1;
        }
    };

    if paths.is_empty() {
        return 1;
    }

    // 访问校验（批量弹窗）
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_request_auth", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check_batch(&plugin_id, &paths, crate::plugin::fs_auth::FsOp::Read))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, paths = ?paths, "host_fs_request_auth: denied by user");
        return 0;
    }
    1
}


/// 文件系统：删除文件
///
/// 返回：0 成功（文件不存在也视为成功），-1 失败。
/// Android 平台经 Kotlin FileDeletePlugin 删除（分区存储兼容）；
/// 非 Android 平台（桌面 dev 场景）直接 std::fs。
pub(crate) fn host_fs_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_delete: permission denied (fs:write)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_delete: failed to read path");
            return -1;
        }
    };

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_delete", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(fs_auth.check(&plugin_id, &path, crate::plugin::fs_auth::FsOp::Write))
        })
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_delete: access denied by fs_auth");
        return -1;
    }

    // 幂等：不存在视为成功（与桌面端 host_fs_delete 语义一致）
    if !std::path::Path::new(&path).exists() {
        return 0;
    }

    #[cfg(target_os = "android")]
    {
        let path_clone = path.clone();
        let result = guarded_host_call(
            &plugin_id,
            "host_fs_delete(android)",
            Err(crate::AppError::Internal("host_fs_delete(android) panicked".to_string())),
            || {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(crate::plugin::android_plugins::delete_file(&path_clone))
                })
            },
        );
        return match result {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: android delete failed");
                -1
            }
        };
    }

    #[cfg(not(target_os = "android"))]
    {
        match std::fs::remove_file(&path) {
            Ok(()) => 0,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: file delete failed");
                -1
            }
        }
    }
}


/// 文件系统：写入 MediaStore 公共下载目录（接收方向统一落点，M2）
///
/// 参数：(src_ptr, src_len, name_ptr, name_len, mime_ptr, mime_len)
/// 返回：0 成功（文件已入系统公共下载目录），-1 失败（调用方回退私有目录）。
/// 实现经 SafIo 主 seam（Kotlin SafTransferPlugin.writeMediaDownloads），
/// 与命令层 plugin_saf_write_media_downloads 共用同一后端。
pub(crate) fn host_fs_write_media_downloads(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    name_ptr: u32,
    name_len: u32,
    mime_ptr: u32,
    mime_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_write_media_downloads: permission denied (fs:write)");
        return -1;
    }

    let src_path = match read_wasm_string(&mut caller, src_ptr, src_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write_media_downloads: failed to read src path");
            return -1;
        }
    };
    let display_name = match read_wasm_string(&mut caller, name_ptr, name_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write_media_downloads: failed to read display name");
            return -1;
        }
    };
    let mime_type = match read_wasm_string(&mut caller, mime_ptr, mime_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write_media_downloads: failed to read mime type");
            return -1;
        }
    };

    // 落点写公共存储不经 fs_auth 路径白名单（MediaStore 零权限写入，非路径 IO）；
    // 入参 src 校验：必须是宿主解析的 app 下载目录内文件（防止插件任意路径
    // 数据被拷贝进公共下载），基址解析与浏览白名单共用同一函数
    let host_ctx = caller.data().host_ctx.clone();
    let src_path_clone = src_path.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_write_media_downloads", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                crate::plugin::android_plugins::is_within_app_downloads_dir(
                    &host_ctx.app_handle,
                    &src_path_clone,
                ),
            )
        })
    });
    if !allowed {
        tracing::warn!(
            plugin_id = %plugin_id,
            src = %src_path,
            "host_fs_write_media_downloads: src outside app downloads dir, rejected"
        );
        return -1;
    }

    let saf = {
        use tauri::Manager;
        host_ctx.app_handle.state::<crate::plugin::saf_io::SafIoState>()
    };
    let saf_io = saf.inner().0.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_fs_write_media_downloads(saf)",
        Err(crate::AppError::Internal(
            "host_fs_write_media_downloads panicked".to_string(),
        )),
        || saf_io.write_media_downloads(&src_path, &display_name, &mime_type),
    );
    match result {
        Ok(()) => {
            tracing::info!(
                plugin_id = %plugin_id,
                src = %src_path,
                display_name = %display_name,
                "host_fs_write_media_downloads: ok"
            );
            0
        }
        Err(e) => {
            // 失败不视为异常（回退私有目录是正常分支），warn 级记录原因供排查
            tracing::warn!(
                error = %e,
                plugin_id = %plugin_id,
                src = %src_path,
                "host_fs_write_media_downloads failed, caller falls back to private dir"
            );
            -1
        }
    }
}


/// 文件系统：「保存到…」（M3）弹系统保存对话框并流拷贝到用户选择的位置
///
/// 参数：(src_ptr, src_len, name_ptr, name_len, mime_ptr, mime_len)
/// 返回：0 成功（已写入用户选择的位置），-1 失败/用户取消（调用方保留副本）。
/// 实现经 SafIo 主 seam（Kotlin SafTransferPlugin.saveToDocument），src 白名单
/// 校验与 host_fs_write_media_downloads 一致（必须位于 app 下载目录内）。
pub(crate) fn host_fs_save_to_document(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    name_ptr: u32,
    name_len: u32,
    mime_ptr: u32,
    mime_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE) {
        tracing::warn!(plugin_id = %plugin_id, "host_fs_save_to_document: permission denied (fs:write)");
        return -1;
    }

    let src_path = match read_wasm_string(&mut caller, src_ptr, src_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_save_to_document: failed to read src path");
            return -1;
        }
    };
    let suggested_name = match read_wasm_string(&mut caller, name_ptr, name_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_save_to_document: failed to read suggested name");
            return -1;
        }
    };
    let mime_type = match read_wasm_string(&mut caller, mime_ptr, mime_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_save_to_document: failed to read mime type");
            return -1;
        }
    };

    // 入参 src 校验：必须是宿主解析的 app 下载目录内文件（防止插件任意路径
    // 数据被拷贝到用户选择的任意位置），基址解析与浏览白名单共用同一函数
    let host_ctx = caller.data().host_ctx.clone();
    let src_path_clone = src_path.clone();
    let allowed = guarded_host_call(&plugin_id, "host_fs_save_to_document", false, || {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                crate::plugin::android_plugins::is_within_app_downloads_dir(
                    &host_ctx.app_handle,
                    &src_path_clone,
                ),
            )
        })
    });
    if !allowed {
        tracing::warn!(
            plugin_id = %plugin_id,
            src = %src_path,
            "host_fs_save_to_document: src outside app downloads dir, rejected"
        );
        return -1;
    }

    let saf = {
        use tauri::Manager;
        host_ctx.app_handle.state::<crate::plugin::saf_io::SafIoState>()
    };
    let saf_io = saf.inner().0.clone();
    let result = guarded_host_call(
        &plugin_id,
        "host_fs_save_to_document(saf)",
        Err(crate::AppError::Internal(
            "host_fs_save_to_document panicked".to_string(),
        )),
        || saf_io.save_to_document(&src_path, &suggested_name, &mime_type),
    );
    match result {
        Ok(()) => {
            tracing::info!(
                plugin_id = %plugin_id,
                src = %src_path,
                suggested_name = %suggested_name,
                "host_fs_save_to_document: ok"
            );
            0
        }
        Err(e) => {
            // 失败/用户取消：保留私有副本（回退语义），warn 级记录原因
            tracing::warn!(
                error = %e,
                plugin_id = %plugin_id,
                src = %src_path,
                "host_fs_save_to_document failed/cancelled, private copy kept"
            );
            -1
        }
    }
}

// ==================== Message Bus Host Functions ====================
