//! Events Module
//!
//! 全局事件系统：事件抽象、匹配处理器、桌面同步事件、WS 广播转发
//!
//! **票 09 删除 `forwarder::EventForwarder`**：「内核 `SessionManager` 状态订阅 →
//! Tauri 前端事件 `session-status-changed`」的转接通道已退役——它订阅的内核状态
//! 广播对插件会话已无流量（P1-b 真源下沉），前端那条注册名（`session:statusChange`）
//! 两侧本就不一致、且零生产消费方。会话事实的前端可见性由插件经 `host-events` /
//! `host-bus` 自己发布，宿主不再替它转接。

pub mod app_event;
pub mod matcher;
pub mod sync_event;
pub mod sync_handler;

pub use app_event::AppEvent;
pub use matcher::{global_matcher, EventFilter, EventHandler, EventMatcher};
pub use sync_event::DesktopSyncEvent;
pub use sync_handler::SyncEventHandler;
