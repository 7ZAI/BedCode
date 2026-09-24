//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能。
//!
//! **零业务语义（2026-09-23 PTY 解耦票）**：本引擎不接受业务配置类型——
//! 只收调用方算好的 argv（`CommandBuilder`）。shell 包装 / WSL 路径转换 /
//! 业务环境注入都在消费侧（业务会话 = `session/session_manager.rs::launch_command`，
//! 插件私有 PTY = 插件自己）；业务输出汇实现（`SessionOutputSink`）亦已归位
//! `session/session_output.rs`。WSL **发行版列举**（`wsl-distros` 宿主原语）是
//! 平台事实、非业务语义，保留在本模块。

pub mod lifecycle;
pub mod output_sink;
pub mod pty_process;
pub mod pty_reader;
pub mod pty_ring;
pub mod wsl;

pub use lifecycle::{PtyTerminated, PtyTerminationGate};
pub use output_sink::PtyOutputSink;
pub use pty_process::PtySession;
pub use pty_ring::{PtyRing, PtyRingFetch, PtyRingSink};

// Re-export from enums
pub use crate::enums::PtySessionStatus;

// Re-export from submodules
pub use pty_reader::PtyReader;
pub use wsl::{list_distributions, WslDistro};
