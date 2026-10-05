//! 首启 / 读回 — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/keys.rs 迁出）

use super::*;

/// 首启：随机生成 + 落库（hex 长度 = 32B × 2 = 64 字符）
#[test]
fn first_call_generates_and_persists() {
    let store = MockSecretStore::new();
    let ring = load_or_create(&store).expect("generate");
    assert_eq!(ring.active, 1);
    assert_eq!(
        ring.signing_key().expect("signing key").len(),
        JWT_SECRET_KEY_LEN
    );
    let stored = store.get(KEYRING_SECRET_ID).unwrap().expect("persisted");
    // 落库的是**一个** secret 值（整环），不是多行指针
    assert!(
        store.get(JWT_SECRET_KEY_ID).unwrap().is_none(),
        "新真源是 keyring，不得再写退役的 jwt.key 行"
    );
    let file: KeyringFile = serde_json::from_str(&stored).expect("keyring json");
    assert_eq!(file.active, 1);
    let hexed = file.keys.get(&1).expect("gen 1 key");
    assert_eq!(hexed.len(), 64, "32 字节 → 64 hex 字符落库");
    // 熵非全零（新密钥必须随机）
    assert!(ring.signing_key().expect("key").iter().any(|b| *b != 0));
}
/// 二次调用：读回已持久化密钥环（重启稳定，不重新生成）
#[test]
fn second_call_reads_back_persisted_keyring() {
    let store = MockSecretStore::new();
    let first = load_or_create(&store).expect("first");
    let second = load_or_create(&store).expect("second");
    assert_eq!(first.active, second.active, "重启后代次稳定");
    assert_eq!(
        first.signing_key().expect("key"),
        second.signing_key().expect("key"),
        "重启后签发密钥必须稳定（读回持久化值）"
    );
}
/// 损坏存储（非法 JSON / 非 hex / 长度不符 / active 缺密钥）→ **显性失败**，
/// 绝不静默生成新密钥（那会让全部已配对设备静默失效）
#[test]
fn corrupt_keyring_fails_loudly_instead_of_regenerating() {
    let store = MockSecretStore::new();
    let good = load_or_create(&store).expect("first");

    let cases: Vec<(String, &str)> = vec![
        ("not-json!".to_string(), "not valid"),
        (
            serde_json::json!({"active": 1, "keys": {"1": "zz"}}).to_string(),
            "not hex",
        ),
        (
            serde_json::json!({"active": 1, "keys": {"1": hex::encode([0u8; 16])}}).to_string(),
            "length mismatch",
        ),
        (
            serde_json::json!({"active": 7, "keys": {"1": hex::encode([0u8; 32])}}).to_string(),
            "has no key",
        ),
        (
            serde_json::json!({"active": 0, "keys": {"0": hex::encode([0u8; 32])}}).to_string(),
            ">= 1",
        ),
    ];
    for (raw, needle) in cases {
        let s = MockSecretStore::new();
        s.set(KEYRING_SECRET_ID, &raw).expect("seed");
        let err = load_or_create(&s).expect_err("损坏必须报错");
        assert!(err.contains(needle), "损坏形态 [{needle}] 报错不符: {err}");
        // 关键：报错后**没有**被新密钥覆盖（真源未被静默改写）
        assert_eq!(
            s.get(KEYRING_SECRET_ID).expect("read").as_deref(),
            Some(raw.as_str()),
            "解析失败绝不能顺手重写密钥环"
        );
    }
    // 收尾：确认正常环不受影响（对照组）
    let s = MockSecretStore::new();
    let ring = load_or_create(&s).expect("ok");
    assert_eq!(ring.signing_key().expect("key").len(), JWT_SECRET_KEY_LEN);
    assert!(good.signing_key().is_ok());
}
