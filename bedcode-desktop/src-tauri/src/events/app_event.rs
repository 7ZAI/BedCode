//! Global Event System
//!
//! 项目全局事件顶层抽象
//! 所有模块的事件都应实现此 trait

use std::fmt::Debug;

/// 全局事件顶层 trait
/// 项目中所有事件类型都应实现此 trait
pub trait AppEvent: Clone + Send + Sync + Debug {}
