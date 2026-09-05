//! host-platform 逻辑层 —— 通用平台能力域（ADR 0022 v2，issue 13 Phase 2）
//!
//! 系统对话框等与领域无关的平台交互；选源对话框本身即用户授权动作，不叠加
//! 权限门。底层复用既有 peer_transfer 的 SAF/系统选择器实现。

use super::super::{block_on_async, WasmPluginState};
use super::support::guarded_host_call;

/// 系统多文件选择器 → string[] JSON（用户取消为空数组）
pub(crate) fn platform_pick_files(state: &WasmPluginState) -> Result<String, String> {
    let app = require_app(state)?;
    let paths = run(state, "host_platform_pick_files", crate::peer_transfer::peer_pick_files(app))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

/// 系统文件夹选择器（SAF 目录树选择器）→ 树 URI；用户取消返回空串
///
/// WIT 契约即「树 URI」：共享目录条目以 content:// 树 URI 存储（持久化授权
/// 凭据，重启仍有效）。真实路径解析（peer_transfer::peer_pick_folder）仅旧
/// 发送目录选路需要，SAF 化后移动端插件共享目录挂载必须走树 URI——引擎共享
/// 目录注册表校验 SAF 根为 content://，传真实路径会被整批拒绝（真机实证：
/// 共享目录多次选择始终只显示一条）。
pub(crate) fn platform_pick_folder(state: &WasmPluginState) -> Result<String, String> {
    let picked = run(
        state,
        "host_platform_pick_folder",
        crate::plugin::android_plugins::pick_shared_directory_android(),
    )?;
    Ok(picked.map(|(uri, _doc_id, _display_name)| uri).unwrap_or_default())
}

fn require_app(state: &WasmPluginState) -> Result<tauri::AppHandle, String> {
    state
        .host_ctx
        .app_handle
        .as_ref()
        .map(|a| (**a).clone())
        .ok_or_else(|| "platform unavailable in headless context (no app_handle)".to_string())
}

fn run<T, F>(state: &WasmPluginState, name: &'static str, fut: F) -> Result<T, String>
where
    T: Send,
    F: std::future::Future<Output = crate::Result<T>> + Send,
{
    let handle = state.runtime_handle.clone();
    guarded_host_call(&state.plugin_id, name, Err(format!("{name} panicked")), || {
        block_on_async(&handle, fut).map_err(|e| e.to_string())
    })
}
