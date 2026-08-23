//! host-peer 逻辑层 —— 对等网络基础能力（issue 12 切换后新增）
//!
//! 宿主 peer-net 引擎命令面的 WASM 投影：每个函数做权限校验后直接
//! 复用 `peer_net` / `peer_transfer` / `peer_receive` / `peer_remote`
//! 的既有异步实现（与 Tauri 命令同一真源），DTO 以 JSON 字符串过界。
//! 无头上下文（app_handle = None）一律报错，不做静默降级——对等网络
//! 能力依赖节点运行时，降级会产生「看似成功实则空列表」的假象。

use crate::plugin::permission::PERMISSION_PEER;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};

/// 取 AppHandle（无头上下文直接报错）
fn require_app(host_ctx: &WasmHostContext) -> Result<tauri::AppHandle, String> {
    host_ctx
        .app_handle
        .as_ref()
        .map(|a| (**a).clone())
        .ok_or_else(|| "peer-net unavailable in headless context (no app_handle)".to_string())
}

fn denied() -> String {
    "permission denied: peer".to_string()
}

/// 宿主命令返回 crate::Result<T>（AppError）——WASM 边界统一转可读字符串
fn sync_result<T>(r: crate::Result<T>) -> Result<T, String> {
    r.map_err(|e| e.to_string())
}

pub(crate) fn peer_list_devices(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_devices") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dtos = sync_result(block_on_async(crate::peer_net::list_discovered_peers(app)))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize devices failed: {e}"))
}

pub(crate) fn peer_dial(host_ctx: &WasmHostContext, plugin_id: &str, node_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_dial") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let node_id = node_id.to_string();
    let dto = sync_result(block_on_async(crate::peer_net::dial_peer(app, node_id)))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize dial result failed: {e}"))
}

pub(crate) fn peer_disconnect(host_ctx: &WasmHostContext, plugin_id: &str, node_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_disconnect") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let node_id = node_id.to_string();
    sync_result(block_on_async(crate::peer_net::disconnect_peer(app, node_id)))
}

pub(crate) fn peer_respond_consent(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    request_id: &str,
    accepted: bool,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_respond_consent") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let request_id = request_id.to_string();
    sync_result(block_on_async(crate::peer_net::respond_peer_consent(app, request_id, accepted)))
}

pub(crate) fn peer_list_trusted(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_trusted") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dtos = sync_result(block_on_async(crate::peer_net::list_trusted_peers(app)))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize trusted peers failed: {e}"))
}

pub(crate) fn peer_revoke_trusted(host_ctx: &WasmHostContext, plugin_id: &str, node_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_revoke_trusted") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let node_id = node_id.to_string();
    sync_result(block_on_async(crate::peer_net::revoke_trusted_peer(app, node_id)))
}

pub(crate) fn peer_send_files(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    node_id: &str,
    paths_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_send_files") {
        return Err(denied());
    }
    let paths: Vec<String> = serde_json::from_str(paths_json)
        .map_err(|e| format!("send files: invalid paths json: {e}"))?;
    let app = require_app(host_ctx)?;
    let node_id = node_id.to_string();
    let dto = sync_result(block_on_async(crate::peer_transfer::send_files_to_peer(app, node_id, paths)))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_list_transfers(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_transfers") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dtos = sync_result(block_on_async(crate::peer_transfer::list_peer_transfers(app)))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize transfers failed: {e}"))
}

pub(crate) fn peer_cancel_transfer(host_ctx: &WasmHostContext, plugin_id: &str, batch_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_cancel_transfer") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let batch_id = batch_id.to_string();
    sync_result(block_on_async(crate::peer_transfer::cancel_peer_transfer(app, batch_id)))
}

pub(crate) fn peer_retry_transfer(host_ctx: &WasmHostContext, plugin_id: &str, batch_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_retry_transfer") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let batch_id = batch_id.to_string();
    let dto = sync_result(block_on_async(crate::peer_transfer::retry_peer_transfer(app, batch_id)))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_clear_transfer_history(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_clear_transfer_history") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let n = sync_result(block_on_async(crate::peer_transfer::clear_peer_transfer_history(app)))?;
    Ok(n as u32)
}

pub(crate) fn peer_list_receiving(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_receiving") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dtos = sync_result(block_on_async(crate::peer_receive::list_peer_receiving(app)))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize receiving failed: {e}"))
}

pub(crate) fn peer_respond_transfer(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    batch_id: &str,
    accept: bool,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_respond_transfer") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let batch_id = batch_id.to_string();
    let _hit = sync_result(block_on_async(crate::peer_receive::respond_peer_transfer(app, batch_id, accept)))?;
    Ok(())
}

pub(crate) fn peer_cancel_receiving(host_ctx: &WasmHostContext, plugin_id: &str, batch_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_cancel_receiving") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let batch_id = batch_id.to_string();
    sync_result(block_on_async(crate::peer_receive::cancel_peer_receiving(app, batch_id)))
}

pub(crate) fn peer_clear_receiving_history(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_clear_receiving_history") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let n = sync_result(block_on_async(crate::peer_receive::clear_peer_receiving_history(app)))?;
    Ok(n as u32)
}

pub(crate) fn peer_get_receive_settings(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_get_receive_settings") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dto = sync_result(block_on_async(crate::peer_receive::get_peer_receive_settings(app)))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize receive settings failed: {e}"))
}

pub(crate) fn peer_set_receive_policy(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    mode: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_set_receive_policy") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let mode = mode.to_string();
    sync_result(block_on_async(crate::peer_receive::set_peer_receive_policy(app, mode, timeout_secs)))
}

pub(crate) fn peer_list_shared_directories(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_shared_directories") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let dtos = sync_result(block_on_async(crate::peer_net::list_shared_directories(app)))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize shared dirs failed: {e}"))
}

pub(crate) fn peer_remove_shared_directory(host_ctx: &WasmHostContext, plugin_id: &str, id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_remove_shared_directory") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let id = id.to_string();
    sync_result(block_on_async(crate::peer_net::remove_shared_directory(app, id)))
}

pub(crate) fn peer_add_shared_directory(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    request_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_add_shared_directory") {
        return Err(denied());
    }
    #[derive(serde::Deserialize)]
    struct AddRequest {
        name: Option<String>,
        path: Option<String>,
    }
    let req: AddRequest = serde_json::from_str(request_json)
        .map_err(|e| format!("add shared directory: invalid request json: {e}"))?;
    let app = require_app(host_ctx)?;
    let dto = sync_result(block_on_async(crate::peer_net::add_shared_directory(app, req.name, req.path.unwrap_or_default())))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize shared dir failed: {e}"))
}

pub(crate) fn peer_list_shared_roots(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    node_id: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_shared_roots") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let node_id = node_id.to_string();
    let roots = sync_result(block_on_async(crate::peer_remote::list_peer_shared_roots(app, node_id)))?;
    serde_json::to_string(&roots).map_err(|e| format!("serialize shared roots failed: {e}"))
}

pub(crate) fn peer_browse_directory(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    node_id: &str,
    dir_id: &str,
    rel_path: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_browse_directory") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let (node_id, dir_id, rel_path) = (node_id.to_string(), dir_id.to_string(), rel_path.to_string());
    let dto = sync_result(block_on_async(crate::peer_remote::browse_peer_directory(app, node_id, dir_id, rel_path)))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize browse listing failed: {e}"))
}

pub(crate) fn peer_pull_files(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    node_id: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_pull_files") {
        return Err(denied());
    }
    let files: Vec<crate::peer_remote::RemotePullFileDto> = serde_json::from_str(files_json)
        .map_err(|e| format!("pull files: invalid files json: {e}"))?;
    let app = require_app(host_ctx)?;
    let (node_id, dir_id) = (node_id.to_string(), dir_id.to_string());
    let n = sync_result(block_on_async(crate::peer_remote::pull_peer_files(app, node_id, dir_id, files)))?;
    Ok(n as u32)
}

pub(crate) fn peer_pick_files(host_ctx: &WasmHostContext, _plugin_id: &str) -> Result<String, String> {
    // 选源对话框本身即用户授权动作，不再叠加 peer 权限门
    // （无头上下文 dialog 插件自身会失败，错误如实上抛）
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(crate::peer_transfer::peer_pick_files(app)))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

pub(crate) fn peer_pick_folder(host_ctx: &WasmHostContext, _plugin_id: &str) -> Result<String, String> {
    let app = require_app(host_ctx)?;
    let paths = sync_result(block_on_async(crate::peer_transfer::peer_pick_folder(app)))?;
    Ok(paths.into_iter().next().unwrap_or_default())
}

pub(crate) fn peer_set_download_dir(host_ctx: &WasmHostContext, plugin_id: &str, path: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_set_download_dir") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let path = if path.is_empty() { None } else { Some(path.to_string()) };
    sync_result(block_on_async(crate::peer_receive::set_peer_download_dir(app, path)))
}
