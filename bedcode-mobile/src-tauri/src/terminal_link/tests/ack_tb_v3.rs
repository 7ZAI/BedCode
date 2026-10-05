//! ack 节流（回归护栏：语义与旧 TB v3 版一致） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// 达阈值即回发：与空闲时长无关（节流上沿）
#[test]
fn should_send_ack_when_pending_reaches_threshold() {
    assert!(should_send_ack(ACK_BYTES_THRESHOLD, 1_000, 1_000));
    assert!(should_send_ack(ACK_BYTES_THRESHOLD + 1, 1_000, 1_001));
}
/// 回归护栏（背压死锁）：末批不足阈值的积压 ack，空闲兜底窗口到期必须回发。
/// 旧实现仅在收帧时求值该规则，上游被暂停（不再收帧）后积压永不回发
#[test]
fn should_send_ack_flushes_stranded_pending_after_idle_window() {
    let last = 10_000;
    assert!(!should_send_ack(1, last, last + ACK_MAX_IDLE_MS - 1));
    assert!(should_send_ack(1, last, last + ACK_MAX_IDLE_MS));
    assert!(should_send_ack(1024, last, last + ACK_MAX_IDLE_MS + 5));
}
/// pending 为 0 一律不回发：空闲定时器周期调用不得产生空 ack 风暴
#[test]
fn should_send_ack_never_sends_without_pending_bytes() {
    assert!(!should_send_ack(0, 1_000, 1_000));
    assert!(!should_send_ack(0, 1_000, 1_000 + ACK_MAX_IDLE_MS * 100));
}
/// 首次回发（尚无 ack 基准）仍需攒满阈值：避免握手后立即产生零散 ack
#[test]
fn should_send_ack_first_send_waits_for_threshold() {
    assert!(!should_send_ack(1024, 0, 1_000_000));
    assert!(should_send_ack(ACK_BYTES_THRESHOLD, 0, 1_000_000));
}
/// 「任何非零积压都不会滞留」性质：阈值以下的每种 pending 都在空闲窗口内
/// 翻转为回发（若规则被改成「同时要求阈值与空闲」等永不可达组合，此处必红）
#[test]
fn should_send_ack_never_strands_sub_threshold_pending() {
    for pending in [1u64, 2, 512, 4096, ACK_BYTES_THRESHOLD - 1] {
        let last = 1_000;
        assert!(
            !should_send_ack(pending, last, last + ACK_MAX_IDLE_MS - 1),
            "空闲窗口未到不应回发: pending={pending}"
        );
        assert!(
            should_send_ack(pending, last, last + ACK_MAX_IDLE_MS),
            "空闲窗口到期必须回发: pending={pending}"
        );
    }
}
/// 轮询间隔必须落在空闲窗口内（否则积压 ack 的送达被推迟到窗口之外）
#[test]
fn ack_idle_tick_is_within_idle_window() {
    assert!(ACK_IDLE_TICK_MS > 0);
    assert!(ACK_IDLE_TICK_MS <= ACK_MAX_IDLE_MS);
}
