//! 错误类型（票 17 批次 2b：真源 = fork crate `bedcode-wasm-core-mobile::error`）
//!
//! 宿主侧经本模块 re-export 保 `crate::system::error::AppError` /
//! `crate::AppError` 历史路径零改动。移动 `AppError` 形状与批次 1 fork 对齐；
//! 原 `Pty` 变体随迁移退役（全仓零构造点，死变体清理）。

pub use bedcode_wasm_core_mobile::error::{AppError, Result};
