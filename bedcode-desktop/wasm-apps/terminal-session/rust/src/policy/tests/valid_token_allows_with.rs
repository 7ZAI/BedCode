//! 放行（正例 / 边界） — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/policy/mod.rs 迁出）

use super::*;

use crate::pairing::jwt::JwtService;

/// 正例：合法 token（签名有效 + iss/sub/指纹齐全 + 未过期）+ 空镜像 → 放行，
/// claims JSON 可回读
#[test]
fn valid_token_allows_with_claims_json() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    let out = eval(&t, &[]).expect("allow");
    let claims: JwtClaims = serde_json::from_str(&out).expect("claims json");
    assert_eq!(claims.sub, "device-1");
    assert_eq!(claims.fingerprint.as_deref(), Some("fp-abc"));
}
/// 正例：指纹已信任（active=true）→ 放行
#[test]
fn trusted_fingerprint_allows() {
    let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
    assert!(eval(&t, &[active_record("fp-abc")]).is_ok());
}
/// 边界：指纹存在但内核无记录（无信任锚点）→ 从宽放行
/// （搬迁前镜像语义的保持；收紧为「未配对即拒绝」是新协议决策，不在本批次）
#[test]
fn unknown_fingerprint_allows() {
    let t = token("device-1", "BedCode", Some("fp-ghost"), 3600);
    assert!(eval(&t, &[active_record("fp-abc")]).is_ok());
}
/// 边界：指纹缺失 → 放行（无信任锚点，仅凭验签）
#[test]
fn missing_fingerprint_allows() {
    let t = token("device-1", "BedCode", None, 3600);
    assert!(eval(&t, &[]).is_ok());
}
