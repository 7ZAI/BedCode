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

/// 插件 UI 事件：任务状态变更（宿主 emit_event / 消息总线与前端 events.on 共用）
pub const EVENT_TASK_STATUS_CHANGED: &str = "task:status-changed";

/// 插件 UI 事件：会话自动授权模式变更
pub const EVENT_SESSION_MODE_CHANGED: &str = "session:mode-changed";

/// 插件 UI 事件：任务队列变更
pub const EVENT_TASK_QUEUE_CHANGED: &str = "task:queue-changed";

/// 插件 UI 事件：定时自动任务变更（创建/触发/状态更新，v6 ADR 0003）
pub const EVENT_TASK_SCHEDULED_CHANGED: &str = "task:scheduled-changed";

/// 插件 UI 事件：预设任务变更（创建/删除/加入队列，仅桌面端，不广播移动端）
pub const EVENT_TASK_PRESET_CHANGED: &str = "task:preset-changed";
