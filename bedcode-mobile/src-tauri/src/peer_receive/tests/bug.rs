//! 暂停/恢复状态迁移（Bug：暂停成功仍显暂停） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/peer_receive.rs 迁出）

use super::*;

/// 接收侧暂停/恢复目标状态映射：running/pending → paused；paused → running
#[test]
fn receive_pause_target_maps_run_and_pending_to_paused() {
    assert_eq!(receive_pause_target("running", true), Some("paused"));
    assert_eq!(receive_pause_target("pending", true), Some("paused"));
    // 已暂停/终态不再变化
    assert_eq!(receive_pause_target("paused", true), None);
    assert_eq!(receive_pause_target("completed", true), None);
}
/// 恢复必须命中 paused——否则 Resume 帧到达后任务卡在 paused（真机现象）
#[test]
fn receive_pause_target_resume_hits_only_paused() {
    assert_eq!(receive_pause_target("paused", false), Some("running"));
    assert_eq!(receive_pause_target("running", false), None);
    assert_eq!(receive_pause_target("pending", false), None);
    assert_eq!(receive_pause_target("completed", false), None);
}
/// 已完成判定：总量已知且字节已满（total==0 不可判）
#[test]
fn receive_transfer_complete_requires_known_total_and_full_bytes() {
    assert!(receive_transfer_complete(100, 100));
    assert!(receive_transfer_complete(100, 120), "overshoot defensive");
    assert!(!receive_transfer_complete(100, 99));
    assert!(!receive_transfer_complete(0, 0), "unknown total");
    assert!(!receive_transfer_complete(0, 50), "unknown total with bytes");
}
/// full_completed：仅 paused+Completed+满字节 判定为已完成（否则 kept for resume）
#[test]
fn receive_full_completed_only_for_paused_completed_full() {
    assert!(receive_full_completed("paused", 100, 100, true));
    assert!(!receive_full_completed("paused", 99, 100, true), "not full");
    assert!(
        !receive_full_completed("paused", 100, 100, false),
        "non-completed terminal"
    );
    assert!(!receive_full_completed("running", 100, 100, true), "running not paused");
    assert!(!receive_full_completed("paused", 0, 0, true), "unknown total");
}
/// 进度入账：paused 保持暂停（不被打回 running）、字节照常更新
#[test]
fn apply_receive_progress_keeps_paused_but_updates_bytes() {
    let mut task = PeerTransferDto {
        batch_id: "b".to_string(),
        node_id: "a".repeat(64),
        peer_name: "Peer".to_string(),
        direction: "receive".to_string(),
        status: "paused".to_string(),
        files: Vec::new(),
        total_bytes: 100,
        transferred_bytes: 40,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    apply_receive_progress(&mut task, 55, 100, 3.5, 2);
    assert_eq!(task.status, "paused", "暂停期间不得被打回 running");
    assert_eq!(task.transferred_bytes, 55);
    assert_eq!(task.rate_bps, 3.5);
    assert_eq!(task.updated_at_ms, 2);
}
/// 进度入账：running 任务照常推进（total 首次补正）
#[test]
fn apply_receive_progress_advances_running_and_backfills_total() {
    let mut task = PeerTransferDto {
        batch_id: "b".to_string(),
        node_id: "a".repeat(64),
        peer_name: "Peer".to_string(),
        direction: "receive".to_string(),
        status: "running".to_string(),
        files: Vec::new(),
        total_bytes: 0,
        transferred_bytes: 0,
        rate_bps: 0.0,
        detail: None,
        reject_reason: None,
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    apply_receive_progress(&mut task, 10, 200, 1.0, 2);
    assert_eq!(task.status, "running");
    assert_eq!(task.transferred_bytes, 10);
    assert_eq!(task.total_bytes, 200, "首个 Progress 补正总量");
}
