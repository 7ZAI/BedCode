//! 生物认证挑战状态机（票 07）——挑战的签发、单次消费与时效归本插件
//!
//! 语义与宿主旧 `BiometricChallengeManager` 逐字对齐（`utils/auth/biometric.rs`，
//! 该管理器随票 07 宿主退役后由本模块接管）：
//! - 挑战以设备指纹为键：同一设备多条通道共享一次挑战，新签发覆盖旧值；
//! - 单次有效（验证即消费，无论成功与否）、60s 过期（`BIO_CHALLENGE_TTL_SECS`）；
//! - 绑定闸门（已配对且绑定公钥）经 host-auth `biometric-credential-bound`
//!   原语判定——公钥不出宿主（凭据红线），验签执行点同样在宿主
//!   （`biometric-verify-signature` 原语）。
//!
//! 错误文案与宿主 HTTP 端点的 1009 分类逐字对齐（`ChallengeInvalid` →
//! "Biometric challenge invalid or expired" 等）。

use bedcode_plugin_api::host::{HostAuth, HostLog};
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

fn with_registry<T>(f: impl FnOnce(&mut std::collections::HashMap<String, Challenge>) -> T) -> Result<T, String> {
    let mut guard = CHALLENGES.lock().map_err(|e| format!("biometric challenge lock: {e}"))?;
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
/// 绑定闸门不过 → "Biometric credential not bound"；宿主原语故障 → 原文上抛
/// （对应宿主 `BiometricAuthError::Database` 分支）。
pub fn issue_challenge(host: &WasmHost, fingerprint: &str) -> Result<String, String> {
    if fingerprint.is_empty() {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    let bound = host
        .auth_biometric_credential_bound(fingerprint)
        .map_err(|e| e.message)?;
    if !bound {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    let nonce = new_nonce();
    with_registry(|registry| {
        registry.insert(
            fingerprint.to_string(),
            Challenge { nonce: nonce.clone(), created_secs: now_secs(), used: false },
        );
    })?;
    host.log_debug(&format!("biometric challenge issued (fingerprint len {})", fingerprint.len()));
    Ok(nonce)
}

/// 消费挑战：存在、未过期、未使用、匹配（任一不满足 → 挑战作废并报错）
fn consume(fingerprint: &str, nonce: &str) -> Result<(), String> {
    with_registry(|registry| {
        match registry.get_mut(fingerprint) {
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
        }
    })?
}

/// 验证生物认证签名 → 配对记录（`{id, deviceName}`，供 JWT sub 与回执）
///
/// 流程与宿主 `verify_biometric_challenge` 逐段对齐：消费挑战 → 配对存在性
/// （trusted-devices-list 按指纹查活跃记录）→ 绑定判定（宿主原语）→ 验签
/// （宿主原语，公钥不出宿主）。
pub fn verify_signature(
    host: &WasmHost,
    fingerprint: &str,
    nonce: &str,
    signature: &str,
) -> Result<serde_json::Value, String> {
    if fingerprint.is_empty() {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }
    consume(fingerprint, nonce).map_err(|_| MSG_CHALLENGE_INVALID.to_string())?;

    let records = host
        .auth_trusted_devices_list()
        .map_err(|e| e.message)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    let entry = records
        .iter()
        .find(|r| r["deviceFingerprint"].as_str() == Some(fingerprint) && r["isActive"].as_bool() == Some(true));
    let Some(entry) = entry else {
        return Err(MSG_DEVICE_NOT_PAIRED.to_string());
    };

    let bound = host
        .auth_biometric_credential_bound(fingerprint)
        .map_err(|e| e.message)?;
    if !bound {
        return Err(MSG_CREDENTIAL_NOT_BOUND.to_string());
    }

    let valid = host
        .auth_biometric_verify_signature(fingerprint, nonce, signature)
        .map_err(|e| e.message)?;
    if !valid {
        return Err(MSG_SIGNATURE_INVALID.to_string());
    }
    Ok(serde_json::json!({
        "id": entry["id"],
        "deviceName": entry["deviceName"],
    }))
}

/// 测试辅助：清空注册表（单测隔离；生产无调用方）
#[cfg(test)]
pub(crate) fn reset_for_tests() {
    with_registry(|registry| registry.clear()).expect("registry lock");
}
