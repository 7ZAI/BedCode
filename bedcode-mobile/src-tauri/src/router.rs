//! Router Module
//!
//! 消息路由器 - 包含路由注册器、业务路由器和事件转发

pub mod context;
pub mod event;
pub mod registry;
pub mod router;

// Re-export public types
pub use context::ClientRouteContext;
pub use event::MobileEvent;
pub use registry::{message_type_key, ClientRouteHandler, ClientRouteRegistry};
pub use router::{ClientBusinessRouter, ClientBusinessRouterBuilder};

// Re-export handlers from sibling module
pub use crate::handler::{AuthHandler, SyncHandler, SystemHandler, TerminalHandler};
