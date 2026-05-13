//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod pty_process;
pub mod wsl;
pub mod tmux;

pub use pty_process::{
    ExecutionEnvironment, PtyOutputEvent, PtySession, PtySessionState, PtySessionStatus,
    SessionLaunchConfig, WindowsShell,
};
pub use wsl::{
    execute_command, get_default_distro, is_wsl_available, list_distributions, windows_to_wsl_path,
    wsl_to_windows_path, WslDistro,
};
pub use tmux::{
    capture_pane, create_session, create_session_in_dir, get_attach_command, get_tmux_command,
    get_tmux_version, is_tmux_available, kill_session, list_sessions, send_keys, send_special_key,
    session_exists, TmuxSession,
};