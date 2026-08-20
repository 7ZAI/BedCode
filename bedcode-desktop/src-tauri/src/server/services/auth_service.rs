//! Authentication Service
//!
//! 处理设备配对和认证逻辑

use crate::db::Pairing;
use crate::system::app_context::AppContext;
use crate::utils::auth::biometric::verify_biometric_signature;

// ==================== 生物认证（WS 与 HTTP 共用） ====================

/// 生物认证业务错误（WS / HTTP 调用方各自映射为协议错误格式）
#[derive(Debug)]
pub enum BiometricAuthError {
    /// 设备未配对
    NotPaired,
    /// 已配对但未绑定生物凭证公钥
    CredentialNotBound,
    /// 挑战值无效（不存在/过期/已消费/不匹配），携带底层原因
    ChallengeInvalid(String),
    /// 生物签名校验失败
    SignatureInvalid(String),
    /// 数据库访问失败（internal，调用方按服务端异常处理）
    Database(String),
}

/// 签发生物认证挑战值（WS BiometricRequest 与 HTTP biometric-challenge 共用）
///
/// 设备必须已配对且绑定生物凭证公钥，否则拒绝下发（与 WS 旧路径语义一致：
/// 未配对与无凭证统一归为 CredentialNotBound）。挑战以设备指纹为键：
/// 同一设备的多条通道共享一次挑战，单次有效、60s 过期
pub async fn issue_biometric_challenge(fingerprint: &str) -> std::result::Result<String, BiometricAuthError> {
    if fingerprint.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    let pairing = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard.get_pairing_by_fingerprint(fingerprint)
    }
    .map_err(|e| BiometricAuthError::Database(e.to_string()))?;

    let binding_ready = pairing.as_ref().map(|p| !p.public_key.is_empty()).unwrap_or(false);
    if !binding_ready {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    let nonce = AppContext::global().biometric_challenges().generate(fingerprint).await;
    Ok(nonce)
}

/// 验证生物认证签名并返回配对记录（WS BiometricVerify 与 HTTP biometric-verify 共用）
///
/// 按指纹消费挑战值（单次有效）；随后取配对记录，用绑定公钥验签。
/// 成功返回 pairing：调用方凭其 id 签发 JWT，并须保留 public_key（防旧端点覆盖清空）
pub async fn verify_biometric_challenge(
    fingerprint: &str,
    nonce: &str,
    signature: &str,
) -> std::result::Result<Pairing, BiometricAuthError> {
    if fingerprint.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    // 1. 校验并消费挑战值（单次、未过期、匹配）
    if let Err(e) = AppContext::global()
        .biometric_challenges()
        .verify_and_consume(fingerprint, nonce)
        .await
    {
        return Err(BiometricAuthError::ChallengeInvalid(e.to_string()));
    }

    // 2. 取配对记录与绑定的公钥
    let pairing = {
        let db_guard = AppContext::global().db().lock().await;
        db_guard.get_pairing_by_fingerprint(fingerprint)
    }
    .map_err(|e| BiometricAuthError::Database(e.to_string()))?;
    let Some(pairing) = pairing else {
        return Err(BiometricAuthError::NotPaired);
    };
    if pairing.public_key.is_empty() {
        return Err(BiometricAuthError::CredentialNotBound);
    }

    // 3. 验签（生物认证通过后由安全硬件签名）
    if let Err(e) = verify_biometric_signature(&pairing.public_key, nonce, signature) {
        return Err(BiometricAuthError::SignatureInvalid(e.to_string()));
    }

    Ok(pairing)
}

/// 格式化设备显示名称：名称 + 首次连接 IP
pub fn format_device_display_name(device_name: &str, address: &str) -> String {
    // address 格式为 "IP:PORT"，提取 IP 部分
    let ip = address.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(address);
    format!("{} ({})", device_name, ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== format_device_display_name ====================

    #[test]
    fn test_display_name_extracts_ip_with_port() {
        assert_eq!(
            format_device_display_name("My Phone", "192.168.1.5:8080"),
            "My Phone (192.168.1.5)"
        );
    }

    #[test]
    fn test_display_name_ipv6_with_port() {
        // IPv6 地址带端口时，rsplit_once 只切最后一个冒号，括号保留
        assert_eq!(
            format_device_display_name("Phone", "[fe80::1]:8080"),
            "Phone ([fe80::1])"
        );
    }

    #[test]
    fn test_display_name_without_port_keeps_address() {
        assert_eq!(format_device_display_name("Phone", "myhost"), "Phone (myhost)");
    }
}
