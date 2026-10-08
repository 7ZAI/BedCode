//! general — crate 内单元测试（自 bedcode-mobile/wasm-apps/file-transfer/rust/src/settings_store.rs 迁出）

use super::*;

#[test]
fn save_and_push_writes_storage_and_host_primitives() {
    let mut h = MockHost::new();
    let s = TransferSettings {
        receiving_policy: "accept".into(),
        approval_timeout_sec: 999,
        download_dir: Some("D:/dl".into()),
        encryption: true,
        concurrency: 5,
    };
    save_and_push(&mut h, &s).unwrap();
    assert_eq!(h.pushed_policy.borrow().as_slice(), &[("always_accept".to_string(), 600u64)][..]);
    // 回读一致
    let loaded = load(&h).unwrap();
    assert_eq!(loaded, s);
}
#[test]
fn load_or_migrate_returns_default_when_storage_empty() {
    // Phase 4：引擎读接口退役，历史值由宿主一次性迁移导出；插件侧空即默认
    let mut h = MockHost::new();
    let s = load_or_migrate(&mut h).unwrap();
    assert_eq!(s, TransferSettings::default());
    // 预置 storage 后读回一致
    h.storage_set(SETTINGS_KEY, &serde_json::to_value(TransferSettings {
        receiving_policy: "reject".into(),
        approval_timeout_sec: 30,
        download_dir: Some("D:/dl".into()),
        encryption: true,
        concurrency: 3,
    }).unwrap()).unwrap();
    assert_eq!(load_or_migrate(&h).unwrap().receiving_policy, "reject");
}
#[test]
fn concurrency_defaults_to_3_and_clamps_into_range() {
    // 旧 storage 无 concurrency 字段：缺省 3（向后兼容）
    let h = MockHost::new();
    h.storage_set(
        SETTINGS_KEY,
        &serde_json::json!({"receivingPolicy": "ask", "approvalTimeoutSec": 60, "encryption": false}),
    )
    .unwrap();
    assert_eq!(load(&h).unwrap().concurrency, 3);
    assert_eq!(clamp_concurrency(0), 1);
    assert_eq!(clamp_concurrency(9), 8);
    assert_eq!(clamp_concurrency(5), 5);
}
#[test]
fn auto_answer_maps_three_branches() {
    let base = TransferSettings::default();
    assert_eq!(auto_answer(&base), None);
    let accept = TransferSettings { receiving_policy: "accept".into(), ..Default::default() };
    assert_eq!(auto_answer(&accept), Some(true));
    let reject = TransferSettings { receiving_policy: "reject".into(), ..Default::default() };
    assert_eq!(auto_answer(&reject), Some(false));
}
#[test]
fn policy_word_mapping() {
    assert_eq!(policy_to_host("accept"), "always_accept");
    assert_eq!(policy_to_host("reject"), "always_deny");
    assert_eq!(policy_to_host("ask"), "ask");
    assert_eq!(clamp_timeout(5), 10);
    assert_eq!(clamp_timeout(700), 600);
}
