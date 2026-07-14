//! Plugin 系统常量

/// 插件启用状态持久化 key 前缀
///
/// 格式: `plugin.enabled.{plugin_id}`，值为 `"true"` / `"false"`
pub const PLUGIN_ENABLED_KEY_PREFIX: &str = "plugin.enabled.";

/// 插件存储文件子目录名
pub const PLUGIN_STORAGE_DIR: &str = "plugins";

/// 插件存储文件扩展名
pub const PLUGIN_STORAGE_EXT: &str = ".json";

/// 插件激活超时（秒）
pub const PLUGIN_ACTIVATE_TIMEOUT_SECS: u64 = 5;

/// 插件前端模块导入超时（毫秒）
pub const PLUGIN_IMPORT_TIMEOUT_MS: u64 = 5000;
