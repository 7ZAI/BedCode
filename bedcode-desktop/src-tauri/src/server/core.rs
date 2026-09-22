//! 服务器内核层：与传输无关的引擎与共享面
//!
//! 启动与生命周期（`app` / `supervisor` / `port_checker`）、跨传输流量过滤器链（`filter`）、
//! 链路加密（`link_crypto`）、指标（`metrics`）。HTTP 面与 WS 面只**向下**依赖本层，
//! 彼此之间不得横向 import（不变量 I1）。
//!
//! `app.rs` 是本层唯一被豁免「不得反向认识传输面」的文件：HTTP 与 WS 共用一个端口、一个
//! `HttpServer`，这个组合物必然同时看见两面（决策 D4 / I3 豁免，见 `docs/adr/` 与票面 spec）。

pub mod app;
pub mod filter;
pub mod link_crypto;
pub mod metrics;
pub mod port_checker;
pub mod supervisor;
