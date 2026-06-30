//! Global Event System
//!
//! 全局事件匹配处理器
//! 使用 HashMap 存储事件类型与处理器的映射关系

pub mod events;
pub mod handler;

pub use handler::*;
pub use events::AppEvent;