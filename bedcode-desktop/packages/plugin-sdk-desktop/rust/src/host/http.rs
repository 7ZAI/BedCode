//! 宿主能力：HTTP 代理

use super::HostError;

/// HTTP 代理（宿主代为发起请求，插件不直接接触网络）
///
/// 需要 `network:http` 权限。请求格式：
/// ```json
/// {
///   "method": "POST",
///   "url": "https://api.example.com/v1/chat",
///   "headers": { "Authorization": "Bearer xxx" },
///   "body": "{...}",
///   "stream": true,
///   "streamEvent": "my-plugin:stream:xxx"
/// }
/// ```
/// 流式模式立即返回 `{ streamId, streamEvent }`，数据通过 `streamEvent`
/// Tauri 事件逐 chunk 推送；非流式返回完整响应 `{ status, body, headers }`。
///
/// 流式 `sseFormat`（票据 02 宿主零业务语义）：空串 = raw 模式（逐网络 chunk 透传
/// 原始字节，消费侧自行切分）；非空 = 通用 SSE 模式（宿主按事件分隔符切分、透传
/// `data:` 行原文）。OpenAI/Anthropic 等供应商格式**不在宿主解析**，由插件消费侧
/// 自行按格式解析（如 ai-chatbox 前端 adapters 按 apiStyle 分派）。
pub trait HostHttp {
    /// 发起 HTTP 请求
    fn http_fetch(&self, request: &serde_json::Value) -> Result<Option<serde_json::Value>, HostError>;

    // ==================== 服务端域（ABI v29：动态路由注册） ====================

    /// 注册插件 HTTP 端点（WIT `host-http.register-endpoint`，ABI v29）。
    ///
    /// config-json（camelCase）：
    /// `{ path, host?, methods?, auth? }`
    /// - `path`：插件内相对端点段 → 宿主拼出内部可达路径 `/api/plugin/<plugin-id>/<path>`
    ///   （命名空间段由宿主注入）；
    /// - `host`：可选对外 URL 别名（支持 `{id}` 模板段）；
    /// - `methods`：host 别名的允许方法（缺省 `["GET"]`）；
    /// - `auth` = `"jwt"`（缺省，最严：要求宿主已验签）| `"none"`（免凭证转发）。
    ///
    /// 成功 → 返回端点句柄 `http-<uuid>`；`host+method` 被其他插件占用 / 本插件
    /// 内部 path 重复 → `Err`（fail-visible，不覆盖在位者）。
    /// 需要 `network:http` 权限。
    fn http_register_endpoint(&self, config_json: &str) -> Result<String, HostError>;
    /// 注销本插件注册的端点（未知句柄 → `Ok(false)`，他人句柄 → `Err`）；
    /// 插件停用时宿主自动清空其全部注册路由。需要 `network:http` 权限。
    fn http_unregister_endpoint(&self, endpoint_id: &str) -> Result<bool, HostError>;
}
