//! 拒绝（策略四类，语义不变） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/policy/mod.rs 迁出）

use super::*;

use crate::pairing::jwt::JwtService;

/// 反例：iss 策略违反（非 JWT_ISSUER）→ 拒绝
#[test]
fn wrong_issuer_denies() {
    let t = token("device-1", "Evil", Some("fp-abc"), 3600);
    let err = eval(&t, &[]).expect_err("deny");
    assert!(err.contains("issuer"), "拒绝原因必须可读: {}", err);
}
/// 反例：sub 为空 → 拒绝
#[test]
fn empty_subject_denies() {
    let t = token("", "BedCode", Some("fp-abc"), 3600);
    assert!(eval(&t, &[]).is_err());
}
/// 反例：过期 token → 拒绝
#[test]
fn expired_token_denies() {
    // exp = now - 1（严格 `<` 判过期，exp == now 不算过期）
    let claims = JwtClaims::new_at(
        "device-1".to_string(),
        Some("Pixel 9".to_string()),
        Some("fp-abc".to_string()),
        0,
        jwt::now_secs() - 1,
    );
    let t = JwtService::with_key(vec![0x42u8; 32])
        .encode(&claims)
        .expect("encode token");
    assert!(eval(&t, &[]).is_err());
}
/// 反例：结构非法（非三段 / payload 非 base64url / claims 不可解析）→ 拒绝
#[test]
fn malformed_tokens_deny() {
    assert!(eval("one.segment", &[]).is_err());
    assert!(eval("a.b.c", &[]).is_err(), "b 非 base64url");
    // 合法 base64url 但非 claims JSON
    let bad_payload = format!("x.{}.y", jwt::b64url_encode(b"not-json"));
    assert!(eval(&bad_payload, &[]).is_err());
}
/// 边界：撤销优先于其他一切——已撤销设备的**签名有效** token 也拒绝，
/// 且拒绝原因指向撤销（不是签名）
#[test]
fn revocation_overrides_valid_signature() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    let err = eval(&t, &[revoked_record("fp-abc")]).expect_err("deny");
    assert!(err.contains("revoked"));
}
/// 反例：指纹已撤销（active=false）→ 拒绝，带撤销语义
#[test]
fn revoked_fingerprint_denies() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    let err = eval(&t, &[revoked_record("fp-abc")]).expect_err("deny");
    assert!(err.contains("revoked"), "拒绝原因必须可读: {}", err);
}
