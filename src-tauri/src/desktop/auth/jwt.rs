//! JWT Authentication Service
//!
//! 提供 JWT token 生成和验证功能

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// 默认 JWT 过期时间（7 天）
pub const DEFAULT_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// JWT 密钥（硬编码，请根据需要修改）
const JWT_SECRET: &[u8] = b"BedCode_Secure_JWT_Key_2024_Change_In_Production";

/// JWT 算法
const JWT_ALGORITHM: Algorithm = Algorithm::HS256;

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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            sub: subject,
            iss: "BedCode".to_string(),
            iat: now,
            exp: now + expires_in_secs,
            device_name,
            fingerprint,
        }
    }

    /// 检查 token 是否过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp < now
    }

    /// 剩余有效时间（秒）
    pub fn remaining_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
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
    pub fn new() -> Self {
        Self {
            decoding_key: DecodingKey::from_secret(JWT_SECRET),
            encoding_key: EncodingKey::from_secret(JWT_SECRET),
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
        let claims = JwtClaims::new(
            subject,
            device_name,
            fingerprint,
            self.default_expiry_secs,
        );

        let token = encode(
            &Header::new(JWT_ALGORITHM),
            &claims,
            &self.encoding_key,
        )
        .map_err(|e| JwtError::EncodeError(e.to_string()))?;

        Ok(token)
    }

    /// 验证并解码 JWT token
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let validation = Validation::new(JWT_ALGORITHM);
        let token_data = decode::<JwtClaims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    JwtError::TokenExpired
                }
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

        let token = service
            .generate_token("device-123".to_string(), None, None)
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1500));

        let result = service.verify_token(&token);
        assert!(matches!(result, Err(JwtError::TokenExpired)));
    }
}