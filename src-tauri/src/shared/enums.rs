//! Enums Module
//!
//! 公共枚举类型定义

pub mod auth;
pub mod control;
pub mod session;
pub mod special_key;
pub mod sumary;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload, SessionConfigAction, SessionConfigPayload};
pub use session::{SessionStatus, SessionType};
pub use special_key::SpecialKey;
pub use sumary::{QuickActionSummary, SessionConfigSummary, SessionSummary};