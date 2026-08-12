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
/// - Windows：`explorer /select,<path>`（选中目标文件）
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

    let path = std::path::PathBuf::from(&path);
    if !path.exists() {
        return Err(crate::AppError::NotFound(format!(
            "reveal: path not found: {}",
            path.display()
        )));
    }

    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
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
