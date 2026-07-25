//! 插件系统相关常量

/// 插件回调超时（秒）
///
/// on_startup / on_shutdown 等插件回调的最大执行时间
pub const PLUGIN_CALLBACK_TIMEOUT_SECS: u64 = 5;

/// 插件热重载防抖时间（毫秒）
///
/// 同一插件在防抖窗口内只触发一次重载，避免 cargo build 连续写入多次触发
pub const PLUGIN_RELOAD_DEBOUNCE_MS: u64 = 500;

/// Claude Code 配置目录名
pub const CLAUDE_CONFIG_DIR_NAME: &str = ".claude";

/// Claude Code 设置文件名
pub const CLAUDE_SETTINGS_FILE: &str = "settings.json";

/// BedCode Hook 脚本文件名
pub const HOOK_SCRIPT_NAME: &str = "auto_task_hook.py";

/// 环境变量：BedCode 服务器端口
pub const ENV_BEDCODE_PORT: &str = "BEDCODE_PORT";

/// 环境变量：BedCode 认证 Token
pub const ENV_BEDCODE_TOKEN: &str = "BEDCODE_TOKEN";

/// 环境变量：BedCode PTY 会话 ID
pub const ENV_BEDCODE_SESSION_ID: &str = "BEDCODE_SESSION_ID";
