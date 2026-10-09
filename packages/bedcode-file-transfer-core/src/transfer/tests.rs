//! 传输台账行为契约（双端实现的并集：移动端票 06/07/08 修正版为基线，
//! 外加桌面端旧快照通路专有的两条纯函数用例）。
//!
//! 每条用例名即契约陈述；跨端迁移时名称保持不变，便于对照旧位置。

use super::*;
use serde_json::{json, Value};

fn dto(id: &str, direction: &str, status: &str, updated: u64) -> Value {
    json!({
        "batchId": id, "nodeId": "aa", "peerName": "Pixel",
        "direction": direction, "status": status,
        "files": [{"path": "a.txt", "size": 10}],
        "totalBytes": 10u64, "transferredBytes": 5u64, "rateBps": 12.5,
        "createdAtMs": 1u64, "updatedAtMs": updated,
    })
}

fn entry(v: Value) -> TransferEntry {
    serde_json::from_value(v).expect("entry fixture")
}

fn mk(id: &str, direction: &str, status: &str) -> TransferEntry {
    entry(json!({ "batchId": id, "direction": direction, "status": status }))
}

fn send_terminal(id: &str, status: &str, meta: Option<RetryMeta>) -> TransferEntry {
    let mut e = entry(json!({
        "batchId": id, "nodeId": "aa", "direction": "send", "status": status,
    }));
    e.retry_meta = meta;
    e
}

fn send_meta() -> RetryMeta {
    RetryMeta::Send {
        paths: vec!["/sdcard/a.bin".into()],
    }
}

fn progress(batch: &str, transferred: u64, total: u64, ts: u64) -> Value {
    json!({ "kind": "progress", "batchId": batch, "transferred": transferred,
            "total": total, "rateBps": 8.0, "tsMs": ts })
}

fn terminal(batch: &str, state: Value, ts: u64) -> Value {
    json!({ "kind": "terminal", "batchId": batch, "state": state, "tsMs": ts })
}

// ==================== 形状与判据 ====================

#[test]
fn entry_wire_shape_is_camel_case_and_tolerates_partial_payload() {
    let e = entry(json!({ "batchId": "b", "direction": "send", "status": "running" }));
    assert_eq!(e.node_id, "");
    assert_eq!(e.total_bytes, 0);
    assert_eq!(e.rate_bps, 0.0);
    assert!(e.retry_meta.is_none());
    assert!(e.local_path.is_none(), "缺省 None 保持向后兼容");
    // 两条入店路径语义**不同**，必须分开钉：
    // ① 引擎 DTO 路径（entry_from_dto）把空 localPath 归一为 None——避免前端拿到空字符串；
    let empty_dto =
        json!({ "batchId": "b", "direction": "receive", "status": "completed", "localPath": "" });
    assert!(entry_from_dto(&empty_dto).unwrap().local_path.is_none());
    // ② 条目行反序列化（落盘回读）原样保留——历史行不被「读一次就改写」
    assert_eq!(entry(empty_dto).local_path.as_deref(), Some(""));
    let with_path = entry(
        json!({ "batchId": "b", "direction": "receive", "status": "completed", "localPath": "/dl/a.bin" }),
    );
    assert_eq!(with_path.local_path.as_deref(), Some("/dl/a.bin"));
    assert_eq!(
        entry_from_dto(
            &json!({ "batchId": "b", "direction": "receive", "status": "completed", "localPath": "/dl/a.bin" })
        )
        .unwrap()
        .local_path
        .as_deref(),
        Some("/dl/a.bin")
    );
    // 序列化形状（前端与落盘共用）
    let v = serde_json::to_value(&with_path).unwrap();
    assert_eq!(v["batchId"], "b");
    assert_eq!(v["localPath"], "/dl/a.bin");
    // retryMeta 是 tagged enum，wire 形状跨版本稳定
    let mut with_meta = with_path.clone();
    with_meta.retry_meta = Some(RetryMeta::Send {
        paths: vec!["C:/a".into()],
    });
    let mv = serde_json::to_value(&with_meta).unwrap();
    assert_eq!(mv["retryMeta"]["kind"], "send");
    assert_eq!(
        entry(mv).retry_meta,
        Some(RetryMeta::Send {
            paths: vec!["C:/a".into()]
        })
    );
}

#[test]
fn terminal_and_active_predicates_partition_every_status() {
    for s in [
        "completed",
        "failed",
        "rejected",
        "cancelled",
        "interrupted",
    ] {
        let e = mk("b", "send", s);
        assert!(e.is_terminal(), "{s} 是终态");
        assert!(!e.is_active(), "{s} 不是活跃态");
    }
    for s in ["running", "pending", "paused"] {
        let e = mk("b", "send", s);
        assert!(e.is_active(), "{s} 是活跃态");
        assert!(!e.is_terminal(), "{s} 不是终态");
    }
    // 两个判据必须互斥且覆盖全部状态字面量（新增状态忘了归类会被这里抓到）
    let unknown = mk("b", "send", "whatever");
    assert!(!unknown.is_terminal() && !unknown.is_active());
}

// ==================== 合并 ====================

#[test]
fn merge_upserts_by_batch_id_and_keeps_retry_meta() {
    let mut store = vec![];
    assert!(merge_snapshot(
        &mut store,
        &[dto("b1", "send", "running", 10)]
    ));
    store[0].retry_meta = Some(RetryMeta::Send {
        paths: vec!["C:/a.txt".into()],
    });
    // 同内容重放不算变更
    assert!(!merge_snapshot(&mut store, &[]));
    assert!(!merge_snapshot(
        &mut store,
        &[dto("b1", "send", "running", 10)]
    ));
    assert!(merge_snapshot(
        &mut store,
        &[dto("b1", "send", "running", 20)]
    ));
    assert_eq!(store.len(), 1);
    assert_eq!(store[0].updated_at_ms, 20);
    assert_eq!(
        store[0].retry_meta,
        Some(RetryMeta::Send {
            paths: vec!["C:/a.txt".into()]
        })
    );
    // 新批追加
    assert!(merge_snapshot(
        &mut store,
        &[dto("b2", "send", "running", 30)]
    ));
    assert_eq!(store.len(), 2);
}

#[test]
fn interrupted_entry_revived_by_fresh_engine_snapshot() {
    // 插件重启标注 interrupted 后，引擎仍报 running 的批次以引擎为准恢复
    let mut store = vec![entry(
        json!({ "batchId": "live", "direction": "send", "status": "interrupted" }),
    )];
    assert!(merge_snapshot(
        &mut store,
        &[dto("live", "send", "running", 50)]
    ));
    assert_eq!(store[0].status, "running");
}

#[test]
fn terminal_entries_not_revived_by_stale_snapshot() {
    let mut store = vec![];
    merge_snapshot(&mut store, &[dto("b1", "send", "completed", 10)]);
    // 迟到的 running 快照不得复活终态
    assert!(!merge_snapshot(
        &mut store,
        &[dto("b1", "send", "running", 5)]
    ));
    assert_eq!(store[0].status, "completed");
}

// ==================== 旧快照通路（桌面双写期） ====================

#[test]
fn prune_absent_marks_only_active_of_direction() {
    let mut store = vec![
        entry(dto("run", "send", "running", 1)),
        entry(dto("done", "send", "completed", 2)),
        entry(dto("rcv", "receive", "running", 3)),
    ];
    // send 快照不含 run → run 标 interrupted；done 终态不动；receive 不看 send 快照
    let n = prune_absent(&mut store, &["done".to_string()], "send");
    assert_eq!(n, 1);
    assert_eq!(store[0].status, "interrupted");
    assert_eq!(store[1].status, "completed");
    assert_eq!(store[2].status, "running");
}

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
    assert!(
        diffs.iter().any(|d| d.contains("b1 bytes 0 != 5")),
        "字节漂移: {diffs:?}"
    );
    assert!(
        diffs.iter().any(|d| d.contains("b2 missing-in-reduced")),
        "快照有归约无: {diffs:?}"
    );
    // 一致时为空
    assert!(reconcile_diff(
        &store,
        &[json!({"batchId": "b1", "direction": "send", "status": "running", "transferredBytes": 0})],
        "send",
    )
    .is_empty());
    // 归约有、快照无
    let diffs = reconcile_diff(&store, &[], "send");
    assert!(diffs.iter().any(|d| d.contains("b1 missing-in-snapshot")));
}

