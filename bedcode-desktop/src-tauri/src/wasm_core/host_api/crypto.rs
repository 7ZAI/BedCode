//! 宿主加密引擎域实现（v26 host-crypto，desktop 独有双端偏离）
//!
//! 插件经算法名调用宿主聚合的「全局加密方法大全」——引擎只提供**中性算法原语**
//! （ADR 0022 裁剪线），绝不做认证编排 / 协商流程 / 密钥策略。算法实现统一在
//! [`crate::crypto`]（注册表 + 白名单 + 抽象 trait），本模块只做**接线 + 权限门**：
//! WIT 参数 → `crypto/registry` 解析 → 执行 → 错误映射。
//!
//! 权限三域（风险拆分，对齐 ws:client/ws:server 先例）：
//! - `crypto:aead`：`aead_encrypt` / `aead_decrypt` / `aead_generate_key` / `aead_generate_nonce`
//! - `crypto:asym`：`key_agreement_generate` / `key_agreement_shared`
//! - `crypto:kdf`：`kdf_derive`
//!
//! 安全红线（AGENTS §8）：只给中性原语不属于「认证编排」——插件绝不可据此绕过
//! 认证链路；密钥材料由调用方传入，宿主身份密钥（Kd / JWT keystore）不外泄。

use crate::crypto::registry::{resolve_aead, resolve_kdf, resolve_key_agreement};
use crate::wasm_core::permission::{PERMISSION_CRYPTO_AEAD, PERMISSION_CRYPTO_ASYM, PERMISSION_CRYPTO_KDF};

/// 加密原语成功调用审计（AGENTS §8 / 票 04）：只记算法名，不落任何密钥/明文
fn audit(plugin_id: &str, api: &str, algorithm: &str) {
    tracing::info!(plugin_id = %plugin_id, api = %api, algorithm = %algorithm, "host-crypto 原语调用");
}

/// 注册表/提供器错误带上操作上下文（H-09）：`to_string()` 丢掉类型化错误与
/// 操作名，WIT 边界处多种函数共享同一泛错——故障时连是哪一步、哪个算法都
/// 说不清。统一加 `api[algorithm]` 前缀。
fn crypto_err(api: &str, algorithm: &str, e: impl std::fmt::Display) -> String {
    format!("{api}[{algorithm}]: {e}")
}

/// AEAD 加密（`crypto:aead`）
pub(crate) fn aead_encrypt(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_CRYPTO_AEAD, "host_crypto_aead_encrypt") {
        return Err("permission denied: crypto:aead".to_string());
    }
    let provider = resolve_aead(algorithm).map_err(|e| crypto_err("aead_encrypt", algorithm, e))?;
    let out = provider
        .encrypt(key, nonce, plaintext, aad)
        .map_err(|e| crypto_err("aead_encrypt", algorithm, e))?;
    audit(plugin_id, "host_crypto_aead_encrypt", algorithm);
    Ok(out)
}

/// AEAD 解密（`crypto:aead`）
pub(crate) fn aead_decrypt(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_CRYPTO_AEAD, "host_crypto_aead_decrypt") {
        return Err("permission denied: crypto:aead".to_string());
    }
    let provider = resolve_aead(algorithm).map_err(|e| crypto_err("aead_decrypt", algorithm, e))?;
    let out = provider
        .decrypt(key, nonce, ciphertext, aad)
        .map_err(|e| crypto_err("aead_decrypt", algorithm, e))?;
    audit(plugin_id, "host_crypto_aead_decrypt", algorithm);
    Ok(out)
}

/// AEAD 密钥生成（`crypto:aead`）
pub(crate) fn aead_generate_key(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_AEAD,
        "host_crypto_aead_generate_key",
    ) {
        return Err("permission denied: crypto:aead".to_string());
    }
    let key = resolve_aead(algorithm)
        .map_err(|e| crypto_err("aead_generate_key", algorithm, e))?
        .generate_key();
    audit(plugin_id, "host_crypto_aead_generate_key", algorithm);
    Ok(key)
}

/// AEAD nonce 生成（`crypto:aead`）
pub(crate) fn aead_generate_nonce(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_AEAD,
        "host_crypto_aead_generate_nonce",
    ) {
        return Err("permission denied: crypto:aead".to_string());
    }
    let nonce = resolve_aead(algorithm)
        .map_err(|e| crypto_err("aead_generate_nonce", algorithm, e))?
        .generate_nonce();
    audit(plugin_id, "host_crypto_aead_generate_nonce", algorithm);
    Ok(nonce)
}

/// KDF 密钥派生（`crypto:kdf`）
pub(crate) fn kdf_derive(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    length: u32,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(perm, plugin_id, PERMISSION_CRYPTO_KDF, "host_crypto_kdf_derive") {
        return Err("permission denied: crypto:kdf".to_string());
    }
    let provider = resolve_kdf(algorithm).map_err(|e| crypto_err("kdf_derive", algorithm, e))?;
    let out = provider
        .derive(salt, ikm, info, length as usize)
        .map_err(|e| crypto_err("kdf_derive", algorithm, e))?;
    // 审计（H-08）：KDF 派生与密钥协商返回派生秘密，同样必须落审计——
    // 合规监控不能只记加解密而漏记最敏感的派生/协商面
    audit(plugin_id, "host_crypto_kdf_derive", algorithm);
    Ok(out)
}

/// 密钥交换临时密钥对（`crypto:asym`）——返回 `private ‖ public` 定长字节
///
/// **布局契约（H-03）**：单缓冲 = 私钥 ‖ 公钥 顺序拼接，切分偏移 = 私钥长度，
/// 随算法而定（如 X25519 私/公均 32 字节 → 前 32 后 32）。切分知识在插件 SDK
/// 侧（它知道自己在请求哪个算法）；本原语不做算法特定切分（中性原语）。
/// **安全警告**：前半段是私钥材料，任何插件代码/日志都不应把它当公开 blob
/// 输出或打印——日志与错误路径不得携带密钥字节。
pub(crate) fn key_agreement_generate(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !super::check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_ASYM,
        "host_crypto_keyagreement_generate",
    ) {
        return Err("permission denied: crypto:asym".to_string());
    }
    let provider = resolve_key_agreement(algorithm).map_err(|e| crypto_err("key_agreement_generate", algorithm, e))?;
    let (private, public) = provider.generate_keypair();
    let mut out = Vec::with_capacity(private.len() + public.len());
    out.extend_from_slice(&private);
    out.extend_from_slice(&public);
    audit(plugin_id, "host_crypto_keyagreement_generate", algorithm);
    Ok(out)
}

/// 密钥交换共享密钥（`crypto:asym`）
pub(crate) fn key_agreement_shared(
    perm: &dyn crate::wasm_core::host_api::context::PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    local_private: &[u8],
    peer_public: &[u8],
) -> Result<Vec<u8>, String> {
    if !super::check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_ASYM,
        "host_crypto_keyagreement_shared",
    ) {
        return Err("permission denied: crypto:asym".to_string());
    }
    let provider = resolve_key_agreement(algorithm).map_err(|e| crypto_err("key_agreement_shared", algorithm, e))?;
    let out = provider
        .compute_shared(local_private, peer_public)
        .map_err(|e| crypto_err("key_agreement_shared", algorithm, e))?;
    // 审计（H-08）：密钥协商派生共享秘密，与 KDF 同属敏感面，必须落审计
    audit(plugin_id, "host_crypto_keyagreement_shared", algorithm);
    Ok(out)
}

// 测试整体迁至 tests/crypto.rs（#[path] 声明，保持模块树 crypto::tests 不变，
// 与 pty.rs 的同款约定）。
#[cfg(test)]
#[path = "tests/crypto.rs"]
mod tests;
