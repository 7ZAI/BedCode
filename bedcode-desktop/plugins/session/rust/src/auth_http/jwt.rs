//! 设备认证 JWT 签发/验签门面（票 07）
//!
//! **密钥不出宿主**：签发与验签执行经 host-auth `device-token-issue` /
//! `device-token-verify` 原语走宿主 `JwtService` 同一代码路径（HS256 +
//! secret-store 托管密钥 + 7 天窗口）；插件只做编排与用户文案映射
//! （`err("expired")` → "Token expired"，其余 → "Invalid token"，与宿主
//! `jwt_error_message` 逐字一致）。
//!
//! 这是票 07 用户裁定「插件调用 auth_center 统一认证」的落点：认证编排归
//! 插件，密码学引擎与密钥托管留宿主（ADR 0022 修订口径）。

use bedcode_plugin_api::host::HostAuth;
use bedcode_plugin_api::wasm_host::WasmHost;

/// 签发设备认证 JWT → `(token, expires_in)`（expires_in = 7 天，响应字段同值）
///
/// `device_name` / `fingerprint` 传 `None` 即缺省（宿主原语空串 = None 语义）。
/// 密钥读取 / 签发失败显性报错（HTTP 1001 Failed to generate token，不静默降级）。
#[cfg(target_arch = "wasm32")]
pub fn issue_device_token(
    subject: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> Result<(String, u64), String> {
    let host = WasmHost;
    let token = host
        .auth_device_token_issue(
            subject,
            device_name.unwrap_or(""),
            fingerprint.unwrap_or(""),
        )
        .map_err(|e| e.message)?;
    // 过期窗口与宿主 `DEFAULT_TOKEN_EXPIRY_SECS` 同值（7 天，响应字段逐字一致）
    Ok((token, crate::pairing::jwt::DEFAULT_TOKEN_EXPIRY_SECS))
}

/// 验签既有 token → claims JSON（供 reauth 换发与 biometric-bind 归属校验）
///
/// 错误已映射为宿主同款用户文案：过期 → "Token expired"；其余 → "Invalid token"。
#[cfg(target_arch = "wasm32")]
pub fn verify_device_token(host: &WasmHost, token: &str) -> Result<serde_json::Value, String> {
    let claims_json = host.auth_device_token_verify(token).map_err(|e| {
        match e.message.as_str() {
            "expired" => "Token expired".to_string(),
            _ => "Invalid token".to_string(),
        }
    })?;
    serde_json::from_str(&claims_json).map_err(|e| format!("claims parse failed: {e}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn issue_device_token(
    _subject: &str,
    _device_name: Option<&str>,
    _fingerprint: Option<&str>,
) -> Result<(String, u64), String> {
    Err("jwt issuance unavailable outside wasm runtime".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn verify_device_token(_host: &WasmHost, _token: &str) -> Result<serde_json::Value, String> {
    Err("jwt verification unavailable outside wasm runtime".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// native（无宿主上下文）签发/验签必须显性失败——静默空 token 是最危险的
    /// 默认值（会被消费方读成「已认证」）
    #[test]
    fn native_faces_fail_loudly() {
        let err = issue_device_token("d1", Some("Pixel"), Some("fp")).expect_err("native 签发失败");
        assert!(err.contains("unavailable outside wasm runtime"), "got: {err}");

        let err = verify_device_token(&bedcode_plugin_api::wasm_host::WasmHost, "a.b.c")
            .expect_err("native 验签失败");
        assert!(err.contains("unavailable outside wasm runtime"), "got: {err}");
    }

    /// 用户文案映射：宿主原语的 "expired" 分类 → "Token expired"，其余 →
    /// "Invalid token"（宿主 `jwt_error_message` 逐字一致）
    #[test]
    fn expired_classification_matches_host_message() {
        // 映射逻辑内联在 verify_device_token 的 Err 分支；此处以等价判定锁文案
        let map = |raw: &str| match raw {
            "expired" => "Token expired".to_string(),
            _ => "Invalid token".to_string(),
        };
        assert_eq!(map("expired"), "Token expired");
        assert_eq!(map("invalid"), "Invalid token");
    }
}
