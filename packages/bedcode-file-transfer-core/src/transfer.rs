//! 传输任务台账：任务/历史真源（事件归约 + 判据单点）。
//!
//! 双端**同源**实现：以引擎原始事件为唯一任务真源——发送方向吃
//! `peer:transfer-event`，接收方向吃 `peer:receive-event`；按 `batch_id` 归约进本店；
//! 终态归档 + [`HISTORY_CAP`] 条封顶滚动淘汰；重启恢复时 running/pending/paused 态条目
//! 标注 `interrupted`（原批已死，如实呈现）。
//!
//! 判据单点（编排层只调引擎与派发视图，不自带第二份判据）：
//! [`retry_source`] 重试回放判据 · [`send_slot_open`] 发送闸门判据 ·
//! [`push_pull_intent`] / [`take_pull_intent`] 拉取意图队列。
//!
//! 原因码（`cancelled-by-*`）与终态判定只在本文件实现——宿主不再持有产品语义。
//!
//! **时间戳纪律**：全部取自引擎事件载荷（`tsMs` / `updatedAtMs`），本地无时钟
//! （wasm32-unknown-unknown 禁 `std::time`）。
//!
//! ## 双端差异（都在本模块之外）
//!
//! 本模块全部是纯函数与领域类型，无宿主 I/O，故**没有端口**：持久化（桌面 plugin-db 表 /
//! 移动 KV 键）与事件订阅面（桌面仍收旧快照 topic）留在各端编排层。
//! 其中两条是**桌面独有**的历史遗留通路（双写期对账），移动端早已退役，故只保函数、
//! 不参与共享编排：[`prune_absent`]（快照缺席剪枝）、[`reconcile_diff`]（对账偏差清单）。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 历史封顶：终态条目超过即按 `updatedAtMs` 最旧先出
pub const HISTORY_CAP: usize = 200;

/// 拉取意图队列封顶（防失控增长；超出即丢最旧）
///
/// 意图 = 「本插件发起过一次拉取」的待挂载凭证：`batch_id` 由引擎铸造，调用点拿不到，
/// 只能事后凭 `node_id + rel_path` 匹配挂回 `retry_meta`。
pub const PULL_INTENT_CAP: usize = 8;

/// 重试元数据（仅发起方条目携带）：send 记源路径清单；pull 记共享根 + 文件清单
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RetryMeta {
    /// 推送发送：原始本地路径（重试时回放 send-files）
    Send { paths: Vec<String> },
    /// 远端拉取：共享根 + 文件清单（重试时回放 pull-files）
    Pull {
        dir_id: String,
        files: Vec<PullFileSpec>,
    },
}

/// 拉取文件规格（`retry_meta` 回放所需最小集）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullFileSpec {
    pub rel_path: String,
    #[serde(default)]
    pub size: u64,
}

/// 单条传输条目 —— 引擎事件事实的 camelCase 形状 + 插件扩展字段。
///
/// 直接作为对外 wire 形状（事件载荷 / 列表命令返回），不再二次翻译。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferEntry {
    pub batch_id: String,
    #[serde(default)]
    pub node_id: String,
    #[serde(default)]
    pub peer_name: String,
    /// `send` | `receive`
    pub direction: String,
    /// running / pending / paused / completed / failed / rejected / cancelled / interrupted
    pub status: String,
    #[serde(default)]
    pub files: Vec<Value>,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub transferred_bytes: u64,
    #[serde(default)]
    pub rate_bps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_meta: Option<RetryMeta>,
    /// 本机落盘路径（接收方向 completed 归档时宿主按需填充；仅 completed 且本地文件
    /// 仍存在时非空，缺省 None 保持向后兼容）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

impl TransferEntry {
    fn from_dto(dto: &Value) -> Option<TransferEntry> {
        let batch_id = dto.get("batchId")?.as_str()?.to_string();
        Some(TransferEntry {
            batch_id,
            node_id: dto
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            peer_name: dto
                .get("peerName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            direction: dto
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("send")
                .to_string(),
            status: dto
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("running")
                .to_string(),
            files: dto
                .get("files")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            total_bytes: dto.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            transferred_bytes: dto
                .get("transferredBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            rate_bps: dto.get("rateBps").and_then(|v| v.as_f64()).unwrap_or(0.0),
            detail: dto
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            reject_reason: dto
                .get("rejectReason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
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

    /// 是否终态（`interrupted` 视作可重试的终态变体）
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "completed" | "failed" | "rejected" | "cancelled" | "interrupted"
        )
    }

    /// 是否进行中（含接收待应答与用户暂停）
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "pending" | "paused")
    }
}

/// DTO 解析出口（命令编排层入店用）
pub fn entry_from_dto(dto: &Value) -> Option<TransferEntry> {
    TransferEntry::from_dto(dto)
}

// ==================== 合并 / 校正 ====================

/// 条目合并：按 `batch_id` upsert；返回是否发生任何变更。
///
/// 两个消费面共用同一语义（**upsert + 保留本地 `retryMeta` + 终态不被迟到旧值复活**）：
///
/// - 移动端与桌面端「本端发起的会话占位」：`enqueue` / `retry` 拿到传输句柄时先入店最小
///   条目，随后由引擎事件补全明细；
/// - 桌面端旧快照通路（双写期）：`peer:transfer` / `peer:receive` 全量快照作为校正源。
///   移动端该通路已随宿主接收任务表整条退役（快照是「第二真源」，留着会让归约态与快照
///   互相覆盖、掩盖事件丢失）。
///
/// `interrupted` 是插件本地标记，允许被引擎后续快照覆盖回真实状态（插件重启后原批仍在跑
/// 的场景，以引擎为准）；引擎真实终态（completed/failed/rejected/cancelled）不被复活。
pub fn merge_snapshot(store: &mut Vec<TransferEntry>, snapshot: &[Value]) -> bool {
    let mut changed = false;
    for dto in snapshot {
        let Some(incoming) = TransferEntry::from_dto(dto) else {
            continue;
        };
        if let Some(existing) = store.iter_mut().find(|e| e.batch_id == incoming.batch_id) {
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
///
/// **桌面端旧快照通路专用**（双写期对账）；移动端该通路已退役，不调用本函数。
/// 与 [`mark_active_interrupted`] 的区别：这里依赖快照判定「谁缺席」，后者不依赖快照
/// （activate 时快照未到先渲染）。
pub fn prune_absent(
    store: &mut [TransferEntry],
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

/// 对账：归约态（store）与引擎快照在**活跃批**上的差异清单（空列表 = 两路一致）。
///
/// **桌面端旧快照通路专用**（双写验收）：调用方对账后照常 merge，本函数只产出可观测偏差
/// （warn 日志），不决定写谁。
pub fn reconcile_diff(store: &[TransferEntry], snapshot: &[Value], direction: &str) -> Vec<String> {
    let mut diffs = Vec::new();
    let mut snap_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for dto in snapshot {
        let Some(id) = dto.get("batchId").and_then(|v| v.as_str()) else {
            continue;
        };
        snap_ids.insert(id);
        let store_entry = store.iter().find(|e| e.batch_id == id);
        match store_entry {
            None => diffs.push(format!("{direction}:{id} missing-in-reduced")),
            Some(entry) if entry.is_terminal() => diffs.push(format!(
                "{direction}:{id} reduced-terminal-but-snapshot-active"
            )),
            Some(entry) => {
                let snap_status = dto.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if entry.status != snap_status {
                    diffs.push(format!(
                        "{direction}:{id} status {} != {snap_status}",
                        entry.status
                    ));
                }
                let snap_bytes = dto
                    .get("transferredBytes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if entry.transferred_bytes != snap_bytes {
                    diffs.push(format!(
                        "{direction}:{id} bytes {} != {snap_bytes}",
                        entry.transferred_bytes
                    ));
                }
            }
        }
    }
    for entry in store
        .iter()
        .filter(|e| e.direction == direction && e.is_active())
    {
        if !snap_ids.contains(entry.batch_id.as_str()) {
            diffs.push(format!(
                "{direction}:{} missing-in-snapshot",
                entry.batch_id
            ));
        }
    }
    diffs
}

// ==================== 事件归约 ====================

/// 终态结算：`TerminalState` wire 形状 → `(status, detail, reject_reason)`。
///
/// 原因码映射单点（本店即真源，宿主不再映射）：
/// - send 方向 `cancelled`：`byPeer` = 对端（拉取方/接收方）取消 → `cancelled-by-receiver`；
///   本端取消 → `cancelled-by-self`；
/// - receive 方向 `cancelled`：`byPeer` = 对端（发送方）取消 → `cancelled-by-sender`；
///   本端取消 → `cancelled-by-self`；
/// - `rejected` 的 reason 是引擎 wire 枚举（UserRejected/Timeout/…），原样透传。
fn terminal_status_of(direction: &str, state: &Value) -> (String, Option<String>, Option<String>) {
    match state.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "completed" => ("completed".to_string(), None, None),
        "rejected" => (
            "rejected".to_string(),
            None,
            state
                .get("reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        ),
        "cancelled" => {
            let by_peer = state
                .get("byPeer")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            // 对端取消：send 方向的「对端」是接收方（-receiver）、receive 方向的「对端」是
            // 发送方（-sender）；本端取消恒 -self
            let code = if by_peer {
                if direction == "send" {
                    "cancelled-by-receiver"
                } else {
                    "cancelled-by-sender"
                }
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

/// 事件归约：引擎原始事件（`peer:transfer-event` / `peer:receive-event`）→ store 状态推进。
///
/// `direction` 按事件 topic 定向；`peer_name` 仅建行事件需要（运行时从设备缓存解析后传入，
/// 纯函数不做 I/O）。返回是否发生变更。
///
/// - **建行锚点**：`pull-served`（send 供流记账）、`offer-pending`（入站待应答，
///   status=pending）、`pull-started`（本端拉取批次，status=running）——三者都是
///   「只有引擎知道 batch_id」的锚点，故由引擎事件建行，插件不自铸无主会话；
/// - **推进事件**：`progress`（pending→running、字节/速率/总量补正、paused 保持）、
///   `terminal`（结算；paused 保留——用户暂停语义，completed+满字节例外）、
///   `paused` / `resumed`（wire 帧同步）；
/// - 无建行信息的 `progress` / `terminal`（插件激活晚于会话发起）**不凭空建行**，
///   由首屏 `active-transfers` 兜底查询补占位（[`insert_active_projections`]）。
pub fn reduce_event(
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
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else {
                return false;
            };
            if store.iter().any(|e| e.batch_id == batch_id) {
                return false;
            }
            store.insert(
                0,
                TransferEntry {
                    batch_id: batch_id.to_string(),
                    node_id: event
                        .get("nodeId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    peer_name: peer_name.to_string(),
                    direction: direction.to_string(),
                    // 入站 offer 待应答（pending 归待应答视图）；供流记账与本端
                    // 拉取都是数据面已建立（running）
                    status: if kind == "offer-pending" {
                        "pending"
                    } else {
                        "running"
                    }
                    .to_string(),
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
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else {
                return false;
            };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else {
                return false;
            };
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
            entry.rate_bps = event
                .get("rateBps")
                .and_then(|v| v.as_f64())
                .unwrap_or(entry.rate_bps);
            entry.updated_at_ms = ts;
            true
        }
        "paused" | "resumed" => {
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else {
                return false;
            };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else {
                return false;
            };
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
            let Some(batch_id) = event.get("batchId").and_then(|v| v.as_str()) else {
                return false;
            };
            let Some(state) = event.get("state") else {
                return false;
            };
            let Some(entry) = store.iter_mut().find(|e| e.batch_id == batch_id) else {
                return false;
            };
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

/// 首屏兜底占位（`active-transfers` 投影行）：把宿主句柄表在册的活跃批补成最小占位行
/// （缺 `peer_name` / `files`；progress 事件随后补全）。方向取行内 `direction`（缺省 send）。
/// 已存在的 `batch_id` 幂等跳过。返回插入数量。
pub fn insert_active_projections(
    store: &mut Vec<TransferEntry>,
    rows: &[Value],
    peer_name: &str,
) -> usize {
    let mut inserted = 0;
    for row in rows {
        let Some(batch_id) = row.get("batchId").and_then(|v| v.as_str()) else {
            continue;
        };
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
            node_id: row
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            peer_name: peer_name.to_string(),
            direction: row
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("send")
                .to_string(),
            status: row
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("running")
                .to_string(),
            files: vec![],
            total_bytes: row.get("totalBytes").and_then(|v| v.as_u64()).unwrap_or(0),
            transferred_bytes: row
                .get("transferredBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
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

// ==================== 生命周期标注 / 视图 / 乐观结算 ====================

/// 中断标注：把在册进行中条目（running / pending / paused）改标 `interrupted`。
///
/// 两个调用点，同一语义——「引擎侧会话已死，如实呈现，不假装还在跑」：
/// - 载入持久层（插件重启，原批随宿主进程消亡）；
/// - 收到 `node-stopped` 事件（引擎节点下线，在飞批全部中止）。
///
/// paused 同样标注：会话已死，保留 paused 会永久卡死（无 Resume 可写）。
pub fn mark_active_interrupted(entries: &mut [TransferEntry]) -> usize {
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

/// 终态封顶滚动淘汰：按 `updatedAtMs` 升序（最旧先出），超出 [`HISTORY_CAP`] 的最旧终态
/// 条目移除。返回移除数量。
pub fn evict_overflow(entries: &mut Vec<TransferEntry>) -> usize {
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
pub fn clear_terminal(entries: &mut Vec<TransferEntry>) -> usize {
    let before = entries.len();
    entries.retain(|e| !e.is_terminal());
    before - entries.len()
}

/// 发送视图（`tasks-changed` / `list-tasks` 共用）：仅进行中条目（running/pending/paused）；
/// 终态条目一律归历史视图，不再滞留活动队列。
pub fn active_send_entries(entries: &[TransferEntry]) -> Vec<&TransferEntry> {
    entries
        .iter()
        .filter(|e| e.direction == "send" && e.is_active())
        .collect()
}

/// 接收视图（`receiving-changed` / `list-receiving` 共用）：正在接收的 running 条目 +
/// 用户暂停的 paused 条目（暂停也是活跃态，须留在接收队列里供继续/取消）；
/// pending 归待应答（batches），终态归历史视图。
pub fn active_receive_entries(entries: &[TransferEntry]) -> Vec<&TransferEntry> {
    entries
        .iter()
        .filter(|e| e.direction == "receive" && matches!(e.status.as_str(), "running" | "paused"))
        .collect()
}

/// 取消乐观结算：命中条目改标 `cancelled`（引擎事件随后校正/确认）。
/// 仅对进行中条目生效。返回是否命中。
pub fn mark_cancelled(entries: &mut [TransferEntry], batch_id: &str) -> bool {
    for e in entries.iter_mut() {
        if e.batch_id == batch_id && e.is_active() {
            e.status = "cancelled".to_string();
            e.rate_bps = 0.0;
            return true;
        }
    }
    false
}

/// 暂停乐观标记：running 条目改标 `paused`（引擎事件随后确认；暂停释放并发槽）。
pub fn mark_paused(entries: &mut [TransferEntry], batch_id: &str) -> bool {
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
/// 只允许携带 `retry_meta` 的终态条目（即本端发起的失败/被拒/取消/中断批）。
/// 返回是否命中。
pub fn apply_retry(
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

// ==================== 重试判据 / 发送闸门 ====================

/// 重试拒绝原因（可重试判据的失败分类，判据单点化）
///
/// 三类分开而非一个 `None`：前端「点了没反应」需要能区分「条目不存在」
/// 「还在跑（不该重试）」与「不是本端发起（无回放凭证）」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryRefusal {
    /// 店内无此 batchId
    NotFound,
    /// 条目仍在进行中（running / pending / paused）——重试前提是终态
    NotTerminal,
    /// 终态但无 retryMeta（非本端发起：供流记账行、入站待应答批）
    MissingMeta,
}

impl RetryRefusal {
    pub fn message(&self, task_id: &str) -> String {
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

/// 重试回放源解析（重试判据单点）
///
/// 必须在**调引擎之前**求值：send 方向无引擎建行事件（行由插件发起时自建），对不可重试
/// 条目先发后校验会铸出无主会话——永不入店、进度事件打不中行，且占死发送闸门槽位。
/// 返回 `(node_id, retryMeta)` 供回放编排使用。
pub fn retry_source(
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

/// 发送闸门判据：running 批数 < 设置并发上限。
/// 下限 1：并发配置缺失/为 0 时仍允许一个槽位（闸门不得把发送全锁死）。
pub fn send_slot_open(running: usize, limit: u8) -> bool {
    running < limit.max(1) as usize
}

// ==================== 拉取意图队列 ====================

/// 拉取意图入队（封顶在入队点裁剪）
///
/// 入队必须**先于** `peer-pull-files`：引擎铸造 `batchId` 后即经 `peer:receive-event` 的
/// `pull-started` 事件回流，事件处理早于本函数返回时待挂载队列尚无凭证 →
/// `retry_meta` 永挂不上、该行不可重试。
pub fn push_pull_intent(list: &mut Vec<(String, RetryMeta)>, node_id: &str, meta: RetryMeta) {
    list.push((node_id.to_string(), meta));
    while list.len() > PULL_INTENT_CAP {
        list.remove(0);
    }
}

/// 拉取意图取出（按 `node_id` × `rel_path` 匹配，命中即消费）
///
/// 返回**收窄到单文件**的 Pull meta（多选拉取逐文件成批，故按单文件匹配而非整批文件集
/// 相等）。未命中返回 `None`（引擎铸造的批可能不属于本插件）。
pub fn take_pull_intent(
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
mod tests;
