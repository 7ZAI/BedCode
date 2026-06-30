//! Traits - Business trait definitions
//!
//! 仅保留需要多态的 trait（有多个实现或需要 mock 测试）
//! 单一实现的 trait 已内联到各自的实现文件中

pub mod pty_handler;
pub mod pty_output_handler;
pub mod pty_output_listener;

pub use pty_output_handler::PtyOutputHandler;
pub use pty_output_listener::{PtyOutputListener, PtyOutputListenerSync};
