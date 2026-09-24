//! host-crypto 宿主实现测试（`#[path]` 挂载自 host_api/crypto.rs 的 tests 模块）
//!
//! 验证：按名调度的加解密往返、权限三域门禁、秘钥材料长度校验、fail-visible 拒绝。

use super::*;
use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};
use crate::wasm_core::permission::{PERMISSION_CRYPTO_AEAD, PERMISSION_CRYPTO_ASYM, PERMISSION_CRYPTO_KDF};

// ==================== AEAD 域（crypto:aead） ====================

#[test]
fn aead_encrypt_decrypt_roundtrip_authorized() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
    let key = aead_generate_key(ctx.as_ref(), "p1", "aes-256-gcm").expect("key");
    let nonce = aead_generate_nonce(ctx.as_ref(), "p1", "aes-256-gcm").expect("nonce");
    let ct = aead_encrypt(ctx.as_ref(), "p1", "aes-256-gcm", &key, &nonce, b"payload", Some(b"aad")).expect("encrypt");
    let pt = aead_decrypt(ctx.as_ref(), "p1", "aes-256-gcm", &key, &nonce, &ct, Some(b"aad")).expect("decrypt");
    assert_eq!(pt, b"payload");
}

#[test]
fn aead_chacha20_roundtrip_authorized() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
    let key = aead_generate_key(ctx.as_ref(), "p1", "chacha20-poly1305").unwrap();
    let nonce = aead_generate_nonce(ctx.as_ref(), "p1", "chacha20-poly1305").unwrap();
    let ct = aead_encrypt(ctx.as_ref(), "p1", "chacha20-poly1305", &key, &nonce, b"data", None).unwrap();
    let pt = aead_decrypt(ctx.as_ref(), "p1", "chacha20-poly1305", &key, &nonce, &ct, None).unwrap();
    assert_eq!(pt, b"data");
}

#[test]
fn aead_unknown_algorithm_rejected() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
    let err = aead_generate_key(ctx.as_ref(), "p1", "toy-cipher").unwrap_err();
    assert!(err.contains("toy-cipher"), "未知名必须显式失败且带名: {err}");
}

#[test]
fn aead_without_permission_rejected() {
    let ctx = build_host_ctx();
    // 未授权任何 crypto 权限
    let err = aead_generate_key(ctx.as_ref(), "p1", "aes-256-gcm").unwrap_err();
    assert!(err.contains("permission denied"), "无 crypto:aead 必须拒绝: {err}");
}

#[test]
fn aead_wrong_key_length_rejected() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
    // 31 字节密钥（非法），不截断
    let err = aead_encrypt(ctx.as_ref(), "p1", "aes-256-gcm", &[0u8; 31], &[0u8; 12], b"x", None).unwrap_err();
    assert!(err.contains("密钥"), "长度不足必须显式拒绝: {err}");
}

// ==================== 权限域隔离（crypto:aead 不给 keyagreement） ====================

#[test]
fn crypto_domain_isolated_across_permissions() {
    // 只授 crypto:aead → keyagreement（crypto:asym）被拒；只授 crypto:kdf → aead 被拒
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
    assert!(aead_generate_key(ctx.as_ref(), "p1", "aes-256-gcm").is_ok());
    assert!(
        key_agreement_generate(ctx.as_ref(), "p1", "x25519").is_err(),
        "asym 未授权不得放行"
    );

    let ctx2 = build_host_ctx();
    grant_permissions(&ctx2, "p2", &[PERMISSION_CRYPTO_KDF]);
    assert!(kdf_derive(ctx2.as_ref(), "p2", "hkdf-sha256", None, b"ikm", b"info", 32).is_ok());
    assert!(
        aead_generate_key(ctx2.as_ref(), "p2", "aes-256-gcm").is_err(),
        "aead 未授权不得放行"
    );
}

// ==================== KDF 域（crypto:kdf） ====================

#[test]
fn kdf_derive_authorized_deterministic() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_KDF]);
    let k1 = kdf_derive(ctx.as_ref(), "p1", "hkdf-sha256", Some(b"salt"), b"ikm", b"ctx", 32).unwrap();
    let k2 = kdf_derive(ctx.as_ref(), "p1", "hkdf-sha256", Some(b"salt"), b"ikm", b"ctx", 32).unwrap();
    assert_eq!(k1, k2, "同输入幂等");
    assert_eq!(k1.len(), 32);
}

// ==================== 密钥交换域（crypto:asym） ====================

#[test]
fn keyagreement_shared_secret_two_peers() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_ASYM]);
    grant_permissions(&ctx, "p2", &[PERMISSION_CRYPTO_ASYM]);
    let alice = key_agreement_generate(ctx.as_ref(), "p1", "x25519").unwrap();
    let bob = key_agreement_generate(ctx.as_ref(), "p2", "x25519").unwrap();
    assert_eq!(alice.len(), 64, "x25519 定长拼接应为 64 字节");
    assert_eq!(bob.len(), 64);
    let (alice_priv, alice_pub) = alice.split_at(32);
    let (bob_priv, bob_pub) = bob.split_at(32);

    let s_a = key_agreement_shared(ctx.as_ref(), "p1", "x25519", alice_priv, bob_pub).unwrap();
    let s_b = key_agreement_shared(ctx.as_ref(), "p2", "x25519", bob_priv, alice_pub).unwrap();
    assert_eq!(s_a, s_b, "双方共享密钥一致");
    assert_eq!(s_a.len(), 32);
}

#[test]
fn keyagreement_wrong_key_length_rejected() {
    let ctx = build_host_ctx();
    grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_ASYM]);
    let err = key_agreement_shared(ctx.as_ref(), "p1", "x25519", &[0u8; 16], &[0u8; 32]).unwrap_err();
    assert!(err.contains("私钥长度"), "非法私钥长度必须显式拒绝: {err}");
}
