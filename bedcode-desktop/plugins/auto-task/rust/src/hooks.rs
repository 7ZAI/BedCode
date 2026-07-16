//! Hooks 配置管理
//!
//! 管理 Claude Code hooks 配置：
//! - ensure_project_hooks() — 会话启动前为项目配置 hooks
//! - cleanup_project_hooks() — 清理指定项目的 BedCode hooks
//! - cleanup_global_hooks() — 清理旧版全局 hooks

use bedcode_plugin_api::wasm_host::WasmHost;
use serde::{Deserialize, Serialize};

/// 项目级 Hooks 配置结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHooksResult {
    pub success: bool,
    pub message: String,
    pub skipped: bool,
}

/// 为项目配置 Claude Code hooks
///
/// 在会话启动前调用，仅在项目目录下的 `.claude/settings.json` 中配置 hooks。
pub fn ensure_project_hooks(
    host: &WasmHost,
    working_dir: &str,
    port: u16,
    token: &str,
    resource_dir: &str,
) -> ProjectHooksResult {
    let claude_dir_name = ".claude";
    let settings_file = "settings.json";
    let hook_script_name = "bedcode_hook.py";

    let project_path = working_dir.to_string();
    let claude_dir = format!("{}/{}", project_path, claude_dir_name);
    let settings_path = format!("{}/{}", claude_dir, settings_file);

    // 1. 读取现有 settings.json
    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Some(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
    };

    // 检查项目是否已有 BedCode hooks
    let needs_update = match settings.get("hooks") {
        Some(hooks) if is_bedcode_hooks_configured(hooks) => false,
        _ => true,
    };

    if !needs_update {
        host.log_info("Project already has BedCode hooks, skipping");
        return ProjectHooksResult {
            success: true,
            message: "项目已配置 BedCode hooks".to_string(),
            skipped: true,
        };
    }

    // 2. 复制 hook 脚本到项目 .claude/ 目录
    let hook_script_path = format!("{}/{}", claude_dir, hook_script_name);
    let source_script = format!("{}/{}", resource_dir, hook_script_name);
    if !host.fs_copy(&source_script, &hook_script_path) {
        host.log_warn(&format!("Failed to copy {} to project {}", hook_script_name, claude_dir_name));
    }

    // 3. 构建 hooks 配置并写入项目 settings.json
    let hooks_config = build_hooks_config(port, token, &hook_script_path);

    // 合并 hooks：保留非 BedCode hooks，添加 BedCode hooks
    let existing_hooks = settings.get("hooks").cloned().unwrap_or(serde_json::json!({}));
    let merged_hooks = merge_hooks(&existing_hooks, &hooks_config);
    settings["hooks"] = merged_hooks;

    // 写入
    let content = match serde_json::to_string_pretty(&settings) {
        Ok(c) => c,
        Err(e) => {
            host.log_error(&format!("Failed to serialize settings.json: {}", e));
            return ProjectHooksResult {
                success: false,
                message: format!("序列化 settings.json 失败: {}", e),
                skipped: false,
            };
        }
    };
    if !host.fs_write(&settings_path, &content) {
        return ProjectHooksResult {
            success: false,
            message: "写入项目 settings.json 失败".to_string(),
            skipped: false,
        };
    }

    host.log_info(&format!("Project hooks configured in {}", settings_path));

    ProjectHooksResult {
        success: true,
        message: "项目 Hooks 已配置".to_string(),
        skipped: false,
    }
}

/// 清理指定项目的 BedCode hooks
pub fn cleanup_project_hooks(host: &WasmHost, working_dir: &str) -> ProjectHooksResult {
    let settings_path = format!("{}/.claude/settings.json", working_dir);

    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Some(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        None => {
            return ProjectHooksResult {
                success: true,
                message: "项目无 .claude/settings.json，无需清理".to_string(),
                skipped: true,
            };
        }
    };

    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => {
            return ProjectHooksResult {
                success: true,
                message: "项目无 hooks 配置".to_string(),
                skipped: true,
            };
        }
    };

    if !is_bedcode_hooks_configured(hooks) {
        return ProjectHooksResult {
            success: true,
            message: "项目无 BedCode hooks".to_string(),
            skipped: true,
        };
    }

    let cleaned_hooks = remove_bedcode_hooks(hooks);

    if cleaned_hooks.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    } else {
        settings["hooks"] = cleaned_hooks;
    }

    let content = match serde_json::to_string_pretty(&settings) {
        Ok(c) => c,
        Err(e) => {
            return ProjectHooksResult {
                success: false,
                message: format!("序列化 settings.json 失败: {}", e),
                skipped: false,
            };
        }
    };

    if host.fs_write(&settings_path, &content) {
        host.log_info(&format!("Cleaned BedCode hooks from project: {}", working_dir));
        ProjectHooksResult {
            success: true,
            message: "项目 BedCode hooks 已清理".to_string(),
            skipped: false,
        }
    } else {
        ProjectHooksResult {
            success: false,
            message: "写入 settings.json 失败".to_string(),
            skipped: false,
        }
    }
}

/// 清理全局 ~/.claude/settings.json 中的 BedCode hooks
pub fn cleanup_global_hooks(host: &WasmHost) {
    let home_dir = match host.config_get("home_dir") {
        Some(d) => d,
        None => {
            host.log_warn("cleanup_global_hooks: home_dir not available");
            return;
        }
    };

    let settings_path = format!("{}/.claude/settings.json", home_dir);

    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Some(content) => match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(_) => return,
        },
        None => return,
    };

    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => return,
    };

    if !is_bedcode_hooks_configured(hooks) {
        return;
    }

    let cleaned_hooks = remove_bedcode_hooks(hooks);

    if cleaned_hooks.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    } else {
        settings["hooks"] = cleaned_hooks;
    }

    if let Ok(content) = serde_json::to_string_pretty(&settings) {
        if host.fs_write(&settings_path, &content) {
            host.log_info("Cleaned up BedCode hooks from global settings.json");
        }
    }
}

/// 构建 hooks JSON 配置
///
/// 注册所有 Claude Code hook 事件，覆盖完整的状态机生命周期：
/// SessionStart → UserPromptSubmit → PreToolUse → PostToolUse/PostToolUseFailure
/// → Notification → Stop/SubagentStop → SessionEnd
fn build_hooks_config(port: u16, token: &str, hook_script_path: &str) -> serde_json::Value {
    // 环境变量前缀：端口、token、BedCode PTY 会话 ID
    let env_prefix = format!("BEDCODE_PORT={} BEDCODE_TOKEN={} ", port, token);

    let session_start_cmd = format!(
        "{}python \"{}\" session-start",
        env_prefix, hook_script_path
    );
    let user_prompt_submit_cmd = format!(
        "{}python \"{}\" user-prompt-submit",
        env_prefix, hook_script_path
    );
    let pre_tool_use_cmd = format!(
        "{}python \"{}\" pre-tool-use",
        env_prefix, hook_script_path
    );
    let post_tool_use_cmd = format!(
        "{}python \"{}\" post-tool-use",
        env_prefix, hook_script_path
    );
    let post_tool_use_fail_cmd = format!(
        "{}python \"{}\" post-tool-use-fail",
        env_prefix, hook_script_path
    );
    let notification_cmd = format!(
        "{}python \"{}\" notification",
        env_prefix, hook_script_path
    );
    let stop_cmd = format!(
        "{}python \"{}\" stop",
        env_prefix, hook_script_path
    );
    let subagent_stop_cmd = format!(
        "{}python \"{}\" subagent-stop",
        env_prefix, hook_script_path
    );
    let session_end_cmd = format!(
        "{}python \"{}\" session-end",
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
        "UserPromptSubmit": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": user_prompt_submit_cmd,
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
        "PostToolUse": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": post_tool_use_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "PostToolUseFailure": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": post_tool_use_fail_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "Notification": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": notification_cmd,
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
                        "command": stop_cmd,
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
                        "command": subagent_stop_cmd,
                        "timeout": 5
                    }
                ]
            }
        ],
        "SessionEnd": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": session_end_cmd,
                        "timeout": 5
                    }
                ]
            }
        ]
    })
}

/// 检查 hooks 配置是否包含 BedCode hook 命令
fn is_bedcode_hooks_configured(hooks: &serde_json::Value) -> bool {
    let hooks_obj = match hooks.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    // 检查任意事件类型中是否包含 bedcode_hook.py
    for (_event_type, events) in hooks_obj {
        if let Some(events_arr) = events.as_array() {
            for event in events_arr {
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
    }

    false
}

/// 移除所有 BedCode 相关的 hook 条目
fn remove_bedcode_hooks(hooks: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({});

    if let Some(hooks_obj) = hooks.as_object() {
        for (event_type, events) in hooks_obj {
            if let Some(events_arr) = events.as_array() {
                let filtered: Vec<serde_json::Value> = events_arr
                    .iter()
                    .filter(|event| {
                        event
                            .get("hooks")
                            .and_then(|v| v.as_array())
                            .map(|hook_list| {
                                hook_list.iter().all(|h| {
                                    h.get("command")
                                        .and_then(|v| v.as_str())
                                        .map(|cmd| !cmd.contains("bedcode_hook.py"))
                                        .unwrap_or(true)
                                })
                            })
                            .unwrap_or(true)
                    })
                    .cloned()
                    .collect();

                if !filtered.is_empty() {
                    result[event_type] = serde_json::Value::Array(filtered);
                }
            }
        }
    }

    result
}

/// 合并 hooks 配置：保留非 BedCode hooks，替换 BedCode 相关的 hooks
fn merge_hooks(existing: &serde_json::Value, bedcode_hooks: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({});

    if let (Some(existing_obj), Some(bedcode_obj)) = (existing.as_object(), bedcode_hooks.as_object()) {
        // 先放入 BedCode hooks
        for (key, value) in bedcode_obj {
            result[key] = value.clone();
        }

        // 合并已有 hooks：BedCode 事件类型追加非 BedCode 条目，非 BedCode 事件类型直接保留
        for (key, value) in existing_obj {
            if bedcode_obj.contains_key(key) {
                if let Some(existing_events) = value.as_array() {
                    let mut merged_events = match bedcode_obj.get(key).and_then(|v| v.as_array()) {
                        Some(arr) => arr.clone(),
                        None => vec![],
                    };

                    for event in existing_events {
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
                result[key] = value.clone();
            }
        }
    } else {
        result = bedcode_hooks.clone();
    }

    result
}
