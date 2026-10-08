//! 策略闸门语义（票 07 接收侧收敛后的留存面）— crate 内单元测试
//!
//! 票 07 把接收任务状态机（暂停迁移 / 进度入账 / 终态封顶）整体下沉插件后，
//! 宿主侧只剩闸门参数与引擎控制面；对应的状态迁移用例迁至插件
//! `transfer_store::reduce_event`（`pause_frames_only_transition_legal_states`、
//! `progress_keeps_paused_entries_paused`、`terminal_keeps_paused_entries_unless_fully_completed`、
//! `terminal_maps_reason_codes_per_direction`），本文件只守闸门参数口径。

use super::*;

/// 拉取并发上限校验（1..=8；spec §7）
#[test]
fn concurrency_validation_accepts_range_and_rejects_outliers() {
    assert!(validate_concurrency(1).is_ok());
    assert!(validate_concurrency(8).is_ok());
    assert!(validate_concurrency(0).is_err());
    assert!(validate_concurrency(9).is_err());
}

/// ask 策略的询问窗口进引擎闸门（超时即拒——引擎侧执行，宿主只传参）
#[test]
fn ask_policy_carries_timeout_window_into_engine_gate() {
    let policy = settings(POLICY_ASK, 30).build_policy();
    match policy {
        ReceivePolicy::Ask { timeout } => {
            assert_eq!(timeout, std::time::Duration::from_secs(30));
        }
        other => panic!("ask mode must map to ReceivePolicy::Ask, got {other:?}"),
    }
}

/// 自动接受/拒绝档位不携带询问窗口（无弹窗即无倒计时；超时参数不参与判定）
#[test]
fn auto_policy_tiers_ignore_timeout_window() {
    assert!(matches!(
        settings(POLICY_ALWAYS_ACCEPT, 10).build_policy(),
        ReceivePolicy::AlwaysAccept
    ));
    assert!(matches!(
        settings(POLICY_ALWAYS_DENY, 600).build_policy(),
        ReceivePolicy::AlwaysDeny
    ));
}
