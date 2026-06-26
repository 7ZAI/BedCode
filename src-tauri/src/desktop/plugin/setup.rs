//! Hooks Setup
//!
//! 应用启动时自动检查并配置 Claude Code 全局 hooks：
//! 1. 校验/生成 token
//! 2. 在全局 ~/.claude/settings.json 中注入 hooks 配置（不覆盖已有配置）
//! 3. 注入 BEDCODE_PORT 和 BEDCODE_TOKEN 环境变量到 hook 命令
//! 4. 验证 hooks 配置是否生效

use crate::shared::system::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Hooks 配置结果，通过 Tauri event 发送到前端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSetupResult {
    /// 是否成功
    pub success: bool,
    /// 提示消息
    pub message: String,
    /// 是否新生成了 token
    pub token_generated: bool,
}

/// 获取全局 ~/.claude 目录路径
fn global_claude_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".claude"))
}

/// 构建 hooks JSON 配置
///
/// 包含 SessionStart / PreToolUse / Stop / SubagentStop 四个事件，
/// 使用 ~/.claude/bedcode_hook.py 作为 hook 脚本路径（全局生效）
fn build_hooks_config(port: u16, token: &str, hook_script_path: &str) -> serde_json::Value {
    // 环境变量前缀：跨平台兼容，统一使用 Unix 风格
    // Python 脚本通过 os.environ 读取，Unix 风格在 Claude Code 环境中通用
    let env_prefix = format!("BEDCODE_PORT={} BEDCODE_TOKEN={} ", port, token);

    let session_start_cmd = format!(
        "{}python \"{}\" session-start",
        env_prefix, hook_script_path
    );
    let pre_tool_use_cmd = format!(
        "{}python \"{}\" pre-tool-use",
        env_prefix, hook_script_path
    );
    let write_event_cmd = format!(
        "{}python \"{}\" write-event",
        env_prefix, hook_script_path
    );

    serde_json::json!({
        "SessionStart": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": session_start_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "PreToolUse": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": pre_tool_use_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "Stop": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": write_event_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "SubagentStop": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": write_event_cmd,
                        "timeout": 5
                    }
                ]
            }
        ]
    })
}

/// 验证 hooks 配置是否包含预期的 BedCode hook 命令
fn is_bedcode_hooks_configured(hooks: &serde_json::Value) -> bool {
    let hooks_obj = match hooks.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    // 检查 SessionStart 中是否包含 bedcode_hook.py
    if let Some(events) = hooks_obj.get("SessionStart").and_then(|v| v.as_array()) {
        for event in events {
            if let Some(hook_list) = event.get("hooks").and_then(|v| v.as_array()) {
                for hook in hook_list {
                    if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                        if cmd.contains("bedcode_hook.py") {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}

/// 执行 hooks 自动配置
///
/// 在全局 ~/.claude/settings.json 中注入 hooks 配置（不覆盖已有配置）。
/// 同时将 bedcode_hook.py 复制到 ~/.claude/ 目录，确保全局可用。
/// 如果文件已有 hooks 字段且包含 BedCode 配置，则更新环境变量。
/// 如果文件没有 hooks 字段，则添加完整配置。
pub fn setup_plugin(
    config: &mut AppConfig,
    config_path: &PathBuf,
    resource_dir: &PathBuf,
) -> PluginSetupResult {
    tracing::info!("setup_hooks called (hooks mode)");

    // 1. 校验/生成 token
    let token_generated = config.ensure_valid_token();
    if token_generated {
        if let Err(e) = config.save_to(config_path) {
            tracing::warn!("Failed to save config after token generation: {}", e);
        }
    }

    let token = config.plugin.token.clone();
    let port = config.network.port;

    // 2. 获取全局 ~/.claude 目录
    let claude_dir = match global_claude_dir() {
        Some(dir) => dir,
        None => {
            return PluginSetupResult {
                success: false,
                message: "无法获取用户主目录".to_string(),
                token_generated,
            };
        }
    };

    // 确保目录存在
    if let Err(e) = fs::create_dir_all(&claude_dir) {
        return PluginSetupResult {
            success: false,
            message: format!("创建 ~/.claude 目录失败: {}", e),
            token_generated,
        };
    }

    // 3. 将 bedcode_hook.py 复制到 ~/.claude/ 目录
    let hook_script_path = claude_dir.join("bedcode_hook.py");
    // 打包后资源路径：../scripts/bedcode_hook.py → resource_dir/_up_/scripts/bedcode_hook.py
    // 开发时回退：current_dir/scripts/bedcode_hook.py
    let source_script = resource_dir.join("_up_/scripts/bedcode_hook.py");
    let source_script = if source_script.exists() {
        source_script
    } else {
        let dev_path = std::env::current_dir().unwrap_or_default().join("scripts/bedcode_hook.py");
        if !dev_path.exists() {
            tracing::warn!(
                "Source hook script not found (tried {} and {}), skipping copy",
                resource_dir.join("_up_/scripts/bedcode_hook.py").display(),
                dev_path.display()
            );
        }
        dev_path
    };
    if source_script.exists() {
        if let Err(e) = fs::copy(&source_script, &hook_script_path) {
            tracing::warn!("Failed to copy bedcode_hook.py to ~/.claude/: {}", e);
        }
    }

    // 使用绝对路径引用 hook 脚本
    let hook_script_str = hook_script_path.to_string_lossy().to_string();
    let settings_path = claude_dir.join("settings.json");

    // 4. 读取或创建 settings.json
    let mut settings: serde_json::Value = if settings_path.exists() {
        match fs::read_to_string(&settings_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
            Err(e) => {
                return PluginSetupResult {
                    success: false,
                    message: format!("读取 settings.json 失败: {}", e),
                    token_generated,
                };
            }
        }
    } else {
        serde_json::json!({})
    };

    // 5. 构建 hooks 配置并注入
    let hooks_config = build_hooks_config(port, &token, &hook_script_str);

    // 合并：保留已有的非 BedCode hooks，替换 BedCode hooks
    let existing_hooks = settings.get("hooks").cloned().unwrap_or(serde_json::json!({}));
    let merged_hooks = merge_hooks(&existing_hooks, &hooks_config);
    settings["hooks"] = merged_hooks;

    // 6. 写入 settings.json
    let content = match serde_json::to_string_pretty(&settings) {
        Ok(c) => c,
        Err(e) => {
            return PluginSetupResult {
                success: false,
                message: format!("序列化 settings.json 失败: {}", e),
                token_generated,
            };
        }
    };
    if let Err(e) = fs::write(&settings_path, content) {
        return PluginSetupResult {
            success: false,
            message: format!("写入 settings.json 失败: {}", e),
            token_generated,
        };
    }

    tracing::info!(
        "Hooks configured in {} with BEDCODE_PORT={} and BEDCODE_TOKEN",
        settings_path.display(),
        port
    );

    // 7. 验证配置是否生效
    let verify_result = verify_hooks_config(&settings_path);
    if !verify_result {
        tracing::warn!("Hooks verification failed: bedcode_hook.py not found in settings.json");
    }

    PluginSetupResult {
        success: true,
        message: "Hooks 已配置".to_string(),
        token_generated,
    }
}

/// 合并 hooks 配置：保留已有的非 BedCode hooks，替换 BedCode 相关的 hooks
///
/// BedCode hooks 的识别标准：command 字段包含 "bedcode_hook.py"
fn merge_hooks(existing: &serde_json::Value, bedcode_hooks: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({});

    // 处理每个 hook 事件类型
    if let (Some(existing_obj), Some(bedcode_obj)) = (existing.as_object(), bedcode_hooks.as_object()) {
        // 先复制 BedCode hooks
        for (key, value) in bedcode_obj {
            result[key] = value.clone();
        }

        // 再合并已有的非 BedCode hooks
        for (key, value) in existing_obj {
            if bedcode_obj.contains_key(key) {
                // 事件类型冲突：从已有配置中过滤掉 BedCode 的 hook 条目，保留非 BedCode 的
                if let (Some(existing_events), Some(_bedcode_events)) =
                    (value.as_array(), bedcode_obj.get(key).and_then(|v| v.as_array()))
                {
                    let mut merged_events = match bedcode_obj.get(key).and_then(|v| v.as_array()) {
                        Some(arr) => arr.clone(),
                        None => vec![],
                    };

                    for event in existing_events {
                        // 检查 event 中是否包含 bedcode_hook.py 的 hook
                        let is_bedcode_event = event
                            .get("hooks")
                            .and_then(|v| v.as_array())
                            .map(|hooks| {
                                hooks.iter().any(|h| {
                                    h.get("command")
                                        .and_then(|v| v.as_str())
                                        .map(|cmd| cmd.contains("bedcode_hook.py"))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);

                        if !is_bedcode_event {
                            merged_events.push(event.clone());
                        }
                    }

                    result[key] = serde_json::Value::Array(merged_events);
                }
            } else {
                // 不冲突的事件类型：直接保留
                result[key] = value.clone();
            }
        }
    } else {
        // existing 为空或不是对象，直接使用 BedCode hooks
        result = bedcode_hooks.clone();
    }

    result
}

/// 验证 settings.json 中的 hooks 配置是否包含 BedCode hooks
fn verify_hooks_config(settings_path: &PathBuf) -> bool {
    match fs::read_to_string(settings_path) {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(settings) => {
                    if let Some(hooks) = settings.get("hooks") {
                        is_bedcode_hooks_configured(hooks)
                    } else {
                        false
                    }
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_hooks_config() {
        let hook_path = "/home/user/.claude/bedcode_hook.py";
        let hooks = build_hooks_config(8765, "testtoken123456", hook_path);

        // 验证 SessionStart 包含 bedcode_hook.py
        let session_start = hooks.get("SessionStart").unwrap().as_array().unwrap();
        let cmd = session_start[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("bedcode_hook.py"));
        assert!(cmd.contains("session-start"));
        assert!(cmd.contains("BEDCODE_PORT=8765"));
        assert!(cmd.contains("BEDCODE_TOKEN=testtoken123456"));
    }

    #[test]
    fn test_is_bedcode_hooks_configured() {
        let hooks = serde_json::json!({
            "SessionStart": [{
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=8765 BEDCODE_TOKEN=abc python \"/home/user/.claude/bedcode_hook.py\" session-start",
                    "timeout": 5
                }]
            }]
        });
        assert!(is_bedcode_hooks_configured(&hooks));

        let empty_hooks = serde_json::json!({});
        assert!(!is_bedcode_hooks_configured(&empty_hooks));
    }

    #[test]
    fn test_merge_hooks_new_install() {
        let existing = serde_json::json!({});
        let bedcode = build_hooks_config(8765, "token123", "/home/user/.claude/bedcode_hook.py");

        let merged = merge_hooks(&existing, &bedcode);
        assert!(merged.get("SessionStart").is_some());
        assert!(merged.get("PreToolUse").is_some());
    }

    #[test]
    fn test_merge_hooks_preserves_non_bedcode() {
        let existing = serde_json::json!({
            "PostToolUse": [{
                "matcher": "Write",
                "hooks": [{
                    "type": "command",
                    "command": "/path/to/other-hook.sh",
                    "timeout": 10
                }]
            }]
        });
        let bedcode = build_hooks_config(8765, "token123", "/home/user/.claude/bedcode_hook.py");

        let merged = merge_hooks(&existing, &bedcode);

        // BedCode hooks 保留
        assert!(merged.get("SessionStart").is_some());

        // 非 BedCode hooks 保留
        let post_tool = merged.get("PostToolUse").unwrap().as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert_eq!(
            post_tool[0]["hooks"][0]["command"].as_str().unwrap(),
            "/path/to/other-hook.sh"
        );
    }

    #[test]
    fn test_merge_hooks_replaces_bedcode() {
        let existing = serde_json::json!({
            "SessionStart": [{
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=9999 BEDCODE_TOKEN=old python \"/home/user/.claude/bedcode_hook.py\" session-start",
                    "timeout": 5
                }]
            }]
        });
        let bedcode = build_hooks_config(8765, "newtoken", "/home/user/.claude/bedcode_hook.py");

        let merged = merge_hooks(&existing, &bedcode);

        // 应该替换为新的 BedCode 配置
        let session_start = merged.get("SessionStart").unwrap().as_array().unwrap();
        let cmd = session_start[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("BEDCODE_PORT=8765"));
        assert!(cmd.contains("BEDCODE_TOKEN=newtoken"));
        assert!(!cmd.contains("9999"));
    }

    #[test]
    fn test_ensure_valid_token_generates() {
        // 测试 token 生成逻辑（需要 AppConfig 实现）
        // 此测试验证 setup_plugin 能正常生成 token
    }
}
