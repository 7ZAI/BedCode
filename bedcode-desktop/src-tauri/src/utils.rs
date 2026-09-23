//! Utils Module
//!
//! 工具模块 - 认证、输出解析与加密

pub mod auth;
pub mod crypto;
pub mod parser;
/// 会话动作命令桥接（票 10）：重启 / 移除 / 改名 / 尺寸裁决下沉会话中心插件
pub mod session_action_bridge;
/// 会话创建命令桥接（票 09）：创建编排下沉会话中心插件
pub mod session_create_bridge;
/// 会话窄转发层（会话引擎下沉 P1）：宿主侧调用会话的唯一收口点，见模块文档
pub mod session_gateway;
