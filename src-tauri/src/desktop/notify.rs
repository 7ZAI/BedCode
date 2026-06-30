//! Notification Module
//!
//! 通知模块 - 桌面端专用
//!
//! 模块划分:
//! - types.rs: 类型定义
//! - service.rs: 服务实现

mod service;
mod types;

pub use service::NotificationService;
pub use types::{Notification, NotificationPriority, NotificationSettings, NotificationType};