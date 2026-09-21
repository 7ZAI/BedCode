//! Utils Module
//!
//! 工具模块 - 认证、输出解析与加密

pub mod auth;
pub mod crypto;
pub mod parser;
/// 会话配置命令桥接（票 08）：插件真源 ↔ 主库投影
pub mod session_config_bridge;
/// 会话创建命令桥接（票 09）：创建编排下沉会话中心插件
pub mod session_create_bridge;
/// 会话动作命令桥接（票 10）：重启 / 移除 / 改名 / 尺寸裁决下沉会话中心插件
pub mod session_action_bridge;
