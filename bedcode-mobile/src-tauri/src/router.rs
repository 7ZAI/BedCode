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

// 注意：本模块**不** re-export `crate::handler::*`。那层转发曾让 `router` ↔ `handler`
// 构成 import 环——`handler/*` 本就要 import `router::{ClientRouteContext,
// ClientRouteHandler, MobileEvent}` 来实现路由 trait，router 再反向导出 handler
// 形成闭环。消费方（`connection/manager.rs`）已改为直接从 `crate::handler` 取，
// 环随之断开；需要哪层就 import 哪层，模块边界不再靠转发伪装成扁平结构。
