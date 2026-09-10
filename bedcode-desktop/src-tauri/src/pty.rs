//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod command;
mod pty_handler;
pub mod pty_process;
pub mod pty_reader;
pub mod wsl;

pub use pty_handler::{PtyHandler, PtySessionHandler};
pub use pty_process::PtySession;

// Re-export from enums
pub use crate::enums::{ExecutionEnvironment, PtySessionStatus, SessionLaunchConfig, WindowsShell};

// Re-export from submodules
pub use command::build_command;
pub use pty_reader::PtyReader;
pub use wsl::{
    get_default_distro, is_wsl_available, list_distributions, windows_to_wsl_path, wsl_to_windows_path, WslDistro,
};

/// 全局 PTY 输出索引计数器（跨所有会话）
///
/// 仅作路由/日志唯一标记：权威的输出序号（seq）由 SessionOutputManager::on_output
/// 按会话连续分配（队列 max_seq + 1），见 session_output.rs
static OUTPUT_INDEX_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 获取下一个全局输出索引（pub 供其他模块使用）
pub fn next_output_index() -> usize {
    OUTPUT_INDEX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
