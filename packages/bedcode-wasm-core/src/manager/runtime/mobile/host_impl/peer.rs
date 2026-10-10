//! host-peer 逻辑层 —— 对等网络基础能力（ADR 0022 v3 终态 13 原语）
//!
//! 与桌面端同构：权限校验后经 [`crate::host_api::ports::HostEnginePorts`]
//! 调用宿主 peer 四引擎（`peer_net` / `peer_transfer` / `peer_receive` /
//! `peer_remote`，与 Tauri 命令同一真源），DTO 以 JSON / 原始值过界
//! （反序列化责任在宿主端口实现方）。无头上下文（app_handle = None）一律报错。
//!
//! Phase 4 收紧要点（issue 13）：旧命令面全部删除；数据面四函数仅接受
//! session 句柄寻址；句柄表升级为 `handle → {node_id, addr, port}`，
//! 数据面失败且命中「发现缓存缺失」字样时以记忆 endpoint 自动重拨。

use std::sync::Arc;

use super::super::{block_on_async, WasmPluginState};
use super::support::guarded_host_call;
use crate::host_api::ports::HostEnginePorts;

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

/// 宿主引擎调用统一包装（AppError → 可读字符串）——经 [`block_on_async`]
/// 阻塞驱动（host fn 同步语义；worker 线程上自动 block_in_place，重入时改
/// 新线程，避免 "Cannot start a runtime from within a runtime" panic 污染
/// wasmtime Store 导致插件整体失效——2026-08-26 真机实证）。外层
/// [`guarded_host_call`] 兜底隔离残余 panic（与其他 host 域同防御深度）。
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
    let ports: Arc<dyn HostEnginePorts> = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let dial_node_id = node_id.clone();
    let dial_addr = addr.clone();
    let status = run(
        state,
        "host_peer_dial",
        async move { ports.peer_dial_endpoint(&app, dial_node_id, dial_addr, port).await },
    )?;
    match status.as_str() {
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
            let ports: Arc<dyn HostEnginePorts> = state.host_ctx.ports.clone();
            let app = require_app(state)?;
            let redial_node = entry.node_id.clone();
            let redial_addr = entry.addr.clone();
            let redial_port = entry.port;
            run(
                state,
                "host_peer_auto_redial",
                async move {
                    ports
                        .peer_dial_endpoint(&app, redial_node, redial_addr, redial_port)
                        .await
                        .map(|_| ())
                },
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
    let ports: Arc<dyn HostEnginePorts> = state.host_ctx.ports.clone();
    // ① session 句柄 → 断开连接
    if let Some(entry) = with_handles(|t| t.take_session(handle)) {
        let app = require_app(state)?;
        return run(
            state,
            "host_peer_close_session",
            async move { ports.peer_disconnect(&app, entry.node_id).await },
        );
    }
    // ② 发送传输句柄（batch-id）→ 取消发送批
    let app = require_app(state)?;
    let cancelled = run(
        state,
        "host_peer_close_transfer",
        {
            let ports = ports.clone();
            let app = app.clone();
            let handle_owned = handle.to_string();
            async move { ports.peer_cancel_transfer(&app, handle_owned).await }
        },
    );
    if matches!(&cancelled, Ok(true)) {
        return cancelled;
    }
    // ③ 接收侧句柄（batch-id）→ 取消/拒绝接收批（pending 即拒）
    let ports = ports.clone();
    let app2 = app.clone();
    let handle2 = handle.to_string();
    run(
        state,
        "host_peer_close_receiving",
        async move { ports.peer_cancel_receiving(&app2, handle2).await },
    )
}

pub(crate) fn peer_respond_consent(state: &WasmPluginState, request_id: &str, accepted: bool) -> Result<bool, String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let request_id = request_id.to_string();
    run(
        state,
        "host_peer_respond_consent",
        async move { ports.peer_respond_consent(&app, request_id, accepted).await },
    )
}

pub(crate) fn peer_list_trusted(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    run(state, "host_peer_list_trusted", async move { ports.peer_list_trusted(&app).await })
}

pub(crate) fn peer_revoke_trusted(state: &WasmPluginState, node_id: &str) -> Result<bool, String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let node_id = node_id.to_string();
    run(
        state,
        "host_peer_revoke_trusted",
        async move { ports.peer_revoke_trusted(&app, node_id).await },
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
        Detailed { path: String, encrypt: Option<bool> },
    }
    // 票 06 fail-visible：旧产物载荷携带的 `concurrency` 并发脉冲字段已随宿主
    // 并发闸门退役——serde 未知字段默认忽略，必须显式检测并显性报错（点名
    // ABI v12 重建），否则旧产物静默超并发（宿主已无闸门可拦）
    let raw: serde_json::Value = serde_json::from_str(paths_json)
        .map_err(|e| format!("send files: invalid paths json: {e}"))?;
    if raw.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|e| e.as_object().is_some_and(|o| o.contains_key("concurrency")))
    }) {
        return Err(
            "send files: payload field 'concurrency' retired in ABI v12 (host-side concurrency gate removed; \
             concurrency is caller-controlled): rebuild plugin artifact with current SDK"
                .to_string(),
        );
    }
    let entries: Vec<SendPathEntry> = serde_json::from_value(raw)
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
    // 返回值收窄为传输句柄（batch-id）：v12 起一次调用 = 一个会话立即发起
    // （宿主并发闸门删除，节流归插件侧闸门）。断线场景由 with_auto_redial
    // 以记忆 endpoint 重拨
    let batch_id = with_auto_redial(state, session, |node_id| {
        let ports = state.host_ctx.ports.clone();
        let app = require_app(state)?;
        let node_id = node_id.to_string();
        let paths = paths.clone();
        let encrypt = if force_encrypt { Some(true) } else { None };
        run(
            state,
            "host_peer_send_files",
            async move { ports.peer_send_files_with_policy(&app, node_id, paths, encrypt).await },
        )
    })?;
    Ok(batch_id)
}

pub(crate) fn peer_respond_transfer(
    state: &WasmPluginState,
    batch_id: &str,
    accept: bool,
) -> Result<(), String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let batch_id = batch_id.to_string();
    run(
        state,
        "host_peer_respond_transfer",
        async move { ports.peer_respond_transfer(&app, batch_id, accept).await },
    )?;
    Ok(())
}

pub(crate) fn peer_set_receive_policy(
    state: &WasmPluginState,
    mode: &str,
    timeout_secs: u64,
) -> Result<(), String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let mode = mode.to_string();
    run(
        state,
        "host_peer_set_receive_policy",
        async move { ports.peer_set_receive_policy(&app, mode, timeout_secs).await },
    )
}

/// 显式暂停进行中的发送批：中断会话连接，任务保留（含已传字节）不落历史。
pub(crate) fn peer_pause_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let batch_id = batch_id.to_string();
    let hit = run(
        state,
        "host_peer_pause_transfer",
        async move { ports.peer_pause_transfer(&app, batch_id).await },
    )?;
    if !hit {
        return Err("pause transfer: no running send batch with that id".to_string());
    }
    Ok(())
}

/// 恢复暂停的发送批：入队并经并发闸门启动，接收端按已写偏移续传。
pub(crate) fn peer_resume_transfer(state: &WasmPluginState, batch_id: &str) -> Result<(), String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let batch_id = batch_id.to_string();
    let hit = run(
        state,
        "host_peer_resume_transfer",
        async move { ports.peer_resume_transfer(&app, batch_id).await },
    )?;
    if !hit {
        return Err("resume transfer: no paused send batch with that id".to_string());
    }
    Ok(())
}

/// 全量幂等替换引擎广播源：条目 `[{ id, name, safTreeUri }]`（camelCase JSON，
/// 移动端共享根均为 SAF 树）；注册表真源在插件侧，此处只同步暴露面镜像。
/// JSON 透传端口（SharedDirEntry/SharedDirRoot 形状归宿主实现解析——C8：
/// 宿主类型不出宿主）。
pub(crate) fn peer_set_shared_roots(state: &WasmPluginState, dirs_json: &str) -> Result<(), String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    let dirs_json = dirs_json.to_string();
    run(
        state,
        "host_peer_set_shared_roots",
        async move { ports.peer_set_shared_roots(&app, dirs_json).await },
    )
}

pub(crate) fn peer_list_shared_roots(state: &WasmPluginState, session: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    with_auto_redial(state, session, |node_id| {
        let ports = state.host_ctx.ports.clone();
        let app = require_app(state)?;
        let node_id = node_id.to_string();
        run(
            state,
            "host_peer_list_shared_roots",
            async move { ports.peer_list_shared_roots(&app, node_id).await },
        )
    })
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
    with_auto_redial(state, session, |node_id| {
        let ports = state.host_ctx.ports.clone();
        let app = require_app(state)?;
        let node_id = node_id.to_string();
        let dir_id = dir_id.clone();
        let rel_path = rel_path.clone();
        run(
            state,
            "host_peer_browse_directory",
            async move { ports.peer_browse_directory(&app, node_id, dir_id, rel_path).await },
        )
    })
}

pub(crate) fn peer_pull_files(
    state: &WasmPluginState,
    session: &str,
    dir_id: &str,
    files_json: &str,
) -> Result<u32, String> {
    require_peer_permission(state)?;
    // files_json 契约（RemotePullFileDto 数组）归宿主端口实现解析（C8）
    let dir_id = dir_id.to_string();
    let files_json_owned = files_json.to_string();
    with_auto_redial(state, session, |node_id| {
        let ports = state.host_ctx.ports.clone();
        let app = require_app(state)?;
        let node_id = node_id.to_string();
        let dir_id = dir_id.clone();
        let files_json = files_json_owned.clone();
        run(
            state,
            "host_peer_pull_files",
            async move { ports.peer_pull_files(&app, node_id, dir_id, files_json).await },
        )
    })
}

// ==================== host-peer 对齐 19（票 04 新增 5 原语） ====================
// 对齐桌面 v34 形状：set-download-dir / start-node / stop-node / active-transfers /
// collect-outgoing。`resume-all-transfers` 随票 06（批量恢复编排下沉插件）整面退役。

/// 设置接收落点目录（空串 = 恢复默认；引擎落盘配置原语，ADR 0022 v3）
pub(crate) fn peer_set_download_dir(state: &WasmPluginState, path: &str) -> Result<(), String> {
    require_peer_permission(state)?;
    let path = if path.is_empty() { None } else { Some(path.to_string()) };
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    run(
        state,
        "host_peer_set_download_dir",
        async move { ports.peer_set_download_dir(&app, path).await },
    )
}

/// 按需启动本机 peer 节点（引擎级生命周期原语，审计票 12）：幂等，
/// false = 未改变状态；调用方成为节点属主（谁起谁停）
pub(crate) fn peer_start_node(state: &WasmPluginState) -> Result<bool, String> {
    require_peer_permission(state)?;
    let caller = state.plugin_id.clone();
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    run(
        state,
        "host_peer_start_node",
        async move { ports.peer_start_node(&app, &caller).await },
    )
}

/// 属主插件让节点下线（幂等）；非属主拒绝（文案不回带属主 id）
pub(crate) fn peer_stop_node(state: &WasmPluginState) -> Result<bool, String> {
    require_peer_permission(state)?;
    let caller = state.plugin_id.clone();
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    run(
        state,
        "host_peer_stop_node",
        async move { ports.peer_stop_node(&app, &caller).await },
    )
}

/// 活跃传输批清单（引擎会话事实投影，供插件事件归约状态机首屏重建）：
/// send 句柄表 + receive pending 询问表 + pull 会话表三表聚合投影
/// （聚合在宿主端口实现内完成——三表真源在宿主引擎）
pub(crate) fn peer_active_transfers(state: &WasmPluginState) -> Result<String, String> {
    require_peer_permission(state)?;
    let ports = state.host_ctx.ports.clone();
    let app = require_app(state)?;
    run(
        state,
        "host_peer_active_transfers",
        async move { ports.peer_active_transfers(&app).await },
    )
}

/// 发送源收集（目录递归 + 批内同名去重 → `[{ path, size }]` JSON）：
/// 仅读元数据不读内容；路径应来自 pick-* 用户选择（选择即授权）
pub(crate) fn peer_collect_outgoing(state: &WasmPluginState, paths_json: &str) -> Result<String, String> {
    require_peer_permission(state)?;
    let paths: Vec<String> =
        serde_json::from_str(paths_json).map_err(|e| format!("collect outgoing: invalid paths json: {e}"))?;
    let ports = state.host_ctx.ports.clone();
    run(
        state,
        "host_peer_collect_outgoing",
        async move { ports.peer_collect_outgoing(paths).await },
    )
}
