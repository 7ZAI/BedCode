//! 引擎面 utils 模块（bedcode-wasm-core 纯净性收口票 05）
//!
//! 认证中心桥接（`auth_center` 桥接门/裁决面）、会话窄转发（`session_gateway`）
//! 与测试夹具（`test_tokens`）已回迁宿主 lib（用户裁定：宿主薄壳，不留 wasm
//! core）。本模块只保留**连接身份**（`identity`，真源 bedcode-server-base）。
//!
//! 注册表与 WIT 绑定面（`host_api::auth_center` + `host_api/auth.rs` 的
//! `auth-method-invoke` 实现链）仍在 wasm-core——那是机制。lib 单向依赖本 crate
//! 取用（`bedcode_wasm_core::host_api::auth_center::*`）。

pub mod auth;