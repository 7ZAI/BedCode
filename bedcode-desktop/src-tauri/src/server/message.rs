//! WebSocket Message Types
//!
//! 定义移动端和桌面端之间的通信协议

// Re-export types from ws::message module
pub use crate::server::ws::message::Message;

// Re-export types from shared enums module (payload types)
pub use crate::enums::{
    AuthPayload, AuthStage, SessionControlAction, SessionControlPayload,
    SessionConfigAction, SessionConfigPayload, TerminalAction, TerminalPayload,
    QuickActionSummary, SessionConfigSummary, SessionSummary, KeyCombo,
};

// Re-export types from connection_types module
pub use crate::server::connection_types::{
    AuthPayload as ConnAuthPayload, AuthStage as ConnAuthStage, DeviceConnectionEvent,
    DeviceConnectionInfo, PairingCodeGeneratedEvent,
};