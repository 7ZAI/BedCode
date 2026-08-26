//! host-peer 逻辑层 —— 对等网络基础能力（issue 12 切换后新增）
//!
//! 与桌面端同构：权限校验后直接复用 `peer_net` / `peer_transfer` /
//! `peer_receive` / `peer_remote` 的既有异步实现（与 Tauri 命令同一真源），
//! DTO 以 JSON 字符串过界。无头上下文（app_handle = None）一律报错。

use super::super::{block_on_async, WasmPluginState};
use super::support::guarded_host_call;

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
/// 经 [`block_on_async`] 阻塞驱动（host fn 同步语义；worker 线程上自动
/// block_in_place，重入时改新线程，避免 "Cannot start a runtime from within
/// a runtime" panic 污染 wasmtime Store 导致插件整体失效——2026-08-26 真机实证）。
/// 外层 [`guarded_host_call`] 兜底隔离残余 panic（与其他 host 域同防御深度）。
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

pub(crate) fn peer_list_devices(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, "host_peer_list_devices", crate::peer_net::list_discovered_peers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize devices failed: {e}"))
}

pub(crate) fn peer_dial(state: &WasmPluginState, node_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, "host_peer_dial", crate::peer_net::dial_peer(app, node_id.to_string()))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize dial result failed: {e}"))
}

pub(crate) fn peer_disconnect(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_disconnect", crate::peer_net::disconnect_peer(app, node_id.to_string()))
}

pub(crate) fn peer_respond_consent(state: &WasmPluginState, request_id: &str, accepted: bool) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_respond_consent", crate::peer_net::respond_peer_consent(app, request_id.to_string(), accepted))
}

pub(crate) fn peer_list_trusted(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, "host_peer_list_trusted", crate::peer_net::list_trusted_peers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize trusted peers failed: {e}"))
}

pub(crate) fn peer_revoke_trusted(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_revoke_trusted", crate::peer_net::revoke_trusted_peer(app, node_id.to_string()))
}

pub(crate) fn peer_send_files(state: &WasmPluginState, node_id: &str, paths_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let paths: Vec<String> =
        serde_json::from_str(paths_json).map_err(|e| format!("send files: invalid paths json: {e}"))?;
    let app = require_app(state)?;
    let dto = run(state, "host_peer_send_files", crate::peer_transfer::send_files_to_peer(app, node_id.to_string(), paths))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_list_transfers(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, "host_peer_list_transfers", crate::peer_transfer::list_peer_transfers(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize transfers failed: {e}"))
}

pub(crate) fn peer_cancel_transfer(state: &WasmPluginState, batch_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_cancel_transfer", crate::peer_transfer::cancel_peer_transfer(app, batch_id.to_string()))
}

pub(crate) fn peer_retry_transfer(state: &WasmPluginState, batch_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, "host_peer_retry_transfer", crate::peer_transfer::retry_peer_transfer(app, batch_id.to_string()))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize transfer dto failed: {e}"))
}

pub(crate) fn peer_clear_transfer_history(state: &WasmPluginState) -> Result<u32, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let n = run(state, "host_peer_clear_transfer_history", crate::peer_transfer::clear_peer_transfer_history(app))?;
    Ok(n as u32)
}

pub(crate) fn peer_list_receiving(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, "host_peer_list_receiving", crate::peer_receive::list_peer_receiving(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize receiving failed: {e}"))
}

pub(crate) fn peer_respond_transfer(state: &WasmPluginState, batch_id: &str, accept: bool) -> Result<(), String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let _hit = run(state, "host_peer_respond_transfer", crate::peer_receive::respond_peer_transfer(app, batch_id.to_string(), accept))?;
    Ok(())
}

pub(crate) fn peer_cancel_receiving(state: &WasmPluginState, batch_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_cancel_receiving", crate::peer_receive::cancel_peer_receiving(app, batch_id.to_string()))
}

