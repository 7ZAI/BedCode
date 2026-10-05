//! 重连退避收敛（防手写第二张表） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// 行为契约（2026-10-04 审计 P0-1 同型缺陷的终端链路版本）：终端链路的退避
/// 必须与设备级事件通道**同一来源**。旧实现自建 `500→…→8000` 手写表，
/// 其中 500 低于全局下限 1000 —— 与事件通道那次「护栏挂在死代码上」是同一类
/// 教训，只是这次护栏和被保护的对象都在跑，却各用各的数。
///
/// 正例：序列逐轮等于 `ReconnectManager` 默认等比退避（1s/2s/4s/8s/16s）
#[tokio::test]
async fn terminal_backoff_matches_shared_reconnect_manager_sequence() {
    let policy = terminal_reconnect_policy();
    for expect_ms in [1000u64, 2000, 4000, 8000, 16000] {
        policy.start().await.expect("无限重试配置下不应耗尽");
        let delay = policy.get_delay().await;
        // 抖动是 0~+10% 的正偏移，故断言下界与「不超过 1.1×」上界
        assert!(
            delay.as_millis() as u64 >= expect_ms,
            "第 {} 轮退避 {}ms 低于等比基线 {}ms",
            policy.get_retry_count().await,
            delay.as_millis(),
            expect_ms
        );
        assert!(
            delay.as_millis() as u64 <= expect_ms + expect_ms / 10,
            "第 {} 轮退避 {}ms 超出 +10% 抖动上界",
            policy.get_retry_count().await,
            delay.as_millis()
        );
    }
}
/// 反例（最关键的一条）：单次等待不得低于全局下限 1s。旧实现在首轮就是
/// 500ms —— 与 2026-09-29 那次 616 次/98 秒自愈风暴同型。
#[tokio::test]
async fn terminal_backoff_never_breaches_global_minimum_delay() {
    let policy = terminal_reconnect_policy();
    policy.start().await.unwrap();
    let delay = policy.get_delay().await;
    assert!(
        delay >= std::time::Duration::from_millis(MIN_RECONNECT_DELAY_MS),
        "首轮退避 {}ms 击穿了全局下限 {}ms",
        delay.as_millis(),
        MIN_RECONNECT_DELAY_MS
    );
}
/// 退避封顶必须生效：长时间断开后单轮等待不得无限增长
#[tokio::test]
async fn terminal_backoff_is_capped() {
    let policy = terminal_reconnect_policy();
    for _ in 0..10 {
        policy.start().await.unwrap();
    }
    let delay = policy.get_delay().await;
    assert!(
        delay <= std::time::Duration::from_millis(DEFAULT_MAX_DELAY_MS + DEFAULT_MAX_DELAY_MS / 10),
        "第 10 轮退避 {}ms 超出封顶 {}ms",
        delay.as_millis(),
        DEFAULT_MAX_DELAY_MS
    );
}
/// 终端链路保持既有语义：**无限**重试（随 subscribe 生命周期销毁，不做
/// 「N 次后交还用户」的裁决）。若有人改成有限轮次，退避耗尽会让
/// `link_io` 直接 return 而不再自愈——终端永久卡死。
#[tokio::test]
async fn terminal_reconnect_is_unlimited_by_design() {
    let policy = terminal_reconnect_policy();
    for _ in 0..20 {
        assert!(
            policy.start().await.is_some(),
            "第 {} 轮被截断：终端链路应无限重试",
            policy.get_retry_count().await + 1
        );
    }
    assert!(!policy.is_abandoned().await);
}
/// 反例（2026-10-04 OCR M-01 回归锁）：链路稳定回到 live 必须复位退避
/// 序列。修复前 `link_io` 只在失败路径调 `policy.start()`，从不调
/// `on_success()`——retry_count 在整个链路生命周期累积，指数序列爬到
/// 封顶（30s）后，之后每次断线（哪怕刚经过健康期）都从 30s 起退，而非
/// 从 1s 重来。事件通道（connection/manager.rs reconnect 成功分支）对同一
/// 策略调 on_success，终端链路与它行为分叉（「单一事实源」名存实亡）。
///
/// 正例：多轮失败（计数爬到高位）后收到 `subscribed` 门控帧 → 计数归零，
/// 下一轮 `start()` 回到初始退避而非封顶值。
#[tokio::test]
async fn subscribed_resets_backoff_sequence_after_failures() {
    let (tx, _rx) = mpsc::channel::<Outbound>(8);
    let link = TerminalLink::new(
        "s1".to_string(),
        Arc::new(NullSink),
        tx,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Mutex::new(None)),
    );

    let policy = terminal_reconnect_policy();
    // 多轮失败：退避爬到封顶
    for _ in 0..10 {
        policy.start().await.unwrap();
    }
    let capped = policy.get_delay().await;
    let before = policy.get_retry_count().await;
    assert!(before >= 10, "前置：退避计数应先爬上来（实际 {before}）");

    // 链路恢复：收到 subscribed 门控帧（link_io 的 connect_once 收帧路径）
    let subscribed = handle_control_text(
        &link,
        r#"{"type":"subscribed","mode":"live"}"#,
        &policy,
    )
    .await;
    assert!(subscribed.is_ok(), "subscribed 帧处理不得报错（LinkExit 无 Debug，断言 is_ok）");

    // 反例断言：计数归零 → 下一次失败从初始退避重来，而不是沿用封顶值
    assert_eq!(
        policy.get_retry_count().await,
        0,
        "恢复 live 后退避计数必须归零（on_success 语义）"
    );
    let next = policy.start().await.unwrap();
    assert!(
        next < capped,
        "复位后首轮退避 {:?} 应远低于封顶前 {:?}——否则健康期后的断线仍 30s 起退",
        next,
        capped
    );
    assert!(
        next >= std::time::Duration::from_millis(MIN_RECONNECT_DELAY_MS),
        "复位后同样受 1s 下限保护（实际 {:?}",
        next
    );
}
