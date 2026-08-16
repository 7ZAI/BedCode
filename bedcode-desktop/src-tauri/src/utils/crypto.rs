//! Crypto Utilities
//!
//! 加密/解密工具模块，覆盖项目中三类安全场景：
//!
//! - **HTTP 报文加密**：对称 AEAD（AES-256-GCM / ChaCha20-Poly1305）配合 HKDF 派生会话密钥，
//!   适合高吞吐、低延迟的请求/响应加密。
//! - **常规非对称加密**：RSA-OAEP 加解密、RSA-PSS 签名/验签，以及 X25519 ECDH 密钥协商，
//!   用于公钥分发、握手认证与一次性密钥封装。
//! - **文件加密传输**：混合加密（非对称封装会话密钥 + 对称 AEAD 加密载荷），
//!   一次封装即可安全传输任意大小文件，详见 [`hybrid`]。
//!
//! 模块组织按职责扁平拆分，每个子模块聚焦单一算法族，互不依赖，便于单独测试与替换。

pub mod aes_gcm;
pub mod chacha;
pub mod hybrid;
pub mod kdf;
pub mod rsa;
pub mod x25519;

pub use aes_gcm::{
    decrypt as aes_decrypt, encrypt as aes_encrypt, generate_key as aes_generate_key,
    generate_nonce as aes_generate_nonce,
};
pub use chacha::{
    decrypt as chacha_decrypt, encrypt as chacha_encrypt,
    generate_key as chacha_generate_key, generate_nonce as chacha_generate_nonce,
};
pub use hybrid::{HybridCiphertext, HybridEnvelope, x25519_decrypt, x25519_encrypt};
pub use kdf::{derive_aes_key, hkdf_sha256};
pub use rsa::{
    RsaKeyPair, RsaPublicKey, rsa_decrypt, rsa_encrypt_public, rsa_generate, rsa_sign,
    rsa_verify_public,
};
pub use x25519::{X25519KeyPair, X25519SharedSecret, x25519_diffie_hellman, x25519_generate};
