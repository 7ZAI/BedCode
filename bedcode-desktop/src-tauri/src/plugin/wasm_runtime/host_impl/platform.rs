//! host-platform 逻辑层 —— 通用平台能力域（ADR 0022 v2，issue 13 Phase 2）
//!
//! 系统对话框等与领域无关的平台交互。选源对话框本身即用户授权动作，
//! 不叠加权限门（与原 host-peer 内 pick 实现同口径）。底层复用 peer_transfer
//! 的既有实现（Phase 4 旧接口退役时实现本体迁来或改挂通用 dialog 层）。

use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 系统多文件选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_files(host_ctx: &WasmHostContext) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(crate::peer_transfer::peer_pick_files(app)))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

/// 系统文件夹选择器 → 绝对路径；用户取消返回空串
pub(crate) fn platform_pick_folder(host_ctx: &WasmHostContext) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(crate::peer_transfer::peer_pick_folder(app)))?;
    Ok(paths.into_iter().next().unwrap_or_default())
}

fn require_app(host_ctx: &WasmHostContext) -> Result<tauri::AppHandle, String> {
    host_ctx
        .app_handle
        .as_ref()
        .map(|a| (**a).clone())
        .ok_or_else(|| "platform unavailable in headless context (no app_handle)".to_string())
}

fn sync_result<T>(r: crate::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}
