//! Global Event System
//!
//! 全局事件匹配处理器 - 事件源注册、处理器注册和自动桥接

pub mod events;
pub mod handler;

pub use handler::*;
pub use events::AppEvent;
