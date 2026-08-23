//! host-peer 逻辑层 —— 对等网络基础能力（issue 12 切换后新增）
//!
//! 与桌面端同构：权限校验后直接复用 `peer_net` / `peer_transfer` /
//! `peer_receive` / `peer_remote` 的既有异步实现（与 Tauri 命令同一真源），
//! DTO 以 JSON 字符串过界。无头上下文（app_handle = None）一律报错。

use crate::plugin::wasm_runtime::WasmPluginState;

/// 权限守卫：peer 能力统一门禁
fn require_peer_permission(state: &WasmPluginState) -> Result<(), String> {
    if state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_PEER)
    {
        Ok(())
    } else {
        Err("permission denied: peer".to_string())
    }
}

/// 取 AppHandle（无头上下文直接报错）
fn require_app(state: &WasmPluginState) -> Result<tauri::AppHandle, String> {
    state
        .host_ctx
        .app_handle
        .as_ref()
        .map(|a| (**a).clone())
        .ok_or_else(|| "peer-net unavailable in headless context (no app_handle)".to_string())
}

/// 宿主命令返回 crate::Result<T>（AppError）——WASM 边界统一转可读字符串，
/// 并经插件专属 runtime handle 阻塞驱动（host fn 同步语义）
fn run<T>(state: &WasmPluginState, fut: impl std::future::Future<Output = crate::Result<T>>) -> Result<T, String> {
    state
        .runtime_handle
        .block_on(fut)
        .map_err(|e| e.to_string())
}

pub(crate) fn peer_list_devices(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, crate::peer_net::list_discovered_peers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize devices failed: {e}"))
}

pub(crate) fn peer_dial(state: &WasmPluginState, node_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, crate::peer_net::dial_peer(app, node_id.to_string()))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize dial result failed: {e}"))
}

pub(crate) fn peer_disconnect(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_net::disconnect_peer(app, node_id.to_string()))
}

pub(crate) fn peer_respond_consent(state: &WasmPluginState, request_id: &str, accepted: bool) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_net::respond_peer_consent(app, request_id.to_string(), accepted))
}

pub(crate) fn peer_list_trusted(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, crate::peer_net::list_trusted_peers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize trusted peers failed: {e}"))
}

pub(crate) fn peer_revoke_trusted(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_net::revoke_trusted_peer(app, node_id.to_string()))
}

pub(crate) fn peer_send_files(state: &WasmPluginState, node_id: &str, paths_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let paths: Vec<String> =
        serde_json::from_str(paths_json).map_err(|e| format!("send files: invalid paths json: {e}"))?;
    let app = require_app(state)?;
    let dto = run(state, crate::peer_transfer::send_files_to_peer(app, node_id.to_string(), paths))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_list_transfers(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, crate::peer_transfer::list_peer_transfers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize transfers failed: {e}"))
}

pub(crate) fn peer_cancel_transfer(state: &WasmPluginState, batch_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_transfer::cancel_peer_transfer(app, batch_id.to_string()))
}

pub(crate) fn peer_retry_transfer(state: &WasmPluginState, batch_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, crate::peer_transfer::retry_peer_transfer(app, batch_id.to_string()))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_clear_transfer_history(state: &WasmPluginState) -> Result<u32, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let n = run(state, crate::peer_transfer::clear_peer_transfer_history(app))?;
    Ok(n as u32)
}

pub(crate) fn peer_list_receiving(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, crate::peer_receive::list_peer_receiving(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize receiving failed: {e}"))
}

pub(crate) fn peer_respond_transfer(state: &WasmPluginState, batch_id: &str, accept: bool) -> Result<(), String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let _hit = run(state, crate::peer_receive::respond_peer_transfer(app, batch_id.to_string(), accept))?;
    Ok(())
}

pub(crate) fn peer_cancel_receiving(state: &WasmPluginState, batch_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_receive::cancel_peer_receiving(app, batch_id.to_string()))
}

pub(crate) fn peer_clear_receiving_history(state: &WasmPluginState) -> Result<u32, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let n = run(state, crate::peer_receive::clear_peer_receiving_history(app))?;
    Ok(n as u32)
}

pub(crate) fn peer_get_receive_settings(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, crate::peer_receive::get_peer_receive_settings(app))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize receive settings failed: {e}"))
}

pub(crate) fn peer_set_receive_policy(state: &WasmPluginState, mode: &str, timeout_secs: u64) -> Result<(), String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_receive::set_peer_receive_policy(app, mode.to_string(), timeout_secs))
}

pub(crate) fn peer_list_shared_directories(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, crate::peer_net::list_shared_directories(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize shared dirs failed: {e}"))
}

pub(crate) fn peer_remove_shared_directory(state: &WasmPluginState, id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, crate::peer_net::remove_shared_directory(app, id.to_string()))
}

pub(crate) fn peer_add_shared_directory(state: &WasmPluginState, _request_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    // 移动端忽略请求载荷：弹 SAF 目录树选择器，授权与条目落盘由宿主完成
    let app = require_app(state)?;
    let dto = run(state, crate::peer_net::add_shared_directory_saf(app))?
        .ok_or_else(|| "add shared directory: user cancelled".to_string())?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize shared dir failed: {e}"))
}

pub(crate) fn peer_list_shared_roots(state: &WasmPluginState, node_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let roots = run(state, crate::peer_remote::list_peer_shared_roots(app, node_id.to_string()))?;
    serde_json::to_string(&roots).map_err(|e| format!("serialize shared roots failed: {e}"))
}

pub(crate) fn peer_browse_directory(
    state: &WasmPluginState,
    node_id: &str,
    dir_id: &str,
    rel_path: &str,
) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(
        state,
        crate::peer_remote::browse_peer_directory(app, node_id.to_string(), dir_id.to_string(), rel_path.to_string()),
    )?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize browse listing failed: {e}"))
}

pub(crate) fn peer_pull_files(
    state: &WasmPluginState,
    node_id: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    require_peer_permission(state)?;
    let files: Vec<crate::peer_remote::RemotePullFileDto> =
        serde_json::from_str(files_json).map_err(|e| format!("pull files: invalid files json: {e}"))?;
    let app = require_app(state)?;
    let n = run(
        state,
        crate::peer_remote::pull_peer_files(app, node_id.to_string(), dir_id.to_string(), files),
    )?;
    Ok(n as u32)
}

pub(crate) fn peer_pick_files(state: &WasmPluginState) -> Result<String, String> {
    // 选源对话框本身即用户授权动作，不再叠加 peer 权限门
    let app = require_app(state)?;
    let paths = run(state, crate::peer_transfer::peer_pick_files(app))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

pub(crate) fn peer_pick_folder(state: &WasmPluginState) -> Result<String, String> {
    let app = require_app(state)?;
    let paths = run(state, crate::peer_transfer::peer_pick_folder(app))?;
    Ok(paths.into_iter().next().unwrap_or_default())
}
