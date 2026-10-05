//! 退役行清理 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/keys.rs 迁出）

use super::*;

/// 清理 ADR 0033 前的死密钥行：存在则删、不存在则幂等成功
#[test]
fn purge_legacy_key_is_idempotent() {
    let store = MockSecretStore::new();
    store
        .set(JWT_SECRET_KEY_ID, &hex::encode([0x41u8; 32]))
        .unwrap();
    assert!(purge_legacy_key(&store).expect("purge"), "存在时报告已清理");
    assert!(store.get(JWT_SECRET_KEY_ID).unwrap().is_none());
    assert!(
        !purge_legacy_key(&store).expect("purge again"),
        "已清理过 → 幂等成功且不谎报"
    );
}
