//! Desktop Enums Module
//!
//! 桌面端枚举类型定义

pub mod pty_status;
pub mod shell;

pub use pty_status::PtySessionStatus;
pub use shell::{ExecutionEnvironment, SessionLaunchConfig, WindowsShell};