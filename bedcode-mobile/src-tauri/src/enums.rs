//! Enums Module
//!
//! 公共枚举类型定义

pub mod auth;
pub mod control;
pub mod plugin;
pub mod session;
pub mod special_key;
pub mod sumary;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use session::{SessionStatus, TaskStatus};
pub use special_key::{KeyCode, KeyCombo};
pub use sumary::{SessionConfigSummary, SessionSummary};
