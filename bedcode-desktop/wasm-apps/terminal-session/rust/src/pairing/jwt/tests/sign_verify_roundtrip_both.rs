//! 验签矩阵（正例 / 反例 / 边界） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/jwt.rs 迁出）

use super::*;

/// 往返：自签自验（无 `kid` 与带 `kid` 两种形态都成立）
#[test]
fn sign_verify_roundtrip_both_kid_forms() {
    for svc in [
        JwtService::with_key(fixed_key_b()),
        JwtService::with_kid(fixed_key_b(), "g3".to_string()),
    ] {
        let token = svc
            .generate_token(
                "device-1".to_string(),
                Some("Pixel".to_string()),
                Some("fp".to_string()),
            )
            .expect("issue");
        let claims = svc.verify_token_with_expiry(&token).expect("verify");
        assert_eq!(claims.sub, "device-1");
        assert_eq!(claims.iss, JWT_ISSUER);
        assert_eq!(claims.device_name.as_deref(), Some("Pixel"));
        assert_eq!(claims.kid, svc.kid.clone());
    }
}
/// 反例：错误密钥 → `InvalidSignature`（不泄露是哪一步失败）
#[test]
fn wrong_key_is_invalid_signature() {
    let token = JwtService::with_key(fixed_key_b())
        .generate_token("device-1".to_string(), None, None)
        .expect("issue");
    let other = JwtService::with_key(vec![0x42u8; 32]);
    assert_eq!(
        other.verify_token_with_expiry(&token).unwrap_err(),
        JwtError::InvalidSignature
    );
}
/// 反例：过期 → `TokenExpired`（两把密钥都过期时不得退化成「签名错」）
#[test]
fn expired_token_reports_expiry_not_signature() {
    // 过期窗口 0 + 签发时间往前推 > leeway，确保落在 leeway 之外
    let svc = JwtService::with_key_and_expiry(fixed_key_b(), 0);
    let token = svc
        .generate_token_at(
            "device-1".to_string(),
            None,
            None,
            now_secs() - VERIFY_LEEWAY_SECS - 100,
        )
        .expect("issue");
    let err = svc.verify_token_with_expiry(&token).unwrap_err();
    assert_eq!(err, JwtError::TokenExpired, "过期必须是过期（文案可区分）");
    // 多密钥路径同样短路为「过期」而不是尝试完全部密钥后报签名错
    let signing = fixed_key_b();
    let other = vec![0x42u8; 32];
    let keys: Vec<&[u8]> = vec![&signing[..], &other[..]];
    assert_eq!(
        verify_with_keys(&keys, &token, now_secs()).unwrap_err(),
        JwtError::TokenExpired
    );
}
/// 反例：结构畸形（非三段 / 非 base64url / alg 非 HS256 / claims 非 JSON）
/// → `InvalidToken` / `VerifyError`，**不得**误报为签名错
#[test]
fn malformed_tokens_are_not_signature_errors() {
    let key = fixed_key_b();
    let keys: Vec<&[u8]> = vec![&key[..]];
    for bad in [
        "one.segment",
        "",
        "a.b.c",
        &format!("x.{}.y", b64url_encode(b"not-json")),
        // alg 改成 HS512（signature 段保持合法 base64url）
        &format!(
            "{}.{}.{}",
            b64url_encode(br#"{"typ":"JWT","alg":"HS512"}"#),
            b64url_encode(br#"{"sub":"d","iss":"BedCode","iat":1,"exp":9999999999}"#),
            b64url_encode(&[0u8; 32])
        ),
    ] {
        let err = verify_with_keys(&keys, bad, now_secs()).expect_err("畸形必须拒绝");
        assert!(
            matches!(err, JwtError::InvalidToken | JwtError::VerifyError(_)),
            "结构类错误不得报成签名错: {bad} -> {err:?}"
        );
    }
}
/// 边界：空密钥集 = 配置错误（与「签名不对」区分，不静默当通过）
#[test]
fn empty_key_set_is_a_configuration_error() {
    let err = verify_with_keys(&[], "a.b.c", now_secs()).expect_err("空密钥集");
    assert!(
        matches!(err, JwtError::VerifyError(ref m) if m.contains("no verification key")),
        "got: {err:?}"
    );
}
