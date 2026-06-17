//! HTTP Router Config
//!
//! 桌面端 HTTP API 路由配置
//! 使用 Builder 模式注册 HTTP 路由，将路由组装逻辑从 WebSocketManager 中解耦

use crate::shared::websocket::server::http_router::{HttpRouter, HttpRouteHandler};
use hyper::Method;
use std::sync::Arc;

/// HTTP 路由配置构建器
///
/// 职责：集中注册桌面端 HTTP API 路由，生成 HttpRouter 实例
/// Handler 通过 AppContext::global() 获取依赖，无需外部传参
pub struct HttpRouterConfig {
    router: HttpRouter,
}

impl HttpRouterConfig {
    pub fn new() -> Self {
        Self {
            router: HttpRouter::new(),
        }
    }

    /// 注册文件树 API (POST /api/file-tree)
    pub fn register_file_tree(mut self) -> Self {
        let handler = Arc::new(
            crate::desktop::server::handlers::FileTreeHandler,
        );
        self.router.register(Method::POST, "/api/file-tree", handler);
        self
    }

    /// 注册自定义路由
    pub fn register(
        mut self,
        method: Method,
        path: &str,
        handler: Arc<dyn HttpRouteHandler>,
    ) -> Self {
        self.router.register(method, path, handler);
        self
    }

    /// 构建最终的 HttpRouter
    pub fn build(self) -> HttpRouter {
        self.router
    }
}

impl Default for HttpRouterConfig {
    fn default() -> Self {
        Self::new()
    }
}
