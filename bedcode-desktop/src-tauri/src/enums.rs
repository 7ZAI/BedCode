//! Enums Module
//!
//! 公共枚举类型定义

pub mod auth;
pub mod control;
pub mod file_service;
pub mod plugin;
pub mod pty_status;
pub mod session;
pub mod shell;
pub mod special_key;
pub mod summary;
pub mod sync;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload, SessionConfigAction, SessionConfigPayload, TerminalAction, TerminalPayload, SubscribeMode};
pub use file_service::{FileServicePayload, MountAnnouncement};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use pty_status::PtySessionStatus;
pub use session::{SessionStatus, SessionType, TaskStatus};
pub use shell::{ExecutionEnvironment, SessionLaunchConfig, WindowsShell};
pub use special_key::{KeyCombo, KeyCode};
pub use summary::{QuickActionSummary, SessionConfigSummary, SessionSummary};
pub use sync::SyncPayload;
