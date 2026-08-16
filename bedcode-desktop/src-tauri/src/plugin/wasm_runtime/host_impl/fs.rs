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

/// 批量请求目录授权（权限 + fs_auth 批量弹窗校验）
///
/// paths-json 为 JSON 字符串数组；返回是否全部同意（拒绝/超时均为 false）
pub(crate) fn fs_request_auth(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    paths_json: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_FS_READ, "host_fs_request_auth") {
        return Err("permission denied".to_string());
    }
    let paths: Vec<String> = serde_json::from_str(paths_json)
        .map_err(|e| format!("fs error: invalid paths json: {}", e))?;
    if paths.is_empty() {
        return Ok(true);
    }
    let fs_auth = host_ctx.fs_auth.clone();
    let allowed = block_on_async(fs_auth.check_batch(plugin_id, &paths, FsOp::Read));
    if !allowed {
        tracing::warn!(
            plugin_id = %plugin_id,
            paths = ?paths,
            "fs_request_auth: denied by user"
        );
        return Ok(false);
    }
    Ok(true)
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
    read_text_file(path).map(Some).or_else(|e| {
        // SDK HostFs 契约：文件不存在返回 Ok(None)（store.rs 等插件依赖此语义处理新建文件）
        if e.kind() == std::io::ErrorKind::NotFound {
            Ok(None)
        } else {
            Err(format!("fs error: file read failed: {}", e))
        }
    })
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    const PLUGIN: &str = "test-plugin";

    /// 每个测试独立的临时目录 + .claude 白名单段根目录
    ///
    /// 无头 fs_auth 只放行白名单路径（弹窗通道不可用），`.claude` 目录段命中
    /// 白名单直接绕过校验；TempDir 随测试结束自动清理，测试间互不干扰
    fn claude_temp_root(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join(".claude").join(name);
        std::fs::create_dir_all(&root).expect("create root");
        (dir, root)
    }

    // ==================== 私有纯文件操作辅助 ====================

    /// write_text_file 自动创建不存在的父目录 + 读写往返
    #[test]
    fn write_text_file_creates_parent_dirs_roundtrip() {
        let (_dir, root) = claude_temp_root("roundtrip");
        let path = root.join("a/b/c/roundtrip.txt");
        write_text_file(path.to_str().unwrap(), "hello").expect("write ok");
        assert_eq!(read_text_file(path.to_str().unwrap()).expect("read ok"), "hello");
    }

    /// read_text_file 不存在的文件返回 NotFound（与 std 语义一致，供上层翻译为 None）
    #[test]
    fn read_text_file_missing_returns_not_found() {
        let (_dir, root) = claude_temp_root("missing");
        let path = root.join("missing.txt");
        let err = read_text_file(path.to_str().unwrap()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    /// delete_file 幂等：不存在的文件视为成功
    #[test]
    fn delete_file_missing_idempotent() {
        let (_dir, root) = claude_temp_root("delete");
        let path = root.join("never-exists.txt");
        delete_file(path.to_str().unwrap()).expect("delete missing ok");
        // 写入后删除，再次删除仍 Ok
        write_text_file(path.to_str().unwrap(), "x").unwrap();
        delete_file(path.to_str().unwrap()).expect("delete ok");
        delete_file(path.to_str().unwrap()).expect("delete again ok");
    }

    /// copy_file 目标父目录不存在时自动创建
    #[test]
    fn copy_file_creates_parent_dirs() {
        let (_dir, root) = claude_temp_root("copy");
        let src = root.join("src.txt");
        let dst = root.join("deep/nested/dst.txt");
        write_text_file(src.to_str().unwrap(), "payload").unwrap();
        copy_file(src.to_str().unwrap(), dst.to_str().unwrap()).expect("copy ok");
        assert_eq!(read_text_file(dst.to_str().unwrap()).unwrap(), "payload");
    }

    /// write_file_bytes / read_file_bytes 二进制往返
    #[test]
    fn write_read_file_bytes_roundtrip() {
        let (_dir, root) = claude_temp_root("bytes");
        let path = root.join("data.bin");
        let bytes: Vec<u8> = (0..=255u8).collect();
        write_file_bytes(path.to_str().unwrap(), &bytes).expect("write ok");
        assert_eq!(read_file_bytes(path.to_str().unwrap()).unwrap(), bytes);
    }

    // ==================== 权限门禁 ====================

    /// 无 fs:read 权限：读被拒绝
    #[test]
    fn fs_read_permission_denied() {
        let ctx = build_host_ctx();
        let err = fs_read(&ctx, PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fs:write 权限：写被拒绝
    #[test]
    fn fs_write_permission_denied() {
        let ctx = build_host_ctx();
        let err = fs_write(&ctx, PLUGIN, "/tmp/x", "data").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fs:write 权限：删被拒绝
    #[test]
    fn fs_delete_permission_denied() {
        let ctx = build_host_ctx();
        let err = fs_delete(&ctx, PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 无 fs:read 权限：存在性检查被拒绝
    #[test]
    fn fs_exists_permission_denied() {
        let ctx = build_host_ctx();
        let err = fs_exists(&ctx, PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// fs_copy 需要读+写双权限：只授 fs:read 时在写校验处被拒绝
    #[test]
    fn fs_copy_requires_both_permissions() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let err = fs_copy(&ctx, PLUGIN, "/tmp/a", "/tmp/b").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// fs_request_auth 空路径数组：无需弹窗直接放行（批量请求约定）
    #[test]
    fn fs_request_auth_empty_paths_ok() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        assert!(fs_request_auth(&ctx, PLUGIN, "[]").expect("empty paths ok"));
    }

    /// fs_request_auth 非法 JSON：解析失败
    #[test]
    fn fs_request_auth_invalid_json_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let err = fs_request_auth(&ctx, PLUGIN, "not-json").unwrap_err();
        assert!(err.contains("invalid paths json"), "got: {}", err);
    }

    /// 无 fs:read 权限：批量授权请求被拒绝
    #[test]
    fn fs_request_auth_permission_denied() {
        let ctx = build_host_ctx();
        let err = fs_request_auth(&ctx, PLUGIN, "[]").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    // ==================== 端到端（白名单路径 + 内存上下文） ====================

    /// 写→读往返（fs_write 自动建父目录；SDK 契约：文件不存在 fs_read 返回 Ok(None)）
    #[tokio::test]
    async fn fs_write_then_read_roundtrip() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ, PERMISSION_FS_WRITE]);
        let (_dir, root) = claude_temp_root("e2e-roundtrip");
        let path = root.join("roundtrip.txt");

        fs_write(&ctx, PLUGIN, path.to_str().unwrap(), "hello wasm").expect("write ok");
        let content = fs_read(&ctx, PLUGIN, path.to_str().unwrap())
            .expect("read ok")
            .expect("value");
        assert_eq!(content, "hello wasm");
    }

    /// 不存在的文件：fs_read 返回 Ok(None)（store.rs 等插件依赖此语义处理新建文件）
    #[tokio::test]
    async fn fs_read_missing_file_returns_none() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let (_dir, root) = claude_temp_root("e2e-missing");
        let path = root.join("missing.txt");
        assert!(fs_read(&ctx, PLUGIN, path.to_str().unwrap()).expect("read ok").is_none());
    }

    /// 存在性检查：写入后 true，删除后 false
    #[tokio::test]
    async fn fs_exists_tracks_file_lifecycle() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ, PERMISSION_FS_WRITE]);
        let (_dir, root) = claude_temp_root("e2e-exists");
        let path = root.join("exists.txt");
        assert!(!fs_exists(&ctx, PLUGIN, path.to_str().unwrap()).expect("missing false"));
        fs_write(&ctx, PLUGIN, path.to_str().unwrap(), "x").unwrap();
        assert!(fs_exists(&ctx, PLUGIN, path.to_str().unwrap()).expect("exists true"));
        fs_delete(&ctx, PLUGIN, path.to_str().unwrap()).expect("delete ok");
        assert!(!fs_exists(&ctx, PLUGIN, path.to_str().unwrap()).expect("deleted false"));
    }

    /// fs_delete 幂等：删除不存在的文件同样 Ok
    #[tokio::test]
    async fn fs_delete_missing_idempotent() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ, PERMISSION_FS_WRITE]);
        let (_dir, root) = claude_temp_root("e2e-delete");
        let path = root.join("delete-missing.txt");
        fs_delete(&ctx, PLUGIN, path.to_str().unwrap()).expect("delete missing ok");
    }

    /// fs_copy 端到端：源读授权 + 目标写授权 + 自动创建父目录
    #[tokio::test]
    async fn fs_copy_end_to_end() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ, PERMISSION_FS_WRITE]);
        let (_dir, root) = claude_temp_root("e2e-copy");
        let src = root.join("copy-src.txt");
        let dst = root.join("nested/copy-dst.txt");
        fs_write(&ctx, PLUGIN, src.to_str().unwrap(), "payload").unwrap();
        fs_copy(&ctx, PLUGIN, src.to_str().unwrap(), dst.to_str().unwrap()).expect("copy ok");
        let content = fs_read(&ctx, PLUGIN, dst.to_str().unwrap())
            .expect("read ok")
            .expect("value");
        assert_eq!(content, "payload");
    }
}
