//! Desktop Events Module
//!
//! 桌面端内部事件定义

pub mod sync_event;
pub mod sync_handler;
pub mod ws_server_handler;

pub use sync_event::DesktopSyncEvent;
pub use sync_handler::SyncEventHandler;
pub use ws_server_handler::WsServerEventHandler;