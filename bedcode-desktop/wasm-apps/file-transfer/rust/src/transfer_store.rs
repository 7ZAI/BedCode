//! 传输任务/历史自持存储（issue 13 Phase 3 步骤 3）
//!
//! 插件自有「产品视图」：以引擎 `peer:transfer` / `peer:receive` 全量快照
//! 事件为进度真源，按 batchId merge 进本店；终态归档 + 200 条封顶滚动淘汰；
//! 重启恢复时 running/pending 态条目标注 `interrupted`（原批已死，如实呈现）。
//!
//! **事件归约（传输编排下沉票 2）**：`peer:transfer-event` / `peer:receive-event`
//! 引擎原始事件经 [`reduce_event`] 归约为同一 store 的状态推进——本店自此
//! 成为唯一任务真源（封顶/终态判定/原因码映射单点），快照 merge 退化为
//! 校正 + 对账源（票 3 退役）。原因码（cancelled-by-*）与终态判定只在
//! 本文件实现——宿主不再持有产品语义。
//!
//! 时间戳纪律：全部取自引擎事件载荷（快照 createdAtMs/updatedAtMs 与
//! 原始事件 tsMs），本地无时钟（wasm32-unknown-unknown 禁 std::time）。
//! 纯函数核心与宿主 I/O 分离，cargo test 直测。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 历史封顶：终态条目超过即按 updatedAtMs 最旧先出
pub(crate) const HISTORY_CAP: usize = 200;

/// 重试元数据（仅发起方条目携带）：send 记源路径清单；pull 记共享根 + 文件清单
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum RetryMeta {
    /// 推送发送：原始本地路径（重试时回放 send-files）
    Send { paths: Vec<String> },
    /// 远端拉取：共享根 + 文件清单（重试时回放 pull-files）
    Pull {
        dir_id: String,
        files: Vec<PullFileSpec>,
    },
}

/// 拉取文件规格（retryMeta 回放所需最小集）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PullFileSpec {
    pub rel_path: String,
    #[serde(default)]
    pub size: u64,
}

/// 单条传输条目 —— 引擎 PeerTransferDto camelCase 形状 + 插件扩展字段。
/// 直接作为对外 wire 形状（事件载荷 / 列表命令返回），不再二次翻译。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferEntry {
    pub(crate) batch_id: String,
    #[serde(default)]
    pub(crate) node_id: String,
    #[serde(default)]
    pub(crate) peer_name: String,
    /// `send` | `receive`
    pub(crate) direction: String,
    /// running / pending / completed / failed / rejected / cancelled / interrupted
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) files: Vec<Value>,
    #[serde(default)]
    pub(crate) total_bytes: u64,
    #[serde(default)]
    pub(crate) transferred_bytes: u64,
    #[serde(default)]
    pub(crate) rate_bps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reject_reason: Option<String>,
    #[serde(default)]
    pub(crate) created_at_ms: u64,
    #[serde(default)]
    pub(crate) updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) retry_meta: Option<RetryMeta>,
}

impl TransferEntry {
    fn from_dto(dto: &Value) -> Option<TransferEntry> {
        let batch_id = dto.get("batchId")?.as_str()?.to_string();
        Some(TransferEntry {
            batch_id,
            node_id: dto.get("nodeId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            peer_name: dto.get("peerName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            direction: dto.get("direction").and_then(|v| v.as_str()).unwrap_or("send").to_string(),
            status: dto.get("status").and_then(|v| v.as_str()).unwrap_or("running").to_string(),
            files: dto.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
            total_bytes: dto.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            transferred_bytes: dto.get("transferredBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            rate_bps: dto.get("rateBps").and_then(|v| v.as_f64()).unwrap_or(0.0),
            detail: dto.get("detail").and_then(|v| v.as_str()).map(|s| s.to_string()),
            reject_reason: dto.get("rejectReason").and_then(|v| v.as_str()).map(|s| s.to_string()),
            created_at_ms: dto.get("createdAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
            updated_at_ms: dto.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
            retry_meta: None,
        })
    }

    /// 是否终态（interrupted 视作可重试的终态变体）
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed" | "failed" | "rejected" | "cancelled" | "interrupted"
        )
    }

    /// 是否进行中（含接收待应答）
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "pending" | "paused")
    }
}

/// DTO 解析出口（命令编排层入店用）
pub(crate) fn entry_from_dto(dto: &Value) -> Option<TransferEntry> {
    TransferEntry::from_dto(dto)
}

// ==================== 纯函数核心（cargo 直测） ====================

/// 快照合并：按 batchId upsert 引擎快照条目；返回是否发生任何变更。
///
/// 引擎快照是进度真源——同 batchId 以快照内容整体覆盖（保留本地 retryMeta）；
/// 终态条目永不被迟到的旧快照复活（终态→进行中的覆盖一律忽略）。
/// 本地存在、快照缺席的非终态条目不动（由 [`prune_absent`] 单独裁决）。
pub(crate) fn merge_snapshot(store: &mut Vec<TransferEntry>, snapshot: &[Value]) -> bool {
    let mut changed = false;
    for dto in snapshot {
        let Some(incoming) = TransferEntry::from_dto(dto) else {
            continue;
        };
        if let Some(existing) = store.iter_mut().find(|e| e.batch_id == incoming.batch_id) {
            // 引擎真实终态（completed/failed/rejected/cancelled）不被迟到的旧快照
            // 复活；interrupted 是插件本地标记，允许被引擎后续快照覆盖回真实状态
            // （插件重启后原批仍在跑的场景，以引擎为准）
            let engine_terminal = matches!(
                existing.status.as_str(),
                "completed" | "failed" | "rejected" | "cancelled"
            );
            if engine_terminal && incoming.is_active() {
                continue;
            }
            // 无实质变更不触发持久化（进度事件高频同内容重放）
            let mut current = existing.clone();
            current.retry_meta = None;
            if current == incoming {
                continue;
            }
            let meta = existing.retry_meta.take();
            *existing = incoming;
            existing.retry_meta = meta;
            changed = true;
        } else {
            store.push(incoming);
            changed = true;
        }
    }
    changed
}

