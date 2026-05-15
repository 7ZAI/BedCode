//! WebSocket Message Types
//!
//! 定义移动端和桌面端之间的通信协议
//! 类型已移至 shared::message_types

// Re-export types from shared message_types module
pub use crate::shared::message_types::types::{
    AuthPayload, AuthStage, ControlAction, ControlPayload, InputPayload, Message,
    OutputPayload, QuickActionSummary, SessionConfigSummary, SessionSummary, SpecialKey,
};

// Re-export types from connection_types module
pub use crate::desktop::server::connection_types::{
    AuthPayload as ConnAuthPayload, AuthStage as ConnAuthStage, DeviceConnectionEvent,
    DeviceConnectionInfo, PairingCodeGeneratedEvent,
};

// 注意：connection_types 中的 AuthPayload/AuthStage 与 enum/types 中的相同
// 为了兼容，保留 connection_types 中的定义，但使用 enum_mod 中的版本作为主版本