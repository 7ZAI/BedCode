//! Traits
//!
//! PTY 相关 trait 定义 - 仅保留需要多态的 trait

pub mod pty_handler;
pub mod pty_output_handler;
pub mod pty_output_listener;

pub use pty_output_handler::PtyOutputHandler;
pub use pty_output_listener::{PtyOutputListener, PtyOutputListenerSync};
