//! host-peer 逻辑层 —— 对等网络基础能力（issue 12 切换后新增）
//!
//! 宿主 peer-net 引擎命令面的 WASM 投影：每个函数做权限校验后直接
//! 复用 `peer_net` / `peer_transfer` / `peer_receive` / `peer_remote`
//! 的既有异步实现（与 Tauri 命令同一真源），DTO 以 JSON 字符串过界。
//! 无头上下文（app_handle = None）一律报错，不做静默降级——对等网络
//! 能力依赖节点运行时，降级会产生「看似成功实则空列表」的假象。

use crate::plugin::permission::PERMISSION_PEER;
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use std::sync::LazyLock;

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

// ==================== v2 句柄路由（ADR 0022）====================

/// session 句柄路由表：dial-peer-endpoint 铸造的 `sess-<uuid>` → node_id
///
/// 进程级单例（与引擎连接生命周期对齐：宿主重启即清空，插件重拨即可）。
/// 传输句柄不进表——send/pull 返回的 batch-id 本身即唯一句柄
/// （peer:transfer / peer:receive 事件也以它寻址），close 按「session 表 →
/// 发送取消 → 接收取消」顺序路由，命中即停。
#[derive(Default)]
pub(crate) struct PeerHandleTable {
    sessions: std::collections::HashMap<String, String>,
}

impl PeerHandleTable {
    /// 铸造新 session 句柄并绑定 node_id
    fn mint_session(&mut self, node_id: &str) -> String {
        let handle = format!("sess-{}", uuid::Uuid::new_v4());
        self.sessions.insert(handle.clone(), node_id.to_string());
        handle
    }

    /// 解析句柄 → node_id（不移除；数据面函数按句柄寻址时用）
    fn resolve_session(&self, handle: &str) -> Option<String> {
        self.sessions.get(handle).cloned()
    }

    /// 取出句柄（close 时用）；返回绑定的 node_id
    fn take_session(&mut self, handle: &str) -> Option<String> {
        self.sessions.remove(handle)
    }

    /// 形状兼容函数的双态寻址解析（ADR 0022 过渡期）：session 句柄翻译为
    /// node_id，其余入参原样透传（视为旧式 node-id 直呼）。Phase 4 收紧为
    /// 仅接受句柄。
    fn resolve_node_arg(&self, handle_or_node: &str) -> String {
        self.resolve_session(handle_or_node)
            .unwrap_or_else(|| handle_or_node.to_string())
    }
}

static PEER_HANDLES: LazyLock<std::sync::Mutex<PeerHandleTable>> =
    LazyLock::new(|| std::sync::Mutex::new(PeerHandleTable::default()));

fn with_handles<T>(f: impl FnOnce(&mut PeerHandleTable) -> T) -> T {
    let mut guard = PEER_HANDLES.lock().expect("peer handle table lock poisoned");
    f(&mut guard)
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
    // ADR 0022 过渡期双态寻址：入参可为 session 句柄或 node-id
    let node_id = with_handles(|t| t.resolve_node_arg(node_id));
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
    let node_id = with_handles(|t| t.resolve_node_arg(node_id));
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
    // ADR 0022 过渡期双态寻址：首参可为 session 句柄或 node-id
    let node_id = with_handles(|t| t.resolve_node_arg(node_id));
    let (dir_id, rel_path) = (dir_id.to_string(), rel_path.to_string());
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
    let node_id = with_handles(|t| t.resolve_node_arg(node_id));
    let dir_id = dir_id.to_string();
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

pub(crate) fn peer_set_transfer_encryption(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_set_transfer_encryption") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    sync_result(block_on_async(crate::peer_receive::set_peer_transfer_encryption(app, enabled)))
}

// ==================== v2 原语（ADR 0022）====================

/// endpoint 拨号：入参 `{ nodeId, addr, port }`（camelCase JSON），返回 session
/// 句柄。仅 connected 态铸造句柄；denied / unreachable 以错误上抛（携带状态字样）。
pub(crate) fn peer_dial_endpoint(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    endpoint_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_dial_endpoint") {
        return Err(denied());
    }
    let mut endpoint: crate::peer_net::DialEndpoint = serde_json::from_str(endpoint_json)
        .map_err(|e| format!("dial endpoint: invalid json: {e}"))?;
    let app = require_app(host_ctx)?;
    let node_id = std::mem::take(&mut endpoint.node_id);
    let dto = sync_result(block_on_async(crate::peer_net::dial_peer_endpoint(app, endpoint)))?;
    match dto.status.as_str() {
        "connected" => Ok(with_handles(|t| t.mint_session(&node_id))),
        other => Err(format!("dial endpoint failed: peer {other}")),
    }
}

/// 统一资源关闭：session 句柄 = 断开连接；其余按传输句柄路由（先发送批取消，
/// 后接收批取消/拒）。关闭 pending 接收批即拒绝——闸门 fail-safe 的自然结果。
/// 返回是否命中并关闭了任意资源。
pub(crate) fn peer_close(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    handle: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_close") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    // ① session 句柄 → 断开连接
    if let Some(node_id) = with_handles(|t| t.take_session(handle)) {
        return sync_result(block_on_async(crate::peer_net::disconnect_peer(app, node_id)));
    }
    // ② 发送传输句柄（batch-id）→ 取消发送批
    let cancelled = sync_result(block_on_async(crate::peer_transfer::cancel_peer_transfer(
        app.clone(),
        handle.to_string(),
    )));
    if matches!(&cancelled, Ok(true)) {
        return cancelled;
    }
    // ③ 接收侧句柄（batch-id）→ 取消/拒绝接收批（pending 即拒）
    sync_result(block_on_async(crate::peer_receive::cancel_peer_receiving(app, handle.to_string())))
}

/// 全量幂等替换引擎广播源：条目 `[{ id, name, path }]`（camelCase JSON，桌面
/// 端均为 Fs 根）；注册表真源在插件侧，此处只同步暴露面镜像。
pub(crate) fn peer_set_shared_roots(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    dirs_json: &str,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_set_shared_roots") {
        return Err(denied());
    }
    #[derive(serde::Deserialize)]
    struct SharedRootSeed {
        id: String,
        name: String,
        path: String,
    }
    let seeds: Vec<SharedRootSeed> = serde_json::from_str(dirs_json)
        .map_err(|e| format!("set shared roots: invalid dirs json: {e}"))?;
    let entries = seeds
        .into_iter()
        .map(|s| bedcode_peer_net::SharedDirEntry {
            id: s.id,
            name: s.name,
            root: bedcode_peer_net::SharedDirRoot::Fs { path: std::path::PathBuf::from(s.path) },
        })
        .collect();
    let app = require_app(host_ctx)?;
    sync_result(block_on_async(crate::peer_net::set_shared_roots(app, entries)))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::PeerHandleTable;

    #[test]
    fn mint_and_resolve_session_roundtrip() {
        let mut t = PeerHandleTable::default();
        let node = "aa".repeat(32);
        let h = t.mint_session(&node);
        assert!(h.starts_with("sess-"));
        assert_eq!(t.resolve_session(&h).as_deref(), Some(node.as_str()));
        // 未知句柄解析为 None
        assert!(t.resolve_session("sess-unknown").is_none());
    }

    #[test]
    fn take_session_removes_entry() {
        let mut t = PeerHandleTable::default();
        let h = t.mint_session("node-x");
        assert_eq!(t.take_session(&h).as_deref(), Some("node-x"));
        // 二次取出即无（close 幂等语义由返回 false 表达）
        assert_eq!(t.take_session(&h), None);
        assert!(t.resolve_session(&h).is_none());
    }

    #[test]
    fn resolve_node_arg_passthrough_non_handle() {
        let mut t = PeerHandleTable::default();
        let node = "ab".repeat(32);
        // 非 session 句柄原样透传（旧式 node-id 直呼）
        assert_eq!(t.resolve_node_arg(&node), node);
        assert_eq!(t.resolve_node_arg("some-batch-id"), "some-batch-id");
        // 句柄命中则翻译
        let h = t.mint_session(&node);
        assert_eq!(t.resolve_node_arg(&h), node);
    }

    #[test]
    fn minted_handles_are_unique() {
        let mut t = PeerHandleTable::default();
        let h1 = t.mint_session("n1");
        let h2 = t.mint_session("n1");
        assert_ne!(h1, h2);
    }
}
