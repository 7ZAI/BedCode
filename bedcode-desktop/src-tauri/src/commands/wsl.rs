//! WSL Commands
//!
//! 提供前端调用的 WSL 相关 Tauri 命令，内部使用 spawn_blocking
//! 避免同步进程调用阻塞 IPC 线程

use crate::Result;

/// 获取已安装的 WSL 发行版列表
///
/// 使用 spawn_blocking 将同步的进程调用移到专用线程，
/// 避免阻塞 Tauri 的 IPC 线程导致前端 UI 卡顿
#[tauri::command]
pub async fn list_wsl_distributions() -> Result<Vec<crate::pty::WslDistro>> {
    tokio::task::spawn_blocking(crate::pty::list_distributions)
        .await
        .map_err(|e| crate::system::error::AppError::Internal(e.to_string()))?
}

/// 检查 WSL 是否可用
///
/// 使用 spawn_blocking 避免阻塞 IPC 线程
#[tauri::command]
pub async fn is_wsl_available() -> bool {
    tokio::task::spawn_blocking(crate::pty::is_wsl_available)
        .await
        .unwrap_or(false)
}
