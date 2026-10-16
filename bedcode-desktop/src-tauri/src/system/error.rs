//! Error types for Claude Code Remote
//!
//! **server-lib-split 迁移**：真源已随 `bedcode-server-base` 拆出（拆分后为
//! `bedcode-server-base` crate 的 `error` 模块）；本文件保留模块路径与全部公开
//! 名字，宿主其余代码经 `crate::system::error::*` 引用不受影响。
//!
//! 共享错误类型 - 桌面端和移动端都可用
//!
//! 桌面端专属：错误信封（ADR 0030）——跨进程失败一律承载为 `{ code, request_id, params? }`，
//! 技术详情（原文 / anyhow 链 / 堆栈）**永不出产生方进程**；移动端不跟演（error.rs 独立副本）。

pub use bedcode_server_base::error::{new_request_id, AppError, EventEnvelope, Result, DEFAULT_ERROR_CODE};
