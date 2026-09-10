//! host-peer 逻辑层 —— 对等网络基础能力（ADR 0022 v3 终态 13 原语）
//!
//! 宿主 peer-net 引擎的 WASM 投影：每个函数做权限校验后直接复用
//! `peer_net` / `peer_transfer` / `peer_receive` / `peer_remote`
//! 的既有异步实现（与 Tauri 命令同一真源），DTO 以 JSON 字符串过界。
//! 无头上下文（app_handle = None）一律报错，不做静默降级——对等网络
//! 能力依赖节点运行时，降级会产生「看似成功实则空列表」的假象。
//!
//! Phase 4 收紧要点（issue 13）：旧命令面全部删除；数据面四函数仅接受
//! session 句柄寻址；句柄表升级为 `handle → {node_id, addr, port}`
//! （拨号时记忆 endpoint），数据面失败且命中「发现缓存缺失」字样时以
//! 记忆 endpoint 自动重拨（信任检查照走引擎握手）——这是退役
//! DiscoveryCache 的前置条件。

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

/// session 句柄路由表条目：拨号时铸造 `sess-<uuid>` 并记忆 endpoint 三元组，
/// 数据面断线自动重拨的寻址依据。
#[derive(Debug, Clone)]
pub(crate) struct SessionEntry {
    pub node_id: String,
    pub addr: String,
    pub port: u16,
}

/// 进程级单例（与引擎连接生命周期对齐：宿主重启即清空，插件重拨即可）。
/// 传输句柄不进表——send/pull 返回的 batch-id 本身即唯一句柄
/// （peer:transfer / peer:receive 事件也以它寻址），close 按「session 表 →
/// 发送取消 → 接收取消」顺序路由，命中即停。
#[derive(Default)]
pub(crate) struct PeerHandleTable {
    sessions: std::collections::HashMap<String, SessionEntry>,
}

impl PeerHandleTable {
    /// 铸造新 session 句柄并绑定 endpoint
    fn mint_session(&mut self, entry: SessionEntry) -> String {
        let handle = format!("sess-{}", uuid::Uuid::new_v4());
        self.sessions.insert(handle.clone(), entry);
        handle
    }

    /// 解析句柄 → endpoint 条目（不移除；数据面函数按句柄寻址时用）
    fn resolve_session(&self, handle: &str) -> Option<SessionEntry> {
        self.sessions.get(handle).cloned()
    }

    /// 取出句柄（close 时用）
    fn take_session(&mut self, handle: &str) -> Option<SessionEntry> {
        self.sessions.remove(handle)
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

/// 数据面自动重拨包装：仅接受 session 句柄；操作报错且命中引擎「发现缓存
/// 缺失」字样（连接已断/缓存被 TTL 清扫）时，以句柄记忆的 endpoint 重走
/// 引擎握手后重试一次。denied/unreachable 等其他错误原样上抛。
fn with_auto_redial<T>(
    host_ctx: &WasmHostContext,
    handle: &str,
    op: impl Fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let entry = with_handles(|t| t.resolve_session(handle))
        .ok_or_else(|| format!("invalid session handle: {handle}"))?;
    match op(&entry.node_id) {
        Ok(v) => Ok(v),
        Err(e) if e.contains("discovery cache") || e.contains("not in discovery cache") => {
            let app = require_app(host_ctx)?;
            let endpoint = crate::peer_net::DialEndpoint {
                node_id: entry.node_id.clone(),
                addr: entry.addr.clone(),
                port: entry.port,
            };
            tracing::info!(
                node_id = %entry.node_id,
                "peer data-plane auto-redial (handle-remembered endpoint)"
            );
            sync_result(block_on_async(crate::peer_net::dial_peer_endpoint(app, endpoint)))?;
            op(&entry.node_id)
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn peer_dial(host_ctx: &WasmHostContext, plugin_id: &str, endpoint_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_dial") {
        return Err(denied());
    }
    let endpoint: crate::peer_net::DialEndpoint = serde_json::from_str(endpoint_json)
        .map_err(|e| format!("dial endpoint: invalid json: {e}"))?;
    let app = require_app(host_ctx)?;
    // 注意：node_id 必须 clone 而非 take——take 会把 endpoint.node_id 置空，
    // dial_peer_endpoint 对空 node_id 报 "invalid node id ''" 静默失败
    // （2026-09-07 实机实证：桌面点连接无拨号、无任何日志）。移动端同函数因
    // take 后重新构造 endpoint 无此 bug，两端口径保持 clone 语义对齐。
    let node_id = endpoint.node_id.clone();
    let addr = endpoint.addr.clone();
    let port = endpoint.port;
    let dto = sync_result(block_on_async(crate::peer_net::dial_peer_endpoint(app, endpoint)))?;
    match dto.status.as_str() {
        "connected" => Ok(with_handles(|t| {
            t.mint_session(SessionEntry { node_id, addr, port })
        })),
        other => Err(format!("dial endpoint failed: peer {other}")),
    }
}

pub(crate) fn peer_close(host_ctx: &WasmHostContext, plugin_id: &str, handle: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_close") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    // ① session 句柄 → 断开连接
    if let Some(entry) = with_handles(|t| t.take_session(handle)) {
        return sync_result(block_on_async(crate::peer_net::disconnect_peer(app, entry.node_id)));
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
    session: &str,
    paths_json: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_send_files") {
        return Err(denied());
    }
    // 载荷双形态（issue 13 Phase 3 步骤 5）：纯 string 兼容保留；对象元素
    // `{ path, encrypt? }` 携带逐文件加密意图，任一 true → 批量强制加密
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum SendPathEntry {
        Plain(String),
        Detailed {
            path: String,
            encrypt: Option<bool>,
        },
    }
    let entries: Vec<SendPathEntry> = serde_json::from_str(paths_json)
        .map_err(|e| format!("send files: invalid paths json: {e}"))?;
    let mut paths = Vec::with_capacity(entries.len());
    let mut force_encrypt = false;
    for entry in entries {
        match entry {
            SendPathEntry::Plain(path) => paths.push(path),
            SendPathEntry::Detailed { path, encrypt } => {
                if encrypt == Some(true) {
                    force_encrypt = true;
                }
                paths.push(path);
            }
        }
    }
    let _ = require_app(host_ctx)?;
    // 返回值已收窄为传输句柄（batch-id）；Phase 3 起插件自持任务视图，宿主
    // 不再回传整份 DTO。断线场景由 with_auto_redial 以记忆 endpoint 重拨
    let dto = with_auto_redial(host_ctx, session, |node_id| {
        let app = require_app(host_ctx)?;
        sync_result(block_on_async(crate::peer_transfer::send_files_to_peer_with_policy(
            app,
            node_id.to_string(),
            paths.clone(),
            if force_encrypt { Some(true) } else { None },
        )))
    })?;
    Ok(dto.batch_id)
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

pub(crate) fn peer_list_shared_roots(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_list_shared_roots") {
        return Err(denied());
    }
    let roots = with_auto_redial(host_ctx, session, |node_id| {
        let app = require_app(host_ctx)?;
        sync_result(block_on_async(crate::peer_remote::list_peer_shared_roots(app, node_id.to_string())))
    })?;
    serde_json::to_string(&roots).map_err(|e| format!("serialize shared roots failed: {e}"))
}

pub(crate) fn peer_browse_directory(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session: &str,
    dir_id: &str,
    rel_path: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_browse_directory") {
        return Err(denied());
    }
    let (dir_id, rel_path) = (dir_id.to_string(), rel_path.to_string());
    let dto = with_auto_redial(host_ctx, session, |node_id| {
        let app = require_app(host_ctx)?;
        sync_result(block_on_async(crate::peer_remote::browse_peer_directory(
            app,
            node_id.to_string(),
            dir_id.clone(),
            rel_path.clone(),
        )))
    })?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize browse listing failed: {e}"))
}

pub(crate) fn peer_pull_files(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    session: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_pull_files") {
        return Err(denied());
    }
    // RemotePullFileDto 未实现 Clone：闭包内按 JSON 串重复解析（重拨场景才二次执行）
    let dir_id = dir_id.to_string();
    let files_json_owned = files_json.to_string();
    with_auto_redial(host_ctx, session, |node_id| {
        let files: Vec<crate::peer_remote::RemotePullFileDto> =
            serde_json::from_str(&files_json_owned)
                .map_err(|e| format!("pull files: invalid files json: {e}"))?;
        let app = require_app(host_ctx)?;
        sync_result(block_on_async(crate::peer_remote::pull_peer_files(
            app,
            node_id.to_string(),
            dir_id.clone(),
            files,
        )))
    })
    .map(|n| n as u32)
}

pub(crate) fn peer_set_download_dir(host_ctx: &WasmHostContext, plugin_id: &str, path: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PEER, "host_peer_set_download_dir") {
        return Err(denied());
    }
    let app = require_app(host_ctx)?;
    let path = if path.is_empty() { None } else { Some(path.to_string()) };
    sync_result(block_on_async(crate::peer_receive::set_peer_download_dir(app, path)))
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::{PeerHandleTable, SessionEntry};

    fn entry(node: &str) -> SessionEntry {
        SessionEntry { node_id: node.to_string(), addr: "192.168.1.5".to_string(), port: 47821 }
    }

    #[test]
    fn mint_and_resolve_roundtrip_keeps_endpoint() {
        let mut t = PeerHandleTable::default();
        let h = t.mint_session(entry("aa"));
        assert!(h.starts_with("sess-"));
        let got = t.resolve_session(&h).expect("resolved");
        assert_eq!(got.node_id, "aa");
        assert_eq!(got.addr, "192.168.1.5");
        assert_eq!(got.port, 47821);
    }

    #[test]
    fn take_session_removes_entry() {
        let mut t = PeerHandleTable::default();
        let h = t.mint_session(entry("n1"));
        assert_eq!(t.take_session(&h).unwrap().node_id, "n1");
        assert!(t.take_session(&h).is_none());
        assert!(t.resolve_session(&h).is_none());
    }

    #[test]
    fn unknown_handle_is_error_not_passthrough() {
        // Phase 4 收紧：非句柄入参不再透传为 node-id（双态寻址退役）
        let t = PeerHandleTable::default();
        assert!(t.resolve_session("some-node-id").is_none());
    }

    #[test]
    fn minted_handles_are_unique() {
        let mut t = PeerHandleTable::default();
        let h1 = t.mint_session(entry("n1"));
        let h2 = t.mint_session(entry("n1"));
        assert_ne!(h1, h2);
    }
}
