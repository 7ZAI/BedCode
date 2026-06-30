//! WebSocket Message Types
//!
//! 定义移动端和桌面端之间的通信协议
//! 类型已移至 shared::model::message

// Re-export types from shared model module
pub use crate::shared::model::message::Message;

// Re-export types from shared enums module (payload types)
pub use crate::shared::enums::{
    AuthPayload, AuthStage, SessionControlAction, SessionControlPayload,
    SessionConfigAction, SessionConfigPayload, TerminalAction, TerminalPayload,
    QuickActionSummary, SessionConfigSummary, SessionSummary, KeyCombo,
};

// Re-export types from connection_types module
pub use crate::desktop::server::connection_types::{
    AuthPayload as ConnAuthPayload, AuthStage as ConnAuthStage, DeviceConnectionEvent,
    DeviceConnectionInfo, PairingCodeGeneratedEvent,
};