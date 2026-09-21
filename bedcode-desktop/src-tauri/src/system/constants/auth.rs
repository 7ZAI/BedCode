//! 认证相关常量（JWT / 配对）

/// JWT 签发者标识
pub const JWT_ISSUER: &str = "BedCode";

/// 配对码有效期缺省（秒）
///
/// 真源与策略在 `com.bedcode.session` 插件；宿主这边只剩一个用途：
/// `host-auth` 记录面的 `pairing_code_ttl` 设置缺省（`host_impl/config.rs`），
/// 与插件侧 `pairing/code.rs::PAIRING_CODE_TTL_SECS` 同值对齐。
pub const PAIRING_CODE_TTL_SECS: u64 = 60;
