//! Utils Module
//!
//! 工具模块 - 认证、加密 + 会话互调窄转发

pub mod auth;
pub mod crypto;
/// 会话窄转发层（会话引擎下沉 P1）：宿主侧调用会话的唯一收口点，见模块文档
pub mod session_gateway;
