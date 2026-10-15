//! Plugin 系统常量

/// 插件启用状态持久化 key 前缀（应用「已启用 / 可用」偏好）
///
/// 格式: `plugin.enabled.{plugin_id}`，值为 `"true"` / `"false"`。
/// **不再驱动启动流程**：启用只是「这个应用可用」的持久化记录，启动时是否
/// 自动激活只看 [`PLUGIN_AUTOSTART_KEY_PREFIX`]（默认关）。
pub const PLUGIN_ENABLED_KEY_PREFIX: &str = "plugin.enabled.";

/// 插件「启动时自动激活」偏好 key 前缀
///
/// 格式: `plugin.autostart.{plugin_id}`，值为 `"true"` / `"false"`。
/// **缺省即关**：移动端是应用模型——启动只扫描已安装 wasm-app，不启动任何应用；
/// 只有显式把本开关置 `true` 的应用才会在 boot 自动激活，其余等用户在宿主壳里点击。
pub const PLUGIN_AUTOSTART_KEY_PREFIX: &str = "plugin.autostart.";

/// 插件存储文件扩展名
///
/// 已退役常量：插件存储已随 `plugin_storage_db_backed_lock.rs` 迁入主库
/// `plugin_storage` 表，文件落盘面（`PLUGIN_STORAGE_DIR` / `storage_path` /
/// `load_from_disk`）一并删除。本常量保留仅为让该锁能指名它（退役面记账）。
pub const PLUGIN_STORAGE_EXT: &str = ".json";

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
