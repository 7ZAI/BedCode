//! 各消息类型的处理器实现

pub mod auth;
pub mod plugin_event;
pub mod system;

// Re-export handlers
pub use auth::AuthHandler;
pub use plugin_event::PluginEventRouter;
pub use system::SystemHandler;
