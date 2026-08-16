//! 认证相关常量（JWT / 配对 / QR Token）

/// JWT 签发者标识
pub const JWT_ISSUER: &str = "BedCode";

/// 配对码位数
pub const PAIRING_CODE_DIGITS: usize = 6;

/// 配对码有效期（秒）
pub const PAIRING_CODE_TTL_SECS: u64 = 60;

/// QR Token 随机字节数（128-bit = 32 hex 字符）
pub const QR_TOKEN_BYTES: usize = 16;
