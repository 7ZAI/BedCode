//! 文件系统域 Host Functions（三层访问校验：权限 → 白名单 → 弹窗授权）

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use crate::plugin::fs_auth::FsOp;
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::plugin::permission::{PERMISSION_FS_READ, PERMISSION_FS_WRITE};

/// 文件系统：读取文件
///
/// 参数：(path_ptr, path_len, out_ptr)
/// 返回：0 成功，-1 失败。结果写入 out_ptr（8 字节: ptr + len）
pub(super) fn host_fs_read(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string_consume(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_read: failed to read path");
            return -1;
        }
    };

    // 权限校验
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FS_READ, "host_fs_read") {
        return -1;
    }

    // 访问校验（三层策略）
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(&plugin_id, &path, FsOp::Read));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_read: access denied by fs_auth");
        return -1;
    }

    // 执行文件读取
    match std::fs::read_to_string(&path) {
        Ok(content) => match write_wasm_string(&mut caller, &content) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_read: failed to write result to WASM memory");
                -1
            }
        },
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_read: file read failed");
            -1
        }
    }
}

/// 文件系统：写入文件
///
/// 参数：(path_ptr, path_len, data_ptr, data_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_fs_write(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
    data_ptr: u32,
    data_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string_consume(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_write: failed to read path");
            return -1;
        }
    };

    let data = match read_wasm_string_consume(&mut caller, data_ptr, data_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_write: failed to read data");
            return -1;
        }
    };

    // 权限校验
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FS_WRITE, "host_fs_write") {
        return -1;
    }

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(&plugin_id, &path, FsOp::Write));
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
///
/// 参数：(src_ptr, src_len, dst_ptr, dst_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_fs_copy(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    src_ptr: u32,
    src_len: u32,
    dst_ptr: u32,
    dst_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let src = match read_wasm_string_consume(&mut caller, src_ptr, src_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read src path");
            return -1;
        }
    };

    let dst = match read_wasm_string_consume(&mut caller, dst_ptr, dst_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_copy: failed to read dst path");
            return -1;
        }
    };

    // 复制需要读+写权限
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FS_READ, "host_fs_copy") {
        return -1;
    }
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FS_WRITE, "host_fs_copy") {
        return -1;
    }

    // 访问校验（源文件读、目标文件写）
    let fs_auth = host_ctx.fs_auth.clone();
    let plugin_id_clone = plugin_id.clone();
    let src_clone = src.clone();
    let dst_clone = dst.clone();
    let allowed = block_on_async(async {
        let read_ok = fs_auth.check(&plugin_id_clone, &src_clone, FsOp::Read).await;
        if !read_ok {
            return false;
        }
        fs_auth.check(&plugin_id_clone, &dst_clone, FsOp::Write).await
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
