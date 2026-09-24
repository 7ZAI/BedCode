//! Enums Module
//!
//! 公共枚举类型定义

pub mod auth;
pub mod control;
pub mod plugin;
pub mod pty_status;
pub mod special_key;
pub mod summary;
pub mod sync;

// Re-export all public types
pub use auth::{AuthPayload, AuthStage};
pub use control::{SessionControlAction, SessionControlPayload, TerminalAction, TerminalPayload};
pub use plugin::{PluginQuestion, PluginQuestionOption};
pub use pty_status::PtySessionStatus;
// 会话状态/类型已归位 `protocol::session`（票 08 线协议域）；此处为兼容 re-export
// 保留 `enums::SessionStatus` / `enums::SessionType` 路径，避免破坏既有 import。
// 新增会话 wire 形状一律放 `protocol/`，不再落在本目录。
pub use crate::protocol::session::{SessionStatus, SessionType};
pub use special_key::{KeyCode, KeyCombo};
pub use summary::SessionSummary;
pub use sync::SyncPayload;
