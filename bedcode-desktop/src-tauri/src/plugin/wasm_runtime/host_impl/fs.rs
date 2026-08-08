//! 文件系统域宿主实现（三层访问校验：权限 → 白名单 → 弹窗授权）
//!
//! 读写/复制均支持 WSL UNC 路径（`\\wsl.localhost\` / `\\wsl$\`）：
//! 发行版 Stopped 时 UNC 路径不可达，自动改用 wsl.exe 桥接访问。

use super::wsl_fs;
use crate::plugin::fs_auth::FsOp;
use crate::plugin::permission::{PERMISSION_FS_READ, PERMISSION_FS_WRITE};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

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

/// 读取文本文件（权限 + 三层访问校验）
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

/// 写入文本文件（权限 + 三层访问校验）
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

/// 复制文件（读源 + 写目标双授权）
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

/// 删除文件（权限 + 三层访问校验；文件不存在视为成功，幂等）
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

/// 检查文件是否存在（权限 + 三层访问校验，支持 WSL UNC 路径）
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
