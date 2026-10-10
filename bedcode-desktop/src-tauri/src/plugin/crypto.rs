//! host-crypto 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 04 自内核迁出（`host_api/crypto.rs` 整文件删除，
//! 内核反向锁 `crypto_domain_must_not_return_to_wasm_core` 防回接）。与 pty / mdns /
//! ws / http 等「窄端口」域不同，host-crypto **没有独立能力 crate**——装配方就是
//! 宿主本 crate（孤儿规则见 [`super::bindings`] 模块文档），域函数与 WIT impl 同处，
//! 不经端口、不经 `submit_domain_ports_installer!`（guest 调用经 WIT impl 直接取
//! `WasmPluginState` 携带的宿主上下文，无进程级/实例级两级装配面）。
//!
//! ## 哪些机制**刻意留在宿主**（AGENTS §5.1.3 允许的薄壳）
//!
//! - **权限三域门**（`crypto:aead` / `crypto:asym` / `crypto:kdf`，风险拆分对齐
//!   `ws:client`/`ws:server` 先例）：复用 wasm-core 的 `host_api::check_permission`
//!   （同一份 PermissionManager、同一条拒绝 warn 路径），闸门不应可插拔；
//! - **审计**（AGENTS §8 / 票 04）：成功调用只记算法名，不落任何密钥/明文——日志
//!   与错误路径不得携带密钥字节；
//! - **算法解析**：统一在 `bedcode-crypto-engine`（注册表 + 白名单 + trait，中性
//!   原语），本文件只做「WIT 参数 → 解析 → 执行 → 错误映射」的接线。
//!
//! ## 安全红线（AGENTS §8）
//!
//! 只给中性原语不属于「认证编排」——插件绝不可据此绕过认证链路；密钥材料由调用方
//! 传入，宿主身份密钥（Kd / JWT keystore）不外泄。未知算法名显性失败（fail-visible），
//! 错误带 `api[algorithm]` 上下文（H-09），密钥协商返回 `private ‖ public` 定长拼接
//! （切分知识在插件 SDK 侧，H-03）。

use bedcode_crypto_engine::registry::{resolve_aead, resolve_kdf, resolve_key_agreement};
use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::{
    PERMISSION_CRYPTO_AEAD, PERMISSION_CRYPTO_ASYM, PERMISSION_CRYPTO_KDF,
};
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{PermissionScope, WasmHostContext};

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "crypto";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-crypto"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["crypto:aead", "crypto:asym", "crypto:kdf"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 26`：host-crypto 三域原语在 ABI v26 引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 26,
};

/// host-crypto 能力模块
pub struct CryptoModule;

