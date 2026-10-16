//! 拒绝（策略四类，语义不变） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/policy/mod.rs 迁出）

use super::*;

/// native 路径显性失败（无宿主环境）
#[test]
fn native_path_fails_without_host() {
    assert!(verify_device_token("x.y.z").is_err());
}
