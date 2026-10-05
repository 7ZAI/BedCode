//! 投递失败上抛（多帧输入的半截输入护栏） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// C-IN-008 正例：通道仍活着 → 投递成功
#[tokio::test]
async fn send_out_succeeds_while_io_task_alive() {
    let (tx, mut rx) = mpsc::channel::<Outbound>(8);
    let link = TerminalLink::new(
        "s1".to_string(),
        Arc::new(NullSink),
        tx,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Mutex::new(None)),
    );

    let res = link.send_out(Outbound::TextInput { data: "ls".to_string() }).await;

    assert!(res.is_ok(), "通道存活时投递必须成功，实际={res:?}");
    assert!(matches!(rx.try_recv(), Ok(Outbound::TextInput { .. })), "帧必须真的进了通道");
}
/// C-IN-009 异常：IO 任务退出（通道关闭）→ 必须返回 Err
///
/// 单帧时代「丢弃 + 留痕」尚可接受；多帧输入（命令 + Enter）下静默丢弃会让
/// 「文本已写进 PTY、回车没发」被当成成功返回（PTY 侧不可回滚）。
#[tokio::test]
async fn send_out_reports_error_after_io_task_exited() {
    let (link, _tx) = link_with_channel("s-dead");

    let res = link.send_out(Outbound::TextInput { data: "echo HI".to_string() }).await;

    let err = res.expect_err("通道关闭必须上抛 Err（不得静默丢弃）");
    assert!(err.contains("s-dead"), "错误信息必须点名会话（便于前端/日志定位），实际={err}");
}
