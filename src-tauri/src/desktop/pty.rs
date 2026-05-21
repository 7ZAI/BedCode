//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod command;
pub mod pty_process;
pub mod pty_reader;
pub mod subscription;
pub mod pty_subscription_handler;
pub mod wsl;
pub mod tmux;
mod pty_handler;
mod pty_output_listener;
pub mod frontend_output_handler;

pub use pty_handler::{PtyHandler, PtySessionHandler};
// Re-export from pty_process (同步版本，使用 traits 中的定义)
pub use pty_process::PtySession;
// Re-export async implementation
pub use pty_output_listener::AsyncPtyOutputListener;
// Re-export FrontendOutputHandler
pub use frontend_output_handler::FrontendOutputHandler;
// Re-export PtySubscriptionHandler
pub use pty_subscription_handler::PtySubscriptionHandler;


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
pub use pty_reader::PtyReader;
pub use subscription::{OutputRingBuffer, PtySubscriptionManager, SubscribeResponse, Subscription};

/// 全局 PTY 输出索引计数器（跨所有会话）
static OUTPUT_INDEX_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 获取下一个全局输出索引（pub 供其他模块使用）
pub fn next_output_index() -> usize {
    OUTPUT_INDEX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}