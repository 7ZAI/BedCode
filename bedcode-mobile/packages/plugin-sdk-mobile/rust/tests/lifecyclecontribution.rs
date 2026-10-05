//! LifecycleContribution — 公共 API 集成测试（自 bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs 迁出）

use bedcode_plugin_api_mobile::types::*;

#[test]
fn test_lifecycle_is_declared_mapping() {
    // 宿主按 camelCase 事件名查询声明；未声明/未知事件一律 false
    let mut lc = LifecycleContribution::default();
    assert!(!lc.is_declared("onStartup"));
    lc.on_startup = true;
    lc.on_auth_success = true;
    lc.on_session_stopped = true;
    assert!(lc.is_declared("onStartup"));
    assert!(lc.is_declared("onAuthSuccess"));
    assert!(lc.is_declared("onSessionStopped"));
    assert!(!lc.is_declared("onShutdown"));
    assert!(!lc.is_declared("onDisconnect"));
    // 未知事件名拒绝，防止宿主拼写漂移静默通过
    assert!(!lc.is_declared("onPaused"));
    assert!(!lc.is_declared(""));
}
#[test]
fn test_lifecycle_has_any_declared() {
    // 全空默认 = 无任何生命周期钩子声明
    let lc = LifecycleContribution::default();
    assert!(!lc.has_any_declared());
    // 任一钩子置位即视为有声明（宿主据此决定是否注册回调）
    let mut lc2 = LifecycleContribution::default();
    lc2.on_terminal_input = true;
    assert!(lc2.has_any_declared());
}
