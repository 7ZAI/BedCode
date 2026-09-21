//! JWT Authentication Service
//!
//! 提供 JWT token 生成和验证功能

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::system::constants::auth::JWT_ISSUER;
use crate::utils::auth::host_secrets;

/// 默认 JWT 过期时间（7 天）
pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT 密钥在 secret-store 中的键名（宿主命名空间，见 host_secrets 模块）
pub const JWT_SECRET_KEY_ID: &str = "jwt.key";

/// JWT 密钥长度（字节）。HS256 要求 ≥32 字节（256 bit）
pub const JWT_SECRET_KEY_LEN: usize = 32;

/// JWT 算法
const JWT_ALGORITHM: Algorithm = Algorithm::HS256;

/// 解析 JWT 密钥：secret-store 托管（首启随机生成 + 持久化，重启稳定）；
/// 未托管环境（单测 / 主库不可用）回退进程内随机密钥。
///
/// 进程内只解析一次（OnceLock）：同一进程所有 JwtService 实例必须共享同一
/// 密钥，否则签发/验签跨实例即失效；回退密钥进程内同样稳定（重启后失效，
/// 仅在非生产路径命中，生产在 lib.rs setup 预生成并落库）。
fn resolve_secret() -> Vec<u8> {
    static JWT_KEY: OnceLock<Vec<u8>> = OnceLock::new();
    JWT_KEY
        .get_or_init(|| match host_secrets::get_or_generate(JWT_SECRET_KEY_ID, JWT_SECRET_KEY_LEN) {
            Ok(v) => v,
            Err(e) => {
                // 明文不落日志：只记错误原因，不记密钥
                //
                // 级别 = warn（AGENTS.md §8：可恢复降级）：secret-store 未注入是
                // 无宿主上下文（单测 / headless）的**设计内**路径，见
                // `host_secrets` 模块文档——生产在 lib.rs setup 主库就绪后 init 并
                // 预生成，本分支不影响功能，用 error 会把测试上下文的常态噪音
                // 记成故障（broadcast_shutdown 的「停机零 error」门禁即被此误伤）。
                tracing::warn!(
                    error = %e,
                    key_len = JWT_SECRET_KEY_LEN,
                    "jwt: secret store unavailable, falling back to process-random key (tokens invalidate on restart)"
                );
                let mut buf = vec![0u8; JWT_SECRET_KEY_LEN];
                OsRng.fill_bytes(&mut buf);
                buf
            }
        })
        .clone()
}

/// JWT Claims 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// 主题（设备 ID）
    pub sub: String,
    /// 签发者
    pub iss: String,
    /// 签发时间
    pub iat: u64,
    /// 过期时间
    pub exp: u64,
    /// 设备名称（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 设备指纹（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl JwtClaims {
    /// 创建新的 claims
    pub fn new(
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
        expires_in_secs: u64,
    ) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        Self {
            sub: subject,
            iss: JWT_ISSUER.to_string(),
            iat: now,
            exp: now + expires_in_secs,
            device_name,
            fingerprint,
        }
    }

    /// 检查 token 是否过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.exp < now
    }

    /// 剩余有效时间（秒）
    pub fn remaining_secs(&self) -> u64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if self.exp > now {
            self.exp - now
        } else {
            0
        }
    }
}

/// JWT 服务
pub struct JwtService {
    /// 密钥
    decoding_key: DecodingKey,
    encoding_key: EncodingKey,
    /// 默认过期时间
    default_expiry_secs: u64,
}

impl JwtService {
    /// 创建新的 JWT 服务
    ///
    /// 密钥来自 secret-store（host_secrets 模块）：首启随机生成 + 持久化，
    /// 重启后稳定；进程内只解析一次，所有实例共享同一密钥。
    pub fn new() -> Self {
        let secret = resolve_secret();
        Self {
            decoding_key: DecodingKey::from_secret(&secret),
            encoding_key: EncodingKey::from_secret(&secret),
            default_expiry_secs: DEFAULT_TOKEN_EXPIRY_SECS,
        }
    }

    /// 创建带有自定义过期时间的 JWT 服务
    pub fn with_expiry(expiry_secs: u64) -> Self {
        Self::new().set_default_expiry(expiry_secs)
    }

