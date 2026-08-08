//! 文件系统域 Host Functions（三层访问校验：权限 → 白名单 → 弹窗授权）
//!
//! 读写/复制均支持 WSL UNC 路径（`\\wsl.localhost\` / `\\wsl$\`）：
//! 发行版 Stopped 时 UNC 路径不可达，自动改用 wsl.exe 桥接访问。

use super::memory::{read_wasm_string_consume, write_result_to_out_ptr, write_wasm_string};
use super::wsl_fs;
use crate::plugin::fs_auth::FsOp;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext, WasmPluginState};
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

// ==================== 逻辑层（core 胶水与 Component Model 绑定共用） ====================

/// 逻辑层：读取文本文件（权限 + 三层访问校验）
pub(crate) fn fs_read(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    path: &str,
) -> Result<Option<String>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_READ, "host_fs_read") {
        return Err("permission denied".to_string());
    }
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(plugin_id, path, FsOp::Read));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_read: access denied by fs_auth");
        return Err("permission denied".to_string());
    }
    read_text_file(path)
        .map(Some)
        .map_err(|e| format!("fs error: file read failed: {}", e))
}

/// 逻辑层：写入文本文件（权限 + 三层访问校验）
pub(crate) fn fs_write(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    path: &str,
    data: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_WRITE, "host_fs_write") {
        return Err("permission denied".to_string());
    }
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(plugin_id, path, FsOp::Write));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_write: access denied by fs_auth");
        return Err("permission denied".to_string());
    }
    write_text_file(path, data).map_err(|e| format!("fs error: file write failed: {}", e))
}

/// 逻辑层：复制文件（读源 + 写目标双授权）
pub(crate) fn fs_copy(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    src: &str,
    dst: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_READ, "host_fs_copy") {
        return Err("permission denied".to_string());
    }
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_WRITE, "host_fs_copy") {
        return Err("permission denied".to_string());
    }
    // 访问校验（源文件读、目标文件写）
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(async {
        let read_ok = fs_auth.check(plugin_id, src, FsOp::Read).await;
        if !read_ok {
            return false;
        }
        fs_auth.check(plugin_id, dst, FsOp::Write).await
    });
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, src = %src, dst = %dst, "fs_copy: access denied by fs_auth");
        return Err("permission denied".to_string());
    }
    copy_file(src, dst).map_err(|e| format!("fs error: file copy failed: {}", e))
}

/// 逻辑层：删除文件（权限 + 三层访问校验；文件不存在视为成功，幂等）
pub(crate) fn fs_delete(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    path: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_WRITE, "host_fs_delete") {
        return Err("permission denied".to_string());
    }
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(plugin_id, path, FsOp::Write));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_delete: access denied by fs_auth");
        return Err("permission denied".to_string());
    }
    delete_file(path).map_err(|e| format!("fs error: file delete failed: {}", e))
}

/// 逻辑层：检查文件是否存在（权限 + 三层访问校验，支持 WSL UNC 路径）
pub(crate) fn fs_exists(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    path: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_READ, "host_fs_exists") {
        return Err("permission denied".to_string());
    }
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check(plugin_id, path, FsOp::Read));
    if !allowed {
        tracing::warn!(plugin_id = %plugin_id, path = %path, "fs_exists: access denied by fs_auth");
        return Err("permission denied".to_string());
    }
    // 支持 WSL UNC 路径
    if let Some((distro, wsl_path)) = wsl_fs::parse_wsl_unc_path(path) {
        return wsl_fs::exists_via_wsl(&distro, &wsl_path)
            .map_err(|e| format!("fs error: WSL check failed: {}", e));
    }
    Ok(std::path::Path::new(path).exists())
}

// ==================== Host Functions（core module 胶水） ====================

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

    match fs_read(&host_ctx, &plugin_id, &path) {
        Ok(Some(content)) => match write_wasm_string(&mut caller, &content) {
            Some((ptr, len)) => write_result_to_out_ptr(&mut caller, out_ptr, ptr, len),
            None => {
                tracing::error!(plugin_id = %plugin_id, path = %path, "host_fs_read: failed to write result to WASM memory");
                -1
            }
        },
        Ok(None) => write_result_to_out_ptr(&mut caller, out_ptr, 0, 0),
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

    match fs_write(&host_ctx, &plugin_id, &path, &data) {
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

    match fs_copy(&host_ctx, &plugin_id, &src, &dst) {
        Ok(()) => 0,
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

    match fs_delete(&host_ctx, &plugin_id, &path) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_delete: file delete failed");
            -1
        }
    }
}

/// 文件系统：检查文件是否存在
///
/// 参数：(path_ptr, path_len)
/// 返回：1 存在，0 不存在，-1 错误（权限拒绝或内存读取失败）
pub(super) fn host_fs_exists(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    path_ptr: u32,
    path_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let path = match read_wasm_string_consume(&mut caller, path_ptr, path_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_fs_exists: failed to read path");
            return -1;
        }
    };

    match fs_exists(&host_ctx, &plugin_id, &path) {
        Ok(exists) => {
            if exists {
                1
            } else {
                0
            }
        }
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, path = %path, "host_fs_exists: exists check failed");
            -1
        }
    }
}
