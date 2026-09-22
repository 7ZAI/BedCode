//! PTY (Pseudo Terminal) Manager
//!
//! 提供跨平台的 PTY 管理功能，支持 Windows 原生和 WSL2 环境

pub mod command;
pub mod lifecycle;
pub mod output_sink;
mod pty_handler;
pub mod pty_process;
pub mod pty_reader;
pub mod pty_ring;
pub mod wsl;

pub use lifecycle::{PtyTerminated, PtyTerminationGate};
pub use output_sink::{PtyOutputSink, SessionOutputSink};
pub use pty_handler::{PtyHandler, PtySessionHandler};
pub use pty_process::{PtyCommandSource, PtySession};
pub use pty_ring::{PtyRing, PtyRingFetch, PtyRingSink};

// Re-export from enums
pub use crate::enums::{ExecutionEnvironment, PtySessionStatus, SessionLaunchConfig, WindowsShell};

// Re-export from submodules
pub use command::build_command;
pub use pty_reader::PtyReader;
pub use wsl::{
    get_default_distro, is_wsl_available, list_distributions, windows_to_wsl_path, wsl_to_windows_path, WslDistro,
};