    /// 设置默认过期时间
    pub fn set_default_expiry(mut self, secs: u64) -> Self {
        self.default_expiry_secs = secs;
        self
    }

    /// 生成 JWT token
    pub fn generate_token(
        &self,
        subject: String,
        device_name: Option<String>,
        fingerprint: Option<String>,
    ) -> Result<String, JwtError> {
        let claims = JwtClaims::new(subject, device_name, fingerprint, self.default_expiry_secs);

        let token = encode(&Header::new(JWT_ALGORITHM), &claims, &self.encoding_key)
            .map_err(|e| JwtError::EncodeError(e.to_string()))?;

        Ok(token)
    }

    /// 验证并解码 JWT token
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let validation = Validation::new(JWT_ALGORITHM);
        let token_data = decode::<JwtClaims>(token, &self.decoding_key, &validation).map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidToken => JwtError::InvalidToken,
            jsonwebtoken::errors::ErrorKind::InvalidSignature => JwtError::InvalidSignature,
            _ => JwtError::VerifyError(e.to_string()),
        })?;

        Ok(token_data.claims)
    }

    /// 验证 token 并检查是否过期
    pub fn verify_token_with_expiry(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let claims = self.verify_token(token)?;

        if claims.is_expired() {
            return Err(JwtError::TokenExpired);
        }

        Ok(claims)
    }
}

impl Default for JwtService {
    fn default() -> Self {
        Self::new()
    }
}

/// JWT 错误类型
#[derive(Debug)]
pub enum JwtError {
    /// Token 过期
    TokenExpired,
    /// 无效的 token
    InvalidToken,
    /// 签名无效
    InvalidSignature,
    /// 编码错误
    EncodeError(String),
    /// 验证错误
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

/// JWT 错误 → 用户可读消息（纯函数，供测试；WS 认证与 HTTP 重认证共用，防文案漂移）
///
/// 过期与其余错误区分提示；其余错误统一「Invalid token」不透出内部细节
pub fn jwt_error_message(e: &JwtError) -> &'static str {
    match e {
        JwtError::TokenExpired => "Token expired",
        _ => "Invalid token",
    }
}

/// 生成设备认证 JWT 的便捷函数
pub fn generate_device_token(
    device_id: String,
    device_name: Option<String>,
    fingerprint: Option<String>,
) -> Result<String, JwtError> {
    let service = JwtService::new();
    service.generate_token(device_id, device_name, fingerprint)
}

/// 验证设备 JWT token 的便捷函数
pub fn verify_device_token(token: &str) -> Result<JwtClaims, JwtError> {
    let service = JwtService::new();
    service.verify_token_with_expiry(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_verify_token() {
        let service = JwtService::new();

        let token = service
            .generate_token(
                "device-123".to_string(),
                Some("My Phone".to_string()),
                Some("fingerprint-abc".to_string()),
            )
            .unwrap();

        let claims = service.verify_token(&token).unwrap();

        assert_eq!(claims.sub, "device-123");
        assert_eq!(claims.device_name, Some("My Phone".to_string()));
        assert_eq!(claims.fingerprint, Some("fingerprint-abc".to_string()));
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_token_expiry() {
        let service = JwtService::with_expiry(1); // 1 second expiry

        let token = service.generate_token("device-123".to_string(), None, None).unwrap();

        // exp 以秒级截断（exp = 签发秒 + 1），1500ms 睡眠可能未跨秒导致 flaky；
        // 睡 2.1s 确保越过 exp 边界（`exp < now` 需 now ≥ exp + 1s）
        std::thread::sleep(std::time::Duration::from_millis(2100));

        // 用严格过期检查 API：verify_token 走 jsonwebtoken 默认 Validation（leeway=60s），
        // 过期 60 秒内的 token 仍会通过，无法表达本测试的意图
        let result = service.verify_token_with_expiry(&token);
        assert!(matches!(result, Err(JwtError::TokenExpired)));
    }

    #[test]
    fn jwt_error_message_distinguishes_expiry_only() {
        assert_eq!(jwt_error_message(&JwtError::TokenExpired), "Token expired");
        // 其余错误统一不区分（不透出内部细节）
        assert_eq!(jwt_error_message(&JwtError::InvalidToken), "Invalid token");
        assert_eq!(jwt_error_message(&JwtError::InvalidSignature), "Invalid token");
        assert_eq!(
            jwt_error_message(&JwtError::EncodeError("x".to_string())),
            "Invalid token"
        );
        assert_eq!(
            jwt_error_message(&JwtError::VerifyError("x".to_string())),
            "Invalid token"
        );
    }

    /// 进程内所有 JwtService 实例共享同一密钥：A 签发 → B 验签必须通过
    /// （OnceLock 缓存；若逐实例随机密钥，token 立即可验失败）
    #[test]
    fn all_service_instances_share_same_key() {
        let token = JwtService::new()
            .generate_token("device-shared".to_string(), None, None)
            .unwrap();
        let claims = JwtService::new().verify_token_with_expiry(&token).unwrap();
        assert_eq!(claims.sub, "device-shared");
    }

    /// 便捷函数（generate_device_token / verify_device_token）与直接服务
    /// 构造走同一密钥源（票 05 密钥治理：改读 secret-store 后无行为回退）
    #[test]
    fn convenience_fns_share_key_with_service() {
        let token = generate_device_token("device-conv".to_string(), None, None).unwrap();
        let claims = verify_device_token(&token).unwrap();
        assert_eq!(claims.sub, "device-conv");
        // 与 JwtService 直构实例交叉验证
        let claims2 = JwtService::new().verify_token_with_expiry(&token).unwrap();
        assert_eq!(claims2.sub, "device-conv");
    }

    /// 对照测试（票 07 pairing，票 06 改指会话中心）：宿主 jsonwebtoken 9.3.1 与
    /// 会话中心插件 pairing 域 HS256 自实现「同一输入同输出」。固定 key（32B 0x00..=0x1f）+ 固定 claims
    /// （注入 iat/exp）→ 期望 token 与插件侧
    /// `plugins/terminal-session/rust/src/pairing/jwt.rs::plugin_token_matches_host_jsonwebtoken_vector`
    /// 断言的是**同一常量**（签名段为插件实现产出；算法锚点 = RFC 7515 §A.1 官方向量）。
    /// 任一侧实现漂移（header 字段顺序 / claims 序列化顺序 / HMAC）双端立即红。
    #[test]
    fn host_jsonwebtoken_matches_plugin_fixed_vector() {
        let key: Vec<u8> = (0u8..=0x1f).collect();
        let claims = JwtClaims {
            sub: "device-1".to_string(),
            iss: "BedCode".to_string(),
            iat: 1700000000,
            exp: 1700604800,
            device_name: Some("My Phone".to_string()),
            fingerprint: Some("fp-abc".to_string()),
        };
        // 插件侧断言同一 token 串（注意：Header::new(HS256) 序列化为
        // {"typ":"JWT","alg":"HS256"}，插件侧常量 HS256_HEADER_JSON 逐字节一致）
        let expected = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJkZXZpY2UtMSIsImlzcyI6IkJlZENvZGUiLCJpYXQiOjE3MDAwMDAwMDAsImV4cCI6MTcwMDYwNDgwMCwiZGV2aWNlX25hbWUiOiJNeSBQaG9uZSIsImZpbmdlcnByaW50IjoiZnAtYWJjIn0.F_jY264ZZ74_BzyVaZBPPPF9H-4K-DYEVJj_bdTLgX8";

        let token = encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(&key))
            .expect("jsonwebtoken encode");
        assert_eq!(token, expected, "宿主 jsonwebtoken 与插件 HS256 实现必须产出同一 token");

        // 反向验签插件 token（固定 key）：签名有效 + claims 一致
        // （固定向量 exp 已过（1700604800 < now），只验签名与结构，关闭 exp 校验）
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        let data = decode::<JwtClaims>(expected, &DecodingKey::from_secret(&key), &validation)
            .expect("jsonwebtoken 必须能验签插件签发的 token");
        assert_eq!(data.claims.sub, "device-1");
        assert_eq!(data.claims.iss, "BedCode");
        assert_eq!(data.claims.device_name, Some("My Phone".to_string()));
        assert_eq!(data.claims.fingerprint, Some("fp-abc".to_string()));
    }
}
