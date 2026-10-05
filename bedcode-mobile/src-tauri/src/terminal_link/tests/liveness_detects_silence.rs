//! 链路活性检测（半开） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// 判活基准必须回落到**建连时刻**：静默 shell 不发 Pong，若首个 Ping 之前
/// 没有基准，这段窗口的死连接检测不到（同事件通道那次修复的同型缺陷）。
///
/// timeout 用 1ms + 30ms 真实等待（Instant 无注入缝，与 heartbeat.rs 既有
/// 超时用例同款取法）。注意 `HeartbeatConfig::new(secs, secs)` 收的是**秒**，
/// 要毫秒级必须用结构体字面量。
#[tokio::test]
async fn liveness_detects_silence_after_mark_connected_without_any_pong() {
    let hb = HeartbeatManager::new(HeartbeatConfig {
        interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
        timeout: std::time::Duration::from_millis(1),
        max_timeouts: 3,
    });
    hb.mark_connected().await;
    assert!(!hb.is_connection_lost().await, "刚建连不应立即判死");
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(
        hb.is_connection_lost().await,
        "建连后无任何入站活动且已超时应判死（半开检测不得失效）"
    );
}
/// 正例：**任意**入站帧都刷新基准。终端的静默期只靠 Pong 会被误判成死链，
/// 只有业务输出帧而无 Pong 时仍必须算「活着」。
#[tokio::test]
async fn any_inbound_activity_refreshes_the_liveness_baseline() {
    let hb = HeartbeatManager::new(HeartbeatConfig {
        interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
        timeout: std::time::Duration::from_millis(1),
        max_timeouts: 3,
    });
    hb.mark_connected().await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(hb.is_connection_lost().await, "前置：静默超时应已判死");

    // 收到业务输出帧（非 Pong）
    hb.on_activity().await;
    assert!(!hb.is_connection_lost().await, "收到入站帧后应立即恢复为「活着」");
}
/// 边界：`mark_connected` 开启新一轮建连，清掉上一轮的基准与超时计数
#[tokio::test]
async fn mark_connected_resets_previous_round_state() {
    let hb = HeartbeatManager::new(HeartbeatConfig {
        interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
        timeout: std::time::Duration::from_millis(1),
        max_timeouts: 3,
    });
    hb.on_activity().await;
    hb.increment_timeout().await;
    hb.increment_timeout().await;
    hb.mark_connected().await;
    assert_eq!(hb.get_consecutive_timeouts().await, 0);
    assert!(!hb.is_connection_lost().await);
}
/// 未 mark_connected（心跳循环从未启动）→ 不擅自判死，判定权交还调用方
#[tokio::test]
async fn liveness_does_not_judge_before_mark_connected() {
    let hb = HeartbeatManager::new(HeartbeatConfig {
        interval: std::time::Duration::from_secs(TERMINAL_HEARTBEAT_INTERVAL_SECS),
        timeout: std::time::Duration::from_millis(1),
        max_timeouts: 3,
    });
    assert!(!hb.is_connection_lost().await);
}
