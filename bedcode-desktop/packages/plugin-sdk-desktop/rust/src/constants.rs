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

/// pi 配置目录名（pi 扩展自动发现根目录）
pub const PI_CONFIG_DIR_NAME: &str = ".pi";

/// pi 扩展目录名（项目级扩展自动发现位置 `.pi/extensions/*.ts`）
pub const PI_EXTENSIONS_DIR_NAME: &str = "extensions";

/// BedCode pi 扩展文件名（状态同步 + 自动授权，随构建打包）
pub const PI_HOOK_SCRIPT_NAME: &str = "pi_task_hook.ts";

/// opencode 配置目录名（插件自动发现根目录）
pub const OPENCODE_CONFIG_DIR_NAME: &str = ".opencode";

/// opencode 插件目录名（项目级插件自动发现位置 `.opencode/plugins/*.ts`）
pub const OPENCODE_PLUGINS_DIR_NAME: &str = "plugins";

/// BedCode opencode 插件文件名（状态同步，随构建打包）
pub const OPENCODE_HOOK_SCRIPT_NAME: &str = "opencode_task_hook.ts";

/// Codex 配置目录名（hooks 自动发现根目录）
pub const CODEX_CONFIG_DIR_NAME: &str = ".codex";

/// Codex hooks 配置文件名（`<repo>/.codex/hooks.json`）
pub const CODEX_HOOKS_FILE: &str = "hooks.json";

/// BedCode Codex hook 脚本文件名（状态同步 + 自动授权，随构建打包）
pub const CODEX_HOOK_SCRIPT_NAME: &str = "codex_task_hook.py";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_constants() {
        // 宿主依赖这些字面量做磁盘路径拼接，改动会导致已装插件找不到资源
        assert_eq!(CLAUDE_CONFIG_DIR_NAME, ".claude");
        assert_eq!(CLAUDE_SETTINGS_FILE, "settings.json");
        assert_eq!(HOOK_SCRIPT_NAME, "auto_task_hook.py");
        assert_eq!(PI_CONFIG_DIR_NAME, ".pi");
        assert_eq!(PI_EXTENSIONS_DIR_NAME, "extensions");
        assert_eq!(PI_HOOK_SCRIPT_NAME, "pi_task_hook.ts");
        assert_eq!(OPENCODE_CONFIG_DIR_NAME, ".opencode");
        assert_eq!(OPENCODE_PLUGINS_DIR_NAME, "plugins");
        assert_eq!(OPENCODE_HOOK_SCRIPT_NAME, "opencode_task_hook.ts");
        assert_eq!(CODEX_CONFIG_DIR_NAME, ".codex");
        assert_eq!(CODEX_HOOKS_FILE, "hooks.json");
        assert_eq!(CODEX_HOOK_SCRIPT_NAME, "codex_task_hook.py");
        assert_eq!(ENV_BEDCODE_PORT, "BEDCODE_PORT");
    }

    #[test]
    fn test_event_constants() {
        // 事件名即消息总线 topic / 前端 events.on 的 key，改动会造成两端失配
        assert_eq!(EVENT_TASK_STATUS_CHANGED, "task:status-changed");
        assert_eq!(EVENT_SESSION_MODE_CHANGED, "session:mode-changed");
        assert_eq!(EVENT_TASK_QUEUE_CHANGED, "task:queue-changed");
        assert_eq!(EVENT_TASK_SCHEDULED_CHANGED, "task:scheduled-changed");
        assert_eq!(EVENT_TASK_PRESET_CHANGED, "task:preset-changed");
    }
}
