//! 执行 agent 识别与 profile registry
//!
//! 任务模型（ADR-0004）中的两项基础能力：
//! - 命令过滤：以 `/` 开头的提交行属于 CLI 命令而非任务，一刀切过滤，
//!   预留白名单扩展点（未来 `/skills xxx` 等任务型斜杠命令放行）
//! - agent 识别：从会话启动命令检测执行 agent（CLI 级粒度），
//!   本期只适配 Claude Code，codex/opencode/pi 留 registry 扩展点

/// Agent profile：CLI 级 agent 能力描述
///
/// 上下文清理命令（clear_command）是核心扩展点：
/// 自动任务执行前需清理上下文防止超限，不同 agent 的清理方式不同
/// （Claude Code 为 `/clear`，其他 agent 待适配时补充）
pub struct AgentProfile {
    /// agent CLI 名称（写入 task_history.agent 字段）
    pub name: &'static str,
    /// 上下文清理命令本体（不含提交符；投递时由调用方按宿主平台拼接 `\r` / `\n`）；None 表示未适配
    pub clear_command: Option<&'static str>,
}

/// Agent profile registry（本期仅 claude 有完整 profile）
pub const AGENT_PROFILES: &[AgentProfile] = &[
    AgentProfile {
        name: "claude",
        clear_command: Some("/clear"),
    },
    AgentProfile {
        name: "codex",
        clear_command: None,
    },
    AgentProfile {
        name: "opencode",
        clear_command: None,
    },
    AgentProfile {
        name: "pi",
        clear_command: None,
    },
];

/// 从会话启动命令检测执行 agent（CLI 级粒度）
///
/// 匹配命令中的 CLI 关键词，如 `claude` / `claude.exe` / 完整路径均识别为 claude。
/// 无法识别返回 "unknown"。
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

/// 获取指定 agent 的上下文清理命令
///
/// 返回 None 表示该 agent 未适配清理命令（不应进入自动任务调度）
pub fn clear_command_for(agent: &str) -> Option<&'static str> {
    AGENT_PROFILES
        .iter()
        .find(|p| p.name == agent)
        .and_then(|p| p.clear_command)
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
        assert_eq!(detect_agent("C:\\Users\\dev\\AppData\\Roaming\\npm\\claude.cmd"), "claude");
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

    // ---------- clear_command_for ----------

    #[test]
    fn clear_command_registry() {
        assert_eq!(clear_command_for("claude"), Some("/clear"));
        assert_eq!(clear_command_for("codex"), None);
        assert_eq!(clear_command_for("unknown"), None);
    }
}
