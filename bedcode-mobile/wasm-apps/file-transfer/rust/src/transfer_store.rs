//! 传输任务/历史自持存储（issue 13 Phase 3 步骤 3）
//!
//! 插件自有「产品视图」：双方向均以引擎原始事件归约为唯一任务真源——发送方向
//! 吃 `peer:transfer-event`（票 06），接收方向吃 `peer:receive-event`
//! （票 07）；按 batchId 归约进本店；终态归档 + 200 条封顶滚动淘汰；重启恢复
//! 时running/pending/paused 态条目标注 `interrupted`（原批已死，如实呈现）。
//!
//! 票 08 把「回放与节流判据」也收进本文件单点：重试判据
//! （[`retry_source`]）、发送闸门判据（[`send_slot_open`]）、拉取意图队列
//! （[`push_pull_intent`] / [`take_pull_intent`]）——编排层（peer.rs）只负责
//! 调引擎与派发视图，不再自带一份判据。
//!
//! 旧快照通路（`peer:receive` 全量列表 + 合并/对账/缺席剪枝）已随宿主接收任务表
//! 退役：快照是「第二真源」，留着会让归约态与快照互相覆盖，掩盖事件丢失。
//!
//! 原因码（cancelled-by-*）与终态判定只在本文件实现——宿主不再持有产品语义。
//!
//! 时间戳纪律：全部取自引擎事件载荷（`tsMs`），本地无时钟
//! （wasm32-unknown-unknown 禁 std::time）。纯函数核心与宿主 I/O 分离，cargo test 直测。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 历史封顶：终态条目超过即按 updatedAtMs 最旧先出
pub(crate) const HISTORY_CAP: usize = 200;

/// 拉取意图队列封顶（防失控增长；超出即丢最旧）
///
/// 意图 = 「本插件发起过一次拉取」的待挂载凭证：`batch_id` 由引擎铸造，调用点
/// 拿不到，只能事后凭`node_id + rel_path` 匹配挂回`retry_meta`（票 08）。
pub(crate) const PULL_INTENT_CAP: usize = 8;

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

/// 单条传输条目 —— 引擎事件事实的 camelCase 形状 + 插件扩展字段。
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
    /// 本机落盘路径（接收方向 completed 归档时宿主按需填充；
    /// 仅 completed 且本地文件仍存在时非空，缺省 None 保持向后兼容）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) local_path: Option<String>,
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
            local_path: dto
                .get("localPath")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        })
    }

    /// 是否终态（interrupted 视作可重试的终态变体）
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed" | "failed" | "rejected" | "cancelled" | "interrupted"
        )
    }

    /// 是否进行中（含接收待应答与用户暂停）
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "pending" | "paused")
    }
}

/// DTO 解析出口（命令编排层入店用）
pub(crate) fn entry_from_dto(dto: &Value) -> Option<TransferEntry> {
    TransferEntry::from_dto(dto)
}

// ==================== 纯函数核心（cargo 直测） ====================

/// 快照合并：按 batchId upsert 本地占位条目；返回是否发生任何变更。
///
/// 仅用于**插件自己发起的会话占位**（`enqueue` / `retry` 拿到传输句柄时先入店
/// 最小条目，随后由引擎事件补全明细）。引擎事件通路不经过本函数——事件归约见
/// [`reduce_event`]，快照不再是第二真源（票 07：宿主接收任务表退役后
/// `peer:receive` 全量快照通路整体删除）。
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

// ==================== 事件归约（票 06 / 票 07） ====================

/// 终态结算：TerminalState wire 形状 → (status, detail, reject_reason)。
///
/// 原因码映射单点（本店即真源，票 06 起宿主不再映射）：
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

/// 事件归约：引擎原始事件（`peer:transfer-event` / `peer:receive-event`）→
/// store 状态推进。`direction` 按事件 topic 定向；`peer_name` 仅建行事件需要
/// （运行时从设备缓存解析后传入，纯函数不做 I/O）。返回是否发生变更。
///
/// - 建行锚点：`pull-served`（send 供流记账）、`offer-pending`（入站待应答，
///   status=pending）、`pull-started`（本端拉取批次，status=running）——三者
///   都是「只有引擎知道 batch_id」的锚点，故由引擎事件建行；
/// - 推进事件：`progress`（pending→running、字节/速率/总量补正、paused 保持）、
///   `terminal`（结算；paused 保留——用户暂停语义，completed+满字节例外）、
///   `paused` / `resumed`（wire 帧同步）；
/// - 无建行信息的 `progress` / `terminal`（插件激活晚于会话发起）不凭空建行，
///   由首屏 active-transfers 兜底查询补占位（[`insert_active_projections`]）。
pub(crate) fn reduce_event(
    store: &mut Vec<TransferEntry>,
    direction: &str,
    event: &Value,
    peer_name: &str,
) -> bool {
    let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let ts = event.get("tsMs").and_then(|v| v.as_u64()).unwrap_or(0);
    match kind {
        // ---- 建行锚点：引擎铸造 batch_id 的三类会话 ----
        "pull-served" | "offer-pending" | "pull-started" => {
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
                    // 入站 offer 待应答（pending 归待应答视图）；供流记账与本端
                    // 拉取都是数据面已建立（running）
                    status: if kind == "offer-pending" { "pending" } else { "running" }.to_string(),
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
                    // 无 retryMeta：供流记账的源清单只有接收方持有；入站待应答
                    // 不可重试；本端拉取的重试元数据由调用方按 rel_path 回填
                    // （batch_id 引擎铸造，pull-files 调用点事前拿不到）
                    retry_meta: None,
                    local_path: None,
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
            entry.transferred_bytes = event
                .get("transferred")
                .and_then(|v| v.as_u64())
                .unwrap_or(entry.transferred_bytes);
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
pub(crate) fn insert_active_projections(
    store: &mut Vec<TransferEntry>,
    rows: &[Value],
    peer_name: &str,
) -> usize {
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
            local_path: None,
        });
        inserted += 1;
    }
    inserted
}

/// 中断标注：把在册进行中条目（running / pending / paused）改标 `interrupted`。
///
/// 两个调用点，同一语义——「引擎侧会话已死，如实呈现，不假装还在跑」：
/// - 载入持久层（插件重启，原批随宿主进程消亡）；
/// - 收到 `node-stopped` 事件（引擎节点下线，在飞批全部中止）。
///
/// paused 同样标注：会话已死，保留 paused 会永久卡死（无 Resume 可写）。
pub(crate) fn mark_active_interrupted(entries: &mut [TransferEntry]) -> usize {
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
            // retryMeta 保留：新批仍由本端发起，仍可再次回放
            e.updated_at_ms = now_ms.max(e.updated_at_ms);
            return true;
        }
    }
    false
}

/// 重试拒绝原因（可重试判据的失败分类，票 08：判据单点化）
///
/// 三类分开而非一个 None：前端「点了没反应」需要能区分「条目不存在」
/// 「还在跑（不该重试）」与「不是本端发起（无回放凭证）」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryRefusal {
    /// 店内无此batchId
    NotFound,
    /// 条目仍在进行中（running / pending / paused）——重试前提是终态
    NotTerminal,
    /// 终态但无 retryMeta（非本端发起：供流记账行、入站待应答批）
    MissingMeta,
}

impl RetryRefusal {
    pub(crate) fn message(&self, task_id: &str) -> String {
        match self {
            Self::NotFound => format!("task not found: {task_id}"),
            Self::NotTerminal => {
                format!("task still in flight, not retryable: {task_id}")
            }
            Self::MissingMeta => {
                format!("task not retryable (initiator metadata missing): {task_id}")
            }
        }
    }
}

/// 重试回放源解析（票 08：重试判据单点）
///
/// 必须在**调引擎之前**求值：send 方向无引擎建行事件（行由插件发起时自建），
/// 对不可重试条目先发后校验会铸出无主会话——永不入店、进度事件打不中行，
/// 且占死发送闸门槽位。返回 `(node_id, retryMeta)` 供回放编排使用。
pub(crate) fn retry_source(
    entries: &[TransferEntry],
    task_id: &str,
) -> Result<(String, RetryMeta), RetryRefusal> {
    let entry = entries
        .iter()
        .find(|e| e.batch_id == task_id)
        .ok_or(RetryRefusal::NotFound)?;
    if !entry.is_terminal() {
        return Err(RetryRefusal::NotTerminal);
    }
    let meta = entry.retry_meta.clone().ok_or(RetryRefusal::MissingMeta)?;
    Ok((entry.node_id.clone(), meta))
}

/// 发送闸门判据（纯函数，票 08）：running 批数 < 设置并发上限。
/// 下限 1：并发配置缺失/为 0 时仍允许一个槽位（闸门不得把发送全锁死）。
pub(crate) fn send_slot_open(running: usize, limit: u8) -> bool {
    running < limit.max(1) as usize
}

// ==================== 拉取意图队列（票 08） ====================

/// 拉取意图入队（封顶在入队点裁剪，票 08）
///
/// 入队必须**先于** `peer-pull-files`：引擎铸造 batchId 后即经
/// `peer:receive-event` 的 `pull-started` 事件回流，事件处理早于本函数返回时
/// 待挂载队列尚无凭证 → `retry_meta` 永挂不上、该行不可重试。
pub(crate) fn push_pull_intent(
    list: &mut Vec<(String, RetryMeta)>,
    node_id: &str,
    meta: RetryMeta,
) {
    list.push((node_id.to_string(), meta));
    while list.len() > PULL_INTENT_CAP {
        list.remove(0);
    }
}

/// 拉取意图取出（按 node_id × rel_path 匹配，命中即消费）
///
/// 返回**收窄到单文件**的 Pull meta（多选拉取逐文件成批，故按单文件匹配而非
/// 整批文件集相等）。未命中返回 None（引擎铸造的批可能不属于本插件）。
pub(crate) fn take_pull_intent(
    list: &mut Vec<(String, RetryMeta)>,
    node_id: &str,
    rel_path: &str,
) -> Option<RetryMeta> {
    let idx = list.iter().position(|(node, meta)| {
        node == node_id
            && matches!(meta, RetryMeta::Pull { files, .. }
                if files.iter().any(|f| f.rel_path == rel_path))
    })?;
    let (_, meta) = list.remove(idx);
    let RetryMeta::Pull { dir_id, files } = meta else {
        return None;
    };
    let spec = files
        .into_iter()
        .find(|f| f.rel_path == rel_path)
        .unwrap_or(PullFileSpec {
            rel_path: rel_path.to_string(),
            size: 0,
        });
    Some(RetryMeta::Pull {
        dir_id,
        files: vec![spec],
    })
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    // 票 08 新增分组（重试判据 / 发送闸门 / 拉取意图队列）拆至
    // `transfer_store/tests/`：子模块经 `use super::*` 可见全部纯函数
    mod retry_and_gate;

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
    fn mark_active_interrupted_covers_running_pending_and_paused() {
        // 引擎侧会话已死（插件重启 / 节点下线）：三类活跃态全部标注 interrupted，
        // 终态不动
        let mut entries = vec![
            serde_json::from_value::<TransferEntry>(dto("r", "send", "running", 1)).unwrap(),
            serde_json::from_value::<TransferEntry>(dto("p", "receive", "pending", 2)).unwrap(),
            serde_json::from_value::<TransferEntry>(json!({
                "batchId": "pz", "direction": "receive", "status": "paused",
            }))
            .unwrap(),
            serde_json::from_value::<TransferEntry>(dto("c", "send", "completed", 3)).unwrap(),
        ];
        assert_eq!(mark_active_interrupted(&mut entries), 3);
        assert_eq!(entries[0].status, "interrupted");
        assert_eq!(entries[1].status, "interrupted");
        assert_eq!(entries[2].status, "interrupted", "paused 同样标注（保留会永久卡死）");
        assert_eq!(entries[3].status, "completed");
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
        // paused 显示在「正在发送」队列（用户可继续/取消）
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

    // ==================== 事件归约（票 06） ====================

    fn progress(batch: &str, transferred: u64, total: u64, ts: u64) -> Value {
        json!({ "kind": "progress", "batchId": batch, "transferred": transferred,
                "total": total, "rateBps": 8.0, "tsMs": ts })
    }

    fn terminal(batch: &str, state: Value, ts: u64) -> Value {
        json!({ "kind": "terminal", "batchId": batch, "state": state, "tsMs": ts })
    }

    /// pull-served 建行：本端供流的记账行由事件自建（宿主不再代记 send 任务），
    /// 且不可重试（无 retryMeta —— 源清单只有接收方持有）
    #[test]
    fn pull_served_creates_non_retryable_send_row() {
        let mut store = vec![];
        let event = json!({
            "kind": "pull-served", "batchId": "b-1", "nodeId": "aa",
            "files": [{"path": "x.bin", "size": 9}], "totalSize": 9, "tsMs": 7u64,
        });
        assert!(reduce_event(&mut store, "send", &event, "Pixel"));
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].direction, "send");
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].peer_name, "Pixel");
        assert_eq!(store[0].total_bytes, 9);
        assert!(store[0].retry_meta.is_none(), "供流记账行不可重试");
        // 重复事件不重建
        assert!(!reduce_event(&mut store, "send", &event, "Pixel"));
        assert_eq!(store.len(), 1);
    }

    /// progress 归约：pending→running、字节/速率/总量补正；无建行信息的事件
    /// 不凭空建行（由首屏 active-transfers 兜底）
    #[test]
    fn progress_advances_known_batch_only() {
        let mut store = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "b-1", "direction": "send", "status": "pending",
        }))
        .unwrap()];
        assert!(reduce_event(&mut store, "send", &progress("b-1", 40, 100, 5), ""));
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].transferred_bytes, 40);
        assert_eq!(store[0].total_bytes, 100);
        assert_eq!(store[0].updated_at_ms, 5);
        // 未知批次不建行
        assert!(!reduce_event(&mut store, "send", &progress("ghost", 1, 2, 6), ""));
        assert_eq!(store.len(), 1);
    }

    /// progress 不把用户暂停打回 running（残留 Progress 会让「恢复」弹回「暂停」）
    #[test]
    fn progress_keeps_paused_entries_paused() {
        let mut store = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "b-1", "direction": "send", "status": "paused",
        }))
        .unwrap()];
        assert!(reduce_event(&mut store, "send", &progress("b-1", 10, 100, 5), ""));
        assert_eq!(store[0].status, "paused");
        // 字节照常更新，供恢复后进度衔接
        assert_eq!(store[0].transferred_bytes, 10);
    }

    /// terminal 归约 + 原因码映射单点：send 与 receive 方向的「对端取消」
    /// 语义不同（-receiver / -sender），本端取消恒 -self
    #[test]
    fn terminal_maps_reason_codes_per_direction() {
        let mk = |batch: &str| {
            vec![serde_json::from_value::<TransferEntry>(json!({
                "batchId": batch, "direction": "send", "status": "running",
                "totalBytes": 10u64, "transferredBytes": 4u64,
            }))
            .unwrap()]
        };
        let cancelled = json!({ "type": "cancelled", "byPeer": true });
        let mut send = mk("s");
        assert!(reduce_event(&mut send, "send", &terminal("s", cancelled.clone(), 9), ""));
        assert_eq!(send[0].status, "cancelled");
        assert_eq!(send[0].detail.as_deref(), Some("cancelled-by-receiver"));

        let mut recv = mk("r");
        assert!(reduce_event(&mut recv, "receive", &terminal("r", cancelled, 9), ""));
        assert_eq!(recv[0].detail.as_deref(), Some("cancelled-by-sender"));

        let mut self_cancel = mk("x");
        assert!(reduce_event(
            &mut self_cancel,
            "send",
            &terminal("x", json!({ "type": "cancelled", "byPeer": false }), 9),
            ""
        ));
        assert_eq!(self_cancel[0].detail.as_deref(), Some("cancelled-by-self"));
    }

    /// terminal 结算细节：completed 归整满额、rejected 透传引擎 reason、
    /// failed 落 detail、未知 type 兜底 failed
    #[test]
    fn terminal_settles_each_engine_state() {
        let mk = || {
            vec![serde_json::from_value::<TransferEntry>(json!({
                "batchId": "b", "direction": "send", "status": "running",
                "totalBytes": 10u64, "transferredBytes": 9u64,
            }))
            .unwrap()]
        };
        let mut done = mk();
        assert!(reduce_event(&mut done, "send", &terminal("b", json!({ "type": "completed" }), 3), ""));
        assert_eq!(done[0].status, "completed");
        assert_eq!(done[0].transferred_bytes, 10, "completed 归整为满额");
        assert_eq!(done[0].rate_bps, 0.0);

        let mut rejected = mk();
        assert!(reduce_event(
            &mut rejected,
            "send",
            &terminal("b", json!({ "type": "rejected", "reason": "UserRejected" }), 3),
            ""
        ));
        assert_eq!(rejected[0].status, "rejected");
        assert_eq!(rejected[0].reject_reason.as_deref(), Some("UserRejected"));

        let mut failed = mk();
        assert!(reduce_event(
            &mut failed,
            "send",
            &terminal("b", json!({ "type": "failed", "detail": "io" }), 3),
            ""
        ));
        assert_eq!(failed[0].status, "failed");
        assert_eq!(failed[0].detail.as_deref(), Some("io"));

        let mut unknown = mk();
        assert!(reduce_event(&mut unknown, "send", &terminal("b", json!({}), 3), ""));
        assert_eq!(unknown[0].status, "failed");
        assert_eq!(unknown[0].detail.as_deref(), Some("unknown terminal"));
    }

    /// 用户暂停分支：终态事件只是中断确认，不落终态；completed 且字节已满
    /// 是例外（UI 滞后点暂停的已完成任务应结算 completed）
    #[test]
    fn terminal_keeps_paused_entries_unless_fully_completed() {
        let mut paused = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "b", "direction": "send", "status": "paused",
            "totalBytes": 10u64, "transferredBytes": 4u64,
        }))
        .unwrap()];
        assert!(!reduce_event(&mut paused, "send", &terminal("b", json!({ "type": "completed" }), 3), ""));
        assert_eq!(paused[0].status, "paused");

        let mut full = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "b", "direction": "send", "status": "paused",
            "totalBytes": 10u64, "transferredBytes": 10u64,
        }))
        .unwrap()];
        assert!(reduce_event(&mut full, "send", &terminal("b", json!({ "type": "completed" }), 3), ""));
        assert_eq!(full[0].status, "completed");
    }

    /// paused/resumed 帧同步：只在合法迁移时改写；终态条目不被复活
    #[test]
    fn pause_frames_only_transition_legal_states() {
        let mk = |status: &str| {
            vec![serde_json::from_value::<TransferEntry>(json!({
                "batchId": "b", "direction": "send", "status": status,
            }))
            .unwrap()]
        };
        let paused_ev = json!({ "kind": "paused", "batchId": "b", "tsMs": 4u64 });
        let resumed_ev = json!({ "kind": "resumed", "batchId": "b", "tsMs": 5u64 });

        let mut running = mk("running");
        assert!(reduce_event(&mut running, "send", &paused_ev, ""));
        assert_eq!(running[0].status, "paused");
        assert!(reduce_event(&mut running, "send", &resumed_ev, ""));
        assert_eq!(running[0].status, "running");
        // 已 running 再收 resumed：幂等不改写
        assert!(!reduce_event(&mut running, "send", &resumed_ev, ""));

        let mut done = mk("completed");
        assert!(!reduce_event(&mut done, "send", &paused_ev, ""));
        assert_eq!(done[0].status, "completed");
    }

    /// 首屏兜底：active-transfers 投影行补占位（激活晚于事件时），已存在
    /// batchId 幂等跳过、终态批不入店
    #[test]
    fn insert_active_projections_bootstraps_missing_rows_only() {
        let mut store = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "known", "direction": "send", "status": "running",
        }))
        .unwrap()];
        let rows = vec![
            json!({ "batchId": "known", "direction": "send", "status": "running" }),
            json!({ "batchId": "fresh", "direction": "send", "status": "paused",
                    "totalBytes": 10u64, "transferredBytes": 3u64, "updatedAtMs": 12u64 }),
            json!({ "batchId": "old", "direction": "send", "status": "completed" }),
        ];
        assert_eq!(insert_active_projections(&mut store, &rows, "Pixel"), 1);
        let fresh = store.iter().find(|e| e.batch_id == "fresh").expect("fresh row");
        assert_eq!(fresh.status, "paused");
        assert_eq!(fresh.total_bytes, 10);
        assert_eq!(fresh.transferred_bytes, 3);
        assert_eq!(fresh.updated_at_ms, 12);
        assert!(store.iter().all(|e| e.batch_id != "old"), "终态批不入店");
    }

    // ==================== 事件归约 · 接收方向锚点（票 07） ====================

    /// offer-pending 建行：入站待应答行（status=pending，方向 receive），
    /// 重复事件幂等不入店；offer 事实自带文件清单与总量，无需快照补全
    #[test]
    fn offer_pending_creates_pending_receive_row() {
        let mut store = vec![];
        let event = json!({
            "kind": "offer-pending", "batchId": "of-1", "nodeId": "aa",
            "files": [{"path": "a.txt", "size": 10}, {"path": "b.txt", "size": 5}],
            "totalSize": 15u64, "tsMs": 7u64,
        });
        assert!(reduce_event(&mut store, "receive", &event, "Pixel"));
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].direction, "receive");
        assert_eq!(store[0].status, "pending");
        assert_eq!(store[0].peer_name, "Pixel");
        assert_eq!(store[0].node_id, "aa");
        assert_eq!(store[0].total_bytes, 15);
        assert_eq!(store[0].files.len(), 2);
        assert_eq!(store[0].created_at_ms, 7);
        assert!(store[0].retry_meta.is_none(), "入站待应答不可重试（源清单在对端）");
        // 重复事件不重建
        assert!(!reduce_event(&mut store, "receive", &event, "Pixel"));
        assert_eq!(store.len(), 1);
    }

    /// pull-started 建行：本端拉取批次（status=running，直接进接收视图）；
    /// 待应答视图（pending）不得收它
    #[test]
    fn pull_started_creates_running_receive_row() {
        let mut store = vec![];
        let event = json!({
            "kind": "pull-started", "batchId": "pull-9-0", "nodeId": "bb",
            "files": [{"path": "docs/x.bin", "size": 42}], "totalSize": 42u64, "tsMs": 9u64,
        });
        assert!(reduce_event(&mut store, "receive", &event, ""));
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].total_bytes, 42);
        assert_eq!(store[0].files[0]["path"], "docs/x.bin");
        let recv: Vec<&str> =
            active_receive_entries(&store).iter().map(|e| e.batch_id.as_str()).collect();
        assert_eq!(recv, vec!["pull-9-0"], "拉取批次进接收视图而非待应答");
    }

    /// offer-pending → progress 推进：待应答批应答后进数据面（pending→running），
    /// 字节/总量随引擎 Progress 补正
    #[test]
    fn offer_pending_progress_advances_after_user_reply() {
        let mut store = vec![serde_json::from_value::<TransferEntry>(json!({
            "batchId": "of-1", "direction": "receive", "status": "pending",
        }))
        .unwrap()];
        assert!(reduce_event(
            &mut store,
            "receive",
            &progress("of-1", 30, 100, 4),
            ""
        ));
        assert_eq!(store[0].status, "running");
        assert_eq!(store[0].transferred_bytes, 30);
        assert_eq!(store[0].total_bytes, 100);
    }
}
