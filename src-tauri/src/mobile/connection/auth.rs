//! Auth Module - 认证功能
//!
//! 提供设备认证相关功能
//!
//! 注意: 认证逻辑已整合到 pairing.rs 中，此文件作为预留扩展点
//! 目前认证功能通过 pairing 模块提供

pub use super::pairing::PairingModule;