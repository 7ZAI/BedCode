//! Utils Module
//!
//! 工具模块 - 认证、输出解析与加密

pub mod auth;
pub mod crypto;
pub mod parser;
/// 会话窄转发层（会话引擎下沉 P1）：宿主侧调用会话的唯一收口点，见模块文档
pub mod session_gateway;
