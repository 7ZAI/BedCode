//! 设备认证 JWT 签发/验签门面（中心自持密钥 · ADR 0033）
//!
//! **入场密钥的真源在本插件**：签发与验签都走 `pairing::jwt`（HS256 自实现），
//! 密钥材料来自 `pairing::keys` 的密钥环（`host-auth secret-store`，属主 = 本插件）。
//! 宿主 `utils/auth/jwt.rs` 已随 ABI v33 整个退役——**不再有 `device-token-issue` /
//! `device-token-verify` 原语**。
//!
//! 两层结构（可测性）：纯函数 [`issue_token_with`] / [`verify_token_with`] 收注入的
//! 密钥，wasm 包装 [`issue_device_token`] / [`verify_device_token`] 负责从密钥环取
//! 密钥。native（无宿主）路径下包装层显性失败，纯函数层可被单测直接覆盖。
//!
//! 文案映射沿用迁移前宿主同款契约（`pairing::jwt::jwt_error_message`）：过期 →
//! "Token expired"，其余 → "Invalid token"，不透出内部细节。

use crate::pairing::jwt::{self, DEFAULT_TOKEN_EXPIRY_SECS};

// ==================== 纯函数层（注入密钥，native 单测直接覆盖） ====================

/// 用指定密钥签发设备入场 token → `(token, expires_in)`
///
/// `expires_in` 与响应字段同值（7 天），也是密钥轮换的**宽限期**下界。
pub fn issue_token_with(
    key: &[u8],
    kid: Option<&str>,
    subject: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(String, u64), String> {
    if subject.is_empty() {
        return Err("jwt issuance requires non-empty subject".to_string());
    }
    let svc = match kid {
        Some(kid) => jwt::JwtService::with_kid(key.to_vec(), kid.to_string()),
        None => jwt::JwtService::with_key(key.to_vec()),
    };
    let token = svc
        .generate_token(
            subject.to_string(),
            device_name.filter(|s| !s.is_empty()).map(str::to_string),
            fingerprint.filter(|s| !s.is_empty()).map(str::to_string),
        )
        .map_err(|e| format!("jwt issuance failed: {e}"))?;
    Ok((token, DEFAULT_TOKEN_EXPIRY_SECS))
}

/// 用候选密钥集验签既有 token → claims JSON（供 reauth 换发与归属校验）
///
/// `keys` 由密钥环给出且**有序**（当前代优先）——轮换后宽限期内旧 token 仍可验签。
/// 错误文本已映射为用户可读文案（过期 / 其余），不透出内部细节。
///
/// **`kid` 不是授权门**（有意为之，别把它当安全判据）：`kid` 只是代次**诊断标签**。
/// 认证的真正判据是「签名能否用环内某把密钥验过」——若某 token 来自已被裁掉的代次，
/// 那把密钥已不在环内，签名验不过，**先就拒了**，不需要 `kid` 再拒一次。反过来，
/// 拿到环内密钥的人写什么 `kid` 都验得过（他已经有密钥），所以拿 `kid` 做闸门既
/// 冗余又给人「kid 参与了安全决策」的错觉。陌生 `kid` 的**诊断**由密钥环层做
/// （[`crate::pairing::keys::Keyring::knows_kid`]）。
pub fn verify_token_with(keys: &[&[u8]], token: &str) -> Result<serde_json::Value, String> {
    let claims = match jwt::verify_with_keys(keys, token, jwt::now_secs()) {
        Ok(claims) => claims,
        Err(e) => return Err(jwt::jwt_error_message(&e).to_string()),
    };
    serde_json::to_value(&claims).map_err(|e| format!("claims parse failed: {e}"))
}

// ==================== wasm 运行时层（从密钥环取密钥） ====================

/// 签发设备认证 JWT → `(token, expires_in)`
///
/// 密钥不可用（存储故障 / 密钥环损坏）→ **显性报错**，不降级为进程随机密钥
/// （那比重启即全灭更糟：用户以为已配对，实际每次重启全灭）。
#[cfg(target_arch = "wasm32")]
pub fn issue_device_token(
    subject: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(String, u64), String> {
    let ring = crate::pairing::keys::keyring_from_host_auth()?;
    let key = ring.signing_key()?;
    issue_token_with(
        key,
        Some(&ring.active_kid()),
        subject,
        device_name,
        fingerprint,
    )
}

/// 验签既有 token → claims JSON（供 reauth 换发与 biometric-bind 归属校验）
#[cfg(target_arch = "wasm32")]
pub fn verify_device_token(token: &str) -> Result<serde_json::Value, String> {
    let ring = crate::pairing::keys::keyring_from_host_auth()?;
    verify_token_with(&ring.verification_keys(), token)
}

/// 轮换入场签发密钥（ADR 0033 D4）→ `{ rotated: true, kid, previousKid? }`
///
/// 触发面 = `auth-grant` 的 `jwt` / `rotate-key` 动作（宿主命令面经
/// `host-auth auth-method-invoke` 零解析转发进来）。**不撤销既有 token**——
/// 撤销是撤销域的职责，轮换只换签发密钥；上一代在宽限期内继续可验签。
#[cfg(target_arch = "wasm32")]
pub fn rotate_signing_key() -> Result<serde_json::Value, String> {
    use bedcode_plugin_api::host::HostLog;
    let previous_kid = crate::pairing::keys::keyring_from_host_auth()
        .map(|r| r.active_kid())
        .ok();
    let ring = crate::pairing::keys::rotate_from_host_auth()?;
    let kid = ring.active_kid();
    // 凭据红线：只记 kid（代次标识）不记密钥任何片段
    bedcode_plugin_api::wasm_host::WasmHost.log_info(&format!(
        "device token signing key rotated to kid {kid} (previous generation kept for grace window)"
    ));
    Ok(serde_json::json!({
        "rotated": true,
        "kid": kid,
        "previousKid": previous_kid,
    }))
}

/// native（cargo test）路径：密钥依赖宿主 → 显性失败
#[cfg(not(target_arch = "wasm32"))]
pub fn issue_device_token(
    _subject: &str,
    _device_name: Option<&str>,
    _fingerprint: Option<&str>,
) -> Result<(String, u64), String> {
    Err("jwt issuance unavailable outside wasm runtime".to_string())
}

/// native 路径：验签同样显性失败（静默成功是最危险的默认值）
#[cfg(not(target_arch = "wasm32"))]
pub fn verify_device_token(_token: &str) -> Result<serde_json::Value, String> {
    Err("jwt verification unavailable outside wasm runtime".to_string())
}

/// native 路径：轮换显性失败
#[cfg(not(target_arch = "wasm32"))]
pub fn rotate_signing_key() -> Result<serde_json::Value, String> {
    Err("key rotation unavailable outside wasm runtime".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_a() -> Vec<u8> {
        (0u8..=0x1f).collect()
    }
    fn key_b() -> Vec<u8> {
        vec![0x42u8; 32]
    }

    // ==================== 签发 ====================

    /// 签发：往返可验，claims 形状正确，`expires_in` = 7 天
    #[test]
    fn issue_then_verify_roundtrip() {
        let (token, expires_in) = issue_token_with(
            &key_a(),
            Some("g1"),
            "device-1",
            Some("Pixel 9"),
            Some("fp-abc"),
        )
        .expect("issue");
        assert_eq!(expires_in, DEFAULT_TOKEN_EXPIRY_SECS);
        let claims = verify_token_with(&[&key_a()], &token).expect("verify");
        assert_eq!(claims["sub"], "device-1");
        assert_eq!(claims["device_name"], "Pixel 9");
        assert_eq!(claims["fingerprint"], "fp-abc");
        assert_eq!(claims["kid"], "g1");
    }

    /// 空串可选字段 = 缺省（不序列化成 `""`）——迁移前的空串 = None 语义保持
    #[test]
    fn empty_optional_fields_are_omitted() {
        let (token, _) =
            issue_token_with(&key_a(), Some("g1"), "device-1", Some(""), Some("")).expect("issue");
        let claims = verify_token_with(&[&key_a()], &token).expect("verify");
        assert!(claims.get("device_name").is_none(), "空串设备名不得出现");
        assert!(claims.get("fingerprint").is_none(), "空串指纹不得出现");
    }

    /// 反例：空 subject 拒绝（签不出「无主」凭证）
    #[test]
    fn empty_subject_is_rejected() {
        let err = issue_token_with(&key_a(), Some("g1"), "", None, None).expect_err("empty sub");
        assert!(err.contains("non-empty subject"), "got: {err}");
    }

    /// `kid = None` 的签发形态（迁移兼容：不带该字段）
    #[test]
    fn issue_without_kid_omits_the_field() {
        let (token, _) = issue_token_with(&key_a(), None, "device-1", None, None).expect("issue");
        let claims = verify_token_with(&[&key_a()], &token).expect("verify");
        assert!(claims.get("kid").is_none());
    }

    // ==================== 验签拒绝矩阵 ====================

    /// 反例：错误密钥 → 用户文案 "Invalid token"（不区分「签名错」与「结构错」，
    /// 但**不**泄露内部细节）
    #[test]
    fn wrong_key_gives_user_facing_invalid_token() {
        let (token, _) =
            issue_token_with(&key_a(), Some("g1"), "device-1", None, None).expect("issue");
        assert_eq!(
            verify_token_with(&[&key_b()], &token).unwrap_err(),
            "Invalid token"
        );
    }

    /// 反例：结构畸形 → 同款用户文案
    #[test]
    fn malformed_token_gives_user_facing_invalid_token() {
        for bad in ["not-a-jwt", "a.b", "", "a.b.c.d"] {
            assert_eq!(
                verify_token_with(&[&key_a()], bad).unwrap_err(),
                "Invalid token",
                "畸形 token: {bad}"
            );
        }
    }

    /// 反例：过期 → "Token expired"（与迁移前文案逐字一致）
    #[test]
    fn expired_token_gives_expiry_message() {
        // 直接构造一个已过期的 claims 并签名（用固定时间戳注入）
        let claims = jwt::JwtClaims::new_at_with_kid(
            "device-1".to_string(),
            Some("Pixel 9".to_string()),
            Some("fp-abc".to_string()),
            0,
            jwt::now_secs() - 3600,
            Some("g1".to_string()),
        );
        let token = jwt::JwtService::with_kid(key_a(), "g1".to_string())
            .encode(&claims)
            .expect("encode");
        assert_eq!(
            verify_token_with(&[&key_a()], &token).unwrap_err(),
            "Token expired"
        );
    }

    /// 边界：签名有效但 `kid` 写陌生代次 → **仍放行**（`kid` 是诊断标签不是闸门）
    ///
    /// 这条是「反直觉但正确」的一例，值得钉住：拿到环内密钥的一方写什么 `kid` 都验得过，
    /// 拿 `kid` 当闸门只会造出「kid 参与了安全决策」的错觉。真正的闸门是签名。
    #[test]
    fn valid_signature_with_unknown_kid_is_accepted() {
        let mut claims = jwt::JwtClaims::new_at(
            "device-1".to_string(),
            None,
            None,
            DEFAULT_TOKEN_EXPIRY_SECS,
            jwt::now_secs(),
        );
        claims.kid = Some("g99".to_string());
        let token = jwt::JwtService::with_key(key_a())
            .encode(&claims)
            .expect("encode");
        let out = verify_token_with(&[&key_a()], &token).expect("签名有效即放行");
        assert_eq!(out["kid"], "g99", "kid 原样透出（供诊断）");
    }

    // ==================== 轮换跨代（ADR 0033 D4） ====================

    /// 轮换后：旧 token 仍验签通过（宽限期），新 token 用新密钥
    #[test]
    fn rotation_grace_window_keeps_old_tokens_valid() {
        let (old_token, _) =
            issue_token_with(&key_a(), Some("g1"), "device-1", None, None).expect("issue");
        let (new_token, _) =
            issue_token_with(&key_b(), Some("g2"), "device-1", None, None).expect("issue");
        // 轮换后候选 = [新代, 上一代]
        let cur = key_b();
        let prev = key_a();
        let keys: Vec<&[u8]> = vec![&cur[..], &prev[..]];
        let old_claims = verify_token_with(&keys, &old_token).expect("旧 token 宽限期内可验");
        assert_eq!(old_claims["kid"], "g1");
        let new_claims = verify_token_with(&keys, &new_token).expect("新 token 可验");
        assert_eq!(new_claims["kid"], "g2");
    }

    /// 超出宽限期：更早一代的 token 拒绝（候选集已不含该密钥）
    #[test]
    fn token_beyond_grace_window_is_rejected() {
        let (ancient, _) =
            issue_token_with(&key_a(), Some("g1"), "device-1", None, None).expect("issue");
        let newest = vec![0x24u8; 32];
        let prev = key_b();
        let keys: Vec<&[u8]> = vec![&newest[..], &prev[..]];
        assert_eq!(
            verify_token_with(&keys, &ancient).unwrap_err(),
            "Invalid token"
        );
    }

    // ==================== native 包装层 ====================

    /// native（无宿主上下文）包装层必须显性失败——静默空 token 是最危险的默认值
    /// （会被消费方读成「已认证」）
    #[test]
    fn native_wrappers_fail_loudly() {
        let err = issue_device_token("d1", Some("Pixel"), Some("fp")).expect_err("native 签发失败");
        assert!(
            err.contains("unavailable outside wasm runtime"),
            "got: {err}"
        );

        let err = verify_device_token("a.b.c").expect_err("native 验签失败");
        assert!(
            err.contains("unavailable outside wasm runtime"),
            "got: {err}"
        );

        let err = rotate_signing_key().expect_err("native 轮换失败");
        assert!(
            err.contains("unavailable outside wasm runtime"),
            "got: {err}"
        );
    }
}
