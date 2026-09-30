//! HTTP 传输面（server-lib-split 票 04）
//!
//! 动态路由注册表（`registry`）、协议网关（`gateway`：查注册表通用判定/转发）、
//! 插件代理控制器（`controllers`）、请求/响应 DTO 契约锚点（`dtos`）、中间件链
//! （`middleware`）与路由装配（`routes`：两个公开端点 + `/api` scope 及其 `wrap` 链）。
//!
//! 依赖方向（不变量 I2）：本 crate 只**向下**依赖 `bedcode-server-core`（过滤器链 /
//! 链路加密 / 指标）与 `bedcode-server-base`（错误/常量/端口 traits），与
//! `bedcode-server-websocket` 之间零横向 import（I1）。
//!
//! **不依赖宿主 crate**：面内所有「要宿主做的事」经 `bedcode_server_base::ports::get()`
//! 取注入的端口实现——`PluginInvoker`（转发插件 `_http_endpoint` / 激活判定）、
//! `AuthCenter`（连接裁决）、`PathsPort`（应用数据目录）。端口缺席（无头 / 单测）时
//! 各点按其自身语义 fail-visible，不静默降级成「无数据」。
//!
//! 认证档位词汇 `EndpointAuth` 真源在桌面 SDK（`bedcode_plugin_api`），两侧各自直连，
//! 不建共享词汇模块。
//!
//! DTO 口径（AGENTS §5.1.1 B4 合法残留）：`dtos` 内只有 `common_dto`（通用 API 信封）
//! 与会话/配置/文件/git 四组**仅供 `#[cfg(test)]` 黄金形状锁**的类型——锁的是
//! 「移动端看到的字节」这一跨端 wire 契约，路由真身已在插件侧（ABI v29）；
//! 生产路径不得构造这些类型。改它们 = 改跨端 wire 契约，必须两端同步评估。

pub mod controllers;
pub mod dtos;
pub mod face;
pub mod gateway;
pub mod middleware;
pub mod registry;
pub mod routes;

pub use face::HttpTransportFace;
pub use routes::configure_routes;