/// 缺席剪枝：快照已到达且本店中该方向的进行中条目不在快照内 → 标 `interrupted`
/// （引擎重启后原批已死）。返回标注数量。
pub(crate) fn prune_absent(
    store: &mut Vec<TransferEntry>,
    snapshot_ids: &[String],
    direction: &str,
) -> usize {
    let mut marked = 0;
    for entry in store.iter_mut() {
        if entry.direction != direction || !entry.is_active() {
            continue;
        }
        if !snapshot_ids.iter().any(|id| id == &entry.batch_id) {
            entry.status = "interrupted".to_string();
            entry.rate_bps = 0.0;
            marked += 1;
        }
    }
    marked
}

// ==================== 事件归约（传输编排下沉票 2） ====================

/// 终态结算：TerminalState wire 形状 → (status, detail, reject_reason)。
///
/// 原因码映射单点（本店即真源，票 3 起宿主不再映射）：
/// - send 方向 `cancelled`：`byPeer` = 对端（拉取方/接收方）取消 →
///   `cancelled-by-receiver`；本端取消 → `cancelled-by-self`；
/// - receive 方向 `cancelled`：`byPeer` = 对端（发送方）取消 →
///   `cancelled-by-sender`；本端取消 → `cancelled-by-self`；
/// - `rejected` 的 reason 是引擎 wire 枚举（UserRejected/Timeout/…），原样透传。
fn terminal_status_of(direction: &str, state: &Value) -> (String, Option<String>, Option<String>) {
    match state.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "completed" => ("completed".to_string(), None, None),
        "rejected" => (
            "rejected".to_string(),
            None,
            state.get("reason").and_then(|v| v.as_str()).map(|s| s.to_string()),
        ),
        "cancelled" => {
            let by_peer = state.get("byPeer").and_then(|v| v.as_bool()).unwrap_or(false);
            // 对端取消：send 方向的「对端」是接收方（-receiver）、receive 方向的
            // 「对端」是发送方（-sender）；本端取消恒 -self
            let code = if by_peer {
                if direction == "send" { "cancelled-by-receiver" } else { "cancelled-by-sender" }
            } else {
                "cancelled-by-self"
            };
            ("cancelled".to_string(), Some(code.to_string()), None)
        }
        other => (
            "failed".to_string(),
            state
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| other.is_empty().then(|| "unknown terminal".to_string())),
            None,
        ),
    }
}

