//! 共享目录浏览/拉取会话（issue 07）：注册表 + 服务端分发 + 客户端 API。
//!
//! ## 会话形态（与 push 传输同一连接协议族）
//!
//! 可信连接的首帧决定会话类型（[`SharedDirHandler`] 分发）：
//!
//! ```text
//! 浏览: A ── BrowseRequest{dir_id,rel} ──▶ B   校验信任(闸门已保证)→路径安全→列目录
//!       A ◀── BrowseResponse{entries} ─── B   条目已按「目录优先、按名排序」排好
//! 拉取: A ── PullRequest{dir_id,rel} ───▶ B   解析文件 → 以该文件发标准 Offer
//!       (后续 StartFile/Data/FileDone/BatchDone 与 issue 05 push 完全同构，
//!        B=发送角色、A=接收角色；A 端策略恒放行——拉取是用户主动获取)
//! ```
//!
//! 复用而非另起炉灶：拉取的接收半程直接调 [`crate::transfer::receive_files_after_accept`]
//! （断点真源/取消/落位钩子全继承），服务端推流循环镜像 `drive_send` 的
//! 「拆读写半 + 取消感知」骨架。只读约束是结构性的：线协议不存在任何指向
//! 暴露端的写语义帧，路径安全由 [`resolve_rel_path`] 在服务端强制。
//!
//! ## SAF 缝（平台无关边界）
//!
//! crate 不感知 ContentResolver：SAF 根的列目录与读取经宿主注入的
//! [`SharedSafAccess`] 完成。移动端适配层用既有 `SafIo`（list_tree/open_stream）
//! 实现本缝并沿用其无头测试惯例（fake 注入，见 tests/shared_dirs.rs）。

pub mod registry;

pub use registry::{
    resolve_rel_path, sort_entries, BUILTIN_DOWNLOADS_ID, SharedDirEntry, SharedDirRoot,
    SharedDirStore,
};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::NodeId;
use crate::transfer::message::{
    self, CancelOrigin, IncomingFrame, TransferFrame, TRANSFER_PROTOCOL_VERSION,
};
use crate::transfer::{
    emit, next_frame_or_cancel, proto_violation, receive_files_after_accept, run_receive, sess_io,
    CancelToken, FileMeta, RateTracker, TerminalState, TransferConfig, TransferEvent,
};
use crate::transport::{Connection, ConnectionHandler, HandlerFuture};

/// 首帧与会话级等待超时（镜像 transfer 的 OFFER_TIMEOUT：信任放行后对端应立即发起请求）
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// ==================== SAF 访问缝 ====================

/// 适配层「目标不存在」错误的 detail 前缀（服务端据此回 not-found 拒绝，
/// 与一般读取失败 read-failed 区分；不泄露结构——两者对对端都是拒绝）
pub const SAF_NOT_FOUND_PREFIX: &str = "not-found";

/// 构造适配层「目标不存在」错误（[`SharedSafAccess`] 实现方使用）
pub fn saf_not_found(detail: impl std::fmt::Display) -> crate::error::PeerNetError {
    crate::error::PeerNetError::TransferProtocol {
        role: "saf",
        detail: format!("{SAF_NOT_FOUND_PREFIX}: {detail}"),
    }
}

/// 判定一个 crate 错误是否为适配层报告的「目标不存在」
pub(crate) fn is_saf_not_found(e: &crate::error::PeerNetError) -> bool {
    match e {
        crate::error::PeerNetError::TransferProtocol { detail, .. } => {
            detail.starts_with(SAF_NOT_FOUND_PREFIX)
        }
        _ => false,
    }
}

/// SAF 目录树顺序读取器（宿主适配层产出；crate 只按块定位读）
///
/// `read_at` 为同步阻塞调用（Android Kotlin 桥惯例），引擎经 spawn_blocking
/// 调度避免卡住异步运行时；EOF 返回空 Vec。
pub trait SeqReader: Send + Sync {
    /// 文件总字节数
    fn size(&self) -> u64;

    /// 定位读一块（至多 `cap` 字节）；EOF 返回 Ok(vec![])
    fn read_at(&self, offset: u64, cap: usize) -> std::io::Result<Vec<u8>>;
}

/// SAF 共享目录访问缝（宿主实现；crate 侧测试注入 fake）
///
/// 相对路径已过 [`resolve_rel_path`] 清洗（分量列表形态由实现方自行重组）；
/// 「目标不存在」以 [`saf_not_found`] 报告，其余失败为一般错误（read-failed）。
pub trait SharedSafAccess: Send + Sync {
    /// 列出 tree_uri 下 rel 相对路径目录的子条目（排序由引擎统一做）
    fn list_dir(&self, tree_uri: &str, rel: &str) -> crate::Result<Vec<crate::transfer::batch::DirEntry>>;

    /// 打开 rel 指向的文件为顺序读取源（不存在/是目录以 [`saf_not_found`] 报告）
    fn open_read(&self, tree_uri: &str, rel: &str) -> crate::Result<Arc<dyn SeqReader>>;

