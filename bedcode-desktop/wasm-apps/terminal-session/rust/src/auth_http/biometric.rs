//! 生物认证挑战状态机（票 07 + **B-downsink 2026-09-30**）——挑战的签发、单次消费
//! 与时效归本插件，**公钥真源与验签执行点也归本插件**（宿主 host-auth
//! `biometric-*` 三原语已退役）：
//! - 挑战以设备指纹为键：同一设备多条通道共享一次挑战，新签发覆盖旧值；
//! - 单次有效（验证即消费，无论成功与否）、60s 过期（`BIO_CHALLENGE_TTL_SECS`）；
//! - 公钥存本插件私有库 `auth_biometric_keys`（`auth_records::biometric_key_*`），
//!   验签用 WASM 内 p256（`verify_biometric_signature`，与宿主旧实现同构）。
//!
//! 错误文案与宿主 HTTP 端点的 1009 分类逐字对齐（`ChallengeInvalid` →
//! "Biometric challenge invalid or expired" 等）。

use bedcode_plugin_api::host::HostLog;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 挑战有效期（秒）——与宿主 `BIO_CHALLENGE_TTL_SECS` 同值
pub const BIO_CHALLENGE_TTL_SECS: u64 = 60;

/// 用户可见错误文案（宿主 HTTP 端点 1008/1009 分类逐字复刻）
pub const MSG_CREDENTIAL_NOT_BOUND: &str = "Biometric credential not bound";
pub const MSG_CHALLENGE_INVALID: &str = "Biometric challenge invalid or expired";
pub const MSG_DEVICE_NOT_PAIRED: &str = "Device not paired";
pub const MSG_SIGNATURE_INVALID: &str = "Biometric signature verification failed";

/// 挑战条目（单消费标记 + 签发时刻）
struct Challenge {
    nonce: String,
    created_secs: u64,
    used: bool,
}

/// 挑战注册表（wasm 单实例；静态 Mutex 与配对码 / QR 状态机同模式）
static CHALLENGES: std::sync::Mutex<Option<std::collections::HashMap<String, Challenge>>> =
    std::sync::Mutex::new(None);

fn with_registry<T>(
    f: impl FnOnce(&mut std::collections::HashMap<String, Challenge>) -> T,
) -> Result<T, String> {
    let mut guard = CHALLENGES
        .lock()
        .map_err(|e| format!("biometric challenge lock: {e}"))?;
    let registry = guard.get_or_insert_with(std::collections::HashMap::new);
    Ok(f(registry))
}

/// 当前时间（unix 秒，与挑战 TTL 对齐；`pairing::jwt::now_secs` 同源）
fn now_secs() -> u64 {
    crate::pairing::jwt::now_secs()
}

/// 16 字节随机数 → 32 字符 hex（宿主 `BiometricChallenge::new` 同格式）
fn new_nonce() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("getrandom: entropy unavailable");
    hex::encode(buf)
}

