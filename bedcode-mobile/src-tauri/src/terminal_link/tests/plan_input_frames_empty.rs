//! 输入投递计划（文本 + 特殊键 共存契约） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// C-IN-007 边界：两者皆空 → 空计划（不发帧，但不报错）
#[test]
fn plan_input_frames_empty_both_sends_nothing() {
    assert!(plan_input_frames("", None).unwrap().is_empty(), "空输入不得产生任何帧");
}
