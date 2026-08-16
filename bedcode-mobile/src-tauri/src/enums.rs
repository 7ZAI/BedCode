//! Enums Module
//!
//! 公共枚举类型定义

pub mod auth;
pub mod control;
pub mod file_service;
pub mod plugin;
pub mod session;
pub mod special_key;
pub mod sumary;
pub mod sync;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload, SessionConfigAction, SessionConfigPayload, TerminalAction, TerminalPayload, SubscribeMode};
pub use file_service::{FileServicePayload, MountAnnouncement};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use session::{SessionStatus, TaskStatus};
pub use special_key::{KeyCombo, KeyCode};
pub use sumary::{QuickActionSummary, SessionConfigSummary, SessionSummary};
pub use sync::SyncPayload;