/// 签发挑战：闸门（已配对且绑定公钥）→ 覆盖式登记新挑战
///
/// 错误文案即 HTTP 1008 响应的 message（宿主 controller 映射后同形）：
/// 绑定闸门不过 → "Biometric credential not bound"；存储故障 → 原文上抛。
pub fn issue_challenge(host: &WasmHost, fingerprint: &str) -> Result<String, String> {
    if fingerprint.is_empty() {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    // B-downsink：配对活跃 + 私有库公钥存在（不再查 host-auth bound 原语）
    let bound = crate::auth_records::pairing_active(fingerprint)?
        && crate::auth_records::biometric_key_get(fingerprint)?.is_some();
    if !bound {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    let nonce = new_nonce();
    with_registry(|registry| {
        registry.insert(
            fingerprint.to_string(),
            Challenge {
                nonce: nonce.clone(),
                created_secs: now_secs(),
                used: false,
            },
        );
    })?;
    host.log_debug(&format!(
        "biometric challenge issued (fingerprint len {})",
        fingerprint.len()
    ));
    Ok(nonce)
}

/// 消费挑战：存在、未过期、未使用、匹配（任一不满足 → 挑战作废并报错）
fn consume(fingerprint: &str, nonce: &str) -> Result<(), String> {
    with_registry(|registry| match registry.get_mut(fingerprint) {
        None => Err("No active biometric challenge".to_string()),
        Some(challenge) => {
            if now_secs().saturating_sub(challenge.created_secs) >= BIO_CHALLENGE_TTL_SECS {
                registry.remove(fingerprint);
                Err("Biometric challenge expired".to_string())
            } else if challenge.used {
                registry.remove(fingerprint);
                Err("Biometric challenge already consumed".to_string())
            } else if challenge.nonce != nonce {
                Err("Biometric challenge mismatch".to_string())
            } else {
                challenge.used = true;
                registry.remove(fingerprint);
                Ok(())
            }
        }
    })?
}

/// 验证生物认证签名 → 配对记录（`{id, deviceName}`，供 JWT sub 与回执）
///
/// 流程：消费挑战 → 配对存在性（认证中心私有库按指纹查活跃记录）→ 绑定判定
/// + 验签（B-downsink：公钥从本库取，WASM 内 p256 验签，不再经宿主原语）。
pub fn verify_signature(
    _host: &WasmHost,
    fingerprint: &str,
    nonce: &str,
    signature: &str,
) -> Result<serde_json::Value, String> {
    if fingerprint.is_empty() {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    consume(fingerprint, nonce).map_err(|_| MSG_CHALLENGE_INVALID.to_string())?;

    // 配对记录真源 = 认证中心私有库（2026-09-22 下沉；活跃记录判定）
    let entry = crate::auth_records::records()?
        .into_iter()
        .find(|r| r.device_fingerprint == fingerprint && r.is_active);
    let Some(entry) = entry else {
        return Err(MSG_DEVICE_NOT_PAIRED.to_string());
    };

    let Some(public_key) = crate::auth_records::biometric_key_get(fingerprint)? else {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    };

    if !verify_biometric_signature(&public_key, nonce, signature)? {
        return Err(MSG_SIGNATURE_INVALID.to_string());
    }
    Ok(serde_json::json!({
        "id": entry.id,
        "deviceName": entry.device_name,
    }))
}

/// P-256 ECDSA 验证（SPKI DER base64 公钥 + r||s base64 签名）——WASM 内验签
///
/// 与宿主旧 `utils/auth/biometric.rs::verify_biometric_signature` 同构
/// （p256 crate：`from_public_key_der` + `Signature::from_slice` + `verify`）。
/// 公钥合规性错误（编码/解析）→ `Err`；验签失败 → `Ok(false)`（业务上同
/// MSG_SIGNATURE_INVALID）。
pub fn verify_biometric_signature(
    public_key_spki_b64: &str,
    message: &str,
    signature_b64: &str,
) -> Result<bool, String> {
    use base64::Engine;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    let spki_der = base64::engine::general_purpose::STANDARD
        .decode(public_key_spki_b64)
        .map_err(|e| format!("Invalid public key encoding: {}", e))?;
    let verifying_key = VerifyingKey::from_public_key_der(&spki_der)
        .map_err(|e| format!("Invalid public key: {}", e))?;
    let raw_sig = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| format!("Invalid signature encoding: {}", e))?;
    let signature =
        Signature::from_slice(&raw_sig).map_err(|e| format!("Invalid signature: {}", e))?;
    Ok(verifying_key.verify(message.as_bytes(), &signature).is_ok())
}

/// 测试辅助：清空注册表（单测隔离；生产无调用方）
#[cfg(test)]
pub(crate) fn reset_for_tests() {
    with_registry(|registry| registry.clear()).expect("registry lock");
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePublicKey;

    /// 造一对测试密钥 + SPKI base64 公钥 + r\|\|s 原始格式签名（与移动端
    /// Android Keystore 输出一致，手法同宿主旧 biometric.rs 单测）
    fn sign_with_temp_key(message: &str) -> (String, String) {
        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let spki_der = signing_key
            .verifying_key()
            .to_public_key_der()
            .expect("encode public key");
        let spki_b64 = base64::engine::general_purpose::STANDARD.encode(spki_der.as_bytes());
        let signature: p256::ecdsa::Signature = signing_key.sign(message.as_bytes());
        let (r, s) = signature.split_scalars();
        let mut raw = r.to_bytes().to_vec();
        raw.extend_from_slice(&s.to_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        (spki_b64, sig_b64)
    }

    /// WASM 内 p256 验签（B-downsink 后的验签执行点）：合法签名通过；
    /// 篡改消息 / 篡改签名 / 错误公钥均拒绝；编排经私有库（配对 + 公钥行）
    #[test]
    fn verify_biometric_signature_roundtrip() {
        let message = "0123456789abcdef0123456789abcdef";
        let (spki_b64, sig_b64) = sign_with_temp_key(message);

        assert!(
            verify_biometric_signature(&spki_b64, message, &sig_b64).expect("verify"),
            "合法签名必须通过"
        );
        // 篡改消息
        assert!(
            !verify_biometric_signature(&spki_b64, "tampered", &sig_b64).expect("verify tampered"),
            "篡改消息必须拒"
        );
        // 篡改签名（截断 1 字节后重新编码）→ `from_slice` 形态错误，显性 Err
        // （与宿主旧 biometric.rs 单测语义一致：畸形签名是 Err，不是 Ok(false)）
        let mut raw_sig = base64::engine::general_purpose::STANDARD
            .decode(&sig_b64)
            .expect("decode sig");
        raw_sig.truncate(raw_sig.len() - 1);
        let bad_sig = base64::engine::general_purpose::STANDARD.encode(&raw_sig);
        assert!(
            verify_biometric_signature(&spki_b64, message, &bad_sig).is_err(),
            "截断签名（形态错误）必须是 Err"
        );

        // 错误公钥验签拒绝（票据 29 语义：绑定公钥以外的 key 不得通过）
        let (other_spki, _) = sign_with_temp_key(message);
        assert!(
            !verify_biometric_signature(&other_spki, message, &sig_b64).expect("verify wrong key"),
            "错误公钥必须拒绝"
        );
    }

    /// 编码错误（非 base64 / 非法公钥）显性 Err，不静默当「验签失败」
    #[test]
    fn verify_biometric_signature_malformed_inputs_error() {
        assert!(
            verify_biometric_signature("not-base64!!", "m", "x").is_err(),
            "非 base64 公钥 → Err"
        );
        let (spki_b64, sig_b64) = sign_with_temp_key("m");
        assert!(
            verify_biometric_signature(&spki_b64, "m", "%%%%").is_err(),
            "非法签名编码 → Err（不是 Ok(false)）"
        );
    }
}
