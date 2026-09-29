//! JWT 签发/校验策略（HS256，终端会话中心 pairing 域）
//!
//! **入场密钥的真源在本插件**（v33 / ADR 0033）：本模块是签发与验签的**唯一**
//! 密码学实现，宿主 `utils/auth/jwt.rs` 已整个退役（宿主不再持有任何设备 JWT
//! 密码学）。密钥材料经 host-auth secret-store 落库（属主 = 本插件，见 `keys`
//! 模块的密钥环 + 轮换）。
//!
//! HS256 自实现（jsonwebtoken 因 ring 不可 wasm 编译）：base64url(header)
//! + "." + base64url(payload)，HMAC-SHA256(key, signing_input) 签名。
//!
//! - header JSON 序列化顺序与 jsonwebtoken 9.3.1 `Header::new(HS256)` 一致：
//!   `{"typ":"JWT","alg":"HS256"}`（typ 在前，其余可选字段 skip）
//! - claims 字段声明顺序固定：sub / iss / iat / exp /
//!   device_name(skip none) / fingerprint(skip none) / **kid(skip none)**。
//!   `kid` 声明在**最后**且 `skip_serializing_if = "Option::is_none"` ⇒ 不带
//!   `kid` 的 token 与 ADR 0033 之前**逐字节相同**（wire 格式不破）
//! - verify 复刻 jsonwebtoken `Validation::new(HS256)` 默认语义：恰好 3 段、
//!   base64url 解码、alg 必须 HS256、恒定时间验签、`exp` 必填、
//!   `exp < now - leeway(60)` 视为过期（leeway 语义）
//!
//! **轮换（ADR 0033 D4）**：签发只用当前代（`kid` 写入 claims），验签按
//! [`verify_with_keys`] 给出的候选密钥集**逐个尝试**（密钥环有序：当前代优先），
//! 使轮换后宽限期（= 最长 token TTL = 7 天）内旧 token 仍能验签。

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

/// 默认 JWT 过期时间（7 天）——也是密钥轮换的**宽限期**下界（上一代只保留这么久）
pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT 签发者标识（与宿主 `system::constants::auth::JWT_ISSUER` 逐字一致——
///// wire 兼容性要求，移动端与服务端对同一 token 的解读必须相同）
pub const JWT_ISSUER: &str = "BedCode";

/// JWT 密钥长度（字节）。HS256 要求 ≥32 字节（256 bit）
pub const JWT_SECRET_KEY_LEN: usize = 32;

/// 密钥在 host-auth secret-store 中的键名（按插件属主隔离，会话中心自有域）
///
/// **v33（ADR 0033）起本键名只用于**一次性清理**（旧实现写下的零生产调用点的死
/// 密钥行）；真源是 [`crate::pairing::keys::KEYRING_SECRET_ID`]。
pub const JWT_SECRET_KEY_ID: &str = "jwt.key";

/// 验签 leeway（秒）——复刻 jsonwebtoken `Validation::new` 默认值 60
const VERIFY_LEEWAY_SECS: u64 = 60;

/// header JSON（固定，无 kid/cty 等可选字段）——jsonwebtoken 9.3.1
/// `Header::new(Algorithm::HS256)` 序列化结果，顺序 typ → alg
///
/// **注意**：JWT 头的 `kid` 属于 header，但本仓把它放在 **claims** 里
/// （`JwtClaims::kid`）——header 是**逐字节固定**的格式锚点，往里加可选字段会
/// 改变所有 token 的第一段；claims 末尾追加可选字段则不破存量 wire 格式。
const HS256_HEADER_JSON: &str = r#"{"typ":"JWT","alg":"HS256"}"#;

type HmacSha256 = Hmac<Sha256>;

/// base64url 编码（无 padding）——jsonwebtoken 的 `URL_SAFE_NO_PAD`
pub(crate) fn b64url_encode(data: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(data)
}

/// base64url 解码（无 padding）— 解码失败返回 None（jsonwebtoken 解码失败 → InvalidToken）
pub(crate) fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

/// HS256 签名：HMAC-SHA256(key, signing_input) → base64url
///
/// RFC 7515 §3.2 定义；`signing_input` = `b64url(header) + "." + b64url(payload)`
pub(crate) fn sign_hs256(key: &[u8], signing_input: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    b64url_encode(&mac.finalize().into_bytes())
}

/// JWT Claims 结构 —— serde 按**声明顺序**序列化，故本顺序就是 wire 格式锚点。
/// 前六个字段与 ADR 0033 之前逐字一致；`kid` 追加在**最后**且缺省不序列化
/// ⇒ 不带 `kid` 的 token 与迁移前逐字节相同。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JwtClaims {
    /// 主题（设备 ID）
    pub sub: String,
    /// 签发者
    pub iss: String,
    /// 签发时间（unix 秒）
    pub iat: u64,
    /// 过期时间（unix 秒）
    pub exp: u64,
    /// 设备名称（可选，缺省不序列化）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 设备指纹（可选，缺省不序列化）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// 签发代次标识（可选，缺省不序列化）——密钥环的 `active` 代次（`g<n>`）。
    ///
    /// 缺失 = **迁移前**签发的 token（`kid` 字段那时还不存在）或经代次标识
    /// 关闭的形态；两种都按「逐个尝试密钥环候选」处理（`verify_with_keys`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
}

impl JwtClaims {
    /// 创建 claims（expires_in 相对当前时间）—— 无 `kid`（迁移兼容形态）
    pub fn new(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
    ) -> Self {
        Self::new_at(
            subject,
            device_name,
            fingerprint,
            expires_in_secs,
            now_secs(),
        )
    }

    /// 创建 claims 并注入当前时间戳（测试 / 对照用：注入时间戳才有确定性输出）
    pub fn new_at(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
        now_secs: u64,
    ) -> Self {
        Self {
            sub: subject,
            iss: JWT_ISSUER.to_string(),
            iat: now_secs,
            exp: now_secs + expires_in_secs,
            device_name,
            fingerprint,
            kid: None,
        }
    }

    /// 同 [`Self::new_at`]，额外写入签发代次（`kid`）—— 生产签发面用
    pub fn new_at_with_kid(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
        now_secs: u64,
        kid: Option<String>,
    ) -> Self {
        Self {
            kid,
            ..Self::new_at(subject, device_name, fingerprint, expires_in_secs, now_secs)
        }
    }

    /// 检查是否过期（`exp < now`）
    pub fn is_expired_at(&self, now_secs: u64) -> bool {
        self.exp < now_secs
    }

    /// 剩余有效时间（秒），过期钳制为 0
    pub fn remaining_secs_at(&self, now_secs: u64) -> u64 {
        if self.exp > now_secs {
            self.exp - now_secs
        } else {
            0
        }
    }
}

/// JWT 服务（HS256，密钥注入式）——生产构造经 [`crate::pairing::keys::Keyring`]
/// 取密钥环的当前代（+ `kid`），见 `auth_http::jwt` / `policy`
pub struct JwtService {
    key: Vec<u8>,
    /// 写入 claims 的签发代次（`None` = 不写 `kid`，迁移兼容形态）
    kid: Option<String>,
    default_expiry_secs: u64,
}

impl JwtService {
    /// 以固定密钥构造（测试注入 / 迁移兼容形态：不写 `kid`）
    pub fn with_key(key: Vec<u8>) -> Self {
        Self::with_key_and_expiry(key, DEFAULT_TOKEN_EXPIRY_SECS)
    }

    /// 固定密钥 + 自定义默认过期时间
    pub fn with_key_and_expiry(key: Vec<u8>, expiry_secs: u64) -> Self {
        Self {
            key,
            kid: None,
            default_expiry_secs: expiry_secs,
        }
    }

    /// 生产构造：密钥环当前代密钥 + 该代 `kid`（`kid` 写入 claims）
    pub fn with_kid(key: Vec<u8>, kid: String) -> Self {
        Self {
            key,
            kid: Some(kid),
            default_expiry_secs: DEFAULT_TOKEN_EXPIRY_SECS,
        }
    }

    /// 生成 JWT token（写入 `self.kid`，若已设置）
    pub fn generate_token(
        &self,
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) -> Result<String, JwtError> {
        self.generate_token_at(subject, device_name, fingerprint, now_secs())
    }

    /// 生成 JWT token（注入当前时间戳）——测试 / 对照用（注入时间戳才有确定性输出）
    pub fn generate_token_at(
        &self,
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        now_secs: u64,
    ) -> Result<String, JwtError> {
        let claims = JwtClaims::new_at_with_kid(
            subject,
            device_name,
            fingerprint,
            self.default_expiry_secs,
            now_secs,
            self.kid.clone(),
        );
        self.encode(&claims)
    }

    /// 对固定 claims 编码签名（对照测试的核心：两端同一 claims → 同一 token）
    pub fn encode(&self, claims: &JwtClaims) -> Result<String, JwtError> {
        let payload =
            serde_json::to_string(claims).map_err(|e| JwtError::EncodeError(e.to_string()))?;
        let signing_input = format!(
            "{}.{}",
            b64url_encode(HS256_HEADER_JSON.as_bytes()),
            b64url_encode(payload.as_bytes())
        );
        let signature = sign_hs256(&self.key, &signing_input);
        Ok(format!("{}.{}", signing_input, signature))
    }

    /// 验证并解码 JWT token（复刻 jsonwebtoken `Validation::new(HS256)` 默认
    /// 语义：leeway=60，`exp` 必填）——单密钥
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        self.verify_token_at(token, now_secs())
    }

    /// 验证并解码 JWT token（注入当前时间戳，单密钥）
    pub fn verify_token_at(&self, token: &str, now_secs: u64) -> Result<JwtClaims, JwtError> {
        verify_with_key(&self.key, token, now_secs)
    }

    /// 验证 token 并严格检查过期（`exp < now`，无 leeway）——语义同
    /// jsonwebtoken 默认 leeway 对「过期 60 秒内」的放行，此 API 收紧
    pub fn verify_token_with_expiry(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let claims = self.verify_token(token)?;
        if claims.is_expired_at(now_secs()) {
            return Err(JwtError::TokenExpired);
        }
        Ok(claims)
    }
}

/// 单密钥验签（HS256 核心；`JwtService` 与 [`verify_with_keys`] 共用）
///
/// 步骤序：① 三段切分 ② base64url 解码 ③ `alg == HS256` ④ 恒定时间验签
/// ⑤ claims 反序列化 ⑥ `exp` + leeway 校验。
/// **密码学在签名那一步**（此前只解结构）——ADR 0033 起这是入场凭证的唯一验签点。
fn verify_with_key(key: &[u8], token: &str, now_secs: u64) -> Result<JwtClaims, JwtError> {
    // 恰好 3 段（多段/缺段均为 InvalidToken，同 jsonwebtoken）
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidToken);
    }
    let (header_b64, payload_b64, signature_b64) = (parts[0], parts[1], parts[2]);

    // base64url 解码失败 → InvalidToken
    let header_bytes = b64url_decode(header_b64).ok_or(JwtError::InvalidToken)?;
    let payload_bytes = b64url_decode(payload_b64).ok_or(JwtError::InvalidToken)?;
    let signature_bytes = b64url_decode(signature_b64).ok_or(JwtError::InvalidToken)?;

    // header 必须是 JSON 对象且 alg == "HS256"
    // （jsonwebtoken：header 解析失败 → InvalidToken；alg 不匹配 → AlgorithmMismatch
    //   → 映射 VerifyError）
    let header: serde_json::Value =
        serde_json::from_slice(&header_bytes).map_err(|_| JwtError::InvalidToken)?;
    let alg = header
        .get("alg")
        .and_then(|v| v.as_str())
        .ok_or(JwtError::InvalidToken)?;
    if alg != "HS256" {
        return Err(JwtError::VerifyError("AlgorithmMismatch".to_string()));
    }

    // 恒定时间验签（hmac Mac::verify_slice）
    let signing_input = format!("{}.{}", header_b64, payload_b64);
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(signing_input.as_bytes());
    mac.verify_slice(&signature_bytes)
        .map_err(|_| JwtError::InvalidSignature)?;

    // claims 反序列化失败 → InvalidToken（jsonwebtoken decode 同）
    let claims: JwtClaims = serde_json::from_slice(&payload_bytes).map_err(|_| JwtError::InvalidToken)?;

    // required_spec_claims = {"exp"}：缺 exp → MissingRequiredClaim → 映射 VerifyError
    // （exp 类型错误在 jsonwebtoken 为 TryParse::None → 同样 MissingRequiredClaim；
    //   插件侧 serde 整结构反序列化失败 → InvalidToken —— 均拒绝，类别差异仅此
    //   边缘场景，注释留档）。exp 存在性已由 JwtClaims 反序列化保证（exp: u64 非 Option）

    // exp 校验：`exp < now - leeway` 视为过期（jsonwebtoken 9.3.1 validation.rs:
    //   `exp - reject_tokens_expiring_in_less_than < now - leeway` → ExpiredSignature）
    if claims.exp < now_secs.saturating_sub(VERIFY_LEEWAY_SECS) {
        return Err(JwtError::TokenExpired);
    }

    Ok(claims)
}

/// 多密钥验签（轮换宽限期：ADR 0033 D4）
///
/// `keys` 由密钥环给出，**有序**（当前代优先）——轮换后新签发 token 用当前代密钥，
/// 宽限期内旧 token 仍能命中上一代密钥。全部密钥都不匹配 → `InvalidSignature`。
/// 空密钥集 → `VerifyError`（配置错误，与「签名不对」区分开）。
pub fn verify_with_keys(keys: &[&[u8]], token: &str, now_secs: u64) -> Result<JwtClaims, JwtError> {
    if keys.is_empty() {
        return Err(JwtError::VerifyError("no verification key available".to_string()));
    }
    // 先做与密钥无关的解析（结构 / alg）——一次，失败即可短路，不必每把密钥重试
    // 完整路径（避免把「结构错」报成「签名错」）
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidToken);
    }
    let header_bytes = b64url_decode(parts[0]).ok_or(JwtError::InvalidToken)?;
    let header: serde_json::Value =
        serde_json::from_slice(&header_bytes).map_err(|_| JwtError::InvalidToken)?;
    if header.get("alg").and_then(|v| v.as_str()) != Some("HS256") {
        return Err(JwtError::VerifyError("AlgorithmMismatch".to_string()));
    }
    // 逐把密钥尝试签名（恒定时间比较，命中即返回）
    let mut last = JwtError::InvalidSignature;
    for key in keys {
        match verify_with_key(key, token, now_secs) {
            Ok(claims) => return Ok(claims),
            Err(JwtError::TokenExpired) => {
                // 结构与签名都通过了（否则不会走到 claims 解析之后的 exp 判定），
                // 换下一把密钥也一样过期 → 直接返回
                return Err(JwtError::TokenExpired);
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// 当前 unix 秒（wasip3 下经 wasi:clocks；native 测试走 std）
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// JWT 错误类型 — 枚举与宿主 `utils/auth/jwt.rs::JwtError` 对齐
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum JwtError {
    /// Token 过期
    TokenExpired,
    /// 无效的 token（结构/解码失败）
    InvalidToken,
    /// 签名无效
    InvalidSignature,
    /// 编码错误
    EncodeError(String),
    /// 验证错误（算法不匹配 / 缺必填 claim 等）
    VerifyError(String),
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtError::TokenExpired => write!(f, "Token expired"),
            JwtError::InvalidToken => write!(f, "Invalid token"),
            JwtError::InvalidSignature => write!(f, "Invalid signature"),
            JwtError::EncodeError(e) => write!(f, "Encode error: {}", e),
            JwtError::VerifyError(e) => write!(f, "Verify error: {}", e),
        }
    }
}

impl std::error::Error for JwtError {}

/// JWT 错误 → 用户可读消息（纯函数：过期与其余错误区分提示，其余统一「Invalid
/// token」不透出内部细节）
///
/// 这是 ADR 0031/0030 的文案分界点：宿主代签时代的 `auth-grant` 「jwt verify」
/// 与本函数同款映射，迁移期**逐字不变**（避免用户看到同一失败两种文案）。
pub fn jwt_error_message(e: &JwtError) -> &'static str {
    match e {
        JwtError::TokenExpired => "Token expired",
        _ => "Invalid token",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR 0033 之前的 wire 格式冻结向量（无 `kid`）
    const LEGACY_WIRE_VECTOR: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkZXZpY2UtMSIsImlzcyI6IkJlZENvZGUiLCJpYXQiOjE3MDAwMDAwMDAsImV4cCI6MTcwMDYwNDgwMCwiZGV2aWNlX25hbWUiOiJNeSBQaG9uZSIsImZpbmdlcnByaW50IjoiZnAtYWJjIn0.F_jY264ZZ74_BzyVaZBPPPF9H-4K-DYEVJj_bdTLgX8";

    /// 切出 header 段与 (payload 段, 其余)
    fn split_head(token: &str) -> (&str, (&str, &str)) {
        let mut it = token.splitn(3, '.');
        let head = it.next().expect("head");
        let payload = it.next().expect("payload");
        let rest = it.next().expect("signature");
        (head, (payload, rest))
    }

    /// 固定对照密钥 A：RFC 7515 §A.1 官方 test vector 密钥（64 字节，即
    /// `AyM1SysPpbyDfgZld3umj1qzKObwVMkoqQ-EstJQLr_T-1qS0gZH75aKtMN3Yj0iPS4hcgUuTwjAzZr1Z9CAow`
    /// base64url 解码结果）
    fn rfc7515_key() -> Vec<u8> {
        let hex = concat!(
            "0323354b2b0fa5bc837e0665777ba68f",
            "5ab328e6f054c928a90f84b2d2502ebf",
            "d3fb5a92d20647ef968ab4c377623d22",
            "3d2e2172052e4f08c0cd9af567d080a3"
        );
        hex::decode(hex).expect("rfc key hex")
    }

    /// 固定对照密钥 B：32 字节 0x00..=0x1f（HS256 最小安全长度，结构等价向量用）
    fn fixed_key_b() -> Vec<u8> {
        (0u8..=0x1f).collect()
    }

    // ==================== RFC 7515 §A.1 官方 test vector ====================

    /// HS256 官方 test vector（RFC 7515 §A.1.1）：给定 key/header/payload，
    /// 签名必须等于官方输出。注意 RFC 原文 header/payload 含 `\r\n` 换行与
    /// 缩进（为展示方便）；signing input 必须逐字节复刻 RFC 值。
    /// 这是「宿主 jsonwebtoken 与插件自实现等价」的密码学锚点
    /// （jsonwebtoken 的 HMAC 实现与 RFC 向量一致）。
    #[test]
    fn rfc7515_a1_hs256_official_vector() {
        // RFC 7515 §A.1 原文 base64url（header: {"typ":"JWT",\r\n "alg":"HS256"}；
        // payload: {"iss":"joe",\r\n "exp":1300819380,\r\n "http://example.com/is_root":true}）
        let signing_input = concat!(
            "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9",
            ".",
            "eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ",
        );

        // 官方签名值（RFC 7515 §A.1.1，经独立 Python hmac 实现交叉验证）
        assert_eq!(
            sign_hs256(&rfc7515_key(), signing_input),
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
            "HS256 签名必须等于 RFC 7515 A.1 官方输出"
        );
    }

    // ==================== wire 格式冻结向量（ADR 0033） ====================

    /// **wire 格式冻结向量**：固定 key + 固定 claims（无 `kid`，注入 iat/exp）→ 固定
    /// token 串。这是 ADR 0033 迁移期的**兼容性锚点**：该常量逐字节等于 ADR 0033
    /// 之前宿主 `jsonwebtoken` 与本实现共同产出的 token，断言它是为了保证
    /// 「`kid` 是纯追加的可选字段」——一旦有人调整既有字段的顺序 / 名称 / header
    /// 序列化，本用例立即转红（存量 wire 格式破了）。
    #[test]
    fn legacy_wire_format_is_byte_frozen() {
        let svc = JwtService::with_key(fixed_key_b());
        let claims = JwtClaims::new_at(
            "device-1".to_string(),
            Some("My Phone".to_string()),
            Some("fp-abc".to_string()),
            DEFAULT_TOKEN_EXPIRY_SECS,
            1700000000,
        );
        assert_eq!(claims.kid, None, "无 kid 形态的 claims 不带该字段");
        let token = svc.encode(&claims).expect("encode");
        assert_eq!(token, LEGACY_WIRE_VECTOR);
    }

    /// 反例：带 `kid` 的 token **只**多出一个尾字段，既有段与其余 claims 不变
    /// （证明 `kid` 是纯追加，不是格式变更）
    #[test]
    fn kid_is_pure_additive_suffix() {
        let svc = JwtService::with_kid(fixed_key_b(), "g7".to_string());
        let with_kid = svc
            .generate_token_at("device-1".to_string(), None, None, 1700000000)
            .expect("encode");
        let without = JwtService::with_key(fixed_key_b())
            .generate_token_at("device-1".to_string(), None, None, 1700000000)
            .expect("encode");
        let (head_a, (payload_a_b64, _)) = split_head(&with_kid);
        let (head_b, (payload_b_b64, _)) = split_head(&without);
        assert_eq!(head_a, head_b, "header 段必须逐字不变");
        // payload 段：带 kid 者恰好多一个 "kid" 键
        let payload_a =
            String::from_utf8(b64url_decode(payload_a_b64).expect("payload b64url"))
                .expect("payload utf8");
        let payload_b =
            String::from_utf8(b64url_decode(payload_b_b64).expect("payload b64url"))
                .expect("payload utf8");
        assert!(payload_a.starts_with(&payload_b[..payload_b.len() - 1]), "kid 只追加在末尾: {payload_a}");
        assert!(payload_a.ends_with(r#""kid":"g7"}"#), "kid 必须是最后一个字段: {payload_a}");
        // 反序列化往返保留 kid
        let decoded = svc
            .verify_token_at(&with_kid, 1700000000)
            .expect("verify with kid");
        assert_eq!(decoded.kid.as_deref(), Some("g7"));
    }

    // ==================== 验签矩阵（正例 / 反例 / 边界） ====================

    /// 往返：自签自验（无 `kid` 与带 `kid` 两种形态都成立）
    #[test]
    fn sign_verify_roundtrip_both_kid_forms() {
        for svc in [
            JwtService::with_key(fixed_key_b()),
            JwtService::with_kid(fixed_key_b(), "g3".to_string()),
        ] {
            let token = svc
                .generate_token("device-1".to_string(), Some("Pixel".to_string()), Some("fp".to_string()))
                .expect("issue");
            let claims = svc.verify_token_with_expiry(&token).expect("verify");
            assert_eq!(claims.sub, "device-1");
            assert_eq!(claims.iss, JWT_ISSUER);
            assert_eq!(claims.device_name.as_deref(), Some("Pixel"));
            assert_eq!(claims.kid, svc.kid.clone());
        }
    }

    /// 反例：错误密钥 → `InvalidSignature`（不泄露是哪一步失败）
    #[test]
    fn wrong_key_is_invalid_signature() {
        let token = JwtService::with_key(fixed_key_b())
            .generate_token("device-1".to_string(), None, None)
            .expect("issue");
        let other = JwtService::with_key(vec![0x42u8; 32]);
        assert_eq!(
            other.verify_token_with_expiry(&token).unwrap_err(),
            JwtError::InvalidSignature
        );
    }

    /// 反例：过期 → `TokenExpired`（两把密钥都过期时不得退化成「签名错」）
    #[test]
    fn expired_token_reports_expiry_not_signature() {
        // 过期窗口 0 + 签发时间往前推 > leeway，确保落在 leeway 之外
        let svc = JwtService::with_key_and_expiry(fixed_key_b(), 0);
        let token = svc
            .generate_token_at(
                "device-1".to_string(),
                None,
                None,
                now_secs() - VERIFY_LEEWAY_SECS - 100,
            )
            .expect("issue");
        let err = svc.verify_token_with_expiry(&token).unwrap_err();
        assert_eq!(err, JwtError::TokenExpired, "过期必须是过期（文案可区分）");
        // 多密钥路径同样短路为「过期」而不是尝试完全部密钥后报签名错
        let signing = fixed_key_b();
        let other = vec![0x42u8; 32];
        let keys: Vec<&[u8]> = vec![&signing[..], &other[..]];
        assert_eq!(
            verify_with_keys(&keys, &token, now_secs()).unwrap_err(),
            JwtError::TokenExpired
        );
    }

    /// 反例：结构畸形（非三段 / 非 base64url / alg 非 HS256 / claims 非 JSON）
    /// → `InvalidToken` / `VerifyError`，**不得**误报为签名错
    #[test]
    fn malformed_tokens_are_not_signature_errors() {
        let key = fixed_key_b();
        let keys: Vec<&[u8]> = vec![&key[..]];
        for bad in [
            "one.segment",
            "",
            "a.b.c",
            &format!("x.{}.y", b64url_encode(b"not-json")),
            // alg 改成 HS512（signature 段保持合法 base64url）
            &format!(
                "{}.{}.{}",
                b64url_encode(br#"{"typ":"JWT","alg":"HS512"}"#),
                b64url_encode(br#"{"sub":"d","iss":"BedCode","iat":1,"exp":9999999999}"#),
                b64url_encode(&[0u8; 32])
            ),
        ] {
            let err = verify_with_keys(&keys, bad, now_secs()).expect_err("畸形必须拒绝");
            assert!(
                matches!(
                    err,
                    JwtError::InvalidToken | JwtError::VerifyError(_)
                ),
                "结构类错误不得报成签名错: {bad} -> {err:?}"
            );
        }
    }

    /// 边界：空密钥集 = 配置错误（与「签名不对」区分，不静默当通过）
    #[test]
    fn empty_key_set_is_a_configuration_error() {
        let err = verify_with_keys(&[], "a.b.c", now_secs()).expect_err("空密钥集");
        assert!(
            matches!(err, JwtError::VerifyError(ref m) if m.contains("no verification key")),
            "got: {err:?}"
        );
    }

    // ==================== 轮换跨代验签（ADR 0033 D4） ====================

    /// 跨代：轮换后**新旧 token 都能验签**（宽限期内上一代仍是合法验签密钥），
    /// 超出宽限期的更早代（已不在候选集）→ 拒。
    #[test]
    fn rotation_grace_window_accepts_previous_generation_only() {
        let gen1 = JwtService::with_kid(fixed_key_b(), "g1".to_string());
        let gen2_key = vec![0x42u8; 32];
        let gen2 = JwtService::with_kid(gen2_key.clone(), "g2".to_string());
        let gen3_key = vec![0x24u8; 32];
        let gen3 = JwtService::with_kid(gen3_key.clone(), "g3".to_string());

        let t1 = gen1.generate_token("d1".to_string(), None, None).expect("g1 token");
        let t2 = gen2.generate_token("d1".to_string(), None, None).expect("g2 token");
        let t3 = gen3.generate_token("d1".to_string(), None, None).expect("g3 token");

        // 轮换两次后候选 = [g3, g2]（g1 已被裁掉）
        let keys: Vec<&[u8]> = vec![&gen3_key[..], &gen2_key[..]];
        assert_eq!(keys.len(), 2);
        assert!(verify_with_keys(&keys, &t3, now_secs()).is_ok(), "当前代必须可验");
        assert!(verify_with_keys(&keys, &t2, now_secs()).is_ok(), "上一代必须可验（宽限期）");
        assert_eq!(
            verify_with_keys(&keys, &t1, now_secs()).unwrap_err(),
            JwtError::InvalidSignature,
            "超出宽限期的最早一代必须拒绝"
        );
    }

    /// 跨代：**无 `kid`** 的旧 token（迁移前形态）也走同一候选集——否则 D1 一上线
    /// 存量 token 会在「有密钥但认不出 kid」时被误拒
    #[test]
    fn legacy_token_without_kid_still_verifies_against_previous_key() {
        let legacy = JwtService::with_key(vec![0x42u8; 32])
            .generate_token("d1".to_string(), None, None)
            .expect("legacy token");
        let err = verify_with_keys(&[], &legacy, now_secs()).expect_err("空密钥集");
        assert!(
            matches!(&err, JwtError::VerifyError(m) if m.contains("no verification key")),
            "got: {err:?}"
        );
        // 新一代密钥 + 上一代（旧签名）同时在候选里
        let keys: Vec<&[u8]> = vec![&[0x24u8; 32][..], &[0x42u8; 32][..]];
        let claims = verify_with_keys(&keys, &legacy, now_secs()).expect("无 kid 旧 token 可验");
        assert_eq!(claims.kid, None, "旧 token 的 kid 保持缺省");
        assert_eq!(claims.sub, "d1");
    }
}
