//! 终端相关常量

/// 默认终端列数
pub const DEFAULT_COLS: u16 = 80;

/// 默认终端行数
pub const DEFAULT_ROWS: u16 = 24;

/// 终端输入超时（秒）
pub const INPUT_TIMEOUT_SECS: u64 = 5;

/// 会话名中 ID 前缀截取长度
///
/// 自动生成会话名时取 session_id 前 N 个字符，如 "Session-a1b2c3d4"
pub const SESSION_NAME_ID_PREFIX_LEN: usize = 8;
