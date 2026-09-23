//! Crypto 引擎注册表 —— 算法名 → 实现（聚合全局加密方法大全）
//!
//! 宿主加密引擎的唯一入口：按算法名解析出对应能力实现，供内部过滤器与宿主原语
//! （host-crypto，票 03/04）统一调度。白名单是引擎级词汇表（单一真源），未知算法名
//! **显式失败**（fail-visible），绝不留静默回退。
//!
//! 本模块只做「名称 → 实现」映射，不含任何具体算法逻辑（逻辑在 provider）。

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::crypto::provider::{
    AeadProvider, AesGcmProvider, ChaCha20Poly1305Provider, HkdfSha256Provider, KdfProvider, KeyAgreementProvider,
    X25519Provider,
};
use crate::system::error::{AppError, Result};

// ==================== 域注册表 ====================

/// 加密能力注册表：各能力域独立命名空间（避免跨域算法名冲突），名字即白名单
pub struct CryptoProviders {
    aead: HashMap<&'static str, &'static dyn AeadProvider>,
    kdf: HashMap<&'static str, &'static dyn KdfProvider>,
    key_agreement: HashMap<&'static str, &'static dyn KeyAgreementProvider>,
}

impl CryptoProviders {
    fn new() -> Self {
        let mut aead: HashMap<&'static str, &'static dyn AeadProvider> = HashMap::new();
        aead.insert(AesGcmProvider.name(), &AesGcmProvider);
        aead.insert(ChaCha20Poly1305Provider.name(), &ChaCha20Poly1305Provider);

        let mut kdf: HashMap<&'static str, &'static dyn KdfProvider> = HashMap::new();
        kdf.insert(HkdfSha256Provider.name(), &HkdfSha256Provider);

        let mut key_agreement: HashMap<&'static str, &'static dyn KeyAgreementProvider> = HashMap::new();
        key_agreement.insert(X25519Provider.name(), &X25519Provider);

        Self {
            aead,
            kdf,
            key_agreement,
        }
    }
}

/// 静态单例注册表（进程级，不可变）
pub static PROVIDERS: LazyLock<CryptoProviders> = LazyLock::new(CryptoProviders::new);

// ==================== 按名调度（宿主唯一入口） ====================

/// 按名取 AEAD（认证加密）实现
pub fn resolve_aead(name: &str) -> Result<&'static dyn AeadProvider> {
    PROVIDERS
        .aead
        .get(name)
        .copied()
        .ok_or_else(|| AppError::InvalidInput(format!("未知 AEAD 算法: '{name}'（不在白名单）")))
}

/// 按名取 KDF（密钥派生）实现
pub fn resolve_kdf(name: &str) -> Result<&'static dyn KdfProvider> {
    PROVIDERS
        .kdf
        .get(name)
        .copied()
        .ok_or_else(|| AppError::InvalidInput(format!("未知 KDF 算法: '{name}'（不在白名单）")))
}

/// 按名取密钥交换（ECDH）实现
pub fn resolve_key_agreement(name: &str) -> Result<&'static dyn KeyAgreementProvider> {
    PROVIDERS
        .key_agreement
        .get(name)
        .copied()
        .ok_or_else(|| AppError::InvalidInput(format!("未知密钥交换算法: '{name}'（不在白名单）")))
}

// ==================== 白名单词汇（供审计 / 校验 / 测试断言） ====================

/// 已注册 AEAD 算法名列表
pub fn registered_aead_names() -> Vec<&'static str> {
    PROVIDERS.aead.keys().copied().collect()
}

/// 已注册 KDF 算法名列表
pub fn registered_kdf_names() -> Vec<&'static str> {
    PROVIDERS.kdf.keys().copied().collect()
}

/// 已注册密钥交换算法名列表
pub fn registered_key_agreement_names() -> Vec<&'static str> {
    PROVIDERS.key_agreement.keys().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::provider::{AEAD_AES_256_GCM, AEAD_CHACHA20_POLY1305, KDF_HKDF_SHA256, KEY_AGREEMENT_X25519};

    // ==================== 按名调度：合法名命中 ====================

    #[test]
    fn resolve_aead_hits_known_names() {
        assert_eq!(resolve_aead(AEAD_AES_256_GCM).unwrap().name(), AEAD_AES_256_GCM);
        assert_eq!(
            resolve_aead(AEAD_CHACHA20_POLY1305).unwrap().name(),
            AEAD_CHACHA20_POLY1305
        );
    }

    #[test]
    fn resolve_kdf_hits_known_name() {
        assert_eq!(resolve_kdf(KDF_HKDF_SHA256).unwrap().name(), KDF_HKDF_SHA256);
    }

    #[test]
    fn resolve_key_agreement_hits_known_name() {
        assert_eq!(
            resolve_key_agreement(KEY_AGREEMENT_X25519).unwrap().name(),
            KEY_AGREEMENT_X25519
        );
    }

    // ==================== 白名单拒绝：未知名显式失败 ====================

    #[test]
    fn resolve_aead_unknown_rejected() {
        match resolve_aead("toy-cipher") {
            Ok(_) => panic!("未知名必须 fail-visible"),
            Err(e) => {
                assert!(
                    matches!(e, AppError::InvalidInput(_)),
                    "未知名必须 fail-visible，实际: {e}"
                );
                assert!(e.to_string().contains("toy-cipher"), "错误应含算法名: {e}");
            }
        }
    }

    #[test]
    fn resolve_kdf_unknown_rejected() {
        assert!(matches!(resolve_kdf("md5"), Err(AppError::InvalidInput(_))));
    }

    #[test]
    fn resolve_key_agreement_unknown_rejected() {
        assert!(matches!(
            resolve_key_agreement("aes-gcm"),
            Err(AppError::InvalidInput(_))
        ));
    }

    // ==================== AEAD 按名全流程往返 ====================

    #[test]
    fn aead_roundtrip_via_registry_both_algorithms() {
        for name in [AEAD_AES_256_GCM, AEAD_CHACHA20_POLY1305] {
            let p = resolve_aead(name).unwrap();
            let key = p.generate_key();
            let nonce = p.generate_nonce();
            let plaintext = b"registry roundtrip payload";
            let aad = Some(b"ctx-v1" as &[u8]);
            let ct = p.encrypt(&key, &nonce, plaintext, aad).unwrap();
            let pt = p.decrypt(&key, &nonce, &ct, aad).unwrap();
            assert_eq!(pt, plaintext, "算法 {name} 加解密往返不一致");
        }
    }

    #[test]
    fn aead_wrong_key_length_rejected() {
        let p = resolve_aead(AEAD_AES_256_GCM).unwrap();
        // 31 字节密钥，非法
        let err = p.encrypt(&[0u8; 31], &[0u8; 12], b"data", None).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "变长密钥必须显式拒绝: {err}");
    }

    #[test]
    fn aead_wrong_nonce_length_rejected() {
        let p = resolve_aead(AEAD_AES_256_GCM).unwrap();
        let err = p.encrypt(&[0u8; 32], &[0u8; 4], b"data", None).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    // ==================== KDF 按名往返 ====================

    #[test]
    fn kdf_derive_via_registry() {
        let p = resolve_kdf(KDF_HKDF_SHA256).unwrap();
        let key = p.derive(Some(b"salt"), b"ikm", b"ctx", 32).unwrap();
        assert_eq!(key.len(), 32);
        // 幂等：同输入同输出
        let again = p.derive(Some(b"salt"), b"ikm", b"ctx", 32).unwrap();
        assert_eq!(key, again);
    }

    // ==================== 密钥交换按名往返 ====================

    #[test]
    fn x25519_shared_secret_via_registry() {
        let p = resolve_key_agreement(KEY_AGREEMENT_X25519).unwrap();
        let (alice_priv, alice_pub) = p.generate_keypair();
        let (bob_priv, bob_pub) = p.generate_keypair();
        let a = p.compute_shared(&alice_priv, &bob_pub).unwrap();
        let b = p.compute_shared(&bob_priv, &alice_pub).unwrap();
        assert_eq!(a, b, "X25519 双方共享密钥必须一致");
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn x25519_wrong_key_length_rejected() {
        let p = resolve_key_agreement(KEY_AGREEMENT_X25519).unwrap();
        let err = p.compute_shared(&[0u8; 31], &[0u8; 32]).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    // ==================== 白名单词汇 ====================

    #[test]
    fn registered_names_match_expected_minimal_set() {
        let mut aead = registered_aead_names();
        aead.sort();
        assert_eq!(aead, vec![AEAD_AES_256_GCM, AEAD_CHACHA20_POLY1305]);
        assert_eq!(registered_kdf_names(), vec![KDF_HKDF_SHA256]);
        assert_eq!(registered_key_agreement_names(), vec![KEY_AGREEMENT_X25519]);
    }
}
