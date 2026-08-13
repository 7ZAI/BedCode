//! Opener Commands
//!
//! 插件系统文件操作桥接：在系统文件管理器中显示本地文件/目录
//! （传输完成后「打开本地目录」）。命令双重校验（与 fileservice 命令
//! 同模式）：插件处于 Activated 状态 + manifest 声明 system:open 权限。

use crate::plugin::host::PluginHost;
use bedcode_plugin_api::permission::PERMISSION_SYSTEM_OPEN;
use std::sync::Arc;
use tauri::State;

/// 校验插件身份与 system:open 权限
async fn require_system_open(
    plugin_host: &PluginHost,
    plugin_id: &str,
    op: &str,
) -> crate::Result<()> {
    if !plugin_host.is_activated(plugin_id).await {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' is not activated",
            op, plugin_id
        )));
    }
    if !plugin_host
        .permission()
        .check(plugin_id, PERMISSION_SYSTEM_OPEN)
    {
        return Err(crate::AppError::Plugin(format!(
            "{}: plugin '{}' has no system:open permission",
            op, plugin_id
        )));
    }
    Ok(())
}

/// 在系统文件管理器中显示文件/目录
///
/// - Windows：`explorer /select,"<path>"`（选中目标文件）。路径先
///   canonicalize 取原生反斜杠绝对路径并剥 `\\?\` 前缀，避免混合
///   分隔符（HomeDir 带反斜杠 / format! 拼接的 `/`）致 explorer 定位
///   失败；`/select,<path>` 整段加引号容纳路径中的空格/特殊字符。
/// - macOS：`open -R <path>`（Reveal in Finder）
/// - Linux：`xdg-open` 打开所在目录（无 reveal 语义，退化为打开目录）
///
/// explorer 返回码无意义（立即返回），异步 spawn 不等待。
#[tauri::command]
pub async fn plugin_reveal_in_dir(
    plugin_id: String,
    path: String,
    plugin_host: State<'_, Arc<PluginHost>>,
) -> crate::Result<()> {
    require_system_open(&plugin_host, &plugin_id, "plugin_reveal_in_dir").await?;

    // 兼容历史 wasm 产物：旧版插件曾用 POSIX 语义 PathBuf 拼出 `\\?\` verbatim
    // 前缀 + 混合分隔符路径（如 `\\?\D:\下载/file.mkv`），Windows 下
    // exists/canonicalize 直接报 os error 123。先剥 verbatim 前缀（纯正斜杠/
    // 混合分隔符均为宿主 API 接受，canonicalize 会还原原生形态）。
    let path = std::path::PathBuf::from(path.strip_prefix(r"\\?\").unwrap_or(&path));
    if !path.exists() {
        return Err(crate::AppError::NotFound(format!(
            "reveal: path not found: {}",
            path.display()
        )));
    }

    let result = if cfg!(target_os = "windows") {
        // 规范路径给 explorer：同时解两个 Windows /select 剔病
        //   1. 分隔符：HomeDir 返回反斜杠，而 enqueue_download 用 format! 拼成
        //      `C:\Users\x/Downloads/file.mkv` 这类混合分隔符路径；explorer 的
        //      /select 参数不识别反斜杠外的分隔符，会定位失败、打开错位置。
        //   2. 空格/特殊字符：不加引号时 explorer 以空格拆 token，定位到首
        //      个空格前的截断路径。
        // canonicalize 解析 symlink 给出原生反斜杠绝对路径，但会给本地路径加
        //      `\\?\` verbatim 前缀 explorer 不识别，需剖除。网络路径不走日常
        //      Downloads 场景，剩 `\server\...` 形态时直接交给 explorer。
        let clean = path.canonicalize().map_err(|e| {
            crate::AppError::Internal(format!(
                "reveal: canonicalize '{}' failed: {}",
                path.display(),
                e
            ))
        })?;
        let display = clean.display().to_string();
        let display = display
            .strip_prefix(r"\\?\")
            .map(|s| s.to_string())
            .unwrap_or(display);
        std::process::Command::new("explorer")
            .arg(format!("/select,\"{}\"", display))
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg("-R").arg(&path).spawn()
    } else {
        // Linux：无 reveal 语义，退化为打开所在目录
        let dir = path.parent().unwrap_or(&path);
        std::process::Command::new("xdg-open").arg(dir).spawn()
    };

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(crate::AppError::Internal(format!(
            "reveal: failed to launch file manager for '{}': {}",
            path.display(),
            e
        ))),
    }
}
