//! 轮换（ADR 0033 D4） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/keys.rs 迁出）

use super::*;

/// 轮换：新生一代成为 active，原代降为上一代（**仍可验签**——宽限期 7 天）
#[test]
fn rotate_keeps_previous_generation_for_grace_window() {
    let store = MockSecretStore::new();
    let before = load_or_create(&store).expect("first");
    let old_key = before.signing_key().expect("key").to_vec();

    let after = rotate(&store).expect("rotate");
    assert_eq!(after.active, 2, "轮换推进代次");
    assert_ne!(
        after.signing_key().expect("key"),
        old_key.as_slice(),
        "新代必须换新密钥"
    );
    // 上一代仍在验签候选里（跨代验签是轮换的全部意义）
    let verify_keys = after.verification_keys();
    assert_eq!(verify_keys.len(), 2, "轮换后应有两代可验签");
    assert!(
        verify_keys.contains(&old_key.as_slice()),
        "宽限期内旧代必须仍能验签"
    );
    // 当前代优先
    assert_eq!(verify_keys[0], after.signing_key().expect("key"));
    // kid 语义
    assert_eq!(after.active_kid(), "g2");
    assert!(after.knows_kid(Some("g2")));
    assert!(after.knows_kid(Some("g1")), "上一代 kid 仍被接受");
    assert!(!after.knows_kid(Some("g99")), "未知 kid 必须被识别出来");
    assert!(after.knows_kid(None), "旧 token 无 kid → 一律走密钥尝试");
}
/// 连续轮换两次：环内**恒**至多两代（更老的裁掉——token 早过宽限期）
#[test]
fn rotation_is_bounded_and_drops_expired_generations() {
    let store = MockSecretStore::new();
    let gen1_key = load_or_create(&store)
        .expect("g1")
        .signing_key()
        .expect("k")
        .to_vec();
    let gen2_key = rotate(&store)
        .expect("g2")
        .signing_key()
        .expect("k")
        .to_vec();
    let ring3 = rotate(&store).expect("g3");
    assert_eq!(ring3.active, 3);
    assert_eq!(
        ring3.verification_keys().len(),
        MAX_GENERATIONS,
        "环内代次必须有界，不无限累积"
    );
    let keys = ring3.verification_keys();
    assert!(keys.contains(&gen2_key.as_slice()), "上一代保留");
    assert!(
        !keys.contains(&gen1_key.as_slice()),
        "超出宽限期的最早一代必须被裁掉"
    );
    // 落库形状也只含两代（不是只在内存里裁）
    let stored = store.get(KEYRING_SECRET_ID).expect("read").expect("row");
    let file: KeyringFile = serde_json::from_str(&stored).expect("json");
    assert_eq!(file.keys.len(), MAX_GENERATIONS);
    assert!(!file.keys.contains_key(&1));
}
/// 轮换后重启：读回的是新代（轮换是持久的，不因重启回退）
#[test]
fn rotation_survives_restart() {
    let store = MockSecretStore::new();
    rotate(&store).expect("g2");
    let reloaded = load_or_create(&store).expect("reload");
    assert_eq!(reloaded.active, 2, "轮换必须落盘，重启不回退");
}
