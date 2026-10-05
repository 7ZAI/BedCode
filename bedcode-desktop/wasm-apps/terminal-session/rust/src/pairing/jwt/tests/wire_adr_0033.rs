//! wire 格式冻结向量（ADR 0033） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/jwt.rs 迁出）

use super::*;

/// **wire 格式冻结向量**：固定 key + 固定 claims（无 `kid`，注入 iat/exp）→ 固定
/// token 串。这是 ADR 0033 迁移期的**兼容性锚点**：该常量逐字节等于 ADR 0033
/// 之前宿主 `jsonwebtoken` 与本实现共同产出的 token，断言它是为了保证
/// 「`kid` 是纯追加的可选字段」——一旦有人调整既有字段的顺序 / 名称 / header
/// 序列化，本用例立即转红（存量 wire 格式破了）。
#[test]
fn legacy_wire_format_is_byte_frozen() {
    let svc = JwtService::with_key(fixed_key_b());
    let claims = JwtClaims::new_at(
        "device-1".to_string(),
        Some("My Phone".to_string()),
        Some("fp-abc".to_string()),
        DEFAULT_TOKEN_EXPIRY_SECS,
        1700000000,
    );
    assert_eq!(claims.kid, None, "无 kid 形态的 claims 不带该字段");
    let token = svc.encode(&claims).expect("encode");
    assert_eq!(token, LEGACY_WIRE_VECTOR);
}
/// 反例：带 `kid` 的 token **只**多出一个尾字段，既有段与其余 claims 不变
/// （证明 `kid` 是纯追加，不是格式变更）
#[test]
fn kid_is_pure_additive_suffix() {
    let svc = JwtService::with_kid(fixed_key_b(), "g7".to_string());
    let with_kid = svc
        .generate_token_at("device-1".to_string(), None, None, 1700000000)
        .expect("encode");
    let without = JwtService::with_key(fixed_key_b())
        .generate_token_at("device-1".to_string(), None, None, 1700000000)
        .expect("encode");
    let (head_a, (payload_a_b64, _)) = split_head(&with_kid);
    let (head_b, (payload_b_b64, _)) = split_head(&without);
    assert_eq!(head_a, head_b, "header 段必须逐字不变");
    // payload 段：带 kid 者恰好多一个 "kid" 键
    let payload_a = String::from_utf8(b64url_decode(payload_a_b64).expect("payload b64url"))
        .expect("payload utf8");
    let payload_b = String::from_utf8(b64url_decode(payload_b_b64).expect("payload b64url"))
        .expect("payload utf8");
    assert!(
        payload_a.starts_with(&payload_b[..payload_b.len() - 1]),
        "kid 只追加在末尾: {payload_a}"
    );
    assert!(
        payload_a.ends_with(r#""kid":"g7"}"#),
        "kid 必须是最后一个字段: {payload_a}"
    );
    // 反序列化往返保留 kid
    let decoded = svc
        .verify_token_at(&with_kid, 1700000000)
        .expect("verify with kid");
    assert_eq!(decoded.kid.as_deref(), Some("g7"));
}
