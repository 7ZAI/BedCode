//! Hooks Setup
//!
//! 管理 Claude Code hooks 配置：
//! 1. ensure_token() — 应用启动时校验/生成 token
//! 2. cleanup_global_hooks() — 清理旧版全局 hooks（迁移到项目级后不再需要）
//! 3. ensure_project_hooks() — 会话启动前为项目配置 hooks（项目级作用域）

use crate::system::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Token 配置结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSetupResult {
    /// 是否成功
    pub success: bool,
    /// 提示消息
    pub message: String,
    /// 是否新生成了 token
    pub token_generated: bool,
}

/// 项目级 Hooks 配置结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHooksResult {
    /// 是否成功
    pub success: bool,
    /// 提示消息
    pub message: String,
    /// 是否跳过（非 Claude 命令或已配置）
    pub skipped: bool,
}

/// 获取全局 ~/.claude 目录路径
fn global_claude_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|d| d.join(".claude"))
}

/// 确保 plugin token 合法，不合法则生成新 token
///
/// 仅处理 token 生成/校验，不再写入全局 hooks 配置。
/// 当前已不再需要校验/生成 token，逻辑已注释保留。
pub fn ensure_token(config: &mut AppConfig, config_path: &PathBuf) -> TokenSetupResult {
    tracing::info!("ensure_token called (token validation skipped)");

    // // 校验/生成 token 逻辑已禁用，不再需要
    // let token_generated = config.ensure_valid_token();
    // if token_generated {
    //     if let Err(e) = config.save_to(config_path) {
    //         return TokenSetupResult {
    //             success: false,
    //             message: format!("Token 生成后保存配置失败: {}", e),
    //             token_generated,
    //         };
    //     }
    // }

    TokenSetupResult {
        success: true,
        // if token_generated {
        //     "Token 已生成".to_string()
        // } else {
        //     "Token 已验证".to_string()
        // },
        message: "Token 校验已跳过".to_string(),
        token_generated: false,
    }
}

/// 清理全局 ~/.claude/settings.json 中的 BedCode hooks
///
/// 迁移到项目级 hooks 后，全局 hooks 不再需要。
/// 此函数在应用启动时调用一次，移除旧的全局 BedCode hooks 配置。
/// 保留所有非 BedCode 的 hooks 和其他设置。
/// 幂等操作：如果已经清理过，则为 no-op。
pub fn cleanup_global_hooks() {
    tracing::info!("cleanup_global_hooks called");

    let claude_dir = match global_claude_dir() {
        Some(dir) => dir,
        None => return,
    };

    let settings_path = claude_dir.join("settings.json");

    let mut settings: serde_json::Value = match fs::read_to_string(&settings_path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(_) => return,
        },
        Err(_) => return,
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
        if let Err(e) = fs::write(&settings_path, content) {
            tracing::warn!("Failed to write cleaned settings.json: {}", e);
        } else {
            tracing::info!("Cleaned up BedCode hooks from global settings.json");
        }
    }
}

