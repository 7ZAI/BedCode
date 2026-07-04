//! Events Module
//!
//! 全局事件系统：事件抽象、匹配处理器、桌面同步事件、前端转发

pub mod app_event;
pub mod forwarder;
pub mod matcher;
pub mod sync_event;
pub mod sync_handler;

pub use app_event::AppEvent;
pub use forwarder::EventForwarder;
pub use matcher::{EventHandler, EventFilter, EventMatcher, global_matcher};
pub use sync_event::DesktopSyncEvent;
pub use sync_handler::SyncEventHandler;
