//! HTTP 传输面
//!
//! 动态路由注册表（`registry`）、协议网关（`gateway`：查注册表通用判定/转发）、
//! 插件代理控制器（`controllers`）、请求/响应 DTO 契约锚点（`dtos`）、中间件链
//! （`middleware`）与路由装配（`routes`：两个公开端点 + `/api` scope 及其 `wrap` 链）。
//!
//! 依赖方向（不变量 I2）：本面只**向下**依赖 [`crate::server::core`]（过滤器链 /
//! 链路加密 / 指标），与 `websocket` 面之间零横向 import（I1，硬锁）。认证档位
//! 词汇 `EndpointAuth` 真源在桌面 SDK（`bedcode_plugin_api`），两侧各自直连，
//! 不共享词汇模块。

pub mod controllers;
pub mod dtos;
pub mod gateway;
pub mod middleware;
pub mod registry;
pub mod routes;

pub use routes::configure_routes;
