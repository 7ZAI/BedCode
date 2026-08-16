//! 生物认证挑战值与签名验证
//!
//! 连接握手防重放：桌面端下发一次性随机挑战值，移动端生物认证解锁私钥
//! 签名回传，桌面端用绑定公钥验签后签发 JWT。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::Result;

/// 挑战值字节数（16 字节 = 32 位 hex 字符）
pub const BIO_CHALLENGE_BYTES: usize = 16;
/// 挑战值有效期（秒）
pub const BIO_CHALLENGE_TTL_SECS: u64 = 60;

/// 生物认证挑战
#[derive(Debug, Clone)]
pub struct BiometricChallenge {
    /// 一次性随机挑战值（hex）
    pub nonce: String,
    /// 创建时间
    created_at: Instant,
    /// 是否已消费
    pub used: bool,
}

impl BiometricChallenge {
    fn new() -> Self {
        let random_bytes: [u8; BIO_CHALLENGE_BYTES] = rand::random();
        Self {
            nonce: hex::encode(random_bytes),
            created_at: Instant::now(),
            used: false,
        }
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() >= BIO_CHALLENGE_TTL_SECS
    }
}

/// 挑战值管理器
///
/// 以连接地址为键，保证挑战值只对该 WS 连接有效；单次使用、可过期
pub struct BiometricChallengeManager {
    challenges: Arc<Mutex<HashMap<String, BiometricChallenge>>>,
}

impl BiometricChallengeManager {
    pub fn new() -> Self {
        Self {
            challenges: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 为指定连接生成新挑战值（替换旧的）
    pub async fn generate(&self, addr: &str) -> String {
        let challenge = BiometricChallenge::new();
        let nonce = challenge.nonce.clone();
        self.challenges.lock().await.insert(addr.to_string(), challenge);
        tracing::debug!(addr = %addr, "Biometric challenge issued");
        nonce
    }

    /// 验证并消费挑战值：存在、未过期、未使用、匹配
    pub async fn verify_and_consume(&self, addr: &str, nonce: &str) -> Result<()> {
        let mut guard = self.challenges.lock().await;
        match guard.get_mut(addr) {
            None => Err(crate::AppError::Auth("No active biometric challenge".to_string())),
            Some(challenge) => {
                if challenge.is_expired() {
                    guard.remove(addr);
                    Err(crate::AppError::Auth("Biometric challenge expired".to_string()))
                } else if challenge.used {
                    Err(crate::AppError::Auth("Biometric challenge already used".to_string()))
                } else if challenge.nonce != nonce {
                    Err(crate::AppError::Auth("Biometric challenge mismatch".to_string()))
                } else {
                    challenge.used = true;
                    tracing::debug!(addr = %addr, "Biometric challenge consumed");
                    Ok(())
                }
            }
        }
    }

    /// 清除指定连接的挑战值（断连时调用）
    pub async fn clear(&self, addr: &str) {
        self.challenges.lock().await.remove(addr);
    }
}

impl Default for BiometricChallengeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 验证生物认证签名
///
/// - `public_key_spki_b64`: 绑定公钥（SPKI X.509 DER，base64）
/// - `message`: 被签名的消息（挑战值 hex 字符串的 UTF-8 字节）
/// - `signature_b64`: 签名（原始 r||s 格式，base64）
pub fn verify_biometric_signature(
    public_key_spki_b64: &str,
    message: &str,
    signature_b64: &str,
) -> Result<()> {
    use base64::Engine;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    let spki_der = base64::engine::general_purpose::STANDARD
        .decode(public_key_spki_b64)
        .map_err(|e| crate::AppError::Auth(format!("Invalid public key encoding: {}", e)))?;

    let verifying_key = VerifyingKey::from_public_key_der(&spki_der)
        .map_err(|e| crate::AppError::Auth(format!("Invalid public key: {}", e)))?;

    let raw_sig = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| crate::AppError::Auth(format!("Invalid signature encoding: {}", e)))?;

    let signature = Signature::from_slice(&raw_sig)
        .map_err(|e| crate::AppError::Auth(format!("Invalid signature: {}", e)))?;

    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| crate::AppError::Auth("Biometric signature verification failed".to_string()))?;

    tracing::info!("Biometric signature verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use p256::pkcs8::EncodePublicKey;
    use std::time::Duration;

    #[tokio::test]
    async fn test_challenge_generate_and_consume() {
        let manager = BiometricChallengeManager::new();
        let nonce = manager.generate("127.0.0.1:1234").await;
        assert_eq!(nonce.len(), 32);

        assert!(manager.verify_and_consume("127.0.0.1:1234", &nonce).await.is_ok());
        // 单次使用：重复消费失败
        assert!(manager.verify_and_consume("127.0.0.1:1234", &nonce).await.is_err());
    }

    #[tokio::test]
    async fn test_challenge_mismatch_and_unknown() {
        let manager = BiometricChallengeManager::new();
        let nonce = manager.generate("127.0.0.1:1234").await;
        assert!(manager.verify_and_consume("127.0.0.1:1234", "wrong").await.is_err());
        assert!(manager.verify_and_consume("127.0.0.1:9999", &nonce).await.is_err());
    }

    #[tokio::test]
    async fn test_challenge_clear() {
        let manager = BiometricChallengeManager::new();
        let nonce = manager.generate("127.0.0.1:1234").await;
        manager.clear("127.0.0.1:1234").await;
        assert!(manager.verify_and_consume("127.0.0.1:1234", &nonce).await.is_err());
    }

    #[tokio::test]
    async fn test_challenge_expired() {
        let manager = BiometricChallengeManager::new();
        let nonce = manager.generate("127.0.0.1:1234").await;
        // 手工伪造一个已过期挑战
        {
            let mut guard = manager.challenges.lock().await;
            let mut challenge = BiometricChallenge::new();
            challenge.nonce = nonce.clone();
            challenge.created_at = Instant::now() - Duration::from_secs(BIO_CHALLENGE_TTL_SECS + 1);
            guard.insert("127.0.0.1:1234".to_string(), challenge);
        }
        assert!(manager.verify_and_consume("127.0.0.1:1234", &nonce).await.is_err());
    }

    #[test]
    fn test_verify_signature_roundtrip() {
        use p256::ecdsa::signature::Signer;
        use p256::ecdsa::SigningKey;

        let signing_key = SigningKey::random(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        // 编码公钥为 SPKI base64（与 Android Keystore PublicKey.getEncoded() 一致）
        let spki_der = verifying_key.to_public_key_der().expect("encode public key");
        let spki_b64 = base64::engine::general_purpose::STANDARD.encode(spki_der.as_bytes());

        // 签名：r||s 原始格式（与 Android Keystore 转换后的输出一致）
        let message = "0123456789abcdef0123456789abcdef";
        let signature: p256::ecdsa::Signature = signing_key.sign(message.as_bytes());
        let (r, s) = signature.split_scalars();
        let mut raw = r.to_bytes().to_vec();
        raw.extend_from_slice(&s.to_bytes());
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&raw);

        assert!(verify_biometric_signature(&spki_b64, message, &sig_b64).is_ok());
        // 篡改消息应失败
        assert!(verify_biometric_signature(&spki_b64, "tampered", &sig_b64).is_err());
        // 篡改签名应失败
        let bad_sig = base64::engine::general_purpose::STANDARD.encode(&raw[..63]);
        assert!(verify_biometric_signature(&spki_b64, message, &bad_sig).is_err());
    }
}
