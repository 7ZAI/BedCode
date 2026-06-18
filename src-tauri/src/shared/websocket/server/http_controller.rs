//! HTTP Controller Pattern
//!
//! 提供 Spring MVC 风格的 Controller 抽象
//! 支持方法级路由注册，每个 URL 直接绑定到 Controller 中的具体方法

use crate::Result;
use crate::shared::websocket::server::http_router::{HttpRouter, HttpRouteHandler, HttpRequestContext};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// ==================== Boxed Future ====================

/// Boxed Future 类型别名，避免生命周期问题
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ==================== Method Handler ====================

/// 方法级处理器
///
/// 包装单个 async fn，实现 HttpRouteHandler trait
/// 使用 BoxFuture 避免闭包内的生命周期问题
pub struct MethodHandler {
    handler: Box<dyn for<'a> Fn(&'a HttpRequestContext) -> BoxFuture<'a, Result<String>> + Send + Sync>,
}

impl MethodHandler {
    /// 从闭包创建 MethodHandler
    ///
    /// 使用 for<'a> 语法支持任意生命周期
    pub fn new<F>(handler: F) -> Self
    where
        F: for<'a> Fn(&'a HttpRequestContext) -> BoxFuture<'a, Result<String>> + Send + Sync + 'static,
    {
        Self {
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl HttpRouteHandler for MethodHandler {
    async fn handle(&self, ctx: &HttpRequestContext) -> Result<String> {
        (self.handler)(ctx).await
    }
}

/// 辅助函数：创建 MethodHandler
///
/// 简化注册语法，避免显式类型声明
///
/// # Example
/// ```ignore
/// router.register(
///     Method::POST,
///     "/api/file-tree",
///     Arc::new(method_handler(|ctx| Box::pin(async {
///         // 处理逻辑
///         Ok("response".to_string())
///     })))
/// );
/// ```
pub fn method_handler<F>(handler: F) -> MethodHandler
where
    F: for<'a> Fn(&'a HttpRequestContext) -> BoxFuture<'a, Result<String>> + Send + Sync + 'static,
{
    MethodHandler::new(handler)
}

// ==================== Controller Trait ====================

/// HTTP Controller trait
///
/// 实现此 trait 的 struct 可以自动注册多个路由方法
/// 类似 Spring MVC 的 Controller 模式
///
/// # Example
/// ```ignore
/// pub struct FileController;
///
/// impl FileController {
///     pub async fn get_tree(ctx: &HttpRequestContext) -> Result<String> { ... }
/// }
///
/// impl HttpController for FileController {
///     fn register_routes(self: Arc<Self>, router: &mut HttpRouter) {
///         router.register(
///             Method::POST,
///             "/api/file-tree",
///             Arc::new(method_handler(|ctx| Box::pin(Self::get_tree(ctx))))
///         );
///     }
/// }
/// ```
#[async_trait]
pub trait HttpController: Send + Sync + 'static {
    /// 注册所有路由到 router
    ///
    /// Controller 实现此方法，将各处理方法注册到对应 URL
    fn register_routes(self: Arc<Self>, router: &mut HttpRouter);
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::websocket::server::http_router::ApiResponse;
    use hyper::Method;

    #[tokio::test]
    async fn test_method_handler_basic() {
        let handler = method_handler(|ctx| Box::pin(async {
            let resp = ApiResponse::ok_with_data(serde_json::json!({
                "path": ctx.path,
            }));
            Ok(serde_json::to_string(&resp).unwrap())
        }));

        let ctx = HttpRequestContext {
            method: Method::GET,
            path: "/test".to_string(),
            body: String::new(),
        };

        let result = handler.handle(&ctx).await.unwrap();
        assert!(result.contains("\"code\":0"));
        assert!(result.contains("/test"));
    }

    #[tokio::test]
    async fn test_controller_registration() {
        // 定义测试 Controller
        struct TestController;

        impl TestController {
            async fn handle_get(ctx: &HttpRequestContext) -> Result<String> {
                let resp = ApiResponse::ok_with_data(serde_json::json!({
                    "method": "GET",
                    "path": ctx.path,
                }));
                Ok(serde_json::to_string(&resp).unwrap())
            }

            async fn handle_post(ctx: &HttpRequestContext) -> Result<String> {
                let resp = ApiResponse::ok_with_data(serde_json::json!({
                    "method": "POST",
                    "path": ctx.path,
                    "body": ctx.body,
                }));
                Ok(serde_json::to_string(&resp).unwrap())
            }
        }

        #[async_trait]
        impl HttpController for TestController {
            fn register_routes(self: Arc<Self>, router: &mut HttpRouter) {
                // 注册 GET /test - 使用 Box::pin 包装 Future
                router.register(
                    Method::GET,
                    "/test",
                    Arc::new(method_handler(|ctx| Box::pin(TestController::handle_get(ctx)))),
                );

                // 注册 POST /test
                router.register(
                    Method::POST,
                    "/test",
                    Arc::new(method_handler(|ctx| Box::pin(TestController::handle_post(ctx)))),
                );
            }
        }

        // 创建路由器并注册 Controller
        let mut router = HttpRouter::new();
        Arc::new(TestController).register_routes(&mut router);

        // 测试 GET /test
        let ctx_get = HttpRequestContext {
            method: Method::GET,
            path: "/test".to_string(),
            body: String::new(),
        };
        let result = router.dispatch(&ctx_get).await.unwrap();
        assert!(result.contains("GET"));

        // 测试 POST /test
        let ctx_post = HttpRequestContext {
            method: Method::POST,
            path: "/test".to_string(),
            body: "{\"data\":123}".to_string(),
        };
        let result = router.dispatch(&ctx_post).await.unwrap();
        assert!(result.contains("POST"));
        assert!(result.contains("data"));
    }
}