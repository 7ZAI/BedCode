//! 文件系统域 Host Functions（三层访问校验：权限 → 白名单 → 弹窗授权）
//!
//! 读写/复制均支持 WSL UNC 路径（`\\wsl.localhost\` / `\\wsl$\`）：
//! 发行版 Stopped 时 UNC 路径不可达，自动改用 wsl.exe 桥接访问。

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use super::wsl_fs;
use crate::plugin::fs_auth::FsOp;
use crate::plugin::wasm_runtime::{block_on_async, WasmPluginState};
use crate::plugin::permission::{PERMISSION_FS_READ, PERMISSION_FS_WRITE};

/// 读取文本文件（WSL UNC 路径走 wsl.exe 桥接）
fn read_text_file(path: &str) -> std::io::Result<String> {
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::read_to_string_via_wsl(&distro, &wsl_path);
    }
    std::fs::read_to_string(path)
}

/// 写入文本文件（WSL UNC 路径走 wsl.exe 桥接，自动创建父目录）
fn write_text_file(path: &str, content: &str) -> std::io::Result<()> {
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::write_bytes_via_wsl(&distro, &wsl_path, content.as_bytes());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}

/// 读取文件原始字节（WSL UNC 路径走 wsl.exe 桥接）
fn read_file_bytes(path: &str) -> std::io::Result<Vec<u8>> {
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::read_bytes_via_wsl(&distro, &wsl_path);
    }
    std::fs::read(path)
}

/// 写入文件原始字节（WSL UNC 路径走 wsl.exe 桥接，自动创建父目录）
fn write_file_bytes(path: &str, content: &[u8]) -> std::io::Result<()> {
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::write_bytes_via_wsl(&distro, &wsl_path, content);
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}

/// 复制文件（源或目标为 WSL UNC 路径时拆为读源 + 写目标，支持跨域复制）
fn copy_file(src: &str, dst: &str) -> std::io::Result<()> {
    if wsl_fs::is_wsl_unc_path(src) || wsl_fs::is_wsl_unc_path(dst) {
        let data = read_file_bytes(src)?;
        return write_file_bytes(dst, &data);
    }
    if let Some(parent) = std::path::Path::new(dst).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::copy(src, dst).map(|_| ())
}

/// 删除文件（WSL UNC 路径走 wsl.exe 桥接；文件不存在视为成功，幂等）
fn delete_file(path: &str) -> std::io::Result<()> {
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::delete_via_wsl(&distro, &wsl_path);
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

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
    match read_text_file(&path) {
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

    match write_text_file(&path, &data) {
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

    match copy_file(&src, &dst) {
        Ok(_) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, src = %src, dst = %dst, "host_fs_copy: file copy failed");
            -1
        }
    }
}

/// 文件系统：删除文件
///
/// 参数：(path_ptr, path_len)
/// 返回：0 成功（文件不存在也视为成功），-1 失败
pub(super) fn host_fs_delete(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string_consume(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_delete: failed to read path");
            return -1;
        }
    };

    // 权限校验（删除属于写操作）
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_FS_WRITE, "host_fs_delete") {
        return -1;
    }

    // 访问校验
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(&plugin_id, &path, FsOp::Write));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "host_fs_delete: access denied by fs_auth");
        return -1;
    }

    match delete_file(&path) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: file delete failed");
            -1
        }
    }
}
