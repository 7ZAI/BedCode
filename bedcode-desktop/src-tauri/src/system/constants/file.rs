//! 文件操作相关常量

/// 文件树扫描最大递归深度
pub const FILE_TREE_MAX_DEPTH: usize = 20;

/// 文件内容读取上限（字节）
///
/// 超过此大小的文件拒绝读取，防止传输过大文件
pub const FILE_CONTENT_MAX_SIZE_BYTES: u64 = 2 * 1024 * 1024; // 2MB
