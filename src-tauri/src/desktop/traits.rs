//! Traits - Business trait definitions
//!
//! 定义桌面端业务逻辑的 trait 接口

pub mod config_mapper;
pub mod naming_service;
pub mod output_cache;
pub mod pty_handler;
pub mod pty_output_handler;
pub mod pty_output_listener;
pub mod pty_registry;
pub mod session_event_bus;
pub mod session_info_registry;

pub use pty_output_handler::PtyOutputHandler;
pub use pty_output_listener::{PtyOutputListener, PtyOutputListenerSync};