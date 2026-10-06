//! PTY Session Status（垫片，wasm-core 纯净性收口票 02）
//!
//! 真源已随 PTY 引擎迁出至 `bedcode-pty-engine`。本文件是纯 `pub use` 垫片——
//! 保持 `crate::enums::PtySessionStatus` / `crate::enums::pty_status::*` 路径不变，
//! 且**类型身份**与引擎 crate 唯一（wasm-core 与引擎共用一个 `PtySessionStatus`，
//! 避免两份定义在 `PtyTerminated::status` 比较处错位）。

pub use bedcode_pty_engine::PtySessionStatus;