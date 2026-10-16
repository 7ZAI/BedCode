//! Utils Module
//!
//! 工具模块 - 认证、加密 + 会话互调窄转发
//!
//! **整核抽出（wasm-core-whole-crate）**：`auth/auth_center`、`auth/test_tokens`、
//! `session_gateway` 已迁入 `bedcode-wasm-core` crate；本文件保留 `crate::utils::*`
//! 路径（`pub use` 垫片，spec §3.1），`identity` / `crypto` 真源在 base / crypto
//! crate，原样保留。

pub mod auth;
pub mod crypto;
pub mod session_gateway;
