//! 传输任务/历史自持存储（issue 13 Phase 3 步骤 3）
//!
//! 插件自有「产品视图」：以引擎 `peer:transfer` / `peer:receive` 全量快照
//! 事件为进度真源，按 batchId merge 进本店；终态归档 + 200 条封顶滚动淘汰；
//! 重启恢复时 running/pending 态条目标注 `interrupted`（原批已死，如实呈现）。
//!
//! 时间戳纪律：全部取自引擎事件载荷（createdAtMs/updatedAtMs），本地无时钟
//! （wasm32-unknown-unknown 禁 std::time）。纯函数核心与宿主 I/O 分离，
//! cargo test 直测。

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

    /// 是否进行中（含接收待应答）
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "running" | "pending")
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

/// 重启恢复标注：载入持久层后把 running/pending 改标 `interrupted`。
/// 与 prune_absent 不同，它不依赖快照（activate 时快照未到先渲染）。
pub(crate) fn mark_interrupted_on_load(entries: &mut [TransferEntry]) -> usize {
    let mut marked = 0;
    for e in entries.iter_mut() {
        if e.status == "running" || e.status == "pending" {
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

/// 接收视图（receiving-changed / list-receiving 共用）：仅正在接收的
/// running 条目；pending 归待应答（batches），终态归历史视图。
pub(crate) fn active_receive_entries(entries: &[TransferEntry]) -> Vec<&TransferEntry> {
    entries
        .iter()
        .filter(|e| e.direction == "receive" && e.status == "running")
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
}
