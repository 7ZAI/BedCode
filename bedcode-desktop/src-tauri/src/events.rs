//! Desktop Sync Events
//!
//! 桌面端内部同步事件定义

pub mod sync_event;
pub mod sync_handler;

pub use sync_event::DesktopSyncEvent;
pub use sync_handler::SyncEventHandler;
