//! CLI 配置文件路径（家目录相对段）
//!
//! pi `models.json` / `auth.json`、opencode `opencode.json`、claude
//! `settings.json` + 自建桥接文件（provider-config.sh / anthropic-bridge.mjs，
//! 存在任一即视为桥接体系在用）。反向导入与应用共用。

// ==================== 路径（家目录相对段） ====================

pub(crate) fn pi_models_path(home: &str) -> String {
    format!("{home}/.pi/agent/models.json")
}

pub(crate) fn pi_auth_path(home: &str) -> String {
    format!("{home}/.pi/agent/auth.json")
}

pub(crate) fn opencode_cfg_path(home: &str) -> String {
    format!("{home}/.config/opencode/opencode.json")
}

pub(crate) fn claude_settings_path(home: &str) -> String {
    format!("{home}/.claude/settings.json")
}

/// claude 自建桥接文件（存在任一即视为桥接体系在用）
pub(super) fn bridge_paths(home: &str) -> [String; 2] {
    [
        format!("{home}/.claude/provider-config.sh"),
        format!("{home}/.claude/anthropic-bridge.mjs"),
    ]
}
