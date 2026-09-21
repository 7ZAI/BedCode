//! Opener Commands
//!
//! 宿主设置页的「打开日志目录」入口（外壳命令，不经插件权限链）。
//!
//! 平台定位实现本体在引擎模块 [`crate::system::opener`]——同一份实现也被插件
//! 原语 `host-platform.reveal-in-dir` 使用（ABI v22）。原
//! `plugin_reveal_in_dir` 命令 + `system:open` 权限 + 前端 `context.system`
//! 插件 API 桥已随该原语化一并退役（见
//! `.scratch/2026-09-21-host-rust-residue/issues/04`）。

use crate::Result;

/// 打开日志目录（设置页「打开日志目录」按钮；独立命令，无需插件权限）
///
/// 复用 `system::opener::reveal_in_dir` 的平台分发（Windows COM / macOS Finder /
/// Linux xdg-open）
#[tauri::command]
pub fn open_log_dir() -> Result<()> {
    let setup = crate::system::logging::global_setup()
        .ok_or_else(|| crate::AppError::Config("logging not initialized yet".to_string()))?;
    let dir = &setup.log_dir;
    if !dir.exists() {
        return Err(crate::AppError::NotFound(format!(
            "log directory not found: {}",
            dir.display()
        )));
    }
    crate::system::opener::reveal_in_dir(dir)
        .map_err(|e| crate::AppError::Internal(format!("open log directory '{}' failed: {e}", dir.display())))
}
