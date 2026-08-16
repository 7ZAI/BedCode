//! 宿主能力：HTTP 代理

use super::HostError;

/// HTTP 代理（宿主代为发起请求，插件不直接接触网络）
///
/// 需要 `network:http` 权限。请求格式：
/// ```json
/// { "method": "POST", "url": "https://...", "headers": { ... }, "body": "..." }
/// ```
/// 返回完整响应 `{ status, body, headers }`。
pub trait HostHttp {
    /// 发起 HTTP 请求
    fn http_fetch(&self, request: &serde_json::Value) -> Result<Option<serde_json::Value>, HostError>;
}
