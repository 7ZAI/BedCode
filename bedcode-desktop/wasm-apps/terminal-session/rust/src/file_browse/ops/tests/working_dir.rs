//! working_dir 解析 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/file_browse/ops.rs 迁出）

use super::*;

#[test]
fn resolve_working_dir_prefers_session_then_config() {
    let store = crate::config::store::tests::MockConfigStore::new(vec![
        crate::config::model::SessionConfig {
            id: "c1".into(),
            name: "工作台".into(),
            environment: "linux".into(),
            wsl_distro: None,
            working_dir: "/srv/app".into(),
            command: "bash".into(),
            auto_start: false,
            created_at: "2026-09-20T00:00:00Z".into(),
            updated_at: "2026-09-20T00:00:00Z".into(),
        },
    ]);
    let sessions = r#"[{"id":"s1","configId":"c1","name":"dev","status":"Running"}]"#;
    // 会话路径
    assert_eq!(
        resolve_working_dir(&store, Some(sessions), "s1").unwrap(),
        "/srv/app"
    );
    // 会话不存在 → 回退 config_id 路径
    assert_eq!(
        resolve_working_dir(&store, Some(sessions), "c1").unwrap(),
        "/srv/app"
    );
    // 两者都无 → NotFound 文案与宿主逐字一致
    let err = resolve_working_dir(&store, Some(sessions), "ghost").unwrap_err();
    assert_eq!(err, "Not found: Session/Config not found: ghost");
}
