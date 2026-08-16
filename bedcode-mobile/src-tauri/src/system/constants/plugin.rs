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

/// WASM 插件文件扩展名
pub const WASM_FILE_EXT: &str = ".wasm";

/// 插件 manifest 文件名
pub const PLUGIN_MANIFEST_FILE: &str = "plugin.json";

/// 插件来源标记文件名（插件目录内，内容形如 `apk-asset:1.1.11` / `file-install` / `remote-download`）
pub const PLUGIN_SOURCE_MARKER: &str = ".bedcode-source";

/// 来源标记：APK 内置（内容含 app 版本，用于升级刷新）
pub const SOURCE_APK_ASSET: &str = "apk-asset";

/// 来源标记：本地文件安装
pub const SOURCE_FILE_INSTALL: &str = "file-install";

/// 来源标记：远程 URL 安装
pub const SOURCE_REMOTE_DOWNLOAD: &str = "remote-download";

/// APK assets 中内置插件目录
pub const APK_PLUGINS_DIR: &str = "plugins";

/// 插件数据目录（app_data_dir 下）
pub const PLUGIN_DATA_DIR: &str = "plugins";

/// 远程下载临时目录
pub const PLUGIN_DOWNLOAD_TEMP_DIR: &str = "plugins/_download_tmp";

/// 远程下载连接超时（秒）
pub const PLUGIN_DOWNLOAD_CONNECT_TIMEOUT_SECS: u64 = 10;

/// 远程下载读取超时（秒）
pub const PLUGIN_DOWNLOAD_READ_TIMEOUT_SECS: u64 = 60;

/// SHA256 哈希前缀
pub const SHA256_PREFIX: &str = "sha256-";
