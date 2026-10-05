//! 输入投递计划（文本 + 特殊键 共存契约） — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

/// C-IN-001 正例：纯文本（输入栏「发送」= 不带回车）
#[test]
fn plan_input_frames_text_only_sends_one_text_frame() {
    assert_eq!(
        plan_shape("ls -la", None).unwrap(),
        vec!["text:ls -la".to_string()],
        "纯文本必须且只发一帧 text"
    );
}
/// C-IN-002 反例（回归锁）：纯特殊键（快捷键 / 方向键 / Ctrl+C）→ 只发 binary
#[test]
fn plan_input_frames_key_only_sends_one_binary_frame() {
    assert_eq!(
        plan_shape("", Some("ctrl_c")).unwrap(),
        vec!["bytes:03".to_string()],
        "纯特殊键必须且只发一帧 binary"
    );
}
/// C-IN-003 正例（历史缺陷回归锁）：「命令 + Enter」两帧共存且**文本在前**
///
/// 缺陷版实现是 `if 有键 … else if 有文本`，输入栏唯一的生产路径
/// （前端恒传 specialKey="enter"）只发得出裸回车、命令文本被丢弃。
#[test]
fn plan_input_frames_keeps_text_before_special_key() {
    assert_eq!(
        plan_shape("echo HI", Some("enter")).unwrap(),
        vec!["text:echo HI".to_string(), "bytes:0d".to_string()],
        "命令 + Enter 必须先发文本帧再发回车帧（帧序即写入序）"
    );
}
/// C-IN-004 反例（变异探针）：把文本帧丢掉/换序，这条断言必须失败——
/// 与 C-IN-003 组成同一契约的正反两面，禁止只留顺序断言
#[test]
fn plan_input_frames_order_is_observable_by_plan_length_and_payload() {
    let both = plan_shape("echo HI", Some("enter")).unwrap();
    let text_only = plan_shape("echo HI", None).unwrap();
    assert_eq!(both.len(), 2, "有键 + 有文本必须产生两帧");
    assert_eq!(
        both[0], text_only[0],
        "共存计划的第一帧必须与纯文本计划的第一帧一致（即文本未被丢弃）"
    );
}
/// C-IN-005 异常：不支持的键名 → Err，且**一帧都不投递**（半截输入不可回滚）
#[test]
fn plan_input_frames_rejects_unknown_key_without_partial_send() {
    let err = plan_shape("echo HI", Some("no_such_key")).unwrap_err();
    assert!(err.contains("no_such_key"), "错误信息必须点名非法键名，实际={err}");
}
/// C-IN-006 边界：空串键名视作「无特殊键」（前端可能传 Some("")）
#[test]
fn plan_input_frames_empty_key_name_is_treated_as_absent() {
    assert_eq!(
        plan_shape("echo HI", Some("")).unwrap(),
        vec!["text:echo HI".to_string()],
        "空键名不得产生 binary 帧"
    );
}
