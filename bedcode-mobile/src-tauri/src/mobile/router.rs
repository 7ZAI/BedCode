//! Mobile Router Module
//!
//! 移动端消息路由器，包含路由注册器和业务路由器实现

pub mod event;
pub mod context;
pub mod registry;
pub mod router;

// Re-export public types
pub use event::MobileEvent;
pub use context::ClientRouteContext;
pub use registry::{ClientRouteRegistry, ClientRouteHandler, message_type_key};
pub use router::{ClientBusinessRouter, ClientBusinessRouterBuilder};

// Re-export handlers from sibling module (mobile::handler)
pub use crate::mobile::handler::{AuthHandler, SyncHandler, SystemHandler, TerminalHandler};