//! JWT 签发/校验策略（HS256，认证中心 pairing 模块 · 票 07）
//!
//! 语义从宿主 `src-tauri/src/utils/auth/jwt.rs`（jsonwebtoken 9.3.1）平移，
//! 宿主保留密码学引擎与密钥托管（spec §3：密钥托管 secret-store 明文不出宿主；
//! 验签执行点仍留宿主中间件，本模块供认证中心自身语义决策与对照测试）。
//!
//! HS256 自实现（jsonwebtoken 因 ring 不可 wasm 编译，spec §3）：base64url(header)
//! + "." + base64url(payload)，HMAC-SHA256(key, signing_input) 签名。与宿主的
//! 行为等价约束（对照测试「同一输入同输出」）：
//!
//! - header JSON 序列化顺序与 jsonwebtoken 9.3.1 `Header::new(HS256)` 一致：
//!   `{"typ":"JWT","alg":"HS256"}`（typ 在前，其余可选字段 skip）
//! - claims 字段声明顺序与宿主 `JwtClaims` 一致：sub / iss / iat / exp /
//!   device_name(skip none) / fingerprint(skip none)
//! - verify 复刻 jsonwebtoken `Validation::new(HS256)` 默认语义：恰好 3 段、
//!   base64url 解码、alg 必须 HS256、恒定时间验签、`exp` 必填、
//!   `exp < now - leeway(60)` 视为过期（leeway 语义）
//! - 错误映射对齐宿主 `JwtError`：ExpiredSignature → TokenExpired、
//!   InvalidToken / InvalidSignature 直映、其余（AlgorithmMismatch /
//!   MissingRequiredClaim 等）→ VerifyError

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

/// 默认 JWT 过期时间（7 天）— 与宿主 `DEFAULT_TOKEN_EXPIRY_SECS` 对齐
pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT 签发者标识 — 与宿主 `system::constants::auth::JWT_ISSUER` 对齐
pub const JWT_ISSUER: &str = "BedCode";

/// JWT 密钥长度（字节）。HS256 要求 ≥32 字节（256 bit）— 与宿主 `JWT_SECRET_KEY_LEN` 对齐
pub const JWT_SECRET_KEY_LEN: usize = 32;

/// 密钥在 host-auth secret-store 中的键名（认证中心插件域）
pub const JWT_SECRET_KEY_ID: &str = "jwt.key";

/// 验签 leeway（秒）— 复刻 jsonwebtoken `Validation::new` 默认值 60
const VERIFY_LEEWAY_SECS: u64 = 60;

/// header JSON（固定，无 kid/cty 等可选字段）— jsonwebtoken 9.3.1
/// `Header::new(Algorithm::HS256)` 序列化结果，顺序 typ → alg
const HS256_HEADER_JSON: &str = r#"{"typ":"JWT","alg":"HS256"}"#;

type HmacSha256 = Hmac<Sha256>;

/// base64url 编码（无 padding）— jsonwebtoken 的 `URL_SAFE_NO_PAD`
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

/// JWT Claims 结构 — 字段声明顺序与宿主 `utils/auth/jwt.rs::JwtClaims` 严格一致
/// （serde 按声明顺序序列化，是「同一输入同输出」的格式锚点）
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
}

impl JwtClaims {
    /// 创建 claims（expires_in 相对当前时间）— 语义同宿主 `JwtClaims::new`。
    /// 命令面/对照测试走 `new_at`（注入时间戳）；本方法保留为语义完整 API
    #[allow(dead_code)]
    pub fn new(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
    ) -> Self {
        Self::new_at(subject, device_name, fingerprint, expires_in_secs, now_secs())
    }

    /// 创建 claims 并注入当前时间戳（对照测试用：宿主/插件同一输入同一输出）
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
        }
    }

    /// 检查是否过期（`exp < now`）— 语义同宿主 `is_expired`
    pub fn is_expired_at(&self, now_secs: u64) -> bool {
        self.exp < now_secs
    }

    /// 剩余有效时间（秒），过期钳制为 0 — 语义同宿主 `remaining_secs`。
    /// 命令面走 `remaining_secs_at`；本方法保留为语义完整 API
    #[allow(dead_code)]
    pub fn remaining_secs_at(&self, now_secs: u64) -> u64 {
        if self.exp > now_secs {
            self.exp - now_secs
        } else {
            0
        }
    }
}

/// JWT 服务（HS256）— 语义同宿主 `JwtService`（密钥注入式，宿主侧经
/// secret-store 解析；插件侧经 host-auth secret-store，见 `keys` 模块）
pub struct JwtService {
    key: Vec<u8>,
    default_expiry_secs: u64,
}

impl JwtService {
    /// 以固定密钥构造（测试注入；生产路径经 `keys::ensure_jwt_key` 取 host-auth 托管密钥）
    pub fn with_key(key: Vec<u8>) -> Self {
        Self::with_key_and_expiry(key, DEFAULT_TOKEN_EXPIRY_SECS)
    }

    /// 以固定密钥 + 自定义默认过期时间构造 — 对应宿主 `JwtService::with_expiry`
    pub fn with_key_and_expiry(key: Vec<u8>, expiry_secs: u64) -> Self {
        Self {
            key,
            default_expiry_secs: expiry_secs,
        }
    }

    /// 生成 JWT token — 语义同宿主 `generate_token`
    pub fn generate_token(
        &self,
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) -> Result<String, JwtError> {
        self.generate_token_at(subject, device_name, fingerprint, now_secs())
    }

    /// 生成 JWT token（注入当前时间戳）— 对照测试用（同一输入同输出）
    pub fn generate_token_at(
        &self,
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        now_secs: u64,
    ) -> Result<String, JwtError> {
        let claims =
            JwtClaims::new_at(subject, device_name, fingerprint, self.default_expiry_secs, now_secs);
        self.encode(&claims)
    }

    /// 对固定 claims 编码签名（对照测试的核心：两端同一 claims → 同一 token）
    pub fn encode(&self, claims: &JwtClaims) -> Result<String, JwtError> {
        let payload = serde_json::to_string(claims).map_err(|e| JwtError::EncodeError(e.to_string()))?;
        let signing_input = format!("{}.{}", b64url_encode(HS256_HEADER_JSON.as_bytes()), b64url_encode(payload.as_bytes()));
        let signature = sign_hs256(&self.key, &signing_input);
        Ok(format!("{}.{}", signing_input, signature))
    }

    /// 验证并解码 JWT token — 复刻宿主 `verify_token`（jsonwebtoken
    /// `Validation::new(HS256)` 默认语义：leeway=60，`exp` 必填）
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        self.verify_token_at(token, now_secs())
    }

    /// 验证并解码 JWT token（注入当前时间戳）— 对照测试用
    pub fn verify_token_at(&self, token: &str, now_secs: u64) -> Result<JwtClaims, JwtError> {
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
        //   → 宿主映射 VerifyError —— 插件对齐）
        let header: serde_json::Value = serde_json::from_slice(&header_bytes)
            .map_err(|_| JwtError::InvalidToken)?;
        let alg = header
            .get("alg")
            .and_then(|v| v.as_str())
            .ok_or(JwtError::InvalidToken)?;
        if alg != "HS256" {
            return Err(JwtError::VerifyError("AlgorithmMismatch".to_string()));
        }

        // 恒定时间验签（hmac Mac::verify_slice）
        let signing_input = format!("{}.{}", header_b64, payload_b64);
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC accepts any key length");
        mac.update(signing_input.as_bytes());
        mac.verify_slice(&signature_bytes)
            .map_err(|_| JwtError::InvalidSignature)?;

        // claims 反序列化失败 → InvalidToken（jsonwebtoken decode 同）
        let claims: JwtClaims = serde_json::from_slice(&payload_bytes)
            .map_err(|_| JwtError::InvalidToken)?;

        // required_spec_claims = {"exp"}：缺 exp → MissingRequiredClaim → 宿主映射 VerifyError
        // （exp 类型错误在宿主为 TryParse::None → 同样 MissingRequiredClaim；插件侧
        //   serde 整结构反序列化失败 → InvalidToken —— 均拒绝，类别差异仅此边缘场景，
        //   对照测试不覆盖，注释留档）
        // exp 存在性已由 JwtClaims 反序列化保证（exp: u64 非 Option）

        // exp 校验：`exp < now - leeway` 视为过期（jsonwebtoken 9.3.1 validation.rs:
        //   `exp - reject_tokens_expiring_in_less_than < now - leeway` → ExpiredSignature）
        if claims.exp < now_secs.saturating_sub(VERIFY_LEEWAY_SECS) {
            return Err(JwtError::TokenExpired);
        }

        Ok(claims)
    }

    /// 验证 token 并严格检查过期（`exp < now`，无 leeway）— 语义同宿主
    /// `verify_token_with_expiry`（jsonwebtoken 默认 leeway=60 对「过期 60 秒内」
    /// 的 token 放行，此 API 收紧）
    pub fn verify_token_with_expiry(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let claims = self.verify_token(token)?;
        if claims.is_expired_at(now_secs()) {
            return Err(JwtError::TokenExpired);
        }
        Ok(claims)
    }
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

/// JWT 错误 → 用户可读消息（纯函数，与宿主 `jwt_error_message` 同一契约）：
/// 过期与其余错误区分提示；其余错误统一「Invalid token」不透出内部细节
pub fn jwt_error_message(e: &JwtError) -> &'static str {
    match e {
        JwtError::TokenExpired => "Token expired",
        _ => "Invalid token",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ==================== 结构等价对照向量（宿主 jsonwebtoken 同一常量断言） ====================

    /// 结构等价向量 B：固定 key + 固定 claims（注入 iat/exp）→ 固定 token。
    /// **宿主侧对照**：`bedcode-desktop/src-tauri/src/utils/auth/jwt.rs` 测试
    /// `host_jsonwebtoken_matches_plugin_fixed_vector` 以同一 key/claims 用
    /// jsonwebtoken 9.3.1 encode，断言输出与本常量逐字符一致，并反向验签。
    #[test]
    fn plugin_token_matches_host_jsonwebtoken_vector() {
        let svc = JwtService::with_key(fixed_key_b());
        let claims = JwtClaims::new_at(
            "device-1".to_string(),
            Some("My Phone".to_string()),
            Some("fp-abc".to_string()),
            DEFAULT_TOKEN_EXPIRY_SECS,
            1700000000,
        );
        let token = svc.encode(&claims).expect("encode");
        // 期望 token 由宿主 jsonwebtoken 9.3.1 对同一输入产生（宿主对照测试
        // `host_jsonwebtoken_matches_plugin_fixed_vector` 固化同一常量并反向验签；
        // 若任一实现漂移此处立即暴露）。签名段为插件 HS256 实现产出，算法
        // 正确性由 RFC 7515 A.1 向量锁定。
        assert_eq!(token, "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkZXZpY2UtMSIsImlzcyI6IkJlZENvZGUiLCJpYXQiOjE3MDAwMDAwMDAsImV4cCI6MTcwMDYwNDgwMCwiZGV2aWNlX25hbWUiOiJNeSBQaG9uZSIsImZpbmdlcnByaW50IjoiZnAtYWJjIn0.F_jY264ZZ74_BzyVaZBPPPF9H-4K-DYEVJj_bdTLgX8");
    }
}