/// 从 hooks 配置中移除所有 BedCode 相关的 hook 条目
fn remove_bedcode_hooks(hooks: &serde_json::Value) -> serde_json::Value {
    let mut result = serde_json::json!({});

    if let Some(hooks_obj) = hooks.as_object() {
        for (event_type, events) in hooks_obj {
            if let Some(events_arr) = events.as_array() {
                // 过滤掉包含 bedcode_hook.py 的 event 条目
                let filtered: Vec<serde_json::Value> = events_arr
                    .iter()
                    .filter(|event| {
                        // 保留不包含 bedcode_hook.py 的 event
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

/// 构建 hooks JSON 配置
///
/// 包含 SessionStart / PreToolUse / Stop / SubagentStop 四个事件
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

/// 检查 hooks 配置中的 BEDCODE_TOKEN 是否与当前 token 一致
///
/// 遍历所有 hook 事件类型的 command 字段，提取 BEDCODE_TOKEN 值进行比对。
/// 只要有一个 command 中的 token 不匹配就返回 false。
fn is_token_match_in_hooks(hooks: &serde_json::Value, current_token: &str) -> bool {
    if let Some(hooks_obj) = hooks.as_object() {
        for (_event_type, events) in hooks_obj {
            if let Some(events_arr) = events.as_array() {
                for event in events_arr {
                    if let Some(hook_list) = event.get("hooks").and_then(|v| v.as_array()) {
                        for hook in hook_list {
                            if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                                if cmd.contains("bedcode_hook.py") {
                                    // 从命令中提取 BEDCODE_TOKEN=xxx
                                    if let Some(hook_token) = extract_token_from_command(cmd) {
                                        if hook_token != current_token {
                                            return false;
                                        }
                                    }
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

/// 从 hook command 字符串中提取 BEDCODE_TOKEN 的值
fn extract_token_from_command(cmd: &str) -> Option<String> {
    for part in cmd.split_whitespace() {
        if let Some(token_val) = part.strip_prefix("BEDCODE_TOKEN=") {
            return Some(token_val.to_string());
        }
    }
    None
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

/// 为项目配置 Claude Code hooks
///
/// 在会话启动前调用，仅在项目目录下的 `.claude/settings.json` 中配置 hooks。
/// 如果项目已配置 hooks，则跳过。
/// 所有 I/O 错误仅记录日志，不阻塞会话创建。
/// 使用 spawn_blocking 避免阻塞 tokio 运行时。
pub async fn ensure_project_hooks(
    working_dir: &str,
    port: u16,
    token: &str,
    resource_dir: &PathBuf,
) -> ProjectHooksResult {
    tracing::info!("ensure_project_hooks called for project: {}", working_dir);

    let working_dir = working_dir.to_string();
    let token = token.to_string();
    let resource_dir = resource_dir.clone();

    tokio::task::spawn_blocking(move || {
        ensure_project_hooks_blocking(&working_dir, port, &token, &resource_dir)
    })
    .await
    .unwrap_or_else(|e| ProjectHooksResult {
        success: false,
        message: format!("Hooks 配置任务异常: {}", e),
        skipped: false,
    })
}

/// ensure_project_hooks 的同步实现，由 spawn_blocking 调用
fn ensure_project_hooks_blocking(
    working_dir: &str,
    port: u16,
    token: &str,
    resource_dir: &PathBuf,
) -> ProjectHooksResult {
    let project_path = PathBuf::from(working_dir);
    let claude_dir = project_path.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    // 1. 读取现有 settings.json（只读一次，后续复用）
    let mut settings: serde_json::Value = if settings_path.exists() {
        match fs::read_to_string(&settings_path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
            Err(e) => {
                tracing::warn!("Failed to read project settings.json: {}", e);
                serde_json::json!({})
            }
        }
    } else {
        serde_json::json!({})
    };

    // 检查项目是否已有 BedCode hooks
    // // 原 token 校验逻辑已禁用：不再比对 BEDCODE_TOKEN 是否一致
    // let needs_update = match settings.get("hooks") {
    //     Some(hooks) if is_bedcode_hooks_configured(hooks) => {
    //         // hooks 存在，但需检查 token 是否与当前配置一致
    //         !is_token_match_in_hooks(hooks, token)
    //     }
    //     _ => true,
    // };
    let needs_update = match settings.get("hooks") {
        Some(hooks) if is_bedcode_hooks_configured(hooks) => {
            // hooks 已存在且包含 BedCode 配置，无需更新
            false
        }
        _ => true,
    };

    if !needs_update {
        tracing::info!("Project already has BedCode hooks, skipping");
        return ProjectHooksResult {
            success: true,
            // message: "项目已配置 BedCode hooks 且 token 一致".to_string(),
            message: "项目已配置 BedCode hooks".to_string(),
            skipped: true,
        };
    }

    // 2. 确保 .claude 目录存在
    if let Err(e) = fs::create_dir_all(&claude_dir) {
        tracing::warn!("Failed to create project .claude dir: {}", e);
        return ProjectHooksResult {
            success: false,
            message: format!("创建项目 .claude 目录失败: {}", e),
            skipped: false,
        };
    }

    // 3. 复制 bedcode_hook.py 到项目 .claude/ 目录
    let hook_script_path = claude_dir.join("bedcode_hook.py");
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
            tracing::warn!("Failed to copy bedcode_hook.py to project .claude/: {}", e);
        }
    }

    // 4. 构建 hooks 配置并写入项目 settings.json
    let hook_script_str = hook_script_path.to_string_lossy().to_string();
    let hooks_config = build_hooks_config(port, token, &hook_script_str);

    // 合并 hooks：保留非 BedCode hooks，添加 BedCode hooks（复用已读取的 settings）
    let existing_hooks = settings.get("hooks").cloned().unwrap_or(serde_json::json!({}));
    let merged_hooks = merge_hooks(&existing_hooks, &hooks_config);
    settings["hooks"] = merged_hooks;

    // 写入
    match serde_json::to_string_pretty(&settings) {
        Ok(content) => {
            if let Err(e) = fs::write(&settings_path, content) {
                tracing::warn!("Failed to write project settings.json: {}", e);
                return ProjectHooksResult {
                    success: false,
                    message: format!("写入项目 settings.json 失败: {}", e),
                    skipped: false,
                };
            }
        }
        Err(e) => {
            tracing::warn!("Failed to serialize project settings.json: {}", e);
            return ProjectHooksResult {
                success: false,
                message: format!("序列化项目 settings.json 失败: {}", e),
                skipped: false,
            };
        }
    }

    // 5. 验证 hooks 配置是否生效
    if !verify_project_hooks(&settings_path) {
        tracing::warn!("Project hooks verification failed: bedcode_hook.py not found in written settings.json");
    }

    tracing::info!(
        "Project hooks configured in {} with BEDCODE_PORT={}",
        settings_path.display(),
        port
    );

    ProjectHooksResult {
        success: true,
        message: "项目 Hooks 已配置".to_string(),
        skipped: false,
    }
}

/// 验证项目 settings.json 中的 hooks 配置是否包含 BedCode hooks
fn verify_project_hooks(settings_path: &PathBuf) -> bool {
    match fs::read_to_string(settings_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(settings) => {
                if let Some(hooks) = settings.get("hooks") {
                    is_bedcode_hooks_configured(hooks)
                } else {
                    false
                }
            }
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_hooks_config() {
        let hook_path = "/home/user/project/.claude/bedcode_hook.py";
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
        let bedcode = build_hooks_config(8765, "token123", "/project/.claude/bedcode_hook.py");

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
        let bedcode = build_hooks_config(8765, "token123", "/project/.claude/bedcode_hook.py");

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
        let bedcode = build_hooks_config(8765, "newtoken", "/project/.claude/bedcode_hook.py");

        let merged = merge_hooks(&existing, &bedcode);

        // 应该替换为新的 BedCode 配置
        let session_start = merged.get("SessionStart").unwrap().as_array().unwrap();
        let cmd = session_start[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(cmd.contains("BEDCODE_PORT=8765"));
        assert!(cmd.contains("BEDCODE_TOKEN=newtoken"));
        assert!(!cmd.contains("9999"));
    }

    #[test]
    fn test_remove_bedcode_hooks() {
        let hooks = serde_json::json!({
            "SessionStart": [{
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=8765 BEDCODE_TOKEN=abc python \"bedcode_hook.py\" session-start",
                    "timeout": 5
                }]
            }],
            "PostToolUse": [{
                "matcher": "Write",
                "hooks": [{
                    "type": "command",
                    "command": "/path/to/other-hook.sh",
                    "timeout": 10
                }]
            }]
        });

        let cleaned = remove_bedcode_hooks(&hooks);

        // SessionStart 应被完全移除（只包含 BedCode hooks）
        assert!(cleaned.get("SessionStart").is_none());

        // PostToolUse 应保留
        let post_tool = cleaned.get("PostToolUse").unwrap().as_array().unwrap();
        assert_eq!(post_tool.len(), 1);
        assert_eq!(
            post_tool[0]["hooks"][0]["command"].as_str().unwrap(),
            "/path/to/other-hook.sh"
        );
    }

    #[test]
    fn test_remove_bedcode_hooks_mixed_event() {
        let hooks = serde_json::json!({
            "SessionStart": [
                {
                    "matcher": "",
                    "hooks": [{
                        "type": "command",
                        "command": "BEDCODE_PORT=8765 python \"bedcode_hook.py\" session-start",
                        "timeout": 5
                    }]
                },
                {
                    "matcher": "custom",
                    "hooks": [{
                        "type": "command",
                        "command": "/path/to/custom-hook.sh",
                        "timeout": 10
                    }]
                }
            ]
        });

        let cleaned = remove_bedcode_hooks(&hooks);

        let session_start = cleaned.get("SessionStart").unwrap().as_array().unwrap();
        assert_eq!(session_start.len(), 1);
        assert_eq!(
            session_start[0]["hooks"][0]["command"].as_str().unwrap(),
            "/path/to/custom-hook.sh"
        );
    }

    #[test]
    fn test_ensure_project_hooks_creates_config() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let working_dir = tmp_dir.path().to_string_lossy().to_string();
        let resource_dir = PathBuf::from("/nonexistent");

        let result = ensure_project_hooks_blocking(&working_dir, 8765, "testtoken123456", &resource_dir);

        assert!(result.success);
        assert!(!result.skipped);

        let settings_path = tmp_dir.path().join(".claude").join("settings.json");
        assert!(settings_path.exists());

        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(settings.get("hooks").is_some());
    }

    #[test]
    fn test_ensure_project_hooks_skips_if_configured() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let working_dir = tmp_dir.path().to_string_lossy().to_string();
        let resource_dir = PathBuf::from("/nonexistent");

        let result1 = ensure_project_hooks_blocking(&working_dir, 8765, "testtoken123456", &resource_dir);
        assert!(result1.success);
        assert!(!result1.skipped);

        // 相同 token 应跳过
        let result2 = ensure_project_hooks_blocking(&working_dir, 8765, "testtoken123456", &resource_dir);
        assert!(result2.success);
        assert!(result2.skipped);
    }

    #[test]
    fn test_ensure_project_hooks_updates_if_token_changed() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let working_dir = tmp_dir.path().to_string_lossy().to_string();
        let resource_dir = PathBuf::from("/nonexistent");

        let result1 = ensure_project_hooks_blocking(&working_dir, 8765, "old_token_123456", &resource_dir);
        assert!(result1.success);
        assert!(!result1.skipped);

        // 不同 token 应更新 hooks
        let result2 = ensure_project_hooks_blocking(&working_dir, 8765, "new_token_654321", &resource_dir);
        assert!(result2.success);
        assert!(!result2.skipped);

        // 验证 token 已更新
        let settings_path = tmp_dir.path().join(".claude").join("settings.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        assert!(content.contains("new_token_654321"));
        assert!(!content.contains("old_token_123456"));
    }

    #[test]
    fn test_extract_token_from_command() {
        let cmd = r#"BEDCODE_PORT=8765 BEDCODE_TOKEN=abc123 python "/home/user/.claude/bedcode_hook.py" session-start"#;
        assert_eq!(extract_token_from_command(cmd), Some("abc123".to_string()));

        let cmd_no_token = r#"python "/home/user/.claude/bedcode_hook.py" session-start"#;
        assert_eq!(extract_token_from_command(cmd_no_token), None);
    }

    #[test]
    fn test_is_token_match_in_hooks() {
        let hooks = serde_json::json!({
            "SessionStart": [{
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=8765 BEDCODE_TOKEN=mytoken123 python \"bedcode_hook.py\" session-start",
                    "timeout": 5
                }]
            }]
        });

        assert!(is_token_match_in_hooks(&hooks, "mytoken123"));
        assert!(!is_token_match_in_hooks(&hooks, "wrongtoken"));
    }

    #[test]
    fn test_verify_project_hooks() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let working_dir = tmp_dir.path().to_string_lossy().to_string();
        let resource_dir = PathBuf::from("/nonexistent");

        // 配置前不应验证通过
        let settings_path = tmp_dir.path().join(".claude").join("settings.json");
        assert!(!verify_project_hooks(&settings_path));

        // 配置后应验证通过
        let result = ensure_project_hooks_blocking(&working_dir, 8765, "testtoken123456", &resource_dir);
        assert!(result.success);
        assert!(verify_project_hooks(&settings_path));
    }
}
