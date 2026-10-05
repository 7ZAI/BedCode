//! 阶段映射 — crate 内单元测试（自 bedcode-mobile/src-tauri/src/terminal_link.rs 迁出）

use super::*;

#[test]
fn link_phase_round_trip_and_api_str() {
    assert_eq!(LinkPhase::from_u8(LinkPhase::Live.as_u8()), LinkPhase::Live);
    assert_eq!(LinkPhase::from_u8(LinkPhase::Auth.as_u8()), LinkPhase::Auth);
    assert_eq!(LinkPhase::from_u8(LinkPhase::Connecting.as_u8()), LinkPhase::Connecting);
    assert_eq!(LinkPhase::from_u8(LinkPhase::Idle.as_u8()), LinkPhase::Idle);
    assert_eq!(LinkPhase::from_u8(99), LinkPhase::Idle);
    assert_eq!(LinkPhase::Live.as_api_str(), "live");
    assert_eq!(LinkPhase::Idle.as_api_str(), "idle");
    // 新协议无独立 history 阶段：u8=3 即 live（旧版 4 已退役）
    assert_eq!(
        LinkPhase::from_u8(4),
        LinkPhase::Idle,
        "旧 TB v3 的 history 阶段号不得再被识别"
    );
}
