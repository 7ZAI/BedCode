//! 执行 agent 识别与 profile registry
//!
//! 任务模型（ADR-0004）中的两项基础能力：
//! - 命令过滤：以 `/` 开头的提交行属于 CLI 命令而非任务，一刀切过滤，
//!   预留白名单扩展点（未来 `/skills xxx` 等任务型斜杠命令放行）
//! - agent 识别：从会话启动命令检测执行 agent（CLI 级粒度），
//!   通过 AGENT_PROFILES registry 描述每个 agent 的能力（上下文清理命令、
//!   会话集成方式、输入是否作为任务跟踪），新增 agent 只需加一条 profile

/// 会话集成方式：agent 任务状态同步/自动授权的部署载体
///
/// 新增 agent 时在此扩展枚举，并在 hooks.rs 的 ensure/cleanup 分发中
/// 实现对应部署与清理逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIntegration {
    /// Claude Code hooks：`.claude/settings.json` + `auto_task_hook.py`
    ClaudeCodeHooks,
    /// pi 扩展：`.pi/extensions/pi_task_hook.ts`（pi 自动发现，无需注册）
    PiExtension,
    /// opencode 插件：`.opencode/plugins/opencode_task_hook.ts`
    /// （opencode 自动加载项目级插件，无需注册）
    OpenCodePlugin,
    /// Codex hooks：`.codex/hooks.json` + `codex_task_hook.py`
    /// （Codex 从项目 `.codex/` 配置层自动发现 hooks）
    CodexHooks,
    /// 未适配：不部署任何会话集成（无法回传任务状态）
    None,
}

/// Agent profile：CLI 级 agent 能力描述
///
/// 新增 agent 的扩展点（全部在 registry 声明，调度/部署代码零改动）：
/// - clear_command：上下文清理命令，自动任务执行前清理上下文防止超限；
///   None 表示未适配（调度时跳过 clear 直接下发）
/// - session_integration：状态回传的部署载体（hooks / pi 扩展 / 无）
/// - tracks_input：会话输入是否作为任务跟踪（决定 on_input_submitted 是否建任务行）
pub struct AgentProfile {
    /// agent CLI 名称（写入 task_history.agent 字段）
    pub name: &'static str,
    /// 上下文清理命令本体（不含提交符；投递时由调用方按宿主平台拼接 `\r` / `\n`）；
    /// None 表示未适配
    pub clear_command: Option<&'static str>,
    /// 会话集成方式（状态回传载体）
    pub session_integration: SessionIntegration,
    /// 会话输入是否作为任务跟踪（false 时 on_input_submitted 不创建任务行）
    pub tracks_input: bool,
}

/// Agent profile registry
///
/// - claude：完整适配（hooks + /clear）
/// - pi：完整适配（pi 扩展 + /new）—— pi 无 /clear，等效的上下文重建命令是 /new
///   （开启新会话，pi 会话按分支管理，无"清空上下文继续当前会话"的语义）
/// - opencode：完整适配（opencode 插件，状态回传同 pi 扩展机制）。
///   上下文清理命令未适配：opencode 无 /clear，/compact 只压缩不重建，
///   调度时跳过 clear 直接下发（任务行跟踪不受影响）
/// - codex：完整适配（Codex hooks 集成 + /clear）。
///   注意：Codex 项目级 hooks 需用户信任项目 `.codex/` 配置层并在 `/hooks`
///   中确认信任钩子（与 pi 首次 trust 流程同源，宿主无法代答）
pub const AGENT_PROFILES: &[AgentProfile] = &[
    AgentProfile {
        name: "claude",
        clear_command: Some("/clear"),
        session_integration: SessionIntegration::ClaudeCodeHooks,
        tracks_input: true,
    },
    AgentProfile {
        name: "pi",
        clear_command: Some("/new"),
        session_integration: SessionIntegration::PiExtension,
        tracks_input: true,
    },
    AgentProfile {
        name: "codex",
        clear_command: Some("/clear"),
        session_integration: SessionIntegration::CodexHooks,
        tracks_input: true,
    },
    AgentProfile {
        name: "opencode",
        // opencode 无 /clear（/compact 仅压缩上下文），调度跳过 clear 直接下发
        clear_command: None,
        session_integration: SessionIntegration::OpenCodePlugin,
        tracks_input: true,
    },
];

/// 从会话启动命令检测执行 agent（CLI 级粒度）
///
/// 匹配命令中的 CLI 关键词，如 `claude` / `claude.exe` / 完整路径均识别为 claude。
/// 无法识别返回 "unknown"。
///
/// 注意：识别仅用于选择 profile，未知 agent 不拦截会话（profile 查不到时
/// 按未适配处理，调度与部署自动跳过）。
pub fn detect_agent(command: &str) -> &'static str {
    let lower = command.to_lowercase();
    // 按特异性从高到低匹配，避免 "pi" 等短词误命中
    if lower.contains("claude") {
        return "claude";
    }
    if lower.contains("codex") {
        return "codex";
    }
    if lower.contains("opencode") {
        return "opencode";
    }
    // pi 是短词，仅在命令本体为 pi（含路径/扩展名）时匹配，避免误判
    let first_token = lower.split_whitespace().next().unwrap_or("");
    let basename = first_token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_end_matches(".exe");
    if basename == "pi" {
        return "pi";
    }
    "unknown"
}

/// 按名称查找 agent profile；未识别的 agent 返回 None
pub fn profile_for(agent: &str) -> Option<&'static AgentProfile> {
    AGENT_PROFILES.iter().find(|p| p.name == agent)
}

