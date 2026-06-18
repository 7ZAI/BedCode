//! HTTP Router
//!
//! 为 WsServer 提供 HTTP 请求路由能力
//! 与 WS 的 BusinessRouter 并行，为未来移动端 HTTP API 提供基础

use crate::Result;
use async_trait::async_trait;
use hyper::{Method, Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ==================== File Tree Types ====================

/// 文件树节点 — 与前端 FileTreeNode 一一对应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    pub name: String,
    pub node_type: FileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
    /// 仅 folder 有效，前端控制展开/折叠
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    File,
    Folder,
}

/// POST /api/file-tree 请求体
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeRequest {
    /// 会话配置 ID，用于查找 working_dir
    pub session_id: String,
    /// 需要过滤的目录名列表
    /// - 纯名称（如 "node_modules"）：匹配所有层级的该目录
    /// - 带父级路径（如 "src/node_modules"）：仅在父级路径下匹配
    pub exclude_dirs: Vec<String>,
}

// ==================== HTTP Router Core ====================

/// HTTP 响应体类型
pub type HttpBody = Full<Bytes>;

/// HTTP 请求上下文（提取常用字段，供 handler 使用）
#[derive(Debug, Clone)]
pub struct HttpRequestContext {
    /// 请求方法
    pub method: Method,
    /// 请求路径（不含 query string）
    pub path: String,
    /// 请求体（JSON 字符串）
    pub body: String,
}

/// HTTP 统一响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub code: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl ApiResponse<()> {
    pub fn ok() -> Self {
        Self { code: 0, message: "ok".to_string(), data: None }
    }

    pub fn ok_with_data<T: Serialize>(data: T) -> ApiResponse<T> {
        ApiResponse { code: 0, message: "ok".to_string(), data: Some(data) }
    }

    pub fn error(code: u16, message: &str) -> Self {
        Self { code, message: message.to_string(), data: None }
    }
}

/// HTTP 路由处理器 trait
#[async_trait]
pub trait HttpRouteHandler: Send + Sync {
    async fn handle(&self, ctx: &HttpRequestContext) -> Result<String>;
}

/// 路由键：精确匹配 (Method, Path)
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RouteKey {
    method: Method,
    path: String,
}

/// HTTP 路由器
///
/// 基于 (Method, Path) 精确匹配，不支持路径参数，保持简单
pub struct HttpRouter {
    routes: HashMap<RouteKey, Arc<dyn HttpRouteHandler>>,
}

impl HttpRouter {
    pub fn new() -> Self {
        Self { routes: HashMap::new() }
    }

    /// 注册路由（builder 风格，链式调用）
    pub fn route(mut self, method: Method, path: &str, handler: Arc<dyn HttpRouteHandler>) -> Self {
        self.routes.insert(RouteKey { method, path: path.to_string() }, handler);
        self
    }

    /// 注册路由（&mut 风格，用于创建后动态添加）
    pub fn register(&mut self, method: Method, path: &str, handler: Arc<dyn HttpRouteHandler>) {
        self.routes.insert(RouteKey { method, path: path.to_string() }, handler);
    }

    /// 分发请求到对应处理器，未匹配返回 404
    pub async fn dispatch(&self, ctx: &HttpRequestContext) -> Result<String> {
        let key = RouteKey { method: ctx.method.clone(), path: ctx.path.clone() };
        match self.routes.get(&key) {
            Some(handler) => handler.handle(ctx).await,
            None => {
                let resp = ApiResponse::<()>::error(404, &format!("Not Found: {} {}", ctx.method, ctx.path));
                Ok(serde_json::to_string(&resp)?)
            }
        }
    }
}

/// 构建 JSON HTTP 响应（含 CORS 头）
pub fn build_json_response(status: StatusCode, body: &str) -> Response<HttpBody> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

/// 构建 CORS 预检响应
pub fn build_cors_preflight_response() -> Response<HttpBody> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
        .header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        .header("Access-Control-Max-Age", "86400")
        .body(Full::new(Bytes::new()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler;

    #[async_trait]
    impl HttpRouteHandler for EchoHandler {
        async fn handle(&self, ctx: &HttpRequestContext) -> Result<String> {
            let resp = ApiResponse::ok_with_data(serde_json::json!({
                "method": ctx.method.to_string(),
                "path": ctx.path,
            }));
            Ok(serde_json::to_string(&resp)?)
        }
    }

    #[tokio::test]
    async fn test_route_dispatch() {
        let router = HttpRouter::new().route(Method::POST, "/api/test", Arc::new(EchoHandler));
        let ctx = HttpRequestContext { method: Method::POST, path: "/api/test".to_string(), body: String::new() };
        let result = router.dispatch(&ctx).await.unwrap();
        assert!(result.contains("\"code\":0"));
        assert!(result.contains("/api/test"));
    }

    #[tokio::test]
    async fn test_route_not_found() {
        let router = HttpRouter::new();
        let ctx = HttpRequestContext { method: Method::GET, path: "/api/nonexistent".to_string(), body: String::new() };
        let result = router.dispatch(&ctx).await.unwrap();
        assert!(result.contains("\"code\":404"));
    }

    #[test]
    fn test_build_json_response_status() {
        let resp = build_json_response(StatusCode::OK, r#"{"code":0}"#);
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_api_response_ok() {
        let resp = ApiResponse::<()>::ok();
        assert_eq!(resp.code, 0);
    }

    #[test]
    fn test_api_response_error() {
        let resp = ApiResponse::<()>::error(1001, "auth failed");
        assert_eq!(resp.code, 1001);
        assert_eq!(resp.message, "auth failed");
    }
}
