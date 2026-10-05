//! 轮换跨代验签（ADR 0033 D4） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/jwt.rs 迁出）

use super::*;

/// 跨代：轮换后**新旧 token 都能验签**（宽限期内上一代仍是合法验签密钥），
/// 超出宽限期的更早代（已不在候选集）→ 拒。
#[test]
fn rotation_grace_window_accepts_previous_generation_only() {
    let gen1 = JwtService::with_kid(fixed_key_b(), "g1".to_string());
    let gen2_key = vec![0x42u8; 32];
    let gen2 = JwtService::with_kid(gen2_key.clone(), "g2".to_string());
    let gen3_key = vec![0x24u8; 32];
    let gen3 = JwtService::with_kid(gen3_key.clone(), "g3".to_string());

    let t1 = gen1
        .generate_token("d1".to_string(), None, None)
        .expect("g1 token");
    let t2 = gen2
        .generate_token("d1".to_string(), None, None)
        .expect("g2 token");
    let t3 = gen3
        .generate_token("d1".to_string(), None, None)
        .expect("g3 token");

    // 轮换两次后候选 = [g3, g2]（g1 已被裁掉）
    let keys: Vec<&[u8]> = vec![&gen3_key[..], &gen2_key[..]];
    assert_eq!(keys.len(), 2);
    assert!(
        verify_with_keys(&keys, &t3, now_secs()).is_ok(),
        "当前代必须可验"
    );
    assert!(
        verify_with_keys(&keys, &t2, now_secs()).is_ok(),
        "上一代必须可验（宽限期）"
    );
    assert_eq!(
        verify_with_keys(&keys, &t1, now_secs()).unwrap_err(),
        JwtError::InvalidSignature,
        "超出宽限期的最早一代必须拒绝"
    );
}
/// 跨代：**无 `kid`** 的旧 token（迁移前形态）也走同一候选集——否则 D1 一上线
/// 存量 token 会在「有密钥但认不出 kid」时被误拒
#[test]
fn legacy_token_without_kid_still_verifies_against_previous_key() {
    let legacy = JwtService::with_key(vec![0x42u8; 32])
        .generate_token("d1".to_string(), None, None)
        .expect("legacy token");
    let err = verify_with_keys(&[], &legacy, now_secs()).expect_err("空密钥集");
    assert!(
        matches!(&err, JwtError::VerifyError(m) if m.contains("no verification key")),
        "got: {err:?}"
    );
    // 新一代密钥 + 上一代（旧签名）同时在候选里
    let keys: Vec<&[u8]> = vec![&[0x24u8; 32][..], &[0x42u8; 32][..]];
    let claims = verify_with_keys(&keys, &legacy, now_secs()).expect("无 kid 旧 token 可验");
    assert_eq!(claims.kid, None, "旧 token 的 kid 保持缺省");
    assert_eq!(claims.sub, "d1");
}
