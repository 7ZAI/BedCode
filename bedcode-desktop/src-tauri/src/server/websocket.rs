//! WebSocket 传输面
//!
//! 连接骨架（`conn`）、三通道实现（`channel`）、连接/端点注册表（`registry` /
//! `endpoint`）、输出订阅原语（`subscription`）、移动端兼容 wire 协议（`message`）、
//! WsSession 连接态（`session`）、生命周期与优雅停机（`websocket_manager`）、
//! 终端输出端子面（`terminal_ws`）与连接事件类型（`connection_types`）。
//!
//! `services` 承载的会话控制与终端输入**不是 WS 传输原语**（ADR 0022 裁剪线视角，
//! 归属应为会话业务、后续下沉插件线）——本目录只是它的临时住处，见该模块注释。
//! 依赖方向（不变量 I2）：只**向下**依赖 [`crate::server::core`]，与 `http` 面零横向
//! import（I1，票 08 加锁）。认证档位词汇 `EndpointAuth` 直连桌面 SDK
//! （`bedcode_plugin_api`），不建共享词汇模块。

pub mod channel;
pub mod conn;
pub mod connection_types;
pub mod endpoint;
pub mod message;
pub mod registry;
pub mod services;
pub mod session;
pub mod subscription;
pub mod terminal_ws;
pub mod websocket_manager;

pub use websocket_manager::{ClientSummary, ServerEvent, WebSocketManager};