// ==================== 生命周期标注 / 视图 ====================

#[test]
fn mark_active_interrupted_covers_running_pending_and_paused() {
    // 引擎侧会话已死（插件重启 / 节点下线）：三类活跃态全部标注 interrupted，终态不动
    let mut entries = vec![
        entry(dto("r", "send", "running", 1)),
        entry(dto("p", "receive", "pending", 2)),
        entry(json!({ "batchId": "pz", "direction": "receive", "status": "paused" })),
        entry(dto("c", "send", "completed", 3)),
    ];
    assert_eq!(mark_active_interrupted(&mut entries), 3);
    assert_eq!(entries[0].status, "interrupted");
    assert_eq!(entries[1].status, "interrupted");
    assert_eq!(
        entries[2].status, "interrupted",
        "paused 同样标注（保留会永久卡死）"
    );
    assert_eq!(entries[3].status, "completed");
}

#[test]
fn mark_paused_hits_running_only_and_keeps_progress() {
    let mut store = vec![
        entry(dto("run", "send", "running", 1)),
        entry(dto("pend", "send", "pending", 2)),
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
    let store = vec![
        mk("s-run", "send", "running"),
        mk("s-pause", "send", "paused"),
        mk("s-done", "send", "completed"),
        mk("r-pend", "receive", "pending"),
    ];
    let send_ids: Vec<&str> = active_send_entries(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(send_ids, vec!["s-run", "s-pause"]);
}

#[test]
fn active_receive_view_keeps_paused_and_excludes_other_states() {
    // 用户暂停的接收任务（下载/拉取）必须留在接收队列（可继续/取消）；
    // pending 归待应答、终态归历史，均不得出现在接收视图
    let store = vec![
        mk("r-run", "receive", "running"),
        mk("r-pause", "receive", "paused"),
        mk("r-pend", "receive", "pending"),
        mk("r-done", "receive", "completed"),
        mk("r-fail", "receive", "failed"),
    ];
    let recv_ids: Vec<&str> = active_receive_entries(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(recv_ids, vec!["r-run", "r-pause"]);
}

#[test]
fn active_views_exclude_terminal_entries() {
    let store = vec![
        mk("s-run", "send", "running"),
        mk("s-done", "send", "completed"),
        mk("s-fail", "send", "failed"),
        mk("s-inter", "send", "interrupted"),
        mk("r-pend", "receive", "pending"),
        mk("r-run", "receive", "running"),
        mk("r-done", "receive", "completed"),
    ];
    let send_ids: Vec<&str> = active_send_entries(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(send_ids, vec!["s-run"]);
    let recv_ids: Vec<&str> = active_receive_entries(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(recv_ids, vec!["r-run"]);
}

#[test]
fn evict_overflow_evicts_oldest_terminal_first() {
    let mut store = vec![];
    // HISTORY_CAP + 2 条终态 + 1 条进行中：最旧两条终态被逐出，进行中保留
    for i in 0..(HISTORY_CAP as u64 + 2) {
        store.push(entry(json!({
            "batchId": format!("t{i}"), "direction": "send",
            "status": "completed", "updatedAtMs": i,
        })));
    }
    store.push(entry(json!({
        "batchId": "live", "direction": "send", "status": "running", "updatedAtMs": 0u64,
    })));
    let evicted = evict_overflow(&mut store);
    assert_eq!(evicted, 2);
    assert_eq!(store.len(), HISTORY_CAP + 1);
    assert!(store
        .iter()
        .all(|e| e.batch_id != "t0" && e.batch_id != "t1"));
    assert!(store.iter().any(|e| e.batch_id == "live"));
    assert!(store
        .iter()
        .any(|e| e.batch_id == format!("t{}", HISTORY_CAP + 1)));
    // 未超顶不再逐出
    assert_eq!(evict_overflow(&mut store), 0);
}

#[test]
fn clear_terminal_keeps_active() {
    let mut store = vec![
        entry(dto("a", "send", "completed", 1)),
        entry(dto("b", "send", "running", 2)),
    ];
    assert_eq!(clear_terminal(&mut store), 1);
    assert_eq!(store.len(), 1);
    assert_eq!(store[0].batch_id, "b");
}

#[test]
fn mark_cancelled_hits_only_active() {
    let mut store = [
        entry(dto("x", "send", "running", 1)),
        entry(dto("y", "send", "completed", 2)),
    ];
    assert!(mark_cancelled(&mut store, "x"));
    assert!(!mark_cancelled(&mut store, "y"));
    assert!(!mark_cancelled(&mut store, "zz"));
    assert_eq!(store[0].status, "cancelled");
}

#[test]
fn apply_retry_replays_terminal_entry() {
    let mut old = entry(dto("old", "send", "failed", 1));
    old.retry_meta = Some(RetryMeta::Send {
        paths: vec!["C:/a.txt".into()],
    });
    let act = entry(dto("act", "send", "running", 2));
    let mut store = [old, act];
    assert!(apply_retry(&mut store, "old", "new", 99));
    assert_eq!(store[0].batch_id, "new");
    assert_eq!(store[0].status, "running");
    assert_eq!(store[0].updated_at_ms, 99);
    // 进行中条目不可重试
    assert!(!apply_retry(&mut store, "act", "new2", 100));
}

// ==================== 事件归约 ====================

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
    let mut store = vec![entry(
        json!({ "batchId": "b-1", "direction": "send", "status": "pending" }),
    )];
    assert!(reduce_event(
        &mut store,
        "send",
        &progress("b-1", 40, 100, 5),
        ""
    ));
    assert_eq!(store[0].status, "running");
    assert_eq!(store[0].transferred_bytes, 40);
    assert_eq!(store[0].total_bytes, 100);
    assert_eq!(store[0].updated_at_ms, 5);
    // 未知批次不建行
    assert!(!reduce_event(
        &mut store,
        "send",
        &progress("ghost", 1, 2, 6),
        ""
    ));
    assert_eq!(store.len(), 1);
}

/// progress 不把用户暂停打回 running（残留 Progress 会让「恢复」弹回「暂停」）
#[test]
fn progress_keeps_paused_entries_paused() {
    let mut store = vec![entry(
        json!({ "batchId": "b-1", "direction": "send", "status": "paused" }),
    )];
    assert!(reduce_event(
        &mut store,
        "send",
        &progress("b-1", 10, 100, 5),
        ""
    ));
    assert_eq!(store[0].status, "paused");
    // 字节照常更新，供恢复后进度衔接
    assert_eq!(store[0].transferred_bytes, 10);
}

/// terminal 归约 + 原因码映射单点：send 与 receive 方向的「对端取消」
/// 语义不同（-receiver / -sender），本端取消恒 -self
#[test]
fn terminal_maps_reason_codes_per_direction() {
    let mk_batch = |batch: &str| {
        vec![entry(json!({
            "batchId": batch, "direction": "send", "status": "running",
            "totalBytes": 10u64, "transferredBytes": 4u64,
        }))]
    };
    let cancelled = json!({ "type": "cancelled", "byPeer": true });
    let mut send = mk_batch("s");
    assert!(reduce_event(
        &mut send,
        "send",
        &terminal("s", cancelled.clone(), 9),
        ""
    ));
    assert_eq!(send[0].status, "cancelled");
    assert_eq!(send[0].detail.as_deref(), Some("cancelled-by-receiver"));

    let mut recv = mk_batch("r");
    assert!(reduce_event(
        &mut recv,
        "receive",
        &terminal("r", cancelled, 9),
        ""
    ));
    assert_eq!(recv[0].detail.as_deref(), Some("cancelled-by-sender"));

    let mut self_cancel = mk_batch("x");
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
    let mk_batch = || {
        vec![entry(json!({
            "batchId": "b", "direction": "send", "status": "running",
            "totalBytes": 10u64, "transferredBytes": 9u64,
        }))]
    };
    let mut done = mk_batch();
    assert!(reduce_event(
        &mut done,
        "send",
        &terminal("b", json!({ "type": "completed" }), 3),
        ""
    ));
    assert_eq!(done[0].status, "completed");
    assert_eq!(done[0].transferred_bytes, 10, "completed 归整为满额");
    assert_eq!(done[0].rate_bps, 0.0);

    let mut rejected = mk_batch();
    assert!(reduce_event(
        &mut rejected,
        "send",
        &terminal(
            "b",
            json!({ "type": "rejected", "reason": "UserRejected" }),
            3
        ),
        ""
    ));
    assert_eq!(rejected[0].status, "rejected");
    assert_eq!(rejected[0].reject_reason.as_deref(), Some("UserRejected"));

    let mut failed = mk_batch();
    assert!(reduce_event(
        &mut failed,
        "send",
        &terminal("b", json!({ "type": "failed", "detail": "io" }), 3),
        ""
    ));
    assert_eq!(failed[0].status, "failed");
    assert_eq!(failed[0].detail.as_deref(), Some("io"));

    let mut unknown = mk_batch();
    assert!(reduce_event(
        &mut unknown,
        "send",
        &terminal("b", json!({}), 3),
        ""
    ));
    assert_eq!(unknown[0].status, "failed");
    assert_eq!(unknown[0].detail.as_deref(), Some("unknown terminal"));
}

/// 用户暂停分支：终态事件只是中断确认，不落终态；completed 且字节已满
/// 是例外（UI 滞后点暂停的已完成任务应结算 completed）
#[test]
fn terminal_keeps_paused_entries_unless_fully_completed() {
    let mut paused = vec![entry(json!({
        "batchId": "b", "direction": "send", "status": "paused",
        "totalBytes": 10u64, "transferredBytes": 4u64,
    }))];
    assert!(!reduce_event(
        &mut paused,
        "send",
        &terminal("b", json!({ "type": "completed" }), 3),
        ""
    ));
    assert_eq!(paused[0].status, "paused");

    let mut full = vec![entry(json!({
        "batchId": "b", "direction": "send", "status": "paused",
        "totalBytes": 10u64, "transferredBytes": 10u64,
    }))];
    assert!(reduce_event(
        &mut full,
        "send",
        &terminal("b", json!({ "type": "completed" }), 3),
        ""
    ));
    assert_eq!(full[0].status, "completed");
}

/// paused/resumed 帧同步：只在合法迁移时改写；终态条目不被复活
#[test]
fn pause_frames_only_transition_legal_states() {
    let mk_status = |status: &str| {
        vec![entry(
            json!({ "batchId": "b", "direction": "send", "status": status }),
        )]
    };
    let paused_ev = json!({ "kind": "paused", "batchId": "b", "tsMs": 4u64 });
    let resumed_ev = json!({ "kind": "resumed", "batchId": "b", "tsMs": 5u64 });

    let mut running = mk_status("running");
    assert!(reduce_event(&mut running, "send", &paused_ev, ""));
    assert_eq!(running[0].status, "paused");
    assert!(reduce_event(&mut running, "send", &resumed_ev, ""));
    assert_eq!(running[0].status, "running");
    // 已 running 再收 resumed：幂等不改写
    assert!(!reduce_event(&mut running, "send", &resumed_ev, ""));

    let mut done = mk_status("completed");
    assert!(!reduce_event(&mut done, "send", &paused_ev, ""));
    assert_eq!(done[0].status, "completed");
}

/// 首屏兜底：active-transfers 投影行补占位（激活晚于事件时），已存在
/// batchId 幂等跳过、终态批不入店
#[test]
fn insert_active_projections_bootstraps_missing_rows_only() {
    let mut store = vec![entry(
        json!({ "batchId": "known", "direction": "send", "status": "running" }),
    )];
    let rows = vec![
        json!({ "batchId": "known", "direction": "send", "status": "running" }),
        json!({ "batchId": "fresh", "direction": "send", "status": "paused",
                "totalBytes": 10u64, "transferredBytes": 3u64, "updatedAtMs": 12u64 }),
        json!({ "batchId": "old", "direction": "send", "status": "completed" }),
    ];
    assert_eq!(insert_active_projections(&mut store, &rows, "Pixel"), 1);
    let fresh = store
        .iter()
        .find(|e| e.batch_id == "fresh")
        .expect("fresh row");
    assert_eq!(fresh.status, "paused");
    assert_eq!(fresh.total_bytes, 10);
    assert_eq!(fresh.transferred_bytes, 3);
    assert_eq!(fresh.updated_at_ms, 12);
    assert!(store.iter().all(|e| e.batch_id != "old"), "终态批不入店");
}

// ==================== 事件归约 · 接收方向锚点 ====================

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
    assert!(
        store[0].retry_meta.is_none(),
        "入站待应答不可重试（源清单在对端）"
    );
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
    let recv: Vec<&str> = active_receive_entries(&store)
        .iter()
        .map(|e| e.batch_id.as_str())
        .collect();
    assert_eq!(recv, vec!["pull-9-0"], "拉取批次进接收视图而非待应答");
}

/// offer-pending → progress 推进：待应答批应答后进数据面（pending→running），
/// 字节/总量随引擎 Progress 补正
#[test]
fn offer_pending_progress_advances_after_user_reply() {
    let mut store = vec![entry(
        json!({ "batchId": "of-1", "direction": "receive", "status": "pending" }),
    )];
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

// ==================== 重试判据单点 ====================

/// 四条判据各自的判定：不存在 / 仍在跑 / 无回放凭证 / 命中
#[test]
fn retry_source_classifies_every_refusal_and_hit() {
    let mut running = send_terminal("live", "running", Some(send_meta()));
    running.node_id = "aa".into();
    let store = vec![
        running,
        send_terminal("done", "completed", Some(send_meta())),
        // 供流记账行（send 方向但非本端发起，无回放凭证）
        send_terminal("serve", "failed", None),
        send_terminal("paused", "paused", Some(send_meta())),
    ];

    // 终态 + 有凭证 → 命中，回放源带 node_id 与元数据
    let (node_id, meta) = retry_source(&store, "done").expect("terminal entry replayable");
    assert_eq!(node_id, "aa");
    assert_eq!(meta, send_meta());

    assert_eq!(retry_source(&store, "ghost"), Err(RetryRefusal::NotFound));
    assert_eq!(retry_source(&store, "live"), Err(RetryRefusal::NotTerminal));
    assert_eq!(
        retry_source(&store, "paused"),
        Err(RetryRefusal::NotTerminal)
    );
    assert_eq!(
        retry_source(&store, "serve"),
        Err(RetryRefusal::MissingMeta)
    );
}

/// 三类拒绝各有可区分文案（「点了没反应」要能分辨原因）
#[test]
fn retry_refusal_messages_name_the_cause() {
    assert_eq!(RetryRefusal::NotFound.message("x"), "task not found: x");
    assert_eq!(
        RetryRefusal::NotTerminal.message("x"),
        "task still in flight, not retryable: x"
    );
    assert_eq!(
        RetryRefusal::MissingMeta.message("x"),
        "task not retryable (initiator metadata missing): x"
    );
}

/// 排他性：completed / failed / rejected / cancelled / interrupted 全部可重试
/// （interrupted = 引擎会话已死，本就是最典型的重试来源）
#[test]
fn retry_source_accepts_every_terminal_variant() {
    for status in [
        "completed",
        "failed",
        "rejected",
        "cancelled",
        "interrupted",
    ] {
        let store = [send_terminal("t", status, Some(send_meta()))];
        assert!(
            retry_source(&store, "t").is_ok(),
            "{status} 应视为可重试终态"
        );
    }
}

/// 回放后新批仍带凭证（再次终态后可再回放）；进行中不可重试
#[test]
fn apply_retry_keeps_meta_and_never_revives_active_entries() {
    let mut store = vec![send_terminal("old", "failed", Some(send_meta()))];
    assert!(apply_retry(&mut store, "old", "new", 42));
    assert_eq!(store[0].batch_id, "new");
    assert_eq!(store[0].status, "running");
    assert_eq!(
        store[0].retry_meta,
        Some(send_meta()),
        "新批仍由本端发起，须保留回放凭证"
    );
    // 进行中不可重试（apply_retry 与 retry_source 判据一致，不复活活跃条目）
    assert_eq!(retry_source(&store, "new"), Err(RetryRefusal::NotTerminal));
    assert!(!apply_retry(&mut store, "new", "newer", 43));
    // 新批再次失败后仍可回放（凭证没丢）
    assert!(reduce_event(
        &mut store,
        "send",
        &json!({
            "kind": "terminal", "batchId": "new",
            "state": { "type": "failed", "detail": "io" }, "tsMs": 50u64,
        }),
        ""
    ));
    assert_eq!(store[0].detail.as_deref(), Some("io"));
    assert!(retry_source(&store, "new").is_ok(), "再次终态后可重试");
}

// ==================== 发送闸门判据 ====================

/// 槽位判据：running < limit 放行；等于/超出即满；limit ≤ 0 时下限 1
/// （配置缺失不得把发送全锁死）
#[test]
fn send_slot_open_uses_clamped_limit() {
    assert!(send_slot_open(0, 3));
    assert!(send_slot_open(2, 3));
    assert!(!send_slot_open(3, 3));
    assert!(!send_slot_open(4, 3));
    // 下限 1：并发配置为 0 时仍允许一个槽位
    assert!(send_slot_open(0, 0));
    assert!(!send_slot_open(1, 0));
}

// ==================== 拉取意图队列 ====================

/// 意图入队 → `pull-started` 事件按 node × rel_path 取回（收窄到单文件）
#[test]
fn pull_intent_round_trips_by_node_and_rel_path() {
    let mut list = vec![];
    push_pull_intent(
        &mut list,
        "aa",
        RetryMeta::Pull {
            dir_id: "docs".into(),
            files: vec![
                PullFileSpec {
                    rel_path: "docs/x.bin".into(),
                    size: 1,
                },
                PullFileSpec {
                    rel_path: "docs/y.bin".into(),
                    size: 2,
                },
            ],
        },
    );
    // 节点不符 → 不消费（别把别的主机的批挂上本插件凭证）
    assert!(take_pull_intent(&mut list, "bb", "docs/x.bin").is_none());
    assert_eq!(list.len(), 1, "未命中不消费意图");
    // 路径不符 → 不消费
    assert!(take_pull_intent(&mut list, "aa", "docs/z.bin").is_none());
    // 命中：收窄为单文件规格 + 保留共享根
    let meta = take_pull_intent(&mut list, "aa", "docs/y.bin").expect("intent consumed");
    assert_eq!(
        meta,
        RetryMeta::Pull {
            dir_id: "docs".into(),
            files: vec![PullFileSpec {
                rel_path: "docs/y.bin".into(),
                size: 2
            }]
        }
    );
    assert!(list.is_empty(), "取出即消费（幂等挂载）");
    assert!(take_pull_intent(&mut list, "aa", "docs/y.bin").is_none());
}

/// 封顶在入队点裁剪（修：原先只在成功挂载后裁剪，全部挂不上时无界增长）
#[test]
fn pull_intent_queue_is_capped_on_push() {
    let mut list = vec![];
    for i in 0..(PULL_INTENT_CAP + 4) {
        push_pull_intent(
            &mut list,
            "aa",
            RetryMeta::Pull {
                dir_id: "docs".into(),
                files: vec![PullFileSpec {
                    rel_path: format!("f{i}.bin"),
                    size: 0,
                }],
            },
        );
    }
    assert_eq!(list.len(), PULL_INTENT_CAP);
    // 最旧 4 条已丢（连不上 retry_meta 的行如实不可重试），最新一条仍在
    assert!(take_pull_intent(&mut list, "aa", "f0.bin").is_none());
    let last = PULL_INTENT_CAP + 3;
    assert!(take_pull_intent(&mut list, "aa", &format!("f{last}.bin")).is_some());
}
