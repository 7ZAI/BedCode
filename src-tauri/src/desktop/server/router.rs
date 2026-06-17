//! Business Router Module
//!
//! 桌面端 WebSocket 业务消息路由器基础设施
//! 提供基于消息类型的路由、中间件洋葱模型、处理器注册

pub mod business_router;
pub mod handler;
pub mod middleware;
pub mod registry;
pub mod http_router_config;

pub use business_router::{BusinessRouter, BusinessRouterBuilder};
pub use handler::BoxedHandler;
pub use middleware::BusinessMiddleware;
pub use http_router_config::HttpRouterConfig;
