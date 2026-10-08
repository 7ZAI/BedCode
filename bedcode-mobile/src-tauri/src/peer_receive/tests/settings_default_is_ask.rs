//! general — crate 内单元测试（自 bedcode-mobile/src-tauri/src/peer_receive.rs 迁出）

use super::*;

#[test]
fn settings_default_is_ask_with_default_window() {
    let settings = PeerTransferSettings::default();
    assert_eq!(settings.policy_mode, POLICY_ASK);
    assert_eq!(settings.ask_timeout_secs, DEFAULT_ASK_TIMEOUT_SECS);
    assert!(settings.is_valid());
    assert!(matches!(settings.build_policy(), ReceivePolicy::Ask { .. }));
}
#[test]
fn policy_modes_map_to_engine_variants() {
    assert!(matches!(
        settings(POLICY_ALWAYS_ACCEPT, 60).build_policy(),
        ReceivePolicy::AlwaysAccept
    ));
    assert!(matches!(
        settings(POLICY_ALWAYS_DENY, 60).build_policy(),
        ReceivePolicy::AlwaysDeny
    ));
    assert!(matches!(
        settings(POLICY_ASK, 30).build_policy(),
        ReceivePolicy::Ask { .. }
    ));
}
#[test]
fn invalid_modes_and_timeouts_are_rejected_on_load() {
    assert!(!settings("auto_accept", 60).is_valid());
    assert!(!settings(POLICY_ASK, 9).is_valid());
    assert!(!settings(POLICY_ASK, 601).is_valid());
    assert!(settings(POLICY_ASK, 10).is_valid());
    assert!(settings(POLICY_ASK, 600).is_valid());
}
#[test]
fn settings_file_roundtrips_and_rejects_future_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = PeerTransferSettings {
        policy_mode: POLICY_ALWAYS_DENY.to_string(),
        ask_timeout_secs: 120,
        encryption_enabled: true,
        concurrency: 4,
        download_dir: None,
    };
    write_settings_file(dir.path(), &settings).expect("write");
    assert_eq!(read_settings_file(dir.path()).expect("read"), settings);

    let future = r#"{"version":999,"settings":{}}"#;
    std::fs::write(dir.path().join(SETTINGS_FILE), future).expect("write future");
    assert!(read_settings_file(dir.path()).is_err());
}
