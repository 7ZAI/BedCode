//! 结构锁 — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// 结构锁一：新协议**WS 协议实现段**不得出现旧信封 / 旧 TB v3 残留——
/// `Message::`（信封）、TB 帧头常量、`from_offset` / `history_end` 等。
/// `terminal_get_history` 是桌面 HTTP 响应映射（camelCase `minOffset` 等是
/// 桌面 HTTP wire 字段，与 WS 帧协议无关），不在扫描范围
#[test]
fn terminal_link_has_no_legacy_protocol_residue() {
    let root = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{root}/src/terminal_link.rs")).expect("read terminal_link.rs");
    // 截取 WS 协议实现段：到 `terminal_get_history` 为止（之后是 HTTP 历史代理）
    let implementation = src
        .split("pub async fn terminal_get_history")
        .next()
        .unwrap_or(&src)
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&src);
    let mut violations: Vec<String> = Vec::new();
    for (idx, raw) in implementation.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") || line.starts_with("///") || line.starts_with("//!") {
            continue;
        }
        for marker in [
            "Message::",
            "from_offset",
            "history_end",
            "TB_FRAME_HEADER_LEN",
            "snapshot_offset",
            "min_offset",
        ] {
            if line.contains(marker) {
                violations.push(format!("terminal_link.rs:{}: {}", idx + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "terminal_link WS 协议实现段不得出现旧协议残留（票 05）：\n{}",
        violations.join("\n")
    );
}
