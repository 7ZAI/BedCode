//! 共享常量（宿主与插件共同引用，单一事实来源）
//!
//! 宿主 `system::constants::plugin` re-export 本模块常量；
//! 插件直接引用本模块，不再各自硬编码。

/// Claude Code 配置目录名
pub const CLAUDE_CONFIG_DIR_NAME: &str = ".claude";

/// Claude Code 设置文件名
pub const CLAUDE_SETTINGS_FILE: &str = "settings.json";

/// BedCode Hook 脚本文件名
pub const HOOK_SCRIPT_NAME: &str = "auto_task_hook.py";

/// 环境变量：BedCode 服务器端口
pub const ENV_BEDCODE_PORT: &str = "BEDCODE_PORT";
