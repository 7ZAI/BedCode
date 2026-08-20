//! Handler Module - 消息处理器集合
//!
//! 各消息类型的处理器实现

pub mod auth;
pub mod file_service;
pub mod sync;
pub mod system;
pub mod terminal;

// Re-export handlers
pub use auth::AuthHandler;
pub use file_service::FileServiceHandler;
pub use sync::SyncHandler;
pub use system::SystemHandler;
pub use terminal::TerminalHandler;
