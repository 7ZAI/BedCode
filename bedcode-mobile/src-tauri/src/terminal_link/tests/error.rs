//! error 帧分类（会话不存在 → 退避重试） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

#[test]
fn classify_server_error_session_missing_and_other() {
    // 桌面插件 subscribe 失败的消息含「会话不存在」字样（启动竞态/已停止）
    assert!(matches!(
        classify_server_error("会话不存在：s1"),
        ServerErrorClass::SessionMissing
    ));
    assert!(matches!(
        classify_server_error("subscribe: missing sessionId"),
        ServerErrorClass::Other
    ));
    assert!(matches!(
        classify_server_error("host pty write failed: x"),
        ServerErrorClass::Other
    ));
    assert!(matches!(classify_server_error(""), ServerErrorClass::Other));
}
