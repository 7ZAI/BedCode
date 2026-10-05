//! 帧构造（新协议 wire 形状锁） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

#[test]
fn build_subscribe_frame_shape_and_mode() {
    let live = serde_json::from_str::<serde_json::Value>(&build_subscribe_frame("s1", LinkMode::Live)).unwrap();
    assert_eq!(keys(&live), ["mode", "sessionId", "type"]);
    assert_eq!(live["type"], "subscribe");
    assert_eq!(live["sessionId"], "s1");
    assert_eq!(live["mode"], "live");

    let poll = serde_json::from_str::<serde_json::Value>(&build_subscribe_frame("s2", LinkMode::Poll)).unwrap();
    assert_eq!(poll["mode"], "poll", "批量态 mode 帧形状（协议保留能力）");
}
#[test]
fn build_ack_frame_shape() {
    let ack = serde_json::from_str::<serde_json::Value>(&build_ack_frame(4096)).unwrap();
    assert_eq!(keys(&ack), ["offset", "type"]);
    assert_eq!(ack["type"], "ack");
    assert_eq!(ack["offset"], 4096);
}
#[test]
fn build_poll_frame_shape() {
    let poll = serde_json::from_str::<serde_json::Value>(&build_poll_frame()).unwrap();
    assert_eq!(keys(&poll), ["type"]);
    assert_eq!(poll["type"], "poll");
}
/// 文本输入帧：UTF-8 原文（非 base64）+ JSON 转义正确（含引号/控制字符）
#[test]
fn build_input_text_frame_escapes_and_keeps_utf8() {
    let f = build_input_text_frame("ls -la \"a\"\u{1f600}");
    let parsed: serde_json::Value = serde_json::from_str(&f).unwrap();
    assert_eq!(parsed["type"], "input");
    assert_eq!(parsed["data"], "ls -la \"a\"\u{1f600}");
    assert!(!f.contains("base64"), "可打印输入不得走 base64");
}
/// 输入双形态（特殊键 → binary 字节）：UI 提供的全部特殊键都能经
/// KeyCombo::parse + to_pty_bytes 映射（Enter/Tab/Esc/Del/Ctrl+C/Z/L/方向键）
#[test]
fn special_key_to_pty_bytes_covers_ui_keys() {
    let cases: &[(&str, &[u8])] = &[
        ("enter", &[0x0d]),
        ("tab", &[0x09]),
        ("escape", &[0x1b]),
        ("delete", &[0x1b, b'[', b'3', b'~']),
        ("ctrl_c", &[0x03]),
        ("ctrl_z", &[0x1a]),
        ("ctrl_l", &[0x0c]),
        ("arrow_up", &[0x1b, b'[', b'A']),
        ("arrow_down", &[0x1b, b'[', b'B']),
        ("arrow_left", &[0x1b, b'[', b'D']),
        ("arrow_right", &[0x1b, b'[', b'C']),
    ];
    for (name, expect) in cases {
        assert_eq!(
            special_key_to_pty_bytes(name).as_deref(),
            Some(*expect),
            "special key {name} 必须映射到二进制 PTY 字节"
        );
    }
    assert_eq!(special_key_to_pty_bytes("no_such_key"), None);
    assert_eq!(special_key_to_pty_bytes(""), None);
}
