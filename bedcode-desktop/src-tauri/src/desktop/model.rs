//! Desktop Model Module
//!
//! 桌面端数据结构定义

pub mod pty_output;
pub mod session_event;

pub use pty_output::PtyOutputEvent;
pub use session_event::{SessionInfo, SessionRestartEvent, SessionStatusEvent};