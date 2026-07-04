//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod command;
pub mod frontend_output_handler;
mod pty_handler;
mod pty_output;
pub mod pty_output_listener;
pub mod pty_process;
pub mod pty_reader;
pub mod wsl;

pub use pty_handler::{PtyHandler, PtySessionHandler};
pub use pty_output::PtyOutputEvent;
pub use pty_output_listener::{AsyncPtyOutputListener, PtyOutputHandler, PtyOutputListener, PtyOutputListenerSync};
pub use pty_process::PtySession;
pub use frontend_output_handler::FrontendOutputHandler;

// Re-export from enums
pub use crate::enums::{
    ExecutionEnvironment, PtySessionStatus, SessionLaunchConfig, WindowsShell,
};

// Re-export from submodules
pub use wsl::{
    get_default_distro, is_wsl_available, list_distributions, windows_to_wsl_path,
    wsl_to_windows_path, WslDistro,
};
pub use command::build_command;
pub use pty_reader::PtyReader;

/// 全局 PTY 输出索引计数器（跨所有会话）
static OUTPUT_INDEX_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 获取下一个全局输出索引（pub 供其他模块使用）
pub fn next_output_index() -> usize {
    OUTPUT_INDEX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
