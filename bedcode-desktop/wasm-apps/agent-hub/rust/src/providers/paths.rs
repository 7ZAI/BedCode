//! CLI 配置文件路径（家目录相对段）
//!
//! pi `models.json` / `auth.json`、opencode `opencode.json`、claude
//! `settings.json` + 自建桥接文件（provider-config.sh / anthropic-bridge.mjs，
//! 存在任一即视为桥接体系在用）、codex `config.toml`。反向导入与应用共用。

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

pub(crate) fn codex_config_path(home: &str) -> String {
    format!("{home}/.codex/config.toml")
}

pub(crate) fn claude_settings_path(home: &str) -> String {
    format!("{home}/.claude/settings.json")
}

/// claude 视图一次要访问的全部路径（settings + 两桥接文件）：供应商域
/// 入口先批量授权整组（一次弹窗）再逐路径读写——同一业务预见多个文件访问
/// 时用批量授权代替逐个弹窗。宿主 check_batch 对已授权路径静默跳过。
pub(crate) fn claude_auth_paths(home: &str) -> Vec<String> {
    let mut paths = bridge_paths(home).to_vec();
    paths.push(claude_settings_path(home));
    paths
}

/// claude 自建桥接文件（存在任一即视为桥接体系在用）
pub(super) fn bridge_paths(home: &str) -> [String; 2] {
    [
        format!("{home}/.claude/provider-config.sh"),
        format!("{home}/.claude/anthropic-bridge.mjs"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// claude 批量授权路径：settings + 两桥接文件齐备，全部落在 ~/.claude 下
    #[test]
    fn claude_auth_paths_covers_settings_and_bridges() {
        let paths = claude_auth_paths("/home/u");
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().all(|p| p.starts_with("/home/u/.claude/")));
        assert!(paths.iter().any(|p| p.ends_with("settings.json")));
        assert!(paths.iter().any(|p| p.ends_with("provider-config.sh")));
        assert!(paths.iter().any(|p| p.ends_with("anthropic-bridge.mjs")));
    }

    /// codex 配置路径：家目录相对段（应用 / 只读视图共用的唯一真源）
    #[test]
    fn codex_path_under_codex_home() {
        assert_eq!(codex_config_path("/home/u"), "/home/u/.codex/config.toml");
    }
}
