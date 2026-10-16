//! 验签（ADR 0033 新增的第 0 道关） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/policy/mod.rs 迁出）

use super::*;

use crate::pairing::jwt::JwtService;

/// 反例：**签名被篡改** → 拒绝（claims 完全合法时也拒——这是 ADR 0033 把验签
/// 收进本模块后最核心的断言：宿主不再验签，这里是唯一防线）
#[test]
fn tampered_signature_denies_even_with_valid_claims() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    let tampered = format!("{}x", &t[..t.len() - 4]);
    let err = eval(&tampered, &[]).expect_err("篡改签名必须拒绝");
    assert!(err.contains("signature"), "拒绝原因必须指向签名: {err}");
}
/// 反例：**用别的密钥签的 token** → 拒绝（伪造者拿不到密钥环）
#[test]
fn token_signed_by_foreign_key_denies() {
    let mut claims = JwtClaims::new_at(
        "device-1".to_string(),
        Some("Pixel 9".to_string()),
        Some("fp-abc".to_string()),
        3600,
        jwt::now_secs(),
    );
    claims.kid = Some("g1".to_string());
    let foreign = JwtService::with_kid(vec![0x77u8; 32], "g1".to_string())
        .encode(&claims)
        .expect("encode");
    let err = eval(&foreign, &[]).expect_err("外来密钥签的 token 必须拒绝");
    assert!(err.contains("signature"), "got: {err}");
}
/// 反例：**空候选集**（密钥环不可用）→ 拒绝，绝不放行
#[test]
fn empty_key_set_denies_rather_than_allows() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    let err = evaluate(&t, &[], &[]).expect_err("无密钥必须拒绝");
    assert!(err.contains("signature"), "got: {err}");
}
/// 边界：轮换宽限期内，上一代密钥签的 token 仍放行（ADR 0033 D4）
#[test]
fn previous_generation_still_verifies_within_grace_window() {
    // 上一代密钥（ring_keys 的 [1]）签的 token，kid 仍写 g1（代次标识不随密钥变）
    let t = token_with("device-1", "BedCode", Some("fp-abc"), 3600, Some("g1"), 1);
    assert!(eval(&t, &[]).is_ok(), "宽限期内旧代 token 必须放行");
}
/// 边界：无 `kid` 的迁移前形态 token → 仍可验签（不因认不出 kid 而误拒）
#[test]
fn legacy_token_without_kid_still_allows() {
    let t = token_with("device-1", "BedCode", Some("fp-abc"), 3600, None, 0);
    let out = eval(&t, &[]).expect("无 kid 的旧 token 必须放行");
    let claims: JwtClaims = serde_json::from_str(&out).expect("claims json");
    assert_eq!(claims.kid, None);
}
