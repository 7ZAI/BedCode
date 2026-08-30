//! host-peer 逻辑层 —— 对等网络基础能力（ADR 0022 v3 终态 13 原语）
//!
//! 与桌面端同构：权限校验后直接复用 `peer_net` / `peer_transfer` /
//! `peer_receive` / `peer_remote` 的既有异步实现（与 Tauri 命令同一真源），
//! DTO 以 JSON 字符串过界。无头上下文（app_handle = None）一律报错。
//!
//! Phase 4 收紧要点（issue 13）：旧命令面全部删除；数据面四函数仅接受
//! session 句柄寻址；句柄表升级为 `handle → {node_id, addr, port}`，
//! 数据面失败且命中「发现缓存缺失」字样时以记忆 endpoint 自动重拨。

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

/// session 句柄路由表条目：拨号时铸造 `sess-<uuid>` 并记忆 endpoint 三元组
#[derive(Debug, Clone)]
struct SessionEntry {
    node_id: String,
    addr: String,
    port: u16,
}

/// session 句柄路由表：dial-peer 铸造的 `sess-<uuid>` → endpoint 条目
///
/// 进程级单例（与引擎连接生命周期对齐：宿主重启即清空，插件重拨即可）。
/// 传输句柄不进表——send/pull 返回的 batch-id 本身即唯一句柄，close 按
/// 「session 表 → 发送取消 → 接收取消」顺序路由，命中即停。
#[derive(Default)]
struct PeerHandleTable {
    sessions: std::collections::HashMap<String, SessionEntry>,
}

impl PeerHandleTable {
    fn mint_session(&mut self, entry: SessionEntry) -> String {
        let handle = format!("sess-{}", uuid::Uuid::new_v4());
        self.sessions.insert(handle.clone(), entry);
        handle
    }

    fn resolve_session(&self, handle: &str) -> Option<SessionEntry> {
        self.sessions.get(handle).cloned()
    }

    fn take_session(&mut self, handle: &str) -> Option<SessionEntry> {
        self.sessions.remove(handle)
    }
}

static PEER_HANDLES: std::sync::LazyLock<std::sync::Mutex<PeerHandleTable>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(PeerHandleTable::default()));

fn with_handles<T>(f: impl FnOnce(&mut PeerHandleTable) -> T) -> T {
    let mut guard = PEER_HANDLES.lock().expect("peer handle table lock poisoned");
    f(&mut guard)
}

pub(crate) fn peer_dial(state: &WasmPluginState, endpoint_json: &str) -> Result<String, String> {
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
    let node_id = std::mem::take(&mut endpoint.node_id);
    let addr = endpoint.addr.clone();
    let port = endpoint.port;
    let dto = run(
        state,
        "host_peer_dial",
        crate::peer_net::dial_peer_endpoint(
            require_app(state)?,
            crate::peer_net::DialEndpoint {
                node_id: node_id.clone(),
                addr,
                port,
            },
        ),
    )?;
    match dto.status.as_str() {
        "connected" => Ok(with_handles(|t| {
            t.mint_session(SessionEntry { node_id, addr: endpoint.addr, port })
        })),
        other => Err(format!("dial endpoint failed: peer {other}")),
    }
}

/// 数据面自动重拨包装：仅接受 session 句柄；操作报错且命中引擎「发现缓存
/// 缺失」字样时，以句柄记忆的 endpoint 重走引擎握手后重试一次。
fn with_auto_redial<T>(
    state: &WasmPluginState,
    handle: &str,
    op: impl Fn(&str) -> Result<T, String>,
) -> Result<T, String> {
    let entry = with_handles(|t| t.resolve_session(handle))
        .ok_or_else(|| format!("invalid session handle: {handle}"))?;
    match op(&entry.node_id) {
        Ok(v) => Ok(v),
        Err(e) if e.contains("discovery cache") => {
            tracing::info!(node_id = %entry.node_id, "peer data-plane auto-redial");
            run(
                state,
                "host_peer_auto_redial",
                crate::peer_net::dial_peer_endpoint(
                    require_app(state)?,
                    crate::peer_net::DialEndpoint {
                        node_id: entry.node_id.clone(),
                        addr: entry.addr.clone(),
                        port: entry.port,
                    },
                ),
            )?;
            op(&entry.node_id)
        }
        Err(e) => Err(e),
    }
}

/// 统一资源关闭：session 句柄 = 断开连接；其余按传输句柄路由（先发送批取消，
/// 后接收批取消/拒）。关闭 pending 接收批即拒绝——闸门 fail-safe 的自然结果。
pub(crate) fn peer_close(state: &WasmPluginState, handle: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    // ① session 句柄 → 断开连接
    if let Some(entry) = with_handles(|t| t.take_session(handle)) {
        return run(
            state,
            "host_peer_close_session",
            crate::peer_net::disconnect_peer(require_app(state)?, entry.node_id),
        );
    }
    // ② 发送传输句柄（batch-id）→ 取消发送批
    let cancelled = run(
        state,
        "host_peer_close_transfer",
        crate::peer_transfer::cancel_peer_transfer(require_app(state)?, handle.to_string()),
    );
    if matches!(&cancelled, Ok(true)) {
        return cancelled;
    }
    // ③ 接收侧句柄（batch-id）→ 取消/拒绝接收批（pending 即拒）
    run(
        state,
        "host_peer_close_receiving",
        crate::peer_receive::cancel_peer_receiving(require_app(state)?, handle.to_string()),
    )
}

pub(crate) fn peer_respond_consent(state: &WasmPluginState, request_id: &str, accepted: bool) -> Result<bool, String> {
    require_peer_permission(state)?;
    run(
        state,
        "host_peer_respond_consent",
        crate::peer_net::respond_peer_consent(require_app(state)?, request_id.to_string(), accepted),
    )
}

pub(crate) fn peer_list_trusted(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let dtos = run(state, "host_peer_list_trusted", crate::peer_net::list_trusted_peers(require_app(state)?))?;
    serde_json::to_string(&dtos).map_err(|e| format!("serialize trusted peers failed: {e}"))
}

pub(crate) fn peer_revoke_trusted(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    run(
        state,
        "host_peer_revoke_trusted",
        crate::peer_net::revoke_trusted_peer(require_app(state)?, node_id.to_string()),
    )
}

/// 发送一批文件：仅 session 句柄寻址；paths 元素双形态（纯 string 或
/// `{ path, encrypt? }` 对象，任一 true → 批量强制加密）；返回传输句柄。
pub(crate) fn peer_send_files(
    state: &WasmPluginState,
    session: &str,
    paths_json: &str,
) -> Result<String, String> {
    require_peer_permission(state)?;
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
    let dto = with_auto_redial(state, session, |node_id| {
        run(
            state,
            "host_peer_send_files",
            crate::peer_transfer::send_files_to_peer_with_policy(
                require_app(state)?,
                node_id.to_string(),
                paths.clone(),
                if force_encrypt { Some(true) } else { None },
            ),
        )
    })?;
    Ok(dto.batch_id)
}

pub(crate) fn peer_respond_transfer(
    state: &WasmPluginState,
    batch_id: &str,
    accept: bool,
) -> Result<(), String> {
    require_peer_permission(state)?;
    run(
        state,
        "host_peer_respond_transfer",
        crate::peer_receive::respond_peer_transfer(require_app(state)?, batch_id.to_string(), accept),
    )?;
    Ok(())
}

pub(crate) fn peer_set_receive_policy(
    state: &WasmPluginState,
    mode: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    require_peer_permission(state)?;
    run(
        state,
        "host_peer_set_receive_policy",
        crate::peer_receive::set_peer_receive_policy(require_app(state)?, mode.to_string(), timeout_secs),
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
    run(
        state,
        "host_peer_set_shared_roots",
        crate::peer_net::set_shared_roots(require_app(state)?, entries),
    )
}

pub(crate) fn peer_list_shared_roots(state: &WasmPluginState, session: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let roots = with_auto_redial(state, session, |node_id| {
        run(
            state,
            "host_peer_list_shared_roots",
            crate::peer_remote::list_peer_shared_roots(require_app(state)?, node_id.to_string()),
        )
    })?;
    serde_json::to_string(&roots).map_err(|e| format!("serialize shared roots failed: {e}"))
}

pub(crate) fn peer_browse_directory(
    state: &WasmPluginState,
    session: &str,
    dir_id: &str,
    rel_path: &str,
) -> Result<String, String> {
    require_peer_permission(state)?;
    let dir_id = dir_id.to_string();
    let rel_path = rel_path.to_string();
    let dto = with_auto_redial(state, session, |node_id| {
        run(
            state,
            "host_peer_browse_directory",
            crate::peer_remote::browse_peer_directory(
                require_app(state)?,
                node_id.to_string(),
                dir_id.clone(),
                rel_path.clone(),
            ),
        )
    })?;
    serde_json::to_string(&dto).map_err(|e| format!("serialize browse listing failed: {e}"))
}

pub(crate) fn peer_pull_files(
    state: &WasmPluginState,
    session: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    require_peer_permission(state)?;
    // RemotePullFileDto 未实现 Clone：闭包内按 JSON 串重复解析（重拨才二次执行）
    let dir_id = dir_id.to_string();
    let files_json_owned = files_json.to_string();
    with_auto_redial(state, session, |node_id| {
        let files: Vec<crate::peer_remote::RemotePullFileDto> =
            serde_json::from_str(&files_json_owned)
                .map_err(|e| format!("pull files: invalid files json: {e}"))?;
        run(
            state,
            "host_peer_pull_files",
            crate::peer_remote::pull_peer_files(
                require_app(state)?,
                node_id.to_string(),
                dir_id.clone(),
                files,
            ),
        )
    })
    .map(|n| n as u32)
}