impl HostModule for CryptoModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(
        &self,
        linker: &mut wasmtime::component::Linker<WasmPluginState>,
    ) -> wasmtime::Result<()> {
        bedcode::plugin::host_crypto::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: CryptoModule = CryptoModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

/// 属主 = 调用方插件实例的 plugin_id（自 store state 派生，guest 无法伪造）；
/// 权限门在域函数内（下方），本层只做取上下文 → 转发 → 按 WIT `result` 形状返回。
impl bedcode::plugin::host_crypto::Host for WasmPluginState {
    fn aead_encrypt(
        &mut self,
        algorithm: String,
        key: Vec<u8>,
        nonce: Vec<u8>,
        plaintext: Vec<u8>,
        aad: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        aead_encrypt(
            ctx_of(self),
            &self.plugin_id,
            &algorithm,
            &key,
            &nonce,
            &plaintext,
            aad.as_deref(),
        )
    }

    fn aead_decrypt(
        &mut self,
        algorithm: String,
        key: Vec<u8>,
        nonce: Vec<u8>,
        ciphertext: Vec<u8>,
        aad: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        aead_decrypt(
            ctx_of(self),
            &self.plugin_id,
            &algorithm,
            &key,
            &nonce,
            &ciphertext,
            aad.as_deref(),
        )
    }

    fn aead_generate_key(&mut self, algorithm: String) -> Result<Vec<u8>, String> {
        aead_generate_key(ctx_of(self), &self.plugin_id, &algorithm)
    }

    fn aead_generate_nonce(&mut self, algorithm: String) -> Result<Vec<u8>, String> {
        aead_generate_nonce(ctx_of(self), &self.plugin_id, &algorithm)
    }

    fn kdf_derive(
        &mut self,
        algorithm: String,
        salt: Option<Vec<u8>>,
        ikm: Vec<u8>,
        info: Vec<u8>,
        length: u32,
    ) -> Result<Vec<u8>, String> {
        kdf_derive(
            ctx_of(self),
            &self.plugin_id,
            &algorithm,
            salt.as_deref(),
            &ikm,
            &info,
            length,
        )
    }

    fn keyagreement_generate(&mut self, algorithm: String) -> Result<Vec<u8>, String> {
        key_agreement_generate(ctx_of(self), &self.plugin_id, &algorithm)
    }

    fn keyagreement_shared(
        &mut self,
        algorithm: String,
        local_private: Vec<u8>,
        peer_public: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        key_agreement_shared(
            ctx_of(self),
            &self.plugin_id,
            &algorithm,
            &local_private,
            &peer_public,
        )
    }
}

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== 域函数（权限门 + 解析 + 审计；自 `host_api/crypto.rs` 迁入） ====================

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
fn aead_encrypt(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    key: &[u8],
    nonce: &[u8],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_CRYPTO_AEAD, "host_crypto_aead_encrypt") {
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
fn aead_decrypt(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_CRYPTO_AEAD, "host_crypto_aead_decrypt") {
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
fn aead_generate_key(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_CRYPTO_AEAD, "host_crypto_aead_generate_key") {
        return Err("permission denied: crypto:aead".to_string());
    }
    let key = resolve_aead(algorithm)
        .map_err(|e| crypto_err("aead_generate_key", algorithm, e))?
        .generate_key();
    audit(plugin_id, "host_crypto_aead_generate_key", algorithm);
    Ok(key)
}

/// AEAD nonce 生成（`crypto:aead`）
fn aead_generate_nonce(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !check_permission(
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
fn kdf_derive(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[u8],
    length: u32,
) -> Result<Vec<u8>, String> {
    if !check_permission(perm, plugin_id, PERMISSION_CRYPTO_KDF, "host_crypto_kdf_derive") {
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
fn key_agreement_generate(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
) -> Result<Vec<u8>, String> {
    if !check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_ASYM,
        "host_crypto_keyagreement_generate",
    ) {
        return Err("permission denied: crypto:asym".to_string());
    }
    let provider =
        resolve_key_agreement(algorithm).map_err(|e| crypto_err("key_agreement_generate", algorithm, e))?;
    let (private, public) = provider.generate_keypair();
    let mut out = Vec::with_capacity(private.len() + public.len());
    out.extend_from_slice(&private);
    out.extend_from_slice(&public);
    audit(plugin_id, "host_crypto_keyagreement_generate", algorithm);
    Ok(out)
}

/// 密钥交换共享密钥（`crypto:asym`）
fn key_agreement_shared(
    perm: &dyn PermissionScope,
    plugin_id: &str,
    algorithm: &str,
    local_private: &[u8],
    peer_public: &[u8],
) -> Result<Vec<u8>, String> {
    if !check_permission(
        perm,
        plugin_id,
        PERMISSION_CRYPTO_ASYM,
        "host_crypto_keyagreement_shared",
    ) {
        return Err("permission denied: crypto:asym".to_string());
    }
    let provider =
        resolve_key_agreement(algorithm).map_err(|e| crypto_err("key_agreement_shared", algorithm, e))?;
    let out = provider
        .compute_shared(local_private, peer_public)
        .map_err(|e| crypto_err("key_agreement_shared", algorithm, e))?;
    // 审计（H-08）：密钥协商派生共享秘密，与 KDF 同属敏感面，必须落审计
    audit(plugin_id, "host_crypto_keyagreement_shared", algorithm);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::setup_wasm_runtime;

    /// 构造无头（AppHandle=None）的宿主上下文——走内核公开测试装配入口
    /// `setup_wasm_runtime`（与宿主 adapter 测试同款；`manager::capability` 面不对外，
    /// 不能在宿主侧手工拼 `WasmHostContext::new` 的注册表参数）
    fn build_host_ctx() -> Arc<WasmHostContext> {
        let (_, ctx) = setup_wasm_runtime();
        ctx
    }

    // ==================== AEAD 域（crypto:aead） ====================

    #[test]
    fn aead_encrypt_decrypt_roundtrip_authorized() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "p1", &[PERMISSION_CRYPTO_AEAD]);
        let key = aead_generate_key(ctx.as_ref(), "p1", "aes-256-gcm").expect("key");
        let nonce = aead_generate_nonce(ctx.as_ref(), "p1", "aes-256-gcm").expect("nonce");
        let ct = aead_encrypt(
            ctx.as_ref(),
            "p1",
            "aes-256-gcm",
            &key,
            &nonce,
            b"payload",
            Some(b"aad"),
        )
        .expect("encrypt");
        let pt = aead_decrypt(ctx.as_ref(), "p1", "aes-256-gcm", &key, &nonce, &ct, Some(b"aad"))
            .expect("decrypt");
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
        let err =
            aead_encrypt(ctx.as_ref(), "p1", "aes-256-gcm", &[0u8; 31], &[0u8; 12], b"x", None).unwrap_err();
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

    // ==================== 白名单 / 自报三件一致（与 pty/ws/mdns/peer/http 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    ///
    /// `expected_host_modules()` 含本模块名即证明 `expect_host_module!` 行仍在且被收集
    /// （漏了 ⇒ 装载期 unlisted 点名；本用例让它在单测就红）。
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "crypto", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-crypto"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["crypto:aead", "crypto:asym", "crypto:kdf"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }
}