/// 事件归约：引擎原始事件（peer:transfer-event / peer:receive-event）→ store
/// 状态推进。`direction` 按事件 topic 定向（transfer-event = send 方向、
/// receive-event = receive 方向）；`peer_name` 仅建行事件需要（运行时从设备
/// 缓存解析后传入，纯函数不做 I/O）。返回是否发生变更。
///
/// - 建行锚点：`offer-pending`（receive 询问）/ `pull-served`（send 供流记账）；
/// - 推进事件：`progress`（pending→running、字节/速率/总量补正、paused 保持）、
///   `terminal`（结算；paused 保留——用户暂停语义，completed+满字节例外）、
///   `paused` / `resumed`（wire 帧 同步）；
/// - 无建行信息的 `progress` / `terminal`（插件激活晚于会话发起）不凭空建行，
///   由首屏 active-transfers 兜底查询补占位（票 2 运行时）。
pub(crate) fn reduce_event(
    store: &mut Vec<TransferEntry>,
    direction: &str,
    event: &Value,
    peer_name: &str,
) -> bool {
    let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let ts = event.get("tsMs").and_then(|v| v.as_u64()).unwrap_or(0);
    match kind {
        // ---- 建行锚点：询问 / 供流记账 ----
        "offer-pending" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
            if store.iter().any(|e| e.batch_id == batch_id) {
                return false; // 重复询问/已应答：不重建
            }
            store.insert(
                0,
                TransferEntry {
                    batch_id: batch_id.to_string(),
                    node_id: event.get("nodeId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    peer_name: peer_name.to_string(),
                    direction: direction.to_string(),
                    status: "pending".to_string(),
                    files: event
                        .get("files")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default(),
                    total_bytes: event.get("totalSize").and_then(|v| v.as_u64()).unwrap_or(0),
                    transferred_bytes: 0,
                    rate_bps: 0.0,
                    detail: None,
                    reject_reason: None,
                    created_at_ms: ts,
                    updated_at_ms: ts,
                    retry_meta: None,
                },
            );
            true
            }
            "pull-served" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
            if store.iter().any(|e| e.batch_id == batch_id) {
                return false;
            }
            store.insert(
                0,
                TransferEntry {
                    batch_id: batch_id.to_string(),
                    node_id: event.get("nodeId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    peer_name: peer_name.to_string(),
                    direction: direction.to_string(),
                    status: "running".to_string(),
                    files: event
                        .get("files")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default(),
                    total_bytes: event.get("totalSize").and_then(|v| v.as_u64()).unwrap_or(0),
                    transferred_bytes: 0,
                    rate_bps: 0.0,
                    detail: None,
                    reject_reason: None,
                    created_at_ms: ts,
                    updated_at_ms: ts,
                    // 供流记账不可重试：无 retryMeta（源清单只有接收方持有）
                    retry_meta: None,
                },
            );
            true
            }
        // ---- 推进事件 ----
        "progress" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else { return false };
            if entry.is_terminal() {
                return false;
            }
            // paused 不打回 running（残留 Progress 会把「恢复」弹回「暂停」）
            if entry.status == "pending" {
                entry.status = "running".to_string();
            }
            entry.transferred_bytes = event.get("transferred").and_then(|v| v.as_u64()).unwrap_or(entry.transferred_bytes);
            let total = event.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            if total > 0 {
                entry.total_bytes = total;
            }
            entry.rate_bps = event.get("rateBps").and_then(|v| v.as_f64()).unwrap_or(entry.rate_bps);
            entry.updated_at_ms = ts;
            true
        }
        "paused" | "resumed" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else { return false };
            let target = if kind == "paused" {
                matches!(entry.status.as_str(), "running" | "pending").then_some("paused")
            } else if entry.status == "paused" {
                Some("running")
            } else {
                None
            };
            match target {
                Some(t) => {
                    entry.status = t.to_string();
                    entry.rate_bps = 0.0;
                    entry.updated_at_ms = ts;
                    true
                }
                None => false,
            }
        }
        "terminal" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else { return false };
            let Some(state) = event.get("state") else { return false };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else { return false };
            if entry.is_terminal() {
                return false;
            }
            // 用户暂停分支：终态事件只是中断确认——不落终态（completed+满字节例外）
            let full_completed = entry.status == "paused"
                && state.get("type").and_then(|v| v.as_str()) == Some("completed")
                && entry.total_bytes > 0
                && entry.transferred_bytes >= entry.total_bytes;
            if entry.status == "paused" && !full_completed {
                return false;
            }
            let (status, detail, reject_reason) = terminal_status_of(direction, state);
            entry.status = status;
            if entry.status == "completed" && entry.total_bytes > 0 {
                entry.transferred_bytes = entry.total_bytes;
            }
            entry.detail = detail;
            entry.reject_reason = reject_reason;
            entry.rate_bps = 0.0;
            entry.updated_at_ms = ts;
            true
        }
        _ => false,
    }
}

/// 首屏兜底占位（active-transfers 投影行）：把宿主句柄表在册的活跃批补成
/// 最小占位行（缺 peer_name / files；progress 事件随后补全）。方向取行内
/// `direction`（缺省 send）。已存在的 batchId 幂等跳过。返回插入数量。
pub(crate) fn insert_active_projections(store: &mut Vec<TransferEntry>, rows: &[Value], peer_name: &str) -> usize {
    let mut inserted = 0;
    for row in rows {
        let Some(batch_id) = row.get("batchId").and_then(|v| v.as_str()) else { continue };
        if store.iter().any(|e| e.batch_id == batch_id) {
            continue;
        }
        // 终态批不入店（历史由持久层承接）
        let terminal = matches!(
            row.get("status").and_then(|v| v.as_str()),
            Some("completed") | Some("failed") | Some("rejected") | Some("cancelled") | None
        );
        if terminal {
            continue;
        }
        store.push(TransferEntry {
            batch_id: batch_id.to_string(),
            node_id: row.get("nodeId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            peer_name: peer_name.to_string(),
            direction: row.get("direction").and_then(|v| v.as_str()).unwrap_or("send").to_string(),
            status: row.get("status").and_then(|v| v.as_str()).unwrap_or("running").to_string(),
            files: vec![],
            total_bytes: row.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            transferred_bytes: row.get("transferredBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            rate_bps: row.get("rateBps").and_then(|v| v.as_f64()).unwrap_or(0.0),
            detail: None,
            reject_reason: None,
            created_at_ms: row.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
            updated_at_ms: row.get("updatedAtMs").and_then(|v| v.as_u64()).unwrap_or(0),
            retry_meta: None,
        });
        inserted += 1;
    }
    inserted
}

/// 对账：归约态（store）与引擎快照在**活跃批**上的差异清单（票 2 双写验收——
/// 空列表 = 两路一致）。快照仍是校正源：调用方对账后照常 merge，本函数只
/// 产出可观测偏差（warn 日志），不决定写谁。
pub(crate) fn reconcile_diff(store: &[TransferEntry], snapshot: &[Value], direction: &str) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut snap_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for dto in snapshot {
        let Some(id) = dto.get("batchId").and_then(|v| v.as_str()) else { continue };
        snap_ids.insert(id);
        let store_entry = store.iter().find(|e| e.batch_id == id);
        match store_entry {
            None => diffs.push(format!("{direction}:{id} missing-in-reduced")),
            Some(entry) if entry.is_terminal() => {
                diffs.push(format!("{direction}:{id} reduced-terminal-but-snapshot-active"))
            }
            Some(entry) => {
                let snap_status = dto.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if entry.status != snap_status {
                    diffs.push(format!("{direction}:{id} status {} != {snap_status}", entry.status));
                }
                let snap_bytes = dto.get("transferredBytes").and_then(|v| v.as_u64()).unwrap_or(0);
                if entry.transferred_bytes != snap_bytes {
                    diffs.push(format!(
                        "{direction}:{id} bytes {} != {snap_bytes}",
                        entry.transferred_bytes
                    ));
                }
            }
        }
    }
    for entry in store.iter().filter(|e| e.direction == direction && e.is_active()) {
        if !snap_ids.contains(entry.batch_id.as_str()) {
            diffs.push(format!("{direction}:{} missing-in-snapshot", entry.batch_id));
        }
    }
    diffs
}

/// 重启恢复标注：载入持久层后把 running/pending 改标 `interrupted`。
/// 与 prune_absent 不同，它不依赖快照（activate 时快照未到先渲染）。
/// paused 同样标注——宿主会话已随进程消亡，保留 paused 会永久卡死。
pub(crate) fn mark_interrupted_on_load(entries: &mut [TransferEntry]) -> usize {
    let mut marked = 0;
    for e in entries.iter_mut() {
        if e.status == "running" || e.status == "pending" || e.status == "paused" {
            e.status = "interrupted".to_string();
            e.rate_bps = 0.0;
            marked += 1;
        }
    }
    marked
}

/// 终态封顶滚动淘汰：按 updatedAtMs 升序（最旧先出），超出 [`HISTORY_CAP`] 的
/// 最旧终态条目移除。返回移除数量。
pub(crate) fn evict_overflow(entries: &mut Vec<TransferEntry>) -> usize {
    let terminal_count = entries.iter().filter(|e| e.is_terminal()).count();
    if terminal_count <= HISTORY_CAP {
        return 0;
    }
    let mut terminal_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_terminal())
        .map(|(i, _)| i)
        .collect();
    // updatedAtMs 升序；同刻按索引稳定序（先入库先出）
    terminal_indices.sort_by_key(|&i| (entries[i].updated_at_ms, i));
    let evict_n = terminal_count - HISTORY_CAP;
    let victims: std::collections::HashSet<usize> =
        terminal_indices.into_iter().take(evict_n).collect();
    let mut keep = Vec::with_capacity(entries.len() - evict_n);
    for (i, e) in entries.drain(..).enumerate() {
        if !victims.contains(&i) {
            keep.push(e);
        }
    }
    *entries = keep;
    evict_n
}

/// 清空历史：删除全部终态条目，返回清除数量
pub(crate) fn clear_terminal(entries: &mut Vec<TransferEntry>) -> usize {
    let before = entries.len();
    entries.retain(|e| !e.is_terminal());
    before - entries.len()
}

/// 发送视图（tasks-changed / list-tasks 共用）：仅进行中条目
/// （running/pending）；终态条目一律归历史视图，不再滞留活动队列。
pub(crate) fn active_send_entries(entries: &[TransferEntry]) -> Vec<&TransferEntry> {
    entries
        .iter()
        .filter(|e| e.direction == "send" && e.is_active())
        .collect()
}

/// 接收视图（receiving-changed / list-receiving 共用）：正在接收的
/// running 条目 + 用户暂停的 paused 条目（暂停也是活跃态，须留在接收队列里
/// 供继续/取消）；pending 归待应答（batches），终态归历史视图。
pub(crate) fn active_receive_entries(entries: &[TransferEntry]) -> Vec<&TransferEntry> {
    entries
        .iter()
        .filter(|e| e.direction == "receive" && matches!(e.status.as_str(), "running" | "paused"))
        .collect()
}

/// 取消乐观结算：命中条目改标 cancelled（引擎快照随后校正/确认）。
/// 仅对进行中条目生效。返回是否命中。
pub(crate) fn mark_cancelled(entries: &mut [TransferEntry], batch_id: &str) -> bool {
    for e in entries.iter_mut() {
        if e.batch_id == batch_id && e.is_active() {
            e.status = "cancelled".to_string();
            e.rate_bps = 0.0;
            return true;
        }
    }
    false
}

/// 暂停乐观标记：running 条目改标 paused（引擎快照随后确认；暂停释放并发槽）。
pub(crate) fn mark_paused(entries: &mut [TransferEntry], batch_id: &str) -> bool {
    for e in entries.iter_mut() {
        if e.batch_id == batch_id && e.status == "running" {
            e.status = "paused".to_string();
            e.rate_bps = 0.0;
            return true;
        }
    }
    false
}

