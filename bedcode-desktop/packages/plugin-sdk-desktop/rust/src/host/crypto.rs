//! 宿主能力：加密引擎（WIT `host-crypto`，ABI v26，desktop 独有双端偏离）
//!
//! 插件经**算法名**调用宿主聚合的「全局加密方法大全」——引擎只提供中性算法原语
//! （ADR 0022 裁剪线），绝不做认证编排/协商流程/密钥策略。算法名白名单在宿主
//! 注册表（`crypto/registry`），未知名返回错误（fail-visible）；首版最小子集：
//! AEAD（aes-256-gcm / chacha20-poly1305）、KDF（hkdf-sha256）、X25519 密钥交换。
//! 绑定实现（bindgen 自由函数 + `HostError` 映射）在 `wasm_host.rs` 的
//! `impl HostCrypto for WasmHost`。
//!
//! # 权限三域（风险拆分，对齐 ws:client/ws:server 先例）
//!
//! - `crypto:aead`：`aead_encrypt` / `aead_decrypt` / `aead_generate_key` / `aead_generate_nonce`
//! - `crypto:asym`：`key_agreement_generate` / `key_agreement_shared`（密钥交换——密钥面）
//! - `crypto:kdf`：`kdf_derive`（密钥派生高敏：可从共享密钥派生出多方密钥）
//!
//! # 安全约束（AGENTS §8）
//!
//! - 密钥材料一律由调用方传入；宿主身份密钥（Kd / JWT keystore）不足出口。
//! - 本接口只给「中性算法原语」，不给「认证编排」——插件绝不能据此绕过认证链路。

use super::HostError;

/// 一次 X25519 密钥对生成的结果（`{ private, public }`，字节）。
#[derive(Debug, Clone)]
pub struct CryptoKeypair {
    /// 本端私钥（仅宿主内传、不入日志）
    pub private: Vec<u8>,
    /// 对端公钥（用于传播）
    pub public: Vec<u8>,
}

/// 宿主加密引擎能力抽象
pub trait HostCrypto {
    /// AEAD 加密（`crypto:aead`）：算法名 + 密钥/nonce/明文/aad（字节全部插件提供）
    /// `Ok(密文 ‖ 认证标签)`；key/nonce 长度算法定（32/12），宿主校验不截断
    fn aead_encrypt(
        &self,
        algorithm: &str,
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, HostError>;

    /// AEAD 解密（`crypto:aead`）；AEAD 完整性校验失败是明显的完整性错误（非「不存在」）
    fn aead_decrypt(
        &self,
        algorithm: &str,
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, HostError>;

    /// 生成通用 AEAD 随机密钥（`crypto:aead`）；算法名非法 → Err
    fn aead_generate_key(&self, algorithm: &str) -> Result<Vec<u8>, HostError>;

    /// 生成通用 AEAD 随机 nonce（`crypto:aead`）；算法名非法 → Err
    fn aead_generate_nonce(&self, algorithm: &str) -> Result<Vec<u8>, HostError>;

    /// KDF 密钥派生（`crypto:kdf`）：salt（可选）/ ikm / info → 指定长度密钥
    fn kdf_derive(
        &self,
        algorithm: &str,
        salt: Option<&[u8]>,
        ikm: &[u8],
        info: &[u8],
        length: u32,
    ) -> Result<Vec<u8>, HostError>;

    /// 密钥交换临时密钥对（`crypto:asym`）
    fn key_agreement_generate(&self, algorithm: &str) -> Result<CryptoKeypair, HostError>;

    /// 密钥交换共享密钥（`crypto:asym`）；可直接进 `kdf_derive`
    fn key_agreement_shared(
        &self,
        algorithm: &str,
        local_private: &[u8],
        peer_public: &[u8],
    ) -> Result<Vec<u8>, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{PERMISSION_CRYPTO_AEAD, PERMISSION_CRYPTO_ASYM, PERMISSION_CRYPTO_KDF};

    /// 三域权限常量与词汇真源对齐（编译期形状确认；宿主锁做集合相等断言）
    #[test]
    fn crypto_permission_constants_match_vocabulary() {
        assert_eq!(PERMISSION_CRYPTO_AEAD, "crypto:aead");
        assert_eq!(PERMISSION_CRYPTO_ASYM, "crypto:asym");
        assert_eq!(PERMISSION_CRYPTO_KDF, "crypto:kdf");
    }
}