//! 进程创建工具（垫片，wasm-core 纯净性收口票 03）
//!
//! 真源已下沉 `bedcode-server-base::process`（E2/D4：`create_command` 被
//! wasm-core 的 `wsl_fs` / `system::wsl` 与 `bedcode-pty-engine` 两侧消费，
//! 放基础层归属唯一）。本文件是纯 `pub use` 垫片，保持
//! `crate::system::process::create_command` 路径不变。

pub use bedcode_server_base::process::create_command;