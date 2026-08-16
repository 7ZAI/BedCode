//! 终端相关常量

/// 终端背景图片文件名前缀（复制后的文件统一命名为 `<前缀>.<ext>`）
pub const TERMINAL_BG_FILE_PREFIX: &str = "terminal_bg";

/// 支持的终端背景图片扩展名
pub const TERMINAL_BG_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];

/// 背景图片文件大小上限（20MB）
pub const TERMINAL_BG_MAX_BYTES: u64 = 20 * 1024 * 1024;

/// 提交输入行缓冲区的字节上限（每会话）
///
/// 防御性容量限制：正常用户输入（含多行粘贴）远小于此值，
/// 仅在永不提交的异常字节流下才会触及。超限时丢弃前半部分，
/// 保留尾部内容（对日志场景，最近的输入更有价值）
pub const MAX_SUBMITTED_LINE_BUFFER_BYTES: usize = 256 * 1024;
