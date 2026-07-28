//! Hooks 配置管理
//!
//! 管理 Claude Code hooks 配置：
//! - ensure_project_hooks() — 会话启动前为项目配置 hooks
//! - cleanup_project_hooks() — 清理指定项目的插件 hooks
//! - cleanup_global_hooks() — 清理旧版全局 hooks

use bedcode_plugin_api::constants::{CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE, HOOK_SCRIPT_NAME};
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostFs, HostLog, HostSession};
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
    resource_dir: &str,
) -> ProjectHooksResult {
    let claude_dir = format!("{}/{}", working_dir, CLAUDE_CONFIG_DIR_NAME);
    let settings_path = format!("{}/{}", claude_dir, CLAUDE_SETTINGS_FILE);

    // 1. 读取现有 settings.json
    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Ok(Some(content)) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    // 检查项目是否已有插件 hooks 且端口匹配
    let needs_update = match settings.get("hooks") {
        Some(hooks) if is_plugin_hooks_configured(hooks) => {
            // hooks 存在，但需要验证端口是否与当前值匹配
            !is_hooks_port_matching(hooks, port)
        }
        _ => true,
    };

    if !needs_update {
        host.log_info("Project already has plugin hooks with matching config, skipping");
        return ProjectHooksResult {
            success: true,
            message: "项目已配置插件 hooks 且配置匹配".to_string(),
            skipped: true,
        };
    }

    host.log_info(&format!("Updating plugin hooks (port={})", port));

    // 2. 复制 hook 脚本到项目 .claude/ 目录
    let hook_script_path = format!("{}/{}", claude_dir, HOOK_SCRIPT_NAME);
    let source_script = format!("{}/{}", resource_dir, HOOK_SCRIPT_NAME);
    if let Err(e) = host.fs_copy(&source_script, &hook_script_path) {
        host.log_warn(&format!("Failed to copy {} to project {}: {}", HOOK_SCRIPT_NAME, CLAUDE_CONFIG_DIR_NAME, e));
    }

    // 3. 构建 hooks 配置并写入项目 settings.json
    let hooks_config = build_hooks_config(port, &hook_script_path);

    // 合并 hooks：保留非插件 hooks，添加插件 hooks
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
    if let Err(e) = host.fs_write(&settings_path, &content) {
        return ProjectHooksResult {
            success: false,
            message: format!("写入项目 settings.json 失败: {}", e),
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

/// 清理所有项目的插件 hooks
///
/// 遍历所有会话配置，对每个配置的 working_dir 调用 cleanup_project_hooks()。
/// 同时清理全局 hooks。用于插件禁用（deactivate）和应用关闭（on_shutdown）时
/// 确保所有残留的 hooks 配置被移除。
///
/// # Returns
/// 清理结果摘要：清理了多少个项目、跳过了多少、失败了多少
pub fn cleanup_all_project_hooks(host: &WasmHost) -> AllProjectHooksResult {
    let mut result = AllProjectHooksResult::default();

    // 1. 清理全局 hooks
    cleanup_global_hooks(host);
    host.log_info("cleanup_all_project_hooks: global hooks cleaned");

    // 2. 获取所有会话配置
    let configs = match host.session_config_list() {
        Ok(Some(value)) => value,
        _ => {
            host.log_error("cleanup_all_project_hooks: failed to get session config list");
            return result;
        }
    };

    let config_arr = match configs.as_array() {
        Some(arr) => arr,
        None => {
            host.log_error("cleanup_all_project_hooks: session config list is not an array");
            return result;
        }
    };

    host.log_info(&format!(
        "cleanup_all_project_hooks: checking {} session config(s)",
        config_arr.len()
    ));

    // 3. 遍历所有配置，清理每个项目的 hooks
    for config in config_arr {
        let working_dir = config.get("workingDir")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if working_dir.is_empty() {
            continue;
        }

        let cleanup_result = cleanup_project_hooks(host, working_dir);

        if cleanup_result.skipped {
            result.skipped += 1;
        } else if cleanup_result.success {
            result.cleaned += 1;
            host.log_info(&format!(
                "cleanup_all_project_hooks: cleaned hooks for {}",
                working_dir
            ));
        } else {
            result.failed += 1;
            host.log_warn(&format!(
                "cleanup_all_project_hooks: failed to clean hooks for {}: {}",
                working_dir, cleanup_result.message
            ));
        }
    }

    host.log_info(&format!(
        "cleanup_all_project_hooks: done (cleaned={}, skipped={}, failed={})",
        result.cleaned, result.skipped, result.failed
    ));

    result
}

/// 所有项目 hooks 清理结果摘要
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AllProjectHooksResult {
    /// 成功清理的项目数
    pub cleaned: usize,
    /// 跳过的项目数（无 hooks 或无需清理）
    pub skipped: usize,
    /// 清理失败的项目数
    pub failed: usize,
}

/// 清理指定项目的插件 hooks
pub fn cleanup_project_hooks(host: &WasmHost, working_dir: &str) -> ProjectHooksResult {
    let settings_path = format!("{}/{}/{}", working_dir, CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE);

    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Ok(Some(content)) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        _ => {
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

    if !is_plugin_hooks_configured(hooks) {
        return ProjectHooksResult {
            success: true,
            message: "项目无插件 hooks".to_string(),
            skipped: true,
        };
    }

    let cleaned_hooks = remove_plugin_hooks(hooks);

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

    match host.fs_write(&settings_path, &content) {
        Ok(()) => {
            host.log_info(&format!("Cleaned plugin hooks from project: {}", working_dir));
            ProjectHooksResult {
                success: true,
                message: "项目插件 hooks 已清理".to_string(),
                skipped: false,
            }
        }
        Err(e) => ProjectHooksResult {
            success: false,
            message: format!("写入 settings.json 失败: {}", e),
            skipped: false,
        },
    }
}

/// 清理全局 ~/.claude/settings.json 中的插件 hooks
pub fn cleanup_global_hooks(host: &WasmHost) {
    let home_dir = match host.config_get(ConfigKey::HomeDir) {
        Ok(Some(d)) => d,
        _ => {
            host.log_warn("cleanup_global_hooks: home_dir not available");
            return;
        }
    };

    let settings_path = format!("{}/{}/{}", home_dir, CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE);

    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Ok(Some(content)) => match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(_) => return,
        },
        _ => return,
    };

    let hooks = match settings.get("hooks") {
        Some(h) => h,
        None => return,
    };

    if !is_plugin_hooks_configured(hooks) {
        return;
    }

    let cleaned_hooks = remove_plugin_hooks(hooks);

    if cleaned_hooks.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        settings.as_object_mut().map(|o| o.remove("hooks"));
    } else {
        settings["hooks"] = cleaned_hooks;
    }

    if let Ok(content) = serde_json::to_string_pretty(&settings) {
        match host.fs_write(&settings_path, &content) {
            Ok(()) => host.log_info("Cleaned up plugin hooks from global settings.json"),
            Err(e) => host.log_warn(&format!("Failed to clean global settings.json: {}", e)),
        }
    }
}

/// 构建 hooks JSON 配置
///
/// 注册所有 Claude Code hook 事件，覆盖完整的状态机生命周期：
/// SessionStart → UserPromptSubmit → PreToolUse → PostToolUse/PostToolUseFailure
/// → Notification → Stop/SubagentStop → SessionEnd
fn build_hooks_config(port: u16, hook_script_path: &str) -> serde_json::Value {
    // 环境变量前缀：端口（脚本仅在 BedCode 注入 BEDCODE_SESSION_ID 的 PTY 中生效）
    let env_prefix = format!("BEDCODE_PORT={} ", port);

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

/// 检查 hooks 配置是否包含插件 hook 命令
fn is_plugin_hooks_configured(hooks: &serde_json::Value) -> bool {
    let hooks_obj = match hooks.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    // 检查任意事件类型中是否包含 auto_task_hook.py
    for (_event_type, events) in hooks_obj {
        if let Some(events_arr) = events.as_array() {
            for event in events_arr {
                if let Some(hook_list) = event.get("hooks").and_then(|v| v.as_array()) {
                    for hook in hook_list {
                        if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                            if cmd.contains("auto_task_hook.py") {
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

/// 移除所有插件相关的 hook 条目
fn remove_plugin_hooks(hooks: &serde_json::Value) -> serde_json::Value {
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
                                        .map(|cmd| !cmd.contains("auto_task_hook.py"))
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

/// 合并 hooks 配置：保留非插件 hooks，替换插件相关的 hooks
fn merge_hooks(existing: &serde_json::Value, plugin_hooks: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({});

    if let (Some(existing_obj), Some(plugin_obj)) = (existing.as_object(), plugin_hooks.as_object()) {
        // 先放入插件 hooks
        for (key, value) in plugin_obj {
            result[key] = value.clone();
        }

        // 合并已有 hooks：插件事件类型追加非插件条目，非插件事件类型直接保留
        for (key, value) in existing_obj {
            if plugin_obj.contains_key(key) {
                if let Some(existing_events) = value.as_array() {
                    let mut merged_events = match plugin_obj.get(key).and_then(|v| v.as_array()) {
                        Some(arr) => arr.clone(),
                        None => vec![],
                    };

                    for event in existing_events {
                        let is_plugin_event = event
                            .get("hooks")
                            .and_then(|v| v.as_array())
                            .map(|hooks| {
                                hooks.iter().any(|h| {
                                    h.get("command")
                                        .and_then(|v| v.as_str())
                                        .map(|cmd| cmd.contains("auto_task_hook.py"))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);

                        if !is_plugin_event {
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
        result = plugin_hooks.clone();
    }

    result
}

/// 检查现有 hooks 中的端口是否与当前值匹配
///
/// 环境变量前缀格式：BEDCODE_PORT={port}
/// 解析 hook command 中的环境变量，与当前 port 比较
fn is_hooks_port_matching(hooks: &serde_json::Value, port: u16) -> bool {
    let expected_prefix = format!("BEDCODE_PORT={} ", port);

    let hooks_obj = match hooks.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    for (_event_type, events) in hooks_obj {
        if let Some(events_arr) = events.as_array() {
            for event in events_arr {
                if let Some(hook_list) = event.get("hooks").and_then(|v| v.as_array()) {
                    for hook in hook_list {
                        if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                            if cmd.contains("auto_task_hook.py") {
                                // 检查命令中的环境变量前缀是否匹配
                                if !cmd.starts_with(&expected_prefix) {
                                    return false;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    true
}