/// 获取指定 agent 的上下文清理命令
///
/// 返回 None 表示该 agent 未适配清理命令（调度时跳过 clear 直接下发）
pub fn clear_command_for(agent: &str) -> Option<&'static str> {
    profile_for(agent).and_then(|p| p.clear_command)
}

/// 获取指定 agent 的会话集成方式（无 profile 视为 None）
pub fn session_integration_for(agent: &str) -> SessionIntegration {
    profile_for(agent)
        .map(|p| p.session_integration)
        .unwrap_or(SessionIntegration::None)
}

/// agent 是否支持完整的自动任务执行（输入建任务行 + 状态回传）
///
/// 调度链依赖状态回传（终态触发下一个任务），仅识别但无集成的 agent
/// （tracks_input=false 或 integration=None）不视为支持，避免任务行
/// 永远卡在 in_progress 导致队列停滞。
pub fn is_supported(agent: &str) -> bool {
    profile_for(agent)
        .map(|p| p.tracks_input && p.session_integration != SessionIntegration::None)
        .unwrap_or(false)
}

/// 返回所有完整适配的 agent name 列表
///
/// 供前端判断工具栏入口可见性、下拉过滤等场景，避免前端 hardcode 白名单。
pub fn list_supported() -> Vec<&'static str> {
    AGENT_PROFILES
        .iter()
        .filter(|p| p.tracks_input && p.session_integration != SessionIntegration::None)
        .map(|p| p.name)
        .collect()
}

/// 任务型斜杠命令白名单（v1 为空，预留扩展点）
///
/// 白名单内的斜杠命令视为任务而非命令，照常创建任务记录。
/// 待 Claude Code 命令集合稳定后（如 `/skills xxx`）再逐项放行。
fn is_whitelisted_command(line: &str) -> bool {
    let _ = line;
    false
}

/// 判断提交输入行是否为命令（而非任务）
///
/// 一刀切规则：去除前导空白后以 `/` 开头即命令；白名单命中则视为任务。
/// 空行不算命令（由调用方的空行过滤处理）。
pub fn is_command_input(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('/') && !is_whitelisted_command(trimmed)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- detect_agent ----------

    #[test]
    fn detect_agent_claude_variants() {
        assert_eq!(detect_agent("claude"), "claude");
        assert_eq!(detect_agent("claude.exe"), "claude");
        assert_eq!(
            detect_agent("C:\\Users\\dev\\AppData\\Roaming\\npm\\claude.cmd"),
            "claude"
        );
        assert_eq!(detect_agent("/usr/local/bin/claude --model opus"), "claude");
        assert_eq!(detect_agent("Claude"), "claude");
    }

    #[test]
    fn detect_agent_other_agents() {
        assert_eq!(detect_agent("codex"), "codex");
        assert_eq!(detect_agent("opencode"), "opencode");
        assert_eq!(detect_agent("/opt/homebrew/bin/pi"), "pi");
        assert_eq!(detect_agent("pi.exe"), "pi");
    }

    #[test]
    fn detect_agent_unknown_and_pi_no_false_positive() {
        assert_eq!(detect_agent("bash"), "unknown");
        assert_eq!(detect_agent("pwsh -NoLogo"), "unknown");
        // "pi" 作为参数出现不应误判（命令本体是 python）
        assert_eq!(detect_agent("python pi_server.py"), "unknown");
    }

    #[test]
    fn detect_agent_claude_wins_over_pi() {
        // claude 优先级高于 pi 短词匹配
        assert_eq!(detect_agent("claude --pi-mode"), "claude");
    }

    // ---------- is_command_input ----------

    #[test]
    fn command_input_slash_prefix() {
        assert!(is_command_input("/clear"));
        assert!(is_command_input("/model opus"));
        assert!(is_command_input("  /compact")); // 前导空白不影响判定
    }

    #[test]
    fn command_input_non_commands() {
        assert!(!is_command_input("修复这个 bug"));
        assert!(!is_command_input("run /clear as part of the task")); // 斜杠不在行首
        assert!(!is_command_input(""));
    }

    // ---------- clear_command_for / profile registry ----------

    #[test]
    fn clear_command_registry() {
        assert_eq!(clear_command_for("claude"), Some("/clear"));
        // pi 无 /clear，上下文重建用 /new（开启新会话）
        assert_eq!(clear_command_for("pi"), Some("/new"));
        // opencode 无 /clear（/compact 仅压缩），未适配清理命令
        assert_eq!(clear_command_for("opencode"), None);
        // codex 有 /clear（清屏 + 开启全新对话，等价 claude /clear）
        assert_eq!(clear_command_for("codex"), Some("/clear"));
        assert_eq!(clear_command_for("unknown"), None);
    }

    #[test]
    fn profile_capabilities() {
        // claude / pi / opencode：完整支持（建任务行 + 状态回传）
        assert!(is_supported("claude"));
        assert!(is_supported("pi"));
        assert!(is_supported("opencode"));
        assert_eq!(
            session_integration_for("claude"),
            super::SessionIntegration::ClaudeCodeHooks
        );
        assert_eq!(
            session_integration_for("pi"),
            super::SessionIntegration::PiExtension
        );
        assert_eq!(
            session_integration_for("opencode"),
            super::SessionIntegration::OpenCodePlugin
        );

        // codex：完整支持（hooks 集成 + /clear + 建任务行）
        assert!(is_supported("codex"));
        assert_eq!(
            session_integration_for("codex"),
            super::SessionIntegration::CodexHooks
        );

        // 未知 agent：全部能力为空
        assert!(!is_supported("unknown"));
        assert_eq!(
            session_integration_for("unknown"),
            super::SessionIntegration::None
        );
        assert!(profile_for("unknown").is_none());
    }
}
