//! HTTP Client Module
//!
//! 移动端通用 HTTP 客户端，直接调用桌面端 HTTP API
//! 自动注入 JWT Token，统一解析 ApiResponse 格式

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{de::DeserializeOwned, Serialize};

use crate::shared::websocket::server::http_router::ApiResponse;
use crate::mobile::global::get_global_token;
use crate::Result;
use crate::AppError;

/// 默认请求超时
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// 移动端 HTTP 客户端
///
/// 职责：
/// - 维护桌面端 base_url（与 WS 连接同步）
/// - 自动注入 Authorization header
/// - 统一解析 ApiResponse<T> 响应格式
pub struct HttpClient {
    client: reqwest::Client,
    base_url: Arc<RwLock<String>>,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端实例
    pub fn new() -> Arc<Self> {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("Failed to create reqwest client");

        Arc::new(Self {
            client,
            base_url: Arc::new(RwLock::new(String::new())),
        })
    }

    /// 更新桌面端地址（WS 连接成功时调用）
    pub async fn update_base_url(&self, address: &str, port: u16) {
        let url = format!("http://{}:{}", address, port);
        let mut guard = self.base_url.write().await;
        tracing::info!("[HttpClient] base_url updated: {}", url);
        *guard = url;
    }

    /// 清除桌面端地址（WS 断开时调用）
    pub async fn clear_base_url(&self) {
        let mut guard = self.base_url.write().await;
        tracing::info!("[HttpClient] base_url cleared");
        *guard = String::new();
    }

    /// 获取当前 base_url
    async fn get_base_url(&self) -> Result<String> {
        let guard = self.base_url.read().await;
        if guard.is_empty() {
            return Err(AppError::WebSocket("Not connected: no base_url set".to_string()));
        }
        Ok(guard.clone())
    }

    /// 构建带 Token 的请求 builder
    fn with_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let token = get_global_token();
        if token.is_empty() {
            builder
        } else {
            builder.bearer_auth(&token)
        }
    }

    /// GET 请求，自动注入 Token，解析 ApiResponse<T>
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let base = self.get_base_url().await?;
        let url = format!("{}{}", base, path);

        tracing::debug!("[HttpClient] GET {}", url);

        let builder = self.client.get(&url);
        let builder = self.with_auth(builder);
        let resp = builder.send().await
            .map_err(|e| AppError::Internal(format!("HTTP GET {} failed: {}", url, e)))?;

        self.parse_response::<T>(resp).await
    }

    /// POST 请求，自动注入 Token + JSON body，解析 ApiResponse<T>
    pub async fn post<T: Serialize, R: DeserializeOwned>(&self, path: &str, body: &T) -> Result<R> {
        let base = self.get_base_url().await?;
        let url = format!("{}{}", base, path);

        tracing::debug!("[HttpClient] POST {}", url);

        let builder = self.client.post(&url);
        let builder = self.with_auth(builder);
        let builder = builder.json(body);
        let resp = builder.send().await
            .map_err(|e| AppError::Internal(format!("HTTP POST {} failed: {}", url, e)))?;

        self.parse_response::<R>(resp).await
    }

    /// 统一解析 ApiResponse<T> 格式响应
    ///
    /// code == 0 → 返回 data 字段
    /// code != 0 → 返回 AppError::Internal
    async fn parse_response<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "HTTP {} error: {}", status, body
            )));
        }

        let body = resp.text().await
            .map_err(|e| AppError::Internal(format!("Failed to read HTTP response: {}", e)))?;

        tracing::debug!("[HttpClient] Response body: {}", &body[..body.len().min(500)]);

        // 先解析为 ApiResponse<serde_json::Value> 检查 code
        let api_resp: ApiResponse<serde_json::Value> = serde_json::from_str(&body)
            .map_err(|e| AppError::Internal(format!("Failed to parse ApiResponse: {}", e)))?;

        if api_resp.code != 0 {
            return Err(AppError::Internal(format!(
                "API error (code={}): {}", api_resp.code, api_resp.message
            )));
        }

        // 从 data 字段解析目标类型
        let data = api_resp.data
            .ok_or_else(|| AppError::Internal("API response missing data field".to_string()))?;

        serde_json::from_value(data)
            .map_err(|e| AppError::Internal(format!("Failed to parse response data: {}", e)))
    }
}

// ==================== File Tree API ====================

use crate::shared::websocket::server::http_router::{FileTreeRequest, FileTreeNode};

/// 文件树业务 API
pub struct FileTreeApi;

impl FileTreeApi {
    /// 获取文件树
    ///
    /// 调用桌面端 POST /api/file-tree
    pub async fn get_file_tree(
        http: &HttpClient,
        session_id: &str,
        exclude_dirs: Vec<String>,
    ) -> Result<Vec<FileTreeNode>> {
        let req = FileTreeRequest {
            session_id: session_id.to_string(),
            exclude_dirs,
        };
        http.post("/api/file-tree", &req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_new() {
        let client = HttpClient::new();
        // 验证 base_url 初始为空
        let rt = tokio::runtime::Runtime::new().unwrap();
        let base_url = rt.block_on(async {
            client.base_url.read().await.clone()
        });
        assert!(base_url.is_empty());
    }

    #[tokio::test]
    async fn test_update_and_clear_base_url() {
        let client = HttpClient::new();
        client.update_base_url("192.168.1.100", 8080).await;
        let url = client.get_base_url().await.unwrap();
        assert_eq!(url, "http://192.168.1.100:8080");

        client.clear_base_url().await;
        let result = client.get_base_url().await;
        assert!(result.is_err());
    }
}