pub(crate) fn peer_clear_receiving_history(state: &WasmPluginState) -> Result<u32, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let n = run(state, "host_peer_clear_receiving_history", crate::peer_receive::clear_peer_receiving_history(app))?;
    Ok(n as u32)
}

pub(crate) fn peer_get_receive_settings(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dto = run(state, "host_peer_get_receive_settings", crate::peer_receive::get_peer_receive_settings(app))?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize receive settings failed: {e}"))
}

pub(crate) fn peer_set_receive_policy(state: &WasmPluginState, mode: &str, timeout_secs: u64) -> Result<(), String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_set_receive_policy", crate::peer_receive::set_peer_receive_policy(app, mode.to_string(), timeout_secs))
}

pub(crate) fn peer_set_transfer_encryption(state: &WasmPluginState, enabled: bool) -> Result<(), String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_set_transfer_encryption", crate::peer_receive::set_peer_transfer_encryption(app, enabled))
}

pub(crate) fn peer_list_shared_directories(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let dtos = run(state, "host_peer_list_shared_dirs", crate::peer_net::list_shared_directories(app))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize shared dirs failed: {e}"))
}

pub(crate) fn peer_remove_shared_directory(state: &WasmPluginState, id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    run(state, "host_peer_remove_shared_dir", crate::peer_net::remove_shared_directory(app, id.to_string()))
}

pub(crate) fn peer_add_shared_directory(state: &WasmPluginState, _request_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    // 移动端忽略请求载荷：弹 SAF 目录树选择器，授权与条目落盘由宿主完成
    let app = require_app(state)?;
    let dto = run(state, "host_peer_add_shared_dir", crate::peer_net::add_shared_directory_saf(app))?
        .ok_or_else(|| "add shared directory: user cancelled".to_string())?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize shared dir failed: {e}"))
}

pub(crate) fn peer_list_shared_roots(state: &WasmPluginState, node_id: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    let roots = run(state, "host_peer_list_remote_roots", crate::peer_remote::list_peer_shared_roots(app, node_id.to_string()))?;
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
        "host_peer_browse_directory",
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
        "host_peer_pull_files",
        crate::peer_remote::pull_peer_files(app, node_id.to_string(), dir_id.to_string(), files),
    )?;
    Ok(n as u32)
}

pub(crate) fn peer_pick_files(state: &WasmPluginState) -> Result<String, String> {
    // 选源对话框本身即用户授权动作，不再叠加 peer 权限门
    let app = require_app(state)?;
    let paths = run(state, "host_peer_pick_files", crate::peer_transfer::peer_pick_files(app))?;
    serde_json::to_string(&paths).map_err(|e| format!("serialize picked files failed: {e}"))
}

pub(crate) fn peer_pick_folder(state: &WasmPluginState) -> Result<String, String> {
    let app = require_app(state)?;
    let paths = run(state, "host_peer_pick_folder", crate::peer_transfer::peer_pick_folder(app))?;
    Ok(paths.into_iter().next().unwrap_or_default())
}

// ==================== v2 原语（ADR 0022）====================

/// session 句柄路由表：dial-peer-endpoint 铸造的 `sess-<uuid>` → node_id
///
/// 进程级单例（与引擎连接生命周期对齐：宿主重启即清空，插件重拨即可）。
/// 传输句柄不进表——send/pull 返回的 batch-id 本身即唯一句柄，close 按
/// 「session 表 → 发送取消 → 接收取消」顺序路由，命中即停。
#[derive(Default)]
struct PeerHandleTable {
    sessions: std::collections::HashMap<String, String>,
}

impl PeerHandleTable {
    fn mint_session(&mut self, node_id: &str) -> String {
        let handle = format!("sess-{}", uuid::Uuid::new_v4());
        self.sessions.insert(handle.clone(), node_id.to_string());
        handle
    }

    fn resolve_node_arg(&self, handle_or_node: &str) -> String {
        self.sessions
            .get(handle_or_node)
            .cloned()
            .unwrap_or_else(|| handle_or_node.to_string())
    }

    fn take_session(&mut self, handle: &str) -> Option<String> {
        self.sessions.remove(handle)
    }
}

static PEER_HANDLES: std::sync::LazyLock<std::sync::Mutex<PeerHandleTable>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(PeerHandleTable::default()));

fn with_handles<T>(f: impl FnOnce(&mut PeerHandleTable) -> T) -> T {
    let mut guard = PEER_HANDLES.lock().expect("peer handle table lock poisoned");
    f(&mut guard)
}

/// endpoint 拨号：入参 `{ nodeId, addr, port }`（camelCase JSON），返回 session
/// 句柄。仅 connected 态铸造句柄；denied / unreachable 以错误上抛（携带状态字样）。
pub(crate) fn peer_dial_endpoint(state: &WasmPluginState, endpoint_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Endpoint {
        node_id: String,
        addr: String,
        port: u16,
    }
    let mut endpoint: Endpoint = serde_json::from_str(endpoint_json)
        .map_err(|e| format!("dial endpoint: invalid json: {e}"))?;
    let app = require_app(state)?;
    let node_id = std::mem::take(&mut endpoint.node_id);
    let dto = run(
        state,
        "host_peer_dial_endpoint",
        crate::peer_net::dial_peer_endpoint(
            app,
            crate::peer_net::DialEndpoint {
                node_id: node_id.clone(),
                addr: endpoint.addr,
                port: endpoint.port,
            },
        ),
    )?;
    match dto.status.as_str() {
        "connected" => Ok(with_handles(|t| t.mint_session(&node_id))),
        other => Err(format!("dial endpoint failed: peer {other}")),
    }
}

/// 统一资源关闭：session 句柄 = 断开连接；其余按传输句柄路由（先发送批取消，
/// 后接收批取消/拒）。关闭 pending 接收批即拒绝——闸门 fail-safe 的自然结果。
pub(crate) fn peer_close(state: &WasmPluginState, handle: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let app = require_app(state)?;
    // ① session 句柄 → 断开连接
    if let Some(node_id) = with_handles(|t| t.take_session(handle)) {
        return run(state, "host_peer_close_session", crate::peer_net::disconnect_peer(app, node_id));
    }
    // ② 发送传输句柄（batch-id）→ 取消发送批
    let cancelled = run(
        state,
        "host_peer_close_transfer",
        crate::peer_transfer::cancel_peer_transfer(app.clone(), handle.to_string()),
    );
    if matches!(&cancelled, Ok(true)) {
        return cancelled;
    }
    // ③ 接收侧句柄（batch-id）→ 取消/拒绝接收批（pending 即拒）
    run(
        state,
        "host_peer_close_receiving",
        crate::peer_receive::cancel_peer_receiving(app, handle.to_string()),
    )
}

/// 全量幂等替换引擎广播源：条目 `[{ id, name, safTreeUri }]`（camelCase JSON，
/// 移动端共享根均为 SAF 树）；注册表真源在插件侧，此处只同步暴露面镜像。
pub(crate) fn peer_set_shared_roots(state: &WasmPluginState, dirs_json: &str) -> Result<(), String> {
    require_peer_permission(state)?;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SharedRootSeed {
        id: String,
        name: String,
        saf_tree_uri: String,
    }
    let seeds: Vec<SharedRootSeed> = serde_json::from_str(dirs_json)
        .map_err(|e| format!("set shared roots: invalid dirs json: {e}"))?;
    let entries = seeds
        .into_iter()
        .map(|s| bedcode_peer_net::SharedDirEntry {
            id: s.id,
            name: s.name,
            root: bedcode_peer_net::SharedDirRoot::Saf { tree_uri: s.saf_tree_uri },
        })
        .collect();
    let app = require_app(state)?;
    run(state, "host_peer_set_shared_roots", crate::peer_net::set_shared_roots(app, entries))
}
