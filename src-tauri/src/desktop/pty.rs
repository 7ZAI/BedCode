//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod command;
pub mod pty_process;
pub mod reader;
pub mod wsl;
pub mod tmux;
mod pty_handler;

pub use pty_handler::{PtyHandler, PtySessionHandler};
pub use pty_process::PtySession;

// Re-export from enums
pub use crate::desktop::enums::{
    ExecutionEnvironment, PtySessionStatus, SessionLaunchConfig, WindowsShell,
};

// Re-export from model
pub use crate::desktop::model::PtyOutputEvent;

// Re-export from submodules
pub use wsl::{
    execute_command, get_default_distro, is_wsl_available, list_distributions, windows_to_wsl_path,
    wsl_to_windows_path, WslDistro,
};
pub use tmux::{
    capture_pane, create_session, create_session_in_dir, get_attach_command, get_tmux_command,
    get_tmux_version, is_tmux_available, kill_session, list_sessions, send_keys, send_special_key,
    session_exists, TmuxSession,
};
pub use command::build_command;
pub use reader::OutputReader;

/// 全局 PTY 输出索引计数器（跨所有会话）
static OUTPUT_INDEX_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 获取下一个全局输出索引（pub 供其他模块使用）
pub fn next_output_index() -> usize {
    OUTPUT_INDEX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}