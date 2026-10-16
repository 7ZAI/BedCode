//! Crypto Utilities（server-lib-split：真源已随 `bedcode-crypto-engine` crate 拆出）
//!
//! 本文件保留 `crate::utils::crypto::*` 模块路径与全部公开名字（含平铺便捷名
//! `aes_decrypt` / `hkdf_sha256` 等），宿主其余代码引用不受影响。
//!
//! 加密/解密工具模块，覆盖三类安全场景：HTTP 报文加密（AEAD + HKDF）、常规
//! 非对称加密（RSA-OAEP / RSA-PSS / X25519 ECDH）、文件加密传输（混合加密）。
//! 模块组织按职责扁平拆分，每个子模块聚焦单一算法族，互不依赖。

pub use bedcode_crypto_engine::{aes_gcm, chacha, hybrid, kdf, rsa, x25519};

pub use bedcode_crypto_engine::{
    aes_decrypt, aes_encrypt, aes_generate_key, aes_generate_nonce, chacha_decrypt, chacha_encrypt,
    chacha_generate_key, chacha_generate_nonce, derive_aes_key, hkdf_sha256, rsa_decrypt, rsa_encrypt_public,
    rsa_generate, rsa_sign, rsa_verify_public, x25519_decrypt, x25519_diffie_hellman, x25519_encrypt, x25519_generate,
    HybridCiphertext, HybridEnvelope, RsaKeyPair, RsaPublicKey, X25519KeyPair, X25519SharedSecret,
};
