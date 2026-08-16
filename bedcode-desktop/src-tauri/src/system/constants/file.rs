//! 文件操作相关常量

/// 文件树扫描最大递归深度
pub const FILE_TREE_MAX_DEPTH: usize = 20;

/// 文件内容读取上限（字节）
///
/// 超过此大小的文件拒绝读取，防止传输过大文件
pub const FILE_CONTENT_MAX_SIZE_BYTES: u64 = 2 * 1024 * 1024; // 2MB

/// 文件树子节点 HTTP 缓存有效期（秒）
///
/// GET /api/file-tree-children 响应的 Cache-Control max-age 值
/// 30 秒平衡了实时性和性能，用户可通过刷新按钮绕过缓存
pub const FILE_TREE_CHILDREN_CACHE_MAX_AGE_SECS: u32 = 30;