/// 重试回填：新批 ID 替换旧批（同一逻辑传输一条历史），状态复位 running。
/// 只允许携带 retryMeta 的终态条目（即本端发起的失败/被拒/取消/中断批）。
/// 返回是否命中。
pub(crate) fn apply_retry(
    entries: &mut [TransferEntry],
    old_batch_id: &str,
    new_batch_id: &str,
    now_ms: u64,
) -> bool {
    for e in entries.iter_mut() {
        if e.batch_id == old_batch_id && e.is_terminal() && e.retry_meta.is_some() {
            e.batch_id = new_batch_id.to_string();
            e.status = "running".to_string();
            e.transferred_bytes = 0;
            e.rate_bps = 0.0;
            e.detail = None;
            e.reject_reason = None;
            e.updated_at_ms = now_ms.max(e.updated_at_ms);
            return true;
        }
    }
    false
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dto(id: &str, direction: &str, status: &str, updated: u64) -> Value {
        json!({
            "batchId": id, "nodeId": "aa", "peerName": "Pixel",
            "direction": direction, "status": status,
            "files": [{"path": "a.txt", "size": 10}],
            "totalBytes": 10u64, "transferredBytes": 5u64, "rateBps": 12.5,
            "createdAtMs": 1u64, "updatedAtMs": updated,
        })
    }

    #[test]
    fn merge_upserts_by_batch_id_and_keeps_retry_meta() {
        let mut store = vec![];
        assert!(merge_snapshot(&mut store, &[dto("b1", "send", "running", 10)]));
        store[0].retry_meta = Some(RetryMeta::Send { paths: vec!["C:/a.txt".into()] });
        // 同批进度更新不丢 retryMeta；同内容重放不算变更
        assert!(!merge_snapshot(&mut store, &[]));
        assert!(!merge_snapshot(&mut store, &[dto("b1", "send", "running", 10)]));
        assert!(merge_snapshot(&mut store, &[dto("b1", "send", "running", 20)]));
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].updated_at_ms, 20);
        assert_eq!(
            store[0].retry_meta,
            Some(RetryMeta::Send { paths: vec!["C:/a.txt".into()] })
        );
        // 新批追加
        assert!(merge_snapshot(&mut store, &[dto("b2", "send", "running", 30)]));
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn interrupted_entry_revived_by_fresh_engine_snapshot() {
        // 插件重启标注 interrupted 后，引擎仍报 running 的批次以引擎为准恢复
        let mut store = vec![serde_json::from_value::<TransferEntry>(
            json!({ "batchId": "live", "direction": "send", "status": "interrupted" }),
        )
        .unwrap()];
        assert!(merge_snapshot(&mut store, &[dto("live", "send", "running", 50)]));
        assert_eq!(store[0].status, "running");
    }

    #[test]
    fn terminal_entries_not_revived_by_stale_snapshot() {
        let mut store = vec![];
        merge_snapshot(&mut store, &[dto("b1", "send", "completed", 10)]);
        // 迟到的 running 快照不得复活终态
        assert!(!merge_snapshot(&mut store, &[dto("b1", "send", "running", 5)]));
        assert_eq!(store[0].status, "completed");
    }

    #[test]
    fn prune_absent_marks_only_active_of_direction() {
        let mut store = vec![
            serde_json::from_value::<TransferEntry>(dto("run", "send", "running", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("done", "send", "completed", 2)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("rcv", "receive", "running", 3)).unwrap(),
        ];
        // send 快照不含 run → run 标 interrupted；done 终态不动；receive 不看 send 快照
        let n = prune_absent(&mut store, &["done".to_string()], "send");
        assert_eq!(n, 1);
        assert_eq!(store[0].status, "interrupted");
        assert_eq!(store[1].status, "completed");
        assert_eq!(store[2].status, "running");
    }

    #[test]
    fn mark_interrupted_on_load_covers_running_and_pending() {
        let mut entries = vec![
            serde_json::from_value::<TransferEntry>(dto("r", "send", "running", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("p", "receive", "pending", 2)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("c", "send", "completed", 3)).unwrap(),
        ];
        assert_eq!(mark_interrupted_on_load(&mut entries), 2);
        assert_eq!(entries[0].status, "interrupted");
        assert_eq!(entries[1].status, "interrupted");
        assert_eq!(entries[2].status, "completed");
    }

    #[test]
    fn mark_interrupted_covers_paused_too() {
        // 宿主会话随进程消亡：paused 任务同样标注 interrupted（保留会卡死）
        let mut entries = vec![serde_json::from_value::<TransferEntry>(
            json!({ "batchId": "pz", "direction": "send", "status": "paused" }),
        )
        .unwrap()];
        assert_eq!(mark_interrupted_on_load(&mut entries), 1);
        assert_eq!(entries[0].status, "interrupted");
    }

    #[test]
    fn mark_paused_hits_running_only_and_keeps_progress() {
        let mut store = vec![
            serde_json::from_value::<TransferEntry>(dto("run", "send", "running", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("pend", "send", "pending", 2)).unwrap(),
        ];
        store[0].transferred_bytes = 1024;
        assert!(mark_paused(&mut store, "run"));
        assert!(!mark_paused(&mut store, "pend"));
        assert!(!mark_paused(&mut store, "zz"));
        assert_eq!(store[0].status, "paused");
        // 暂停保留已传字节（恢复后续传展示）
        assert_eq!(store[0].transferred_bytes, 1024);
        assert_eq!(store[0].rate_bps, 0.0);
    }

    #[test]
    fn active_views_include_paused_send_entries() {
        // paused 显示在「正在发送」队列（用户可继续/取消），不算并发槽
        let mk = |id: &str, direction: &str, status: &str| {
            serde_json::from_value::<TransferEntry>(json!({
                "batchId": id, "direction": direction, "status": status,
            }))
            .unwrap()
        };
        let store = vec![
            mk("s-run", "send", "running"),
            mk("s-pause", "send", "paused"),
            mk("s-done", "send", "completed"),
            mk("r-pend", "receive", "pending"),
        ];
        let send_ids: Vec<&str> =
            active_send_entries(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(send_ids, vec!["s-run", "s-pause"]);
    }

    #[test]
    fn active_receive_view_keeps_paused_and_excludes_other_states() {
        // 用户暂停的接收任务（下载/拉取）必须留在接收队列（可继续/取消）；
        // pending 归待应答、终态归历史，均不得出现在接收视图
        let mk = |id: &str, status: &str| {
            serde_json::from_value::<TransferEntry>(json!({
                "batchId": id, "direction": "receive", "status": status,
            }))
            .unwrap()
        };
        let store = vec![
            mk("r-run", "running"),
            mk("r-pause", "paused"),
            mk("r-pend", "pending"),
            mk("r-done", "completed"),
            mk("r-fail", "failed"),
        ];
        let recv_ids: Vec<&str> =
            active_receive_entries(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(recv_ids, vec!["r-run", "r-pause"]);
    }

    #[test]
    fn evict_overflow_evicts_oldest_terminal_first() {
        let mut store = vec![];
        // 202 条终态 + 1 条进行中：最旧两条终态被逐出，进行中保留
        for i in 0..202u64 {
            store.push(
                serde_json::from_value::<TransferEntry>(json!({
                    "batchId": format!("t{i}"), "direction": "send",
                    "status": "completed", "updatedAtMs": i,
                }))
                .unwrap(),
            );
        }
        store.push(
            serde_json::from_value::<TransferEntry>(json!({
                "batchId": "live", "direction": "send",
                "status": "running", "updatedAtMs": 0u64,
            }))
            .unwrap(),
        );
        let evicted = evict_overflow(&mut store);
        assert_eq!(evicted, 2);
        assert_eq!(store.len(), 201);
        assert!(store.iter().all(|e| e.batch_id != "t0" && e.batch_id != "t1"));
        assert!(store.iter().any(|e| e.batch_id == "live"));
        assert!(store.iter().any(|e| e.batch_id == "t201"));
        // 未超顶不再逐出
        assert_eq!(evict_overflow(&mut store), 0);
    }

    #[test]
    fn clear_terminal_keeps_active() {
        let mut store = vec![
            serde_json::from_value::<TransferEntry>(dto("a", "send", "completed", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("b", "send", "running", 2)).unwrap(),
        ];
        assert_eq!(clear_terminal(&mut store), 1);
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].batch_id, "b");
    }

    #[test]
    fn active_views_exclude_terminal_entries() {
        let mk = |id: &str, direction: &str, status: &str| {
            serde_json::from_value::<TransferEntry>(json!({
                "batchId": id, "direction": direction, "status": status,
            }))
            .unwrap()
        };
        let store = vec![
            mk("s-run", "send", "running"),
            mk("s-done", "send", "completed"),
            mk("s-fail", "send", "failed"),
            mk("s-inter", "send", "interrupted"),
            mk("r-pend", "receive", "pending"),
            mk("r-run", "receive", "running"),
            mk("r-done", "receive", "completed"),
        ];
        let send_ids: Vec<&str> =
            active_send_entries(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(send_ids, vec!["s-run"]);
        let recv_ids: Vec<&str> =
            active_receive_entries(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(recv_ids, vec!["r-run"]);
    }

    #[test]
    fn mark_cancelled_hits_only_active() {
        let mut store = [
            serde_json::from_value::<TransferEntry>(dto("x", "send", "running", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("y", "send", "completed", 2)).unwrap(),
        ];
        assert!(mark_cancelled(&mut store, "x"));
        assert!(!mark_cancelled(&mut store, "y"));
        assert!(!mark_cancelled(&mut store, "zz"));
        assert_eq!(store[0].status, "cancelled");
    }

    #[test]
    fn apply_retry_replays_terminal_entry() {
        let mut old = serde_json::from_value::<TransferEntry>(dto("old", "send", "failed", 1))
            .unwrap();
        old.retry_meta = Some(RetryMeta::Send { paths: vec!["C:/a.txt".into()] });
        let act = serde_json::from_value::<TransferEntry>(dto("act", "send", "running", 2))
            .unwrap();
        let mut store = [old, act];
        assert!(apply_retry(&mut store, "old", "new", 99));
        assert_eq!(store[0].batch_id, "new");
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].updated_at_ms, 99);
        // 进行中条目不可重试
        assert!(!apply_retry(&mut store, "act", "new2", 100));
    }

    // ==================== 事件归约（传输编排下沉票 2） ====================

    fn ev(kind: &str, batch_id: &str) -> Value {
        json!({ "kind": kind, "batchId": batch_id, "tsMs": 100u64 })
    }

    /// 建行锚点：offer-pending 建 receive pending 行（peer_name / files / 时间戳
    /// 取自事件），同批重复事件不重建
    #[test]
    fn reduce_offer_pending_creates_pending_row_once() {
        let mut store = vec![];
        let event = json!({
            "kind": "offer-pending", "batchId": "b1", "nodeId": "aa",
            "files": [{"path": "a.bin", "size": 5}],
            "totalSize": 5, "tsMs": 42,
        });
        assert!(reduce_event(&mut store, "receive", &event, "Pixel"));
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].direction, "receive");
        assert_eq!(store[0].status, "pending");
        assert_eq!(store[0].peer_name, "Pixel");
        assert_eq!(store[0].node_id, "aa");
        assert_eq!(store[0].total_bytes, 5);
        assert_eq!(store[0].created_at_ms, 42);
        assert_eq!(store[0].updated_at_ms, 42);
        // 幂等：重复 offer 不重建
        assert!(!reduce_event(&mut store, "receive", &event, "Pixel"));
        assert_eq!(store.len(), 1);
    }

    /// 建行锚点：pull-served 建 send running 行且不可重试（无 retryMeta）
    #[test]
    fn reduce_pull_served_creates_send_row_without_retry_meta() {
        let mut store = vec![];
        let event = json!({
            "kind": "pull-served", "batchId": "b2", "nodeId": "bb",
            "files": [{"path": "x.bin", "size": 9}],
            "totalSize": 9, "tsMs": 7,
        });
        assert!(reduce_event(&mut store, "send", &event, "Tab"));
        assert_eq!(store[0].direction, "send");
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].total_bytes, 9);
        assert_eq!(store[0].retry_meta, None, "供流记账不可重试");
        assert!(!reduce_event(&mut store, "send", &event, "Tab"));
    }

    /// progress：pending → running、字节/速率推进、总量补正；paused 保持暂停
    #[test]
    fn reduce_progress_advances_and_backfills_total() {
        let mut store = vec![];
        reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "offer-pending", "batchId": "b", "nodeId": "aa", "files": [], "totalSize": 0, "tsMs": 1}),
            "P",
        );
        // pull 预登记 size=0 场景由首个 Progress 补正 total
        assert!(reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "progress", "batchId": "b", "transferred": 10, "total": 200, "rateBps": 1.5, "tsMs": 2}),
            "",
        ));
        assert_eq!(store[0].status, "running", "pending 推进为 running");
        assert_eq!(store[0].transferred_bytes, 10);
        assert_eq!(store[0].total_bytes, 200, "首个 Progress 补正总量");
        assert_eq!(store[0].rate_bps, 1.5);
        assert_eq!(store[0].updated_at_ms, 2);
        // paused 不被打回 running
        reduce_event(&mut store, "receive", &ev("paused", "b"), "");
        assert!(reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "progress", "batchId": "b", "transferred": 20, "total": 200, "rateBps": 0.0, "tsMs": 3}),
            "",
        ));
        assert_eq!(store[0].status, "paused", "暂停期间不得被打回 running");
        assert_eq!(store[0].transferred_bytes, 20);
        // 未知批与终态批的 progress 被忽略
        assert!(!reduce_event(&mut store, "receive", &ev("progress", "ghost"), ""));
    }

    /// terminal：原因码映射按方向区分（send 的对端取消 = -receiver；receive 的
    /// 对端取消 = -sender）；本端取消恒 -self。原因码落 detail 字段（与快照
    /// wire 形状一致：reject_reason 仅 rejected 变体使用）
    #[test]
    fn reduce_terminal_maps_reason_codes_by_direction() {
        let send_cancel = |store: &mut Vec<TransferEntry>, by_peer: bool| {
            reduce_event(
                store,
                "send",
                &json!({"kind": "pull-served", "batchId": "bs", "nodeId": "aa", "files": [], "totalSize": 10, "tsMs": 1}),
                "P",
            );
            reduce_event(store, "send", &json!({
                "kind": "terminal", "batchId": "bs", "tsMs": 2,
                "state": {"type": "cancelled", "byPeer": by_peer},
            }), "");
            store[0].detail.clone().unwrap()
        };
        assert_eq!(send_cancel(&mut vec![], true), "cancelled-by-receiver");
        assert_eq!(send_cancel(&mut vec![], false), "cancelled-by-self");

        let recv_cancel = |store: &mut Vec<TransferEntry>, by_peer: bool| {
            reduce_event(
                store,
                "receive",
                &json!({"kind": "offer-pending", "batchId": "br", "nodeId": "aa", "files": [], "totalSize": 10, "tsMs": 1}),
                "P",
            );
            reduce_event(
                store,
                "receive",
                &json!({"kind": "progress", "batchId": "br", "transferred": 0, "total": 10, "rateBps": 0.0, "tsMs": 1}),
                "",
            );
            reduce_event(store, "receive", &json!({
                "kind": "terminal", "batchId": "br", "tsMs": 2,
                "state": {"type": "cancelled", "byPeer": by_peer},
            }), "");
            store[0].detail.clone().unwrap()
        };
        assert_eq!(recv_cancel(&mut vec![], true), "cancelled-by-sender");
        assert_eq!(recv_cancel(&mut vec![], false), "cancelled-by-self");
    }

    /// terminal：completed 归整满额 + rejected/failed 透传；paused 保留、
    /// completed+满字节例外结算
    #[test]
    fn reduce_terminal_settles_states_and_respects_paused() {
        // completed：transferred 归整为 total
        let mut store = vec![];
        reduce_event(
            &mut store,
            "send",
            &json!({"kind": "pull-served", "batchId": "b", "nodeId": "aa", "files": [], "totalSize": 10, "tsMs": 1}),
            "P",
        );
        reduce_event(
            &mut store,
            "send",
            &json!({"kind": "progress", "batchId": "b", "transferred": 9, "total": 10, "rateBps": 0.0, "tsMs": 2}),
            "",
        );
        reduce_event(
            &mut store,
            "send",
            &json!({"kind": "terminal", "batchId": "b", "tsMs": 3, "state": {"type": "completed"}}),
            "",
        );
        assert_eq!(store[0].status, "completed");
        assert_eq!(store[0].transferred_bytes, 10, "完成结算归整为满额");

        // paused + 非完成终态：保留（用户暂停语义）
        let mut store = vec![];
        reduce_event(
            &mut store,
            "send",
            &json!({"kind": "pull-served", "batchId": "b", "nodeId": "aa", "files": [], "totalSize": 10, "tsMs": 1}),
            "P",
        );
        reduce_event(&mut store, "send", &ev("paused", "b"), "");
        assert!(!reduce_event(
            &mut store,
            "send",
            &json!({"kind": "terminal", "batchId": "b", "tsMs": 3, "state": {"type": "failed", "detail": "io"}}),
            "",
        ));
        assert_eq!(store[0].status, "paused", "用户暂停的任务不落终态");

        // paused + completed + 满字节：例外结算 completed
        store[0].transferred_bytes = 10;
        assert!(reduce_event(
            &mut store,
            "send",
            &json!({"kind": "terminal", "batchId": "b", "tsMs": 4, "state": {"type": "completed"}}),
            "",
        ));
        assert_eq!(store[0].status, "completed");

        // rejected：reason wire 枚举透传
        let mut store = vec![];
        reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "offer-pending", "batchId": "r", "nodeId": "aa", "files": [], "totalSize": 1, "tsMs": 1}),
            "P",
        );
        reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "progress", "batchId": "r", "transferred": 0, "total": 1, "rateBps": 0.0, "tsMs": 1}),
            "",
        );
        reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "terminal", "batchId": "r", "tsMs": 2, "state": {"type": "rejected", "reason": "UserRejected"}}),
            "",
        );
        assert_eq!(store[0].status, "rejected");
        assert_eq!(store[0].reject_reason.as_deref(), Some("UserRejected"));
        // 终态后迟到的终态事件不重复结算
        assert!(!reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "terminal", "batchId": "r", "tsMs": 3, "state": {"type": "completed"}}),
            "",
        ));
    }

    /// paused/resumed：running/pending → paused；paused → running；终态不变
    #[test]
    fn reduce_pause_resume_syncs_states() {
        let mut store = vec![];
        reduce_event(
            &mut store,
            "receive",
            &json!({"kind": "offer-pending", "batchId": "b", "nodeId": "aa", "files": [], "totalSize": 1, "tsMs": 1}),
            "P",
        );
        assert!(reduce_event(&mut store, "receive", &ev("paused", "b"), ""));
        assert_eq!(store[0].status, "paused");
        assert!(reduce_event(&mut store, "receive", &ev("resumed", "b"), ""));
        assert_eq!(store[0].status, "running");
        // running 收 resumed 无迁移
        assert!(!reduce_event(&mut store, "receive", &ev("resumed", "b"), ""));
        assert!(!reduce_event(&mut store, "receive", &ev("paused", "ghost"), ""));
    }

    /// 首屏兜底占位：活跃批补最小行（幂等、终态与缺 batchId 跳过、方向取行内）
    #[test]
    fn insert_active_projections_fills_missing_active_rows() {
        let mut store = vec![serde_json::from_value::<TransferEntry>(dto("b1", "send", "running", 1)).unwrap()];
        let rows = vec![
            json!({"batchId": "b1", "direction": "send", "status": "running", "totalBytes": 9}),          // 已存在 → 幂等
            json!({"batchId": "b2", "direction": "send", "status": "completed"}),                          // 终态 → 跳过
            json!({"batchId": "b3", "direction": "receive", "status": "running", "totalBytes": 5, "transferredBytes": 2, "updatedAtMs": 8}),
        ];
        assert_eq!(insert_active_projections(&mut store, &rows, ""), 1);
        let b3 = store.iter().find(|e| e.batch_id == "b3").expect("b3 inserted");
        assert_eq!(b3.direction, "receive");
        assert_eq!(b3.total_bytes, 5);
        assert_eq!(b3.transferred_bytes, 2);
        assert_eq!(b3.updated_at_ms, 8);
        assert_eq!(b3.retry_meta, None);
    }

    /// 对账：归约态与快照的偏差清单（缺行 / 状态漂移 / 字节漂移 / 快照缺席）
    #[test]
    fn reconcile_diff_reports_active_drift() {
        let mut store = vec![];
        reduce_event(
            &mut store,
            "send",
            &json!({"kind": "pull-served", "batchId": "b1", "nodeId": "aa", "files": [], "totalSize": 10, "tsMs": 1}),
            "P",
        );
        // 快照：b1 running/5、b2 running（归约缺行）
        let snapshot = vec![
            json!({"batchId": "b1", "direction": "send", "status": "running", "transferredBytes": 5}),
            json!({"batchId": "b2", "direction": "send", "status": "running", "transferredBytes": 0}),
        ];
        let diffs = reconcile_diff(&store, &snapshot, "send");
        assert!(diffs.iter().any(|d| d.contains("b1 bytes 0 != 5")), "字节漂移: {diffs:?}");
        assert!(diffs.iter().any(|d| d.contains("b2 missing-in-reduced")), "快照有归约无: {diffs:?}");
        // 一致时为空
        assert!(reconcile_diff(
            &store,
            &[json!({"batchId": "b1", "direction": "send", "status": "running", "transferredBytes": 0})],
            "send",
        )
        .is_empty());
        // 归约有、快照无（快照已结算场景由调用方先 merge 再对账——此处直接呈现）
        let diffs = reconcile_diff(&store, &[], "send");
        assert!(diffs.iter().any(|d| d.contains("b1 missing-in-snapshot")));
    }
}
