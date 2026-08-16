//! 认证相关常量

/// 配对码位数
pub const PAIRING_CODE_DIGITS: usize = 6;

/// 默认设备名称
///
/// 未设置设备名时的 fallback
pub const DEFAULT_DEVICE_NAME: &str = "Mobile Device";

/// 插件 Token 最小合法长度
///
/// Token 长度 >= 此值且纯 ASCII 才视为合法
pub const MIN_PLUGIN_TOKEN_LEN: usize = 16;
