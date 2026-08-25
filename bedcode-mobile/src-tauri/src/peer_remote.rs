//! 对等网络远端浏览/拉取（issue 11）：浏览可信对端共享目录 + 多文件拉取编排。
//!
//! 浏览为单请求会话——每次操作对对端新拨号（线协议「单连接单请求」契约），
//! 返回引擎排好序的条目与「可能被权限过滤」提示位；拉取按文件逐条独立会话
//! （免协商、断点续传复用 issue 05/06 数据面），任务行在会话发起前预登记进
//! 接收表（[`super::peer_receive`]），进度/终态/取消因此完全复用任务体系。
//!
//! 只读约束：本模块命令面只有列目录与拉取，无任何指向暴露端的写语义。

use std::collections::HashMap;
use std::sync::Mutex;

use bedcode_peer_net::{
    CancelToken, NodeId, PeerNetError, TransferEvent, browse_shared_dir, list_shared_roots,
    pull_shared_file,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::peer_net::{map_peer_net_error, parse_node_id, runtime_snapshot};

// ==================== 常量 ====================

/// 单次拉取批内文件数上限（前端递归枚举的兜底闸；防误选超大目录拖垮会话）
const PULL_FILES_CAP: usize = 512;

// ==================== 数据模型 ====================

/// 远端目录条目（BrowseResponse DirEntry 的 camelCase 形态）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntryDto {
    /// 条目名（不含路径分隔）
    pub name: String,
    /// 是否目录
    pub is_dir: bool,
    /// 文件字节数（目录恒 0）
    pub size: u64,
}

/// 远端目录浏览结果
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBrowseDto {
    /// 子条目（已按「目录优先、按名排序」排好）
    pub entries: Vec<RemoteEntryDto>,
    /// 暴露端提示列表可能不全（Android 存储权限过滤，沿用既有 notice 语义）
    pub filtered: bool,
}

/// 拉取清单单项（前端递归枚举后的扁平文件列表）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePullFileDto {
    /// 暴露端共享目录内的相对路径（必须指向文件）
    pub rel_path: String,
    /// 文件字节数（浏览所得；供任务行进度分母）
    pub size: u64,
}

// ==================== 状态容器 ====================

/// 单个节点代际的拉取会话上下文（装配时建立，停止时取消）
struct SessionCtx {
    /// 接收侧事件通道发送端快照（拉取会话经它入账任务表）
    events: mpsc::Sender<TransferEvent>,
    /// 本代全部拉取令牌的父源：节点停止时 cancel 即中止所有在途/后续会话
    cancel_root: CancelToken,
}

/// Tauri 托管的远端浏览/拉取状态容器
///
/// - `pulls`：进行中的客户端拉取会话（batch_id → 取消令牌）；std Mutex 与
///   consents/connections 同款瞬时临界区；
/// - `session`：当前节点代际上下文（None = 节点未启动）。
#[derive(Default)]
pub struct PeerRemoteState {
    pulls: Mutex<HashMap<String, CancelToken>>,
    session: tokio::sync::Mutex<Option<SessionCtx>>,
}

/// 节点装配期登记拉取会话上下文（start_locked 调用）
pub(crate) async fn register_session(app: &AppHandle, events: mpsc::Sender<TransferEvent>) {
    let state = app.state::<PeerRemoteState>();
    *state.session.lock().await = Some(SessionCtx {
        events,
        cancel_root: CancelToken::new(),
    });
}

/// 节点停止时收尾：取消本代根令牌（中止在途与后续拉取）并摘除上下文
pub(crate) async fn clear_state(app: &AppHandle) {
    let ctx = {
        let state = app.state::<PeerRemoteState>();
        let mut guard = state.session.lock().await;
        guard.take()
    };
    if let Some(ctx) = ctx {
        ctx.cancel_root.cancel();
        let drained: Vec<String> = app
            .state::<PeerRemoteState>()
            .pulls
            .lock()
            .expect("peer remote pulls lock poisoned")
            .drain()
            .map(|(id, _)| id)
            .collect();
        for batch_id in drained {
            tracing::debug!(batch_id = %batch_id, "remote pull dropped on node stop");
        }
    }
}

/// 取消进行中的拉取会话（返回是否命中）；由 [`super::peer_receive::cancel_peer_receiving`]
/// 在服务端会话表未命中时兜底调用
pub(crate) fn cancel_pull(app: &AppHandle, batch_id: &str) -> bool {
    let token = app
        .state::<PeerRemoteState>()
        .pulls
        .lock()
        .expect("peer remote pulls lock poisoned")
        .remove(batch_id);
    match token {
        Some(token) => {
            tracing::info!(batch_id = %batch_id, "cancelling remote pull by host");
            token.cancel();
            true
        }
        None => false,
    }
}

// ==================== 内部辅助 ====================

/// 解析拨号记录并建立到对端的新连接（浏览/拉取共用前置）
///
/// denied/unreachable 以带上下文的错误上抛——入口仅在已连接态展示，
/// 到达此处仍被拒说明信任关系已变化，如实呈现给调用方。
async fn dial_peer(
    app: &AppHandle,
    node_id: &NodeId,
    action: &str,
) -> crate::Result<bedcode_peer_net::Connection> {
    let (node, cache) = runtime_snapshot(app).await.ok_or_else(|| {
        crate::AppError::Internal(format!("peer-net {action} failed: node not started"))
    })?;
    let record = cache.get(node_id).ok_or_else(|| {
        crate::AppError::Internal(format!(
            "peer-net {action} failed: peer {} not in discovery cache (offline or unknown)",
            node_id.as_str()
        ))
    })?;
    node.dial(&record.to_static_peer_record())
        .await
        .map_err(|e| match e {
            PeerNetError::DialDeniedByPeer { .. } => crate::AppError::Internal(format!(
                "peer-net {action} failed: not trusted by peer {}",
                node_id.as_str()
            )),
            other => map_peer_net_error(other),
        })
}

// ==================== 命令面 ====================

/// 对端暴露中的共享根清单 DTO
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSharedRootDto {
    /// 共享目录条目 ID（browse/pull 按此寻址）
    pub id: String,
    /// 展示名
    pub name: String,
}

/// 列可信对端暴露中的共享根（浏览入口：先取根清单再逐根下钻）
#[tauri::command]
pub async fn list_peer_shared_roots(
    app: AppHandle,
    node_id: String,
) -> crate::Result<Vec<PeerSharedRootDto>> {
    let parsed = parse_node_id(&node_id)?;
    let conn = dial_peer(&app, &parsed, "roots").await?;
    let roots = list_shared_roots(conn)
        .await
        .map_err(|e| crate::AppError::Internal(format!("peer-net list shared roots failed: {e}")))?;
    // 诊断插桩：对端共享根可见性（排查移动端看不到桌面共享目录）
    tracing::info!(
        node_id = %parsed,
        count = roots.len(),
        "peer shared roots listed"
    );
    Ok(roots
        .into_iter()
        .map(|root| PeerSharedRootDto { id: root.id, name: root.name })
        .collect())
}

/// 浏览可信对端的共享目录（单请求会话；每操作新拨号）
#[tauri::command]
pub async fn browse_peer_directory(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    rel_path: String,
) -> crate::Result<RemoteBrowseDto> {
    let parsed = parse_node_id(&node_id)?;
    let conn = dial_peer(&app, &parsed, "browse").await?;
    let listing = browse_shared_dir(conn, &dir_id, &rel_path).await.map_err(|e| {
        crate::AppError::Internal(format!(
            "peer-net browse '{rel_path}' in dir '{dir_id}' failed: {e}"
        ))
    })?;
    // 诊断插桩：目录浏览结果可见性（dir_id 形状/条目数）
    tracing::info!(
        node_id = %parsed,
        dir_id = %dir_id,
        rel = %rel_path,
        entries = listing.entries.len(),
        "peer directory browsed"
    );;
    Ok(RemoteBrowseDto {
        entries: listing
            .entries
            .into_iter()
            .map(|entry| RemoteEntryDto {
                name: entry.name,
                is_dir: entry.is_dir,
                size: entry.size,
            })
            .collect(),
        filtered: listing.filtered,
    })
}

/// 从可信对端拉取多个文件到本机下载目录（逐文件独立会话，顺序执行）
///
/// 免协商直取（用户主动获取即放行）；每个文件预登记一条接收任务行，
/// 进度/终态/取消复用 issue 10 任务体系；中断后重试同一文件自动断点续传。
/// 返回成功入队的文件数。
#[tauri::command]
pub async fn pull_peer_files(
    app: AppHandle,
    node_id: String,
    dir_id: String,
    files: Vec<RemotePullFileDto>,
) -> crate::Result<usize> {
    if files.is_empty() {
        return Err(crate::AppError::InvalidInput(
            "pull peer files: file list must not be empty".to_string(),
        ));
    }
    if files.len() > PULL_FILES_CAP {
        return Err(crate::AppError::InvalidInput(format!(
            "pull peer files: {} files exceed cap {PULL_FILES_CAP}",
            files.len()
        )));
    }
    // 诊断插桩：拉取编排入口（逐文件会话另有 dial/serve 日志）
    tracing::info!(
        node_id = %node_id,
        dir_id = %dir_id,
        files = files.len(),
        "peer pull requested"
    );
    for file in &files {
        if file.rel_path.trim().is_empty() {
            return Err(crate::AppError::InvalidInput(
                "pull peer files: rel_path must not be empty".to_string(),
            ));
        }
    }

    let parsed = parse_node_id(&node_id)?;
    // 会话参数在锁外解析快照：拨号可达秒级，不阻塞 start/stop
    let (_, config) =
        super::peer_receive::handler_and_config(&app).await.ok_or_else(|| {
            crate::AppError::Internal("peer-net pull failed: node not started".to_string())
        })?;
    let (events, cancel_root) = {
        let state = app.state::<PeerRemoteState>();
        let guard = state.session.lock().await;
        match guard.as_ref() {
            Some(ctx) => (ctx.events.clone(), ctx.cancel_root.clone()),
            None => {
                return Err(crate::AppError::Internal(
                    "peer-net pull failed: node not started".to_string(),
                ))
            }
        }
    };
    let (node, cache) = runtime_snapshot(&app).await.ok_or_else(|| {
        crate::AppError::Internal("peer-net pull failed: node not started".to_string())
    })?;
    let record = cache.get(&parsed).ok_or_else(|| {
        crate::AppError::Internal(format!(
            "peer-net pull failed: peer {node_id} not in discovery cache (offline or unknown)"
        ))
    })?;

    let count = files.len();
    crate::system::error_boundary::spawn_with_error_boundary(
        "peer_remote_pull",
        run_pull_queue(
            app.clone(),
            parsed,
            node,
            record.to_static_peer_record(),
            dir_id,
            files,
            config,
            cancel_root,
            events,
        ),
    );
    Ok(count)
}

/// 顺序执行拉取队列：每文件独立 batch_id + 独立连接 + 预登记任务行；
/// 单文件失败不阻断后续（任务行如实落 failed，重试即断点续传）
///
/// 取消令牌为本代会话根令牌的 child——节点停止（clear_state）时全部
/// 在途与后续拉取一并中止。
#[allow(clippy::too_many_arguments)]
async fn run_pull_queue(
    app: AppHandle,
    peer: NodeId,
    node: bedcode_peer_net::PeerNetNode,
    record: bedcode_peer_net::StaticPeerRecord,
    dir_id: String,
    files: Vec<RemotePullFileDto>,
    config: bedcode_peer_net::TransferConfig,
    cancel_root: bedcode_peer_net::CancelToken,
    events: mpsc::Sender<TransferEvent>,
) {
    let state = app.state::<PeerRemoteState>();
    for (index, file) in files.into_iter().enumerate() {
        if cancel_root.is_cancelled() {
            tracing::info!("remote pull queue aborted by node stop");
            return;
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let batch_id = format!("pull-{nanos}-{index}");
        let token = CancelToken::child(&cancel_root);
        state
            .pulls
            .lock()
            .expect("peer remote pulls lock poisoned")
            .insert(batch_id.clone(), token.clone());

        // 任务行先于会话登记：首个 Progress 到达前 UI 即呈现进行中
        super::peer_receive::register_remote_pull(
            &app,
            peer.clone(),
            batch_id.clone(),
            file.rel_path.clone(),
            file.size,
        );

        match node.dial(&record).await {
            Ok(conn) => {
                let _ = pull_shared_file(
                    conn,
                    &dir_id,
                    &file.rel_path,
                    &batch_id,
                    config.clone(),
                    events.clone(),
                    token,
                )
                .await;
            }
            Err(e) => {
                tracing::warn!(batch_id = %batch_id, rel = %file.rel_path, "pull dial failed: {e}");
                super::peer_receive::fail_task(&app, &batch_id, format!("dial failed: {e}"));
            }
        }
        state
            .pulls
            .lock()
            .expect("peer remote pulls lock poisoned")
            .remove(&batch_id);
    }
}