    /// 列目录结果可能因本端存储权限被过滤的提示位（issue 11）
    ///
    /// 默认 false；移动端宿主在未授予「所有文件访问权限」等受限态覆写为 true，
    /// 服务端仅在空列表场景把该提示随 BrowseResponse 告知浏览方（沿用既有
    /// notice 语义，不区分「真空目录」与「被过滤」之外的情况）。
    fn may_hide_entries(&self) -> bool {
        false
    }
}

// ==================== 服务端 ====================

/// 共享目录 + push 接收的复合连接处理器（两端宿主的统一装配形状）
///
/// 首帧分流：`Offer` → issue 05/06 接收管线（策略门 + 数据面）；
/// `BrowseRequest` / `PullRequest` → 本模块 serve 流程；其余首帧按协议违规
/// 结束。终态事件沿用 [`TransferEvent::Terminal`] 上报（浏览无批语义，
/// batch_id 用请求的 dir_id 承载便于日志关联）。
pub struct SharedDirHandler {
    store: Arc<SharedDirStore>,
    saf: Option<Arc<dyn SharedSafAccess>>,
    /// 配置/急停/会话表共享核：连接 future 需 'static，经 Arc 移入
    inner: Arc<SharedHandlerInner>,
    events: mpsc::Sender<TransferEvent>,
}

/// [`SharedDirHandler`] 的跨会话共享状态
struct SharedHandlerInner {
    /// 传输配置（RwLock：宿主可在节点运行中热更新策略/落点，issue 10；
    /// 每条连接建立时取一次快照，在途会话不受后续变更影响）
    transfer: std::sync::RwLock<TransferConfig>,
    /// 全局急停令牌：触发即取消当前与未来全部会话
    cancel: CancelToken,
    /// 活动会话登记表（batch_id/dir_id → 子取消令牌）：按批取消入口
    sessions: std::sync::Mutex<HashMap<String, CancelToken>>,
}

impl SharedDirHandler {
    /// 构造复合处理器（store 必注入；saf 仅移动端提供）
    pub fn new(
        store: Arc<SharedDirStore>,
        saf: Option<Arc<dyn SharedSafAccess>>,
        transfer: TransferConfig,
        events: mpsc::Sender<TransferEvent>,
    ) -> Self {
        Self {
            store,
            saf,
            inner: Arc::new(SharedHandlerInner {
                transfer: std::sync::RwLock::new(transfer),
                cancel: CancelToken::new(),
                sessions: std::sync::Mutex::new(HashMap::new()),
            }),
            events,
        }
    }

    /// 引擎事件通道发送端句柄（issue 11）：宿主客户端拉取会话经同一通道
    /// 入账接收任务表，进度/终态与 push 接收共用既有事件管线。
    pub fn event_sender(&self) -> mpsc::Sender<TransferEvent> {
        self.events.clone()
    }

    /// 宿主取消入口的令牌句柄（急停语义：当前与未来全部会话一并取消）
    pub fn cancel_token(&self) -> CancelToken {
        self.inner.cancel.clone()
    }

    /// 热更新传输配置（接收策略 / 落点目录）：设置面变更后调用，下一条
    /// 进入的连接即按新配置分流（issue 10「即时生效」）
    pub fn update_transfer_config(&self, transfer: TransferConfig) {
        if let Ok(mut guard) = self.inner.transfer.write() {
            *guard = transfer;
        }
    }

    /// 按批 ID 取消一条活动会话（push 接收批或 pull 服务流；返回是否命中）。
    /// pending 批不经此路径——询问应答走 OfferPending 的 reply 回执。
    pub fn cancel_transfer(&self, batch_id: &str) -> bool {
        let token = self
            .inner
            .sessions
            .lock()
            .expect("transfer session table lock poisoned")
            .get(batch_id)
            .cloned();
        match token {
            Some(token) => {
                tracing::info!(batch_id = %batch_id, "cancelling peer transfer by host");
                token.cancel();
                true
            }
            None => false,
        }
    }
}

/// 会话登记守卫：构造时把 (key → 令牌) 写入共享表，drop 时摘除
/// （会话正常终态、协议失败、宿主取消三条退出路径统一覆盖）
struct SessionGuard {
    inner: Arc<SharedHandlerInner>,
    key: String,
}

impl SessionGuard {
    fn new(inner: Arc<SharedHandlerInner>, key: String, token: CancelToken) -> Self {
        inner
            .sessions
            .lock()
            .expect("transfer session table lock poisoned")
            .insert(key.clone(), token);
        Self { inner, key }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.inner.sessions.lock() {
            sessions.remove(&self.key);
        }
    }
}

impl ConnectionHandler for SharedDirHandler {
    fn handle(&self, mut conn: Connection) -> HandlerFuture {
        // 对端身份在消费连接前取出：事件与任务面据此展示「谁在传」
        let remote = conn.peer_node_id().clone();
        let store = Arc::clone(&self.store);
        let saf = self.saf.clone();
        let events = self.events.clone();
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            // ---- 首帧分流（信任放行后应立即到达）----
            let first = match tokio::time::timeout(REQUEST_TIMEOUT, message::read_frame(&mut conn))
                .await
            {
                Err(_) => Err(proto_violation("share", "timed out waiting for first frame")),
                Ok(Err(e)) => Err(sess_io("share", e)),
                Ok(Ok(frame)) => Ok(frame),
            };
            // 配置快照：本条连接生命周期内固定，热更新只影响后续连接
            let config = inner
                .transfer
                .read()
                .map(|config| config.clone())
                .unwrap_or_default();
            // 会话子令牌：急停令牌的 child；批 ID 确认后登记供按批取消，
            // 守卫 drop（任何退出路径）时自动摘除登记项
            let cancel = CancelToken::child(&inner.cancel);
            let mut batch_slot: Option<String> = None;
            let state = match first {
                Ok(IncomingFrame::Control(boxed)) => match *boxed {
                    TransferFrame::Offer {
                        protocol_version,
                        batch_id,
                        files,
                        total_size,
                    } => {
                        batch_slot = Some(batch_id.clone());
                        let _guard =
                            SessionGuard::new(Arc::clone(&inner), batch_id.clone(), cancel.clone());
                        // 原样回传预读 Offer，接收管线内部完成版本校验与批登记
                        let pre = IncomingFrame::Control(Box::new(TransferFrame::Offer {
                            protocol_version,
                            batch_id,
                            files,
                            total_size,
                        }));
                        run_receive(
                            conn,
                            &config,
                            &events,
                            &cancel,
                            remote.clone(),
                            &mut batch_slot,
                            Some(pre),
                        )
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("receive session failed: {e}");
                            TerminalState::Failed {
                                detail: e.to_string(),
                            }
                        })
                    }
                    TransferFrame::BrowseRequest {
                        protocol_version,
                        dir_id,
                        rel_path,
                    } => {
                        batch_slot = Some(dir_id.clone());
                        match serve_browse(conn, &store, saf.as_ref(), protocol_version, &dir_id, &rel_path)
                            .await
                        {
                            Ok(()) => TerminalState::Completed,
                            Err(e) => {
                                tracing::warn!("browse session failed: {e}");
                                TerminalState::Failed {
                                    detail: e.to_string(),
                                }
                            }
                        }
                    }
                    TransferFrame::PullRequest {
                        protocol_version,
                        dir_id,
                        rel_path,
                    } => {
                        batch_slot = Some(dir_id.clone());
                        let _guard =
                            SessionGuard::new(Arc::clone(&inner), dir_id.clone(), cancel.clone());
                        match serve_pull(
                            conn,
                            &store,
                            saf.as_ref(),
                            &events,
                            &cancel,
                            remote.clone(),
                            protocol_version,
                            &dir_id,
                            &rel_path,
                        )
                        .await
                        {
                            Ok(state) => state,
                            Err(e) => {
                                tracing::warn!("pull serve failed: {e}");
                                TerminalState::Failed {
                                    detail: e.to_string(),
                                }
                            }
                        }
                    }
                    TransferFrame::RootsRequest { protocol_version } => {
                        batch_slot = Some("roots".to_string());
                        match serve_roots(conn, &store, protocol_version).await {
                            Ok(()) => TerminalState::Completed,
                            Err(e) => {
                                tracing::warn!("roots session failed: {e}");
                                TerminalState::Failed {
                                    detail: e.to_string(),
                                }
                            }
                        }
                    }
                    other => {
                        let _ = message::write_control(
                            &mut conn,
                            &TransferFrame::Decision {
                                accepted: false,
                                reason: Some(crate::transfer::RejectReason::PolicyDenied),
                            },
                        )
                        .await;
                        TerminalState::Failed {
                            detail: format!("unexpected first frame: {other:?}"),
                        }
                    }
                },
                Ok(IncomingFrame::Data(_)) => TerminalState::Failed {
                    detail: "first share frame must be control".to_string(),
                },
                Err(e) => TerminalState::Failed {
                    detail: e.to_string(),
                },
            };
            emit(
                &events,
                TransferEvent::Terminal {
                    remote,
                    batch_id: batch_slot.unwrap_or_default(),
                    state,
                },
            )
            .await;
        })
    }
}

/// 写一条拒绝 Decision（尽力而为：写失败只记日志，连接即将关闭）
async fn write_rejection(
    conn: &mut Connection,
    reason: crate::transfer::RejectReason,
) {
    if let Err(e) = crate::transfer::write_decision(conn, false, Some(reason)).await {
        tracing::debug!("write rejection decision failed: {e}");
    }
}

/// 版本守卫：请求版本大于自身支持 → 报协议违规（调用方先回 PolicyDenied 拒绝帧）
fn check_version(protocol_version: u32) -> crate::Result<()> {
    if protocol_version > TRANSFER_PROTOCOL_VERSION {
        return Err(proto_violation(
            "share",
            format!(
                "request protocol_version {protocol_version} is newer than supported {TRANSFER_PROTOCOL_VERSION}"
            ),
        ));
    }
    Ok(())
}

/// 浏览服务流程：校验版本/条目存在性/路径安全 → 列目录 → 排序上线 → 应答
///
/// 任何失败以 Decision{false, not-found|read-failed|policy-denied} 回话后终止；
/// 「不存在」与「越界」一律归 not-found，不向对端泄露目录结构信息。
async fn serve_browse(
    mut conn: Connection,
    store: &SharedDirStore,
    saf: Option<&Arc<dyn SharedSafAccess>>,
    protocol_version: u32,
    dir_id: &str,
    rel_path: &str,
) -> crate::Result<()> {
    const ROLE: &str = "share";
    if let Err(e) = check_version(protocol_version) {
        write_rejection(&mut conn, crate::transfer::RejectReason::PolicyDenied).await;
        return Err(e);
    }

    let entries = list_shared_dir(store, saf, dir_id, rel_path).await;
    let entries = match entries {
        Ok(entries) => entries,
        Err(ListError::NotFound(reason)) => {
            write_rejection(&mut conn, reason).await;
            return Ok(());
        }
        Err(ListError::Engine(e)) => return Err(e),
    };

    // 空列表 + 宿主提示可能被权限过滤：置提示位随应答告知浏览方（issue 11）
    let filtered = entries.is_empty() && saf.is_some_and(|saf| saf.may_hide_entries());
    message::write_control(
        &mut conn,
        &TransferFrame::BrowseResponse {
            entries: sort_entries(&entries),
            filtered,
        },
    )
    .await
    .map_err(|e| sess_io(ROLE, e))?;
    tracing::info!(dir_id = %dir_id, rel = %rel_path, count = entries.len(), "shared dir listed");
    Ok(())
}

/// 列目录错误分流（NotFound 承载线上拒绝原因；Engine 为本端故障）
enum ListError {
    NotFound(crate::transfer::RejectReason),
    Engine(crate::error::PeerNetError),
}

/// 共享根清单服务流程：校验版本 → 注册表快照（含内置条目，按注册序）→ 应答
async fn serve_roots(
    mut conn: Connection,
    store: &SharedDirStore,
    protocol_version: u32,
) -> crate::Result<()> {
    const ROLE: &str = "share";
    if let Err(e) = check_version(protocol_version) {
        return Err(e);
    }
    let dirs = store
        .list()
        .into_iter()
        .map(|entry| SharedRootMeta {
            id: entry.id,
            name: entry.name,
        })
        .collect();
    message::write_control(&mut conn, &TransferFrame::RootsResponse { dirs })
        .await
        .map_err(|e| sess_io(ROLE, e))?;
    Ok(())
}

/// 解析共享目录条目并列出子项（Fs 走 tokio fs；Saf 走宿主缝）
async fn list_shared_dir(
    store: &SharedDirStore,
    saf: Option<&Arc<dyn SharedSafAccess>>,
    dir_id: &str,
    rel_path: &str,
) -> Result<Vec<crate::transfer::batch::DirEntry>, ListError> {
    use crate::transfer::RejectReason;

    let entry = store.get(dir_id).ok_or(ListError::NotFound(RejectReason::NotFound))?;
    let components =
        resolve_rel_path(rel_path).ok_or(ListError::NotFound(RejectReason::NotFound))?;

    match &entry.root {
        SharedDirRoot::Fs { path } => {
            let mut dir = path.clone();
            for part in &components {
                dir.push(part);
            }
            let mut reader = tokio::fs::read_dir(&dir).await.map_err(|e| {
                // 不区分「不存在」与「权限/IO」——暴露面不泄露结构
                tracing::debug!(dir = %dir.display(), error = %e, "list shared fs dir failed");
                ListError::NotFound(RejectReason::NotFound)
            })?;
            let mut out = Vec::new();
            while let Some(item) = reader.next_entry().await.map_err(|e| {
                ListError::Engine(sess_io("share", e))
            })? {
                let name = item.file_name().to_string_lossy().into_owned();
                let meta = item.metadata().await;
                let (is_dir, size) = match meta {
                    Ok(m) => (m.is_dir(), m.len()),
                    Err(_) => (false, 0),
                };
                out.push(crate::transfer::batch::DirEntry::new(name, is_dir, size));
            }
            Ok(out)
        }
        SharedDirRoot::Saf { tree_uri } => {
            let Some(saf) = saf else {
                tracing::warn!("browse request hit saf root without saf access attached");
                return Err(ListError::NotFound(RejectReason::ReadFailed));
            };
            match saf.list_dir(tree_uri, rel_path) {
                Ok(entries) => Ok(entries),
                Err(e) if is_saf_not_found(&e) => {
                    Err(ListError::NotFound(RejectReason::NotFound))
                }
                Err(e) => Err(ListError::Engine(e)),
            }
        }
    }
}

/// 拉取服务流程：解析文件 → 标准 Offer → 推流（镜像 drive_send 的拆半骨架）→ 收尾
///
/// 源支持两类：Fs 直接 tokio 文件顺序读；Saf 经 [`SeqReader`] 定位读
/// （spawn_blocking 桥接同步 Kotlin 调用）。对端 StartFile 声明的偏移被尊重
/// ——pull 重试时断点续传与 push 同款生效。
#[allow(clippy::too_many_arguments)]
async fn serve_pull(
    mut conn: Connection,
    store: &SharedDirStore,
    saf: Option<&Arc<dyn SharedSafAccess>>,
    events: &mpsc::Sender<TransferEvent>,
    cancel: &CancelToken,
    remote: NodeId,
    protocol_version: u32,
    dir_id: &str,
    rel_path: &str,
) -> crate::Result<TerminalState> {
    const ROLE: &str = "share-sender";

    if let Err(e) = check_version(protocol_version) {
        write_rejection(&mut conn, crate::transfer::RejectReason::PolicyDenied).await;
        return Err(e);
    }

    // ---- 解析目标文件（失败一律 Decision{false} 后按 Failed 终态收场）----
    let resolved = match resolve_pull_target(store, saf, dir_id, rel_path).await {
        Ok(resolved) => resolved,
        Err(PullResolveError::NotFound(reason)) => {
            write_rejection(&mut conn, reason).await;
            return Ok(TerminalState::Failed {
                detail: format!("pull target rejected ({}) for '{rel_path}'", reason.as_str()),
            });
        }
        Err(PullResolveError::Engine(e)) => return Err(e),
    };

    let meta = FileMeta::new(rel_path.to_string(), resolved.size);
    let batch_id = format!(
        "pull-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    // ---- 标准 Offer（拉取免协商：对端收到即进入放行数据面）----
    message::write_control(
        &mut conn,
        &TransferFrame::Offer {
            protocol_version: TRANSFER_PROTOCOL_VERSION,
            batch_id: batch_id.clone(),
            files: vec![meta.clone()],
            total_size: resolved.size,
        },
    )
    .await
    .map_err(|e| sess_io(ROLE, e))?;

    // ---- 拆读写半 + 读半转发任务（drive_send 同款：推流期间取消即时可见）----
    let (mut rd, mut wr) = tokio::io::split(conn);
    let (frame_tx, mut frame_rx) = mpsc::channel::<std::io::Result<IncomingFrame>>(16);
    let reader_task = tokio::spawn(async move {
        loop {
            match message::read_frame(&mut rd).await {
                Ok(frame) => {
                    if frame_tx.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = frame_tx.send(Err(e)).await;
                    break;
                }
            }
        }
    });

    // ---- 等接收端声明起点（断点真源）----
    let start = next_frame_or_cancel(&mut frame_rx, cancel).await?;
    let outcome = match start {
        None => {
            let _ = message::write_control(
                &mut wr,
                &TransferFrame::Cancel { by: CancelOrigin::Sender },
            )
            .await;
            Ok(TerminalState::Cancelled { by_peer: false })
        }
            Some(IncomingFrame::Control(boxed)) => match *boxed {
                TransferFrame::StartFile { index: 0, offset } if offset <= resolved.size => {
                    stream_pull_source(
                        &mut wr,
                        &mut frame_rx,
                        resolved.source,
                        offset,
                        &meta,
                        &batch_id,
                        resolved.size,
                        events,
                        cancel,
                        remote,
                    )
                    .await
                }
            TransferFrame::StartFile { index: 0, offset } => Err(proto_violation(
                ROLE,
                format!("peer reported offset {offset} beyond declared size {}", resolved.size),
            )),
            TransferFrame::Cancel { .. } => Ok(TerminalState::Cancelled { by_peer: true }),
            other => Err(proto_violation(
                ROLE,
                format!("expected start_file for pull, got {other:?}"),
            )),
        },
        Some(IncomingFrame::Data(_)) => Err(proto_violation(
            ROLE,
            "data frame is receiver-to-sender only",
        )),
    };

    reader_task.abort();

    match &outcome {
        Ok(TerminalState::Completed) => {
            tracing::info!(batch_id = %batch_id, dir_id = %dir_id, rel = %rel_path, "file pulled by peer");
        }
        other => {
            tracing::debug!(batch_id = %batch_id, outcome = ?other, "pull serve ended");
        }
    }
    outcome
}

enum PullResolveError {
    NotFound(crate::transfer::RejectReason),
    Engine(crate::error::PeerNetError),
}

struct ResolvedPullTarget {
    size: u64,
    source: PullSource,
}

enum PullSource {
    Fs(tokio::fs::File),
    Remote(Arc<dyn SeqReader>),
}

impl PullSource {
    /// 从 offset 起读一块至 buf（返回实际字节数；EOF 为 0）
    async fn read_chunk(&mut self, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
        match self {
            PullSource::Fs(file) => {
                // 文件已在打开后 seek 到起点；续传重试场景显式再 seek 保证幂等
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                file.read(buf).await
            }
            PullSource::Remote(reader) => {
                let reader = Arc::clone(reader);
                let cap = buf.len();
                let chunk = tokio::task::spawn_blocking(move || reader.read_at(offset, cap))
                    .await
                    .map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, format!("saf read task failed: {e}"))
                    })??;
                let n = chunk.len().min(buf.len());
                buf[..n].copy_from_slice(&chunk[..n]);
                Ok(n)
            }
        }
    }
}

/// 解析拉取目标：条目存在 + 路径安全 + 是可读文件
async fn resolve_pull_target(
    store: &SharedDirStore,
    saf: Option<&Arc<dyn SharedSafAccess>>,
    dir_id: &str,
    rel_path: &str,
) -> Result<ResolvedPullTarget, PullResolveError> {
    use crate::transfer::RejectReason;

    let entry = store.get(dir_id).ok_or(PullResolveError::NotFound(RejectReason::NotFound))?;
    let components =
        resolve_rel_path(rel_path).ok_or(PullResolveError::NotFound(RejectReason::NotFound))?;

    match &entry.root {
        SharedDirRoot::Fs { path } => {
            let mut target = path.clone();
            for part in &components {
                target.push(part);
            }
            let meta = tokio::fs::metadata(&target).await.map_err(|e| {
                tracing::debug!(path = %target.display(), error = %e, "resolve pull target failed");
                PullResolveError::NotFound(RejectReason::NotFound)
            })?;
            if !meta.is_file() {
                return Err(PullResolveError::NotFound(RejectReason::NotFound));
            }
            let file = tokio::fs::OpenOptions::new()
                .read(true)
                .open(&target)
                .await
                .map_err(|e| {
                    tracing::warn!(path = %target.display(), error = %e, "open pull source failed");
                    PullResolveError::NotFound(RejectReason::ReadFailed)
                })?;
            Ok(ResolvedPullTarget {
                size: meta.len(),
                source: PullSource::Fs(file),
            })
        }
        SharedDirRoot::Saf { tree_uri } => {
            let Some(saf) = saf else {
                tracing::warn!("pull request hit saf root without saf access attached");
                return Err(PullResolveError::NotFound(RejectReason::ReadFailed));
            };
            let reader = saf.open_read(tree_uri, rel_path).map_err(|e| match e {
                crate::error::PeerNetError::TransferProtocol { detail, .. }
                    if detail.contains("not-found") =>
                {
                    PullResolveError::NotFound(RejectReason::NotFound)
                }
                other => PullResolveError::Engine(other),
            })?;
            Ok(ResolvedPullTarget {
                size: reader.size(),
                source: PullSource::Remote(reader),
            })
        }
    }
}

/// 单文件推流主循环（drive_send 内环的单文件特化：等取消/对端取消/读源推流），
/// 结束后等待 FileDone 与 BatchDone
#[allow(clippy::too_many_arguments)]
async fn stream_pull_source<W>(
    wr: &mut W,
    frame_rx: &mut mpsc::Receiver<std::io::Result<IncomingFrame>>,
    mut source: PullSource,
    start_offset: u64,
    meta: &FileMeta,
    batch_id: &str,
    total_size: u64,
    events: &mpsc::Sender<TransferEvent>,
    cancel: &CancelToken,
    remote: NodeId,
) -> crate::Result<TerminalState>
where
    W: tokio::io::AsyncWrite + Unpin + Send,
{
    const ROLE: &str = "share-sender";

    let chunk_capacity = 64 * 1024usize; // 与缺省 chunk_size 一致；仅作读缓冲上限
    let mut buf = vec![0u8; chunk_capacity];
    let mut transferred_total: u64 = start_offset;
    let mut rate = RateTracker::new();
    rate.sync_base(transferred_total);

    let mut offset = start_offset;
    let mut remaining = total_size - start_offset;
    while remaining > 0 {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                // 本端取消：告知对端；对端保留 .part 断点
                let _ = message::write_control(
                    wr,
                    &TransferFrame::Cancel { by: CancelOrigin::Sender },
                )
                .await;
                return Ok(TerminalState::Cancelled { by_peer: false });
            }
            frame = frame_rx.recv() => {
                let incoming = match frame {
                    Some(result) => result.map_err(|e| sess_io(ROLE, e))?,
                    None => {
                        return Err(sess_io(
                            ROLE,
                            std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "peer closed connection mid-pull",
                            ),
                        ))
                    }
                };
                match incoming {
                    IncomingFrame::Control(boxed) => match *boxed {
                        TransferFrame::Cancel { .. } => {
                            return Ok(TerminalState::Cancelled { by_peer: true });
                        }
                    other => {
                            return Err(proto_violation(
                                ROLE,
                                format!("unexpected control frame while streaming: {other:?}"),
                            ))
                        }
                    },
                    IncomingFrame::Data(_) => {
                        return Err(proto_violation(ROLE, "data frame is receiver-to-sender only"))
                    }
                }
            }
            read = source.read_chunk(&mut buf, offset) => {
                let n = read.map_err(|e| sess_io(ROLE, e))?;
                if n == 0 {
                    return Err(sess_io(
                        ROLE,
                        std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            format!("source truncated mid-pull: {}", meta.path),
                        ),
                    ));
                }
                message::write_data(wr, &buf[..n])
                    .await
                    .map_err(|e| sess_io(ROLE, e))?;
                remaining -= n as u64;
                offset += n as u64;
                transferred_total += n as u64;
                emit(
                    events,
                    TransferEvent::Progress {
                        remote: remote.clone(),
                        batch_id: batch_id.to_string(),
                        transferred: transferred_total,
                        total: total_size,
                        rate_bps: rate.sample(transferred_total),
                    },
                )
                .await;
            }
        }
    }

    // ---- 等落位确认 ----
    let done = next_frame_or_cancel(frame_rx, cancel).await?;
    match done {
        None => {
            let _ = message::write_control(
                wr,
                &TransferFrame::Cancel { by: CancelOrigin::Sender },
            )
            .await;
            return Ok(TerminalState::Cancelled { by_peer: false });
        }
        Some(IncomingFrame::Control(boxed)) => match *boxed {
            TransferFrame::FileDone { index: 0 } => {}
            TransferFrame::Cancel { .. } => {
                return Ok(TerminalState::Cancelled { by_peer: true });
            }
            other => {
                return Err(proto_violation(
                    ROLE,
                    format!("expected file_done after pull stream, got {other:?}"),
                ))
            }
        },
        Some(IncomingFrame::Data(_)) => {
            return Err(proto_violation(ROLE, "data frame is receiver-to-sender only"))
        }
    }

    // ---- 批完成确认 ----
    let done = next_frame_or_cancel(frame_rx, cancel).await?;
    match done {
        Some(IncomingFrame::Control(boxed)) if matches!(*boxed, TransferFrame::BatchDone {}) => {
            Ok(TerminalState::Completed)
        }
        Some(other) => Err(proto_violation(
            ROLE,
            format!("expected batch_done after pulled file, got {other:?}"),
        )),
        None => {
            let _ = message::write_control(
                wr,
                &TransferFrame::Cancel { by: CancelOrigin::Sender },
            )
            .await;
            Ok(TerminalState::Cancelled { by_peer: false })
        }
    }
}

// ==================== 客户端 ====================

/// 共享根元数据（RootsResponse 线载荷单条）：id 供寻址、name 供展示
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SharedRootMeta {
    /// 共享目录条目 ID（Browse/Pull 按此寻址）
    pub id: String,
    /// 展示名（注册时用户可见名）
    pub name: String,
}

/// 列可信对端暴露中的共享根清单（单请求会话；issue 11）
///
/// 浏览方先取此清单获得 dir_id，再逐根 [`browse_shared_dir`] 下钻。
pub async fn list_shared_roots(mut conn: Connection) -> crate::Result<Vec<SharedRootMeta>> {
    const ROLE: &str = "browser";

    message::write_control(
        &mut conn,
        &TransferFrame::RootsRequest {
            protocol_version: TRANSFER_PROTOCOL_VERSION,
        },
    )
    .await
    .map_err(|e| sess_io(ROLE, e))?;

    let reply = tokio::time::timeout(REQUEST_TIMEOUT, message::read_control(&mut conn))
        .await
        .map_err(|_| proto_violation(ROLE, "timed out waiting for roots response"))?
        .map_err(|e| sess_io(ROLE, e))?;

    match reply {
        TransferFrame::RootsResponse { dirs } => {
            let _ = tokio::io::AsyncWriteExt::shutdown(&mut conn).await;
            Ok(dirs)
        }
        other => Err(proto_violation(
            ROLE,
            format!("unexpected reply to roots request: {other:?}"),
        )),
    }
}

/// 浏览应答（issue 11）：条目 + 「可能被权限过滤」提示位
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseListing {
    /// 子条目（已按「目录优先、按名排序」排好）
    pub entries: Vec<crate::transfer::batch::DirEntry>,
    /// 暴露端提示列表可能不全（Android 存储权限过滤；沿用既有 notice 语义）
    pub filtered: bool,
}

/// 浏览可信对端的共享目录（单请求会话）
///
/// 对端拒绝（不存在/越界/读取失败）映射为带原因说明的错误。
pub async fn browse_shared_dir(
    mut conn: Connection,
    dir_id: &str,
    rel_path: &str,
) -> crate::Result<BrowseListing> {
    const ROLE: &str = "browser";

    message::write_control(
        &mut conn,
        &TransferFrame::BrowseRequest {
            protocol_version: TRANSFER_PROTOCOL_VERSION,
            dir_id: dir_id.to_string(),
            rel_path: rel_path.to_string(),
        },
    )
    .await
    .map_err(|e| sess_io(ROLE, e))?;

    let reply = tokio::time::timeout(REQUEST_TIMEOUT, message::read_control(&mut conn))
        .await
        .map_err(|_| proto_violation(ROLE, "timed out waiting for browse response"))?
        .map_err(|e| sess_io(ROLE, e))?;

    match reply {
        TransferFrame::BrowseResponse { entries, filtered } => {
            let _ = tokio::io::AsyncWriteExt::shutdown(&mut conn).await;
            Ok(BrowseListing { entries, filtered })
        }
        TransferFrame::Decision {
            accepted: false,
            reason,
        } => {
            let reason = reason.unwrap_or(crate::transfer::RejectReason::NotFound);
            Err(proto_violation(
                ROLE,
                format!("browse '{rel_path}' in dir '{dir_id}' rejected: {}", reason.as_str()),
            ))
        }
        other => Err(proto_violation(
            ROLE,
            format!("unexpected reply to browse request: {other:?}"),
        )),
    }
}

/// 拉取可信对端共享目录内的单个文件到本机下载目录（接收角色会话主入口）
///
/// 免协商：用户主动获取即放行（不走接收策略询问）。落位钩子（config.landing）、
/// 断点续传与 push 接收完全同构。终态同时经 `events` 上报与返回值给出。
///
/// `batch_id` 由调用方指定（issue 11）：宿主需在会话发起前以同 ID 预登记
/// 接收任务行，使 Progress/Terminal 事件与按批取消能命中既有任务表。
pub async fn pull_shared_file(
    conn: Connection,
    dir_id: &str,
    rel_path: &str,
    batch_id: &str,
    config: TransferConfig,
    events: mpsc::Sender<TransferEvent>,
    cancel: CancelToken,
) -> crate::Result<TerminalState> {
    let remote = conn.peer_node_id().clone();

    let state =
        match run_pull_session(conn, dir_id, rel_path, batch_id, &config, &events, &cancel).await
        {
            Ok(state) => state,
            Err(e) => {
                tracing::warn!("pull session failed: {e}");
                TerminalState::Failed {
                    detail: e.to_string(),
                }
            }
        };
    emit(
        &events,
        TransferEvent::Terminal {
            remote,
            batch_id: batch_id.to_string(),
            state: state.clone(),
        },
    )
    .await;
    Ok(state)
}

/// 拉取会话核心：PullRequest → Offer（免协商）→ 放行数据面
async fn run_pull_session(
    conn: Connection,
    dir_id: &str,
    rel_path: &str,
    batch_id: &str,
    config: &TransferConfig,
    events: &mpsc::Sender<TransferEvent>,
    cancel: &CancelToken,
) -> crate::Result<TerminalState> {
    const ROLE: &str = "puller";
    let remote = conn.peer_node_id().clone();
    let mut conn = conn;

    message::write_control(
        &mut conn,
        &TransferFrame::PullRequest {
            protocol_version: TRANSFER_PROTOCOL_VERSION,
            dir_id: dir_id.to_string(),
            rel_path: rel_path.to_string(),
        },
    )
    .await
    .map_err(|e| sess_io(ROLE, e))?;

    let offer = tokio::time::timeout(REQUEST_TIMEOUT, message::read_control(&mut conn))
        .await
        .map_err(|_| proto_violation(ROLE, "timed out waiting for pull offer"))?
        .map_err(|e| sess_io(ROLE, e))?;

    let (files, total_size) = match offer {
        TransferFrame::Offer {
            protocol_version,
            batch_id: peer_batch,
            files,
            total_size,
        } => {
            if protocol_version > TRANSFER_PROTOCOL_VERSION {
                return Err(proto_violation(
                    ROLE,
                    format!("offer protocol_version {protocol_version} is newer than supported"),
                ));
            }
            // 对端批 ID 与本地生成的不同不影响语义：落位 .part 命名以对端为准
            let _ = peer_batch;
            (files, total_size)
        }
        TransferFrame::Decision {
            accepted: false,
            reason,
        } => {
            let reason = reason.unwrap_or(crate::transfer::RejectReason::NotFound);
            return Ok(TerminalState::Rejected { reason });
        }
        other => {
            return Err(proto_violation(
                ROLE,
                format!("unexpected reply to pull request: {other:?}"),
            ))
        }
    };

    receive_files_after_accept(
        conn,
        config,
        events,
        cancel,
        remote,
        batch_id,
        &files,
        total_size,
    )
    .await
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// fake SeqReader：内存内容按窗口切片，验证 Remote 源的定位读契约
    struct MemReader {
        content: Vec<u8>,
    }

    impl SeqReader for MemReader {
        fn size(&self) -> u64 {
            self.content.len() as u64
        }

        fn read_at(&self, offset: u64, cap: usize) -> std::io::Result<Vec<u8>> {
            let start = (offset as usize).min(self.content.len());
            let end = (start + cap).min(self.content.len());
            Ok(self.content[start..end].to_vec())
        }
    }

    #[tokio::test]
    async fn remote_source_reads_windowed_chunks_with_eof() {
        let mut source = PullSource::Remote(Arc::new(MemReader {
            content: b"hello shared world".to_vec(),
        }));

        let mut buf = vec![0u8; 6];
        assert_eq!(source.read_chunk(&mut buf, 0).await.expect("chunk@0"), 6);
        assert_eq!(&buf, b"hello ");

        assert_eq!(source.read_chunk(&mut buf, 6).await.expect("chunk@6"), 6);
        assert_eq!(&buf, b"shared");

        // 末段不足一窗：返回剩余字节
        let n = source.read_chunk(&mut buf, 12).await.expect("chunk@12");
        assert_eq!(&buf[..n], b" world");

        // EOF：空返回
        assert_eq!(source.read_chunk(&mut buf, 18).await.expect("eof"), 0);
        assert_eq!(source.read_chunk(&mut buf, 999).await.expect("eof"), 0);
    }

    #[test]
    fn version_guard_rejects_newer_protocol_only() {
        assert!(check_version(TRANSFER_PROTOCOL_VERSION).is_ok());
        assert!(check_version(1).is_ok());
        assert!(check_version(TRANSFER_PROTOCOL_VERSION + 1).is_err());
    }
}
