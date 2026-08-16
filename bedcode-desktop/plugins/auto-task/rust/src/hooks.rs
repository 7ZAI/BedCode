//! 会话集成配置管理
//!
//! 每个适配 agent 在项目目录部署自己的状态回传载体（由 AgentProfile
//! 的 session_integration 驱动）：
//! - claude：`.claude/settings.json` hooks + `auto_task_hook.py`
//! - pi：`.pi/extensions/pi_task_hook.ts`（pi 自动发现，无需注册）
//! - opencode：`.opencode/plugins/opencode_task_hook.ts`（自动加载，无需注册）
//! - codex：`.codex/hooks.json` hooks + `codex_task_hook.py`（Codex 从项目
//!   `.codex/` 配置层自动发现；非托管 hooks 需用户在 `/hooks` 中信任后才能运行）
//! - 其他 agent：未适配，不部署
//!
//! ensure_agent_integration() 为统一入口（会话创建前调用），
//! cleanup_all_agent_integrations() 为统一清理入口（禁用/退出时调用）。

use bedcode_plugin_api::constants::{
    CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE, CODEX_CONFIG_DIR_NAME, CODEX_HOOKS_FILE,
    CODEX_HOOK_SCRIPT_NAME, HOOK_SCRIPT_NAME, OPENCODE_CONFIG_DIR_NAME, OPENCODE_HOOK_SCRIPT_NAME,
    OPENCODE_PLUGINS_DIR_NAME, PI_CONFIG_DIR_NAME, PI_EXTENSIONS_DIR_NAME, PI_HOOK_SCRIPT_NAME,
};
use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostFs, HostLog, HostSession};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde::{Deserialize, Serialize};

use crate::agent::{self, SessionIntegration};

/// 会话集成配置结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIntegrationResult {
    pub success: bool,
    pub message: String,
    pub skipped: bool,
}

/// 按 agent 能力分发部署：新增 agent 时在 SessionIntegration 增加枚举值，
/// 并在本函数补充对应部署逻辑（调用方零改动）
pub fn ensure_agent_integration(
    host: &WasmHost,
    agent_name: &str,
    working_dir: &str,
    port: u16,
    resource_dir: &str,
) -> AgentIntegrationResult {
    match agent::session_integration_for(agent_name) {
        SessionIntegration::ClaudeCodeHooks => {
            ensure_project_hooks(host, working_dir, port, resource_dir)
        }
        SessionIntegration::PiExtension => {
            ensure_pi_extension(host, working_dir, port, resource_dir)
        }
        SessionIntegration::OpenCodePlugin => {
            ensure_opencode_plugin(host, working_dir, port, resource_dir)
        }
        SessionIntegration::CodexHooks => {
            ensure_codex_hooks(host, working_dir, port, resource_dir)
        }
        SessionIntegration::None => {
            host.log_debug(&format!(
                "ensure_agent_integration: agent '{}' has no session integration, skip",
                agent_name
            ));
            AgentIntegrationResult {
                success: true,
                message: format!("agent '{}' 未适配会话集成，跳过", agent_name),
                skipped: true,
            }
        }
    }
}

/// 按 agent 能力分发清理（禁用/应用退出/会话重建时移除项目内的部署文件）
pub fn cleanup_agent_integration(
    host: &WasmHost,
    agent_name: &str,
    working_dir: &str,
) -> AgentIntegrationResult {
    match agent::session_integration_for(agent_name) {
        SessionIntegration::ClaudeCodeHooks => cleanup_project_hooks(host, working_dir),
        SessionIntegration::PiExtension => cleanup_pi_extension(host, working_dir),
        SessionIntegration::OpenCodePlugin => cleanup_opencode_plugin(host, working_dir),
        SessionIntegration::CodexHooks => cleanup_codex_hooks(host, working_dir),
        SessionIntegration::None => {
            host.log_debug(&format!(
                "cleanup_agent_integration: agent '{}' has no session integration, skip",
                agent_name
            ));
            AgentIntegrationResult {
                success: true,
                message: format!("agent '{}' 未适配会话集成，跳过", agent_name),
                skipped: true,
            }
        }
    }
}

/// 清理指定项目的所有已适配 agent 集成（claude hooks + pi 扩展 + opencode 插件 + codex hooks 等）
///
/// 供 cleanup-project-hooks 命令使用：按项目维度清理，不依赖会话当前 agent。
pub fn cleanup_project_all_integrations(
    host: &WasmHost,
    working_dir: &str,
) -> AgentIntegrationResult {
    if working_dir.is_empty() {
        return AgentIntegrationResult {
            success: false,
            message: "working_dir 为空".to_string(),
            skipped: false,
        };
    }

    let mut cleaned = 0usize;
    let mut failed = 0usize;
    for profile in agent::AGENT_PROFILES {
        if profile.session_integration == SessionIntegration::None {
            continue;
        }
        let result = cleanup_agent_integration(host, profile.name, working_dir);
        if result.skipped {
            // 无部署文件视为已清理
            cleaned += 1;
        } else if result.success {
            cleaned += 1;
        } else {
            failed += 1;
            host.log_warn(&format!(
                "cleanup_project_all_integrations: agent '{}' cleanup failed for {}: {}",
                profile.name, working_dir, result.message
            ));
        }
    }

    AgentIntegrationResult {
        success: failed == 0,
        message: format!("清理 {} 个 agent 集成（失败 {}）", cleaned, failed),
        skipped: cleaned == 0 && failed == 0,
    }
}

/// 为项目配置 Claude Code hooks
///
/// 在会话启动前调用，仅在项目目录下的 `.claude/settings.json` 中配置 hooks。
pub fn ensure_project_hooks(
    host: &WasmHost,
    working_dir: &str,
    port: u16,
    resource_dir: &str,
) -> AgentIntegrationResult {
    host.log_debug(&format!(
        "ensure_project_hooks: enter working_dir={:?} port={} resource_dir={:?}",
        working_dir, port, resource_dir
    ));

    let claude_dir = format!("{}/{}", working_dir, CLAUDE_CONFIG_DIR_NAME);
    let settings_path = format!("{}/{}", claude_dir, CLAUDE_SETTINGS_FILE);
    let hook_script_path = format!("{}/{}", claude_dir, HOOK_SCRIPT_NAME);
    let source_script = format!("{}/{}", resource_dir, HOOK_SCRIPT_NAME);

    if working_dir.is_empty() {
        host.log_warn("ensure_project_hooks: empty working_dir");
        return AgentIntegrationResult {
            success: false,
            message: "working_dir 为空".to_string(),
            skipped: false,
        };
    }

    host.log_debug(&format!(
        "ensure_project_hooks: paths claude_dir={:?} settings={:?} hook_script={:?} source_script={:?}",
        claude_dir, settings_path, hook_script_path, source_script
    ));

    // 1. 读取现有 settings.json
    let settings_read = host.fs_read(&settings_path);
    host.log_debug(&format!(
        "ensure_project_hooks: fs_read settings.json => {:?}",
        settings_read.as_ref().map(|r| r.as_ref().map(|c| c.len()))
    ));
    let mut settings: serde_json::Value = match settings_read {
        Ok(Some(content)) => match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(e) => {
                // 已存在但解析失败：视为空配置继续，避免覆盖损坏文件时静默丢弃用户内容
                host.log_warn(&format!(
                    "ensure_project_hooks: settings.json parse failed, treating as empty: {}",
                    e
                ));
                serde_json::json!({})
            }
        },
        Ok(None) => {
            host.log_debug("ensure_project_hooks: settings.json not found, starting empty");
            serde_json::json!({})
        }
        Err(e) => {
            host.log_warn(&format!(
                "ensure_project_hooks: fs_read settings.json failed: {}",
                e
            ));
            serde_json::json!({})
        }
    };

    // 检查项目是否已有插件 hooks 且端口匹配
    let needs_update = match settings.get("hooks") {
        Some(hooks) if is_plugin_hooks_configured(hooks) => {
            // hooks 存在，但需要验证端口是否与当前值匹配
            let port_matches = is_hooks_port_matching(hooks, port);
            host.log_debug(&format!(
                "ensure_project_hooks: existing plugin hooks found, port_matching={}",
                port_matches
            ));
            !port_matches
        }
        Some(_) => {
            host.log_debug(
                "ensure_project_hooks: hooks present but no plugin hook entry, needs update",
            );
            true
        }
        None => {
            host.log_debug("ensure_project_hooks: no hooks key in settings.json, needs update");
            true
        }
    };

    if !needs_update {
        host.log_info("Project already has plugin hooks with matching config, skipping");
        return AgentIntegrationResult {
            success: true,
            message: "项目已配置插件 hooks 且配置匹配".to_string(),
            skipped: true,
        };
    }

    host.log_info(&format!("Updating plugin hooks (port={})", port));

    // 2. 复制 hook 脚本到项目 .claude/ 目录
    host.log_debug(&format!(
        "ensure_project_hooks: fs_copy {:?} -> {:?}",
        source_script, hook_script_path
    ));
    match host.fs_copy(&source_script, &hook_script_path) {
        Ok(()) => host.log_debug(&format!(
            "ensure_project_hooks: fs_copy ok, hook script copied to {:?}",
            hook_script_path
        )),
        Err(e) => {
            // 拷贝失败：上报宿主弹窗提示；不阻塞会话创建（hooks 未装齐时
            // 任务调度能力受限，但会话照常启动，插件保持激活）
            let msg = format!(
                "hook script copy failed: src={:?} dst={:?} err={}",
                source_script, hook_script_path, e
            );
            host.log_error(&format!("ensure_project_hooks: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("复制 hook 脚本失败: {}", e),
                skipped: false,
            };
        }
    }

    // 3. 构建 hooks 配置并写入项目 settings.json
    let hooks_config = build_hooks_config(port, &hook_script_path);

    // 合并 hooks：保留非插件 hooks，添加插件 hooks
    let existing_hooks = settings
        .get("hooks")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let merged_hooks = merge_hooks(&existing_hooks, &hooks_config);
    settings["hooks"] = merged_hooks;

    // 写入
    let content = match serde_json::to_string_pretty(&settings) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("settings.json serialize failed: {}", e);
            host.log_error(&format!("ensure_project_hooks: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("序列化 settings.json 失败: {}", e),
                skipped: false,
            };
        }
    };
    host.log_debug(&format!(
        "ensure_project_hooks: fs_write settings.json len={} path={:?}",
        content.len(),
        settings_path
    ));
    if let Err(e) = host.fs_write(&settings_path, &content) {
        // settings.json 配置失败：上报宿主弹窗提示，不阻塞会话创建
        let msg = format!(
            "settings.json write failed: path={:?} err={}",
            settings_path, e
        );
        host.log_error(&format!("ensure_project_hooks: {}", msg));
        host.mark_plugin_error(&format!("auto-task: {}", msg));
        return AgentIntegrationResult {
            success: false,
            message: format!("写入项目 settings.json 失败: {}", e),
            skipped: false,
        };
    }

    host.log_info(&format!("Project hooks configured in {}", settings_path));

    AgentIntegrationResult {
        success: true,
        message: "项目 Hooks 已配置".to_string(),
        skipped: false,
    }
}

/// 清理所有项目的全部 agent 集成（claude hooks + pi 扩展 + opencode 插件等）
///
/// 遍历所有会话配置，对每个配置的 working_dir 按 AGENT_PROFILES 中已适配
/// 的 agent 逐个清理（cleanup_agent_integration 按 profile 分发）。
/// 同时清理全局 hooks。用于插件禁用（deactivate）和应用关闭（on_shutdown）时
/// 确保所有残留的集成配置被移除。
///
/// # Returns
/// 清理结果摘要：清理了多少个项目、跳过了多少、失败了多少
pub fn cleanup_all_agent_integrations(host: &WasmHost) -> AllAgentIntegrationResult {
    let mut result = AllAgentIntegrationResult::default();

    // 1. 清理全局 hooks
    cleanup_global_hooks(host);
    host.log_info("cleanup_all_agent_integrations: global hooks cleaned");

    // 2. 获取所有会话配置
    let configs = match host.session_config_list() {
        Ok(Some(value)) => value,
        _ => {
            host.log_error("cleanup_all_agent_integrations: failed to get session config list");
            return result;
        }
    };

    let config_arr = match configs.as_array() {
        Some(arr) => arr,
        None => {
            host.log_error("cleanup_all_agent_integrations: session config list is not an array");
            return result;
        }
    };

    host.log_info(&format!(
        "cleanup_all_agent_integrations: checking {} session config(s)",
        config_arr.len()
    ));

    // 3. 遍历所有配置，清理每个项目的全部 agent 集成
    for config in config_arr {
        let working_dir = config
            .get("workingDir")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if working_dir.is_empty() {
            continue;
        }

        for profile in agent::AGENT_PROFILES {
            if profile.session_integration == SessionIntegration::None {
                continue;
            }
            let cleanup_result = cleanup_agent_integration(host, profile.name, working_dir);

            if cleanup_result.skipped {
                result.skipped += 1;
            } else if cleanup_result.success {
                result.cleaned += 1;
                host.log_info(&format!(
                    "cleanup_all_agent_integrations: cleaned '{}' integration for {}",
                    profile.name, working_dir
                ));
            } else {
                result.failed += 1;
                host.log_warn(&format!(
                    "cleanup_all_agent_integrations: failed to clean '{}' integration for {}: {}",
                    profile.name, working_dir, cleanup_result.message
                ));
            }
        }
    }

    host.log_info(&format!(
        "cleanup_all_agent_integrations: done (cleaned={}, skipped={}, failed={})",
        result.cleaned, result.skipped, result.failed
    ));

    result
}

/// 所有项目集成清理结果摘要
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AllAgentIntegrationResult {
    /// 成功清理的项目数
    pub cleaned: usize,
    /// 跳过的项目数（无部署或无需清理）
    pub skipped: usize,
    /// 清理失败的项目数
    pub failed: usize,
}

/// 清理指定项目的插件 hooks
///
/// 1. 从 settings.json 移除插件 hook 条目（**只移除 BedCode 相关，保留用户自己的 hooks 与其他配置**）
/// 2. 删除项目 `.claude/auto_task_hook.py` 脚本（幂等，文件不存在视为成功）
pub fn cleanup_project_hooks(host: &WasmHost, working_dir: &str) -> AgentIntegrationResult {
    let claude_dir = format!("{}/{}", working_dir, CLAUDE_CONFIG_DIR_NAME);
    let settings_path = format!("{}/{}", claude_dir, CLAUDE_SETTINGS_FILE);
    let hook_script_path = format!("{}/{}", claude_dir, HOOK_SCRIPT_NAME);

    // 1. 清理 settings.json 中的插件 hooks，保留用户自己的配置
    let mut settings: serde_json::Value = match host.fs_read(&settings_path) {
        Ok(Some(content)) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    let mut had_plugin_hooks = false;
    if let Some(hooks) = settings.get("hooks").cloned() {
        if is_plugin_hooks_configured(&hooks) {
            had_plugin_hooks = true;

            // 仅移除含 auto_task_hook.py 的条目，用户自定义 hooks 原样保留
            let cleaned_hooks = remove_plugin_hooks(&hooks);
            if cleaned_hooks
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(true)
            {
                settings.as_object_mut().map(|o| o.remove("hooks"));
            } else {
                settings["hooks"] = cleaned_hooks;
            }

            match serde_json::to_string_pretty(&settings) {
                Ok(content) => {
                    if let Err(e) = host.fs_write(&settings_path, &content) {
                        return AgentIntegrationResult {
                            success: false,
                            message: format!("写入 settings.json 失败: {}", e),
                            skipped: false,
                        };
                    }
                }
                Err(e) => {
                    return AgentIntegrationResult {
                        success: false,
                        message: format!("序列化 settings.json 失败: {}", e),
                        skipped: false,
                    };
                }
            }
        }
    }

    // 2. 删除项目中的 hook 脚本（无论 settings 是否还有插件条目，脚本可能残留）
    if let Err(e) = host.fs_delete(&hook_script_path) {
        host.log_warn(&format!(
            "Failed to delete hook script {}: {}",
            hook_script_path, e
        ));
    }

    if had_plugin_hooks {
        host.log_info(&format!(
            "Cleaned plugin hooks from project: {}",
            working_dir
        ));
        AgentIntegrationResult {
            success: true,
            message: "项目插件 hooks 已清理".to_string(),
            skipped: false,
        }
    } else {
        AgentIntegrationResult {
            success: true,
            message: "项目无插件 hooks".to_string(),
            skipped: true,
        }
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

    let settings_path = format!(
        "{}/{}/{}",
        home_dir, CLAUDE_CONFIG_DIR_NAME, CLAUDE_SETTINGS_FILE
    );

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

    if cleaned_hooks
        .as_object()
        .map(|o| o.is_empty())
        .unwrap_or(true)
    {
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

// ==================== pi 扩展部署 ====================

/// 为项目部署 pi 扩展（`.pi/extensions/pi_task_hook.ts`）
///
/// pi 项目级扩展自动发现 `.pi/extensions/*.ts`，无需注册配置；
/// 扩展模板内嵌 BedCode 端口标记（`const BEDCODE_PORT = <port>`），
/// 部署时按当前端口改写，端口变化时自动重新部署（幂等）。
///
/// 注意：pi 仅在项目被信任后加载项目级扩展（--approve / trust 流程），
/// 首次启动需用户在终端确认信任，插件侧无法代答（与 claude hooks 不同：
/// hooks 写入 settings.json 即生效，pi 扩展受 trust 门控）。
pub fn ensure_pi_extension(
    host: &WasmHost,
    working_dir: &str,
    port: u16,
    resource_dir: &str,
) -> AgentIntegrationResult {
    host.log_debug(&format!(
        "ensure_pi_extension: enter working_dir={:?} port={} resource_dir={:?}",
        working_dir, port, resource_dir
    ));

    if working_dir.is_empty() {
        host.log_warn("ensure_pi_extension: empty working_dir");
        return AgentIntegrationResult {
            success: false,
            message: "working_dir 为空".to_string(),
            skipped: false,
        };
    }

    let target = format!(
        "{}/{}/{}/{}",
        working_dir, PI_CONFIG_DIR_NAME, PI_EXTENSIONS_DIR_NAME, PI_HOOK_SCRIPT_NAME
    );
    let source = format!("{}/{}", resource_dir, PI_HOOK_SCRIPT_NAME);
    host.log_debug(&format!(
        "ensure_pi_extension: paths target={:?} source={:?}",
        target, source
    ));

    // 1. 已部署且端口与模板版本均匹配 → 跳过；任一不匹配 → 重新部署
    //    （仅比较端口发现不了脚本内容更新，模板升级须靠版本标记触发重部署）
    match host.fs_read(&target) {
        Ok(Some(existing)) => {
            if pi_extension_port_matches(&existing, port) && pi_extension_version_matches(&existing)
            {
                host.log_info(
                    "ensure_pi_extension: extension already deployed with matching port and template version, skipping",
                );
                return AgentIntegrationResult {
                    success: true,
                    message: "pi 扩展已部署且端口/版本匹配".to_string(),
                    skipped: true,
                };
            }
            host.log_info(&format!(
                "ensure_pi_extension: deployed extension stale (port or template version mismatch), redeploying (port={})",
                port
            ));
        }
        Ok(None) => {
            host.log_debug("ensure_pi_extension: extension not deployed yet");
        }
        Err(e) => {
            host.log_warn(&format!(
                "ensure_pi_extension: fs_read existing extension failed: {}",
                e
            ));
        }
    }

    // 2. 读取模板（插件资源目录随构建打包）
    let template = match host.fs_read(&source) {
        Ok(Some(c)) => c,
        Ok(None) => {
            let msg = format!("pi extension template missing: {:?}", source);
            host.log_error(&format!("ensure_pi_extension: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("pi 扩展模板缺失: {}", source),
                skipped: false,
            };
        }
        Err(e) => {
            let msg = format!(
                "pi extension template read failed: src={:?} err={}",
                source, e
            );
            host.log_error(&format!("ensure_pi_extension: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("读取 pi 扩展模板失败: {}", e),
                skipped: false,
            };
        }
    };

    // 3. 按当前端口改写模板内嵌端口并写入（fs_write 自动创建父目录）
    let content = replace_pi_extension_port(&template, port);
    match host.fs_write(&target, &content) {
        Ok(()) => {
            host.log_info(&format!("ensure_pi_extension: deployed to {:?}", target));
            AgentIntegrationResult {
                success: true,
                message: "pi 扩展已部署".to_string(),
                skipped: false,
            }
        }
        Err(e) => {
            let msg = format!("pi extension write failed: path={:?} err={}", target, e);
            host.log_error(&format!("ensure_pi_extension: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            AgentIntegrationResult {
                success: false,
                message: format!("写入 pi 扩展失败: {}", e),
                skipped: false,
            }
        }
    }
}

/// 清理指定项目的 pi 扩展（幂等，文件不存在视为已清理）
///
/// 只删除 pi_task_hook.ts，不触碰 `.pi/extensions/` 目录下的用户自有扩展。
pub fn cleanup_pi_extension(host: &WasmHost, working_dir: &str) -> AgentIntegrationResult {
    let target = format!(
        "{}/{}/{}/{}",
        working_dir, PI_CONFIG_DIR_NAME, PI_EXTENSIONS_DIR_NAME, PI_HOOK_SCRIPT_NAME
    );
    host.log_debug(&format!("cleanup_pi_extension: target={:?}", target));

    let exists = host.fs_exists(&target).unwrap_or(false);
    if !exists {
        host.log_debug("cleanup_pi_extension: extension not present, skipped");
        return AgentIntegrationResult {
            success: true,
            message: "项目无 pi 扩展".to_string(),
            skipped: true,
        };
    }

    match host.fs_delete(&target) {
        Ok(()) => {
            host.log_info(&format!("cleanup_pi_extension: deleted {:?}", target));
            AgentIntegrationResult {
                success: true,
                message: "项目 pi 扩展已清理".to_string(),
                skipped: false,
            }
        }
        Err(e) => {
            host.log_warn(&format!(
                "cleanup_pi_extension: failed to delete {:?}: {}",
                target, e
            ));
            AgentIntegrationResult {
                success: false,
                message: format!("删除 pi 扩展失败: {}", e),
                skipped: false,
            }
        }
    }
}

/// 模板内端口改写：`const BEDCODE_PORT = <数字>` → 当前端口
///
/// 模板默认端口与运行时端口可能不同（宿主端口可配置），部署时替换；
/// 找不到标记时原样返回（模板被篡改时降级为模板默认端口）。
fn replace_pi_extension_port(content: &str, port: u16) -> String {
    const MARKER: &str = "const BEDCODE_PORT = ";
    match content.find(MARKER) {
        Some(start) => {
            let value_start = start + MARKER.len();
            let digits_len = content[value_start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if digits_len == 0 {
                return content.to_string();
            }
            let end = value_start + digits_len;
            format!("{}{}{}", &content[..value_start], port, &content[end..])
        }
        None => content.to_string(),
    }
}

/// 检查已部署扩展内嵌端口是否与当前端口匹配（幂等跳过判定）
fn pi_extension_port_matches(content: &str, port: u16) -> bool {
    const MARKER: &str = "const BEDCODE_PORT = ";
    content.find(MARKER).and_then(|start| {
        let value_start = start + MARKER.len();
        let digits: String = content[value_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse::<u16>().ok()
    }) == Some(port)
}

/// 模板版本标记：内容升级时递增模板内标记，旧部署副本据此自动重部署
/// （端口匹配检查无法发现脚本内容更新）
const PI_EXTENSION_TEMPLATE_VERSION: &str = "2";

/// 检查已部署扩展是否携带当前模板版本标记
fn pi_extension_version_matches(content: &str) -> bool {
    const MARKER: &str = "@bedcode-template-version ";
    content.find(MARKER).map_or(false, |start| {
        let value_start = start + MARKER.len();
        let digits: String = content[value_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits == PI_EXTENSION_TEMPLATE_VERSION
    })
}

// ==================== opencode 插件部署 ====================

/// 为项目部署 opencode 插件（`.opencode/plugins/opencode_task_hook.ts`）
///
/// opencode 项目级插件自动发现 `.opencode/plugins/*.ts`，无需注册配置；
/// 插件模板内嵌 BedCode 端口标记（`const BEDCODE_PORT = <port>`），
/// 部署时按当前端口改写，端口变化时自动重新部署（幂等）。
/// 部署/版本判定与 pi 扩展同构（模板升级靠版本标记触发重部署）。
pub fn ensure_opencode_plugin(
    host: &WasmHost,
    working_dir: &str,
    port: u16,
    resource_dir: &str,
) -> AgentIntegrationResult {
    host.log_debug(&format!(
        "ensure_opencode_plugin: enter working_dir={:?} port={} resource_dir={:?}",
        working_dir, port, resource_dir
    ));

    if working_dir.is_empty() {
        host.log_warn("ensure_opencode_plugin: empty working_dir");
        return AgentIntegrationResult {
            success: false,
            message: "working_dir 为空".to_string(),
            skipped: false,
        };
    }

    let target = format!(
        "{}/{}/{}/{}",
        working_dir,
        OPENCODE_CONFIG_DIR_NAME,
        OPENCODE_PLUGINS_DIR_NAME,
        OPENCODE_HOOK_SCRIPT_NAME
    );
    let source = format!("{}/{}", resource_dir, OPENCODE_HOOK_SCRIPT_NAME);
    host.log_debug(&format!(
        "ensure_opencode_plugin: paths target={:?} source={:?}",
        target, source
    ));

    // 1. 已部署且端口与模板版本均匹配 → 跳过；任一不匹配 → 重新部署
    //    （仅比较端口发现不了脚本内容更新，模板升级须靠版本标记触发重部署）
    match host.fs_read(&target) {
        Ok(Some(existing)) => {
            if opencode_plugin_port_matches(&existing, port)
                && opencode_plugin_version_matches(&existing)
            {
                host.log_info(
                    "ensure_opencode_plugin: plugin already deployed with matching port and template version, skipping",
                );
                return AgentIntegrationResult {
                    success: true,
                    message: "opencode 插件已部署且端口/版本匹配".to_string(),
                    skipped: true,
                };
            }
            host.log_info(&format!(
                "ensure_opencode_plugin: deployed plugin stale (port or template version mismatch), redeploying (port={})",
                port
            ));
        }
        Ok(None) => {
            host.log_debug("ensure_opencode_plugin: plugin not deployed yet");
        }
        Err(e) => {
            host.log_warn(&format!(
                "ensure_opencode_plugin: fs_read existing plugin failed: {}",
                e
            ));
        }
    }

    // 2. 读取模板（插件资源目录随构建打包）
    let template = match host.fs_read(&source) {
        Ok(Some(c)) => c,
        Ok(None) => {
            let msg = format!("opencode plugin template missing: {:?}", source);
            host.log_error(&format!("ensure_opencode_plugin: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("opencode 插件模板缺失: {}", source),
                skipped: false,
            };
        }
        Err(e) => {
            let msg = format!(
                "opencode plugin template read failed: src={:?} err={}",
                source, e
            );
            host.log_error(&format!("ensure_opencode_plugin: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("读取 opencode 插件模板失败: {}", e),
                skipped: false,
            };
        }
    };

    // 3. 按当前端口改写模板内嵌端口并写入（fs_write 自动创建父目录）
    let content = replace_opencode_plugin_port(&template, port);
    match host.fs_write(&target, &content) {
        Ok(()) => {
            host.log_info(&format!("ensure_opencode_plugin: deployed to {:?}", target));
            AgentIntegrationResult {
                success: true,
                message: "opencode 插件已部署".to_string(),
                skipped: false,
            }
        }
        Err(e) => {
            let msg = format!("opencode plugin write failed: path={:?} err={}", target, e);
            host.log_error(&format!("ensure_opencode_plugin: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            AgentIntegrationResult {
                success: false,
                message: format!("写入 opencode 插件失败: {}", e),
                skipped: false,
            }
        }
    }
}

/// 清理指定项目的 opencode 插件（幂等，文件不存在视为已清理）
///
/// 只删除 opencode_task_hook.ts，不触碰 `.opencode/plugins/` 目录下的
/// 用户自有插件。
pub fn cleanup_opencode_plugin(host: &WasmHost, working_dir: &str) -> AgentIntegrationResult {
    let target = format!(
        "{}/{}/{}/{}",
        working_dir,
        OPENCODE_CONFIG_DIR_NAME,
        OPENCODE_PLUGINS_DIR_NAME,
        OPENCODE_HOOK_SCRIPT_NAME
    );
    host.log_debug(&format!("cleanup_opencode_plugin: target={:?}", target));

    let exists = host.fs_exists(&target).unwrap_or(false);
    if !exists {
        host.log_debug("cleanup_opencode_plugin: plugin not present, skipped");
        return AgentIntegrationResult {
            success: true,
            message: "项目无 opencode 插件".to_string(),
            skipped: true,
        };
    }

    match host.fs_delete(&target) {
        Ok(()) => {
            host.log_info(&format!("cleanup_opencode_plugin: deleted {:?}", target));
            AgentIntegrationResult {
                success: true,
                message: "项目 opencode 插件已清理".to_string(),
                skipped: false,
            }
        }
        Err(e) => {
            host.log_warn(&format!(
                "cleanup_opencode_plugin: failed to delete {:?}: {}",
                target, e
            ));
            AgentIntegrationResult {
                success: false,
                message: format!("删除 opencode 插件失败: {}", e),
                skipped: false,
            }
        }
    }
}

/// 模板内端口改写：`const BEDCODE_PORT = <数字>` → 当前端口
///
/// 模板默认端口与运行时端口可能不同（宿主端口可配置），部署时替换；
/// 找不到标记时原样返回（模板被篡改时降级为模板默认端口）。
fn replace_opencode_plugin_port(content: &str, port: u16) -> String {
    const MARKER: &str = "const BEDCODE_PORT = ";
    match content.find(MARKER) {
        Some(start) => {
            let value_start = start + MARKER.len();
            let digits_len = content[value_start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if digits_len == 0 {
                return content.to_string();
            }
            let end = value_start + digits_len;
            format!("{}{}{}", &content[..value_start], port, &content[end..])
        }
        None => content.to_string(),
    }
}

/// 检查已部署插件内嵌端口是否与当前端口匹配（幂等跳过判定）
fn opencode_plugin_port_matches(content: &str, port: u16) -> bool {
    const MARKER: &str = "const BEDCODE_PORT = ";
    content.find(MARKER).and_then(|start| {
        let value_start = start + MARKER.len();
        let digits: String = content[value_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse::<u16>().ok()
    }) == Some(port)
}

/// 模板版本标记：内容升级时递增模板内标记，旧部署副本据此自动重部署
/// （端口匹配检查无法发现脚本内容更新）
const OPENCODE_PLUGIN_TEMPLATE_VERSION: &str = "1";

/// 检查已部署插件是否携带当前模板版本标记
fn opencode_plugin_version_matches(content: &str) -> bool {
    const MARKER: &str = "@bedcode-template-version ";
    content.find(MARKER).map_or(false, |start| {
        let value_start = start + MARKER.len();
        let digits: String = content[value_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits == OPENCODE_PLUGIN_TEMPLATE_VERSION
    })
}

// ==================== Codex hooks 部署 ====================

/// 为项目部署 Codex hooks（`.codex/hooks.json` + `codex_task_hook.py`）
///
/// Codex 从项目 `.codex/` 配置层自动发现 `hooks.json`（与 Claude 的
/// `.claude/settings.json` 同构），无需注册配置；部署时按当前端口改写命令
/// 前缀并携带脚本模板版本标记，端口或版本任一不匹配即重新部署（幂等）。
///
/// 注意：Codex 的非托管 hooks 必须先经用户信任（`/hooks` 按 hash 审核）才会
/// 运行，且项目 `.codex/` 配置层本身也需被信任；首次使用需用户在 Codex 内
/// 确认一次，插件侧无法代答（与 pi 扩展 trust 门控同源）。
pub fn ensure_codex_hooks(
    host: &WasmHost,
    working_dir: &str,
    port: u16,
    resource_dir: &str,
) -> AgentIntegrationResult {
    host.log_debug(&format!(
        "ensure_codex_hooks: enter working_dir={:?} port={} resource_dir={:?}",
        working_dir, port, resource_dir
    ));

    if working_dir.is_empty() {
        host.log_warn("ensure_codex_hooks: empty working_dir");
        return AgentIntegrationResult {
            success: false,
            message: "working_dir 为空".to_string(),
            skipped: false,
        };
    }

    let codex_dir = format!("{}/{}", working_dir, CODEX_CONFIG_DIR_NAME);
    let hooks_path = format!("{}/{}", codex_dir, CODEX_HOOKS_FILE);
    let hook_script_path = format!("{}/{}", codex_dir, CODEX_HOOK_SCRIPT_NAME);
    let source_script = format!("{}/{}", resource_dir, CODEX_HOOK_SCRIPT_NAME);

    // 1. 读取现有 hooks.json（解析失败视为空，避免覆盖损坏文件时静默丢弃用户内容）
    let mut hooks: serde_json::Value = match host.fs_read(&hooks_path) {
        Ok(Some(content)) => match serde_json::from_str(&content) {
            Ok(val) => val,
            Err(e) => {
                host.log_warn(&format!(
                    "ensure_codex_hooks: hooks.json parse failed, treating as empty: {}",
                    e
                ));
                serde_json::json!({})
            }
        },
        Ok(None) => serde_json::json!({}),
        Err(e) => {
            host.log_warn(&format!(
                "ensure_codex_hooks: fs_read hooks.json failed: {}",
                e
            ));
            serde_json::json!({})
        }
    };

    // 2. 端口与脚本模板版本均匹配 → 跳过；任一不匹配 → 重新部署
    let needs_update = if is_codex_hooks_configured(&hooks) {
        let port_matches = codex_hooks_port_matching(&hooks, port);
        let script_version_matches = match host.fs_read(&hook_script_path) {
            Ok(Some(content)) => codex_hook_version_matches(&content),
            _ => false,
        };
        host.log_debug(&format!(
            "ensure_codex_hooks: existing codex hooks found, port_matching={} script_version_matching={}",
            port_matches, script_version_matches
        ));
        !port_matches || !script_version_matches
    } else {
        true
    };

    if !needs_update {
        host.log_info("Project already has codex hooks with matching config, skipping");
        return AgentIntegrationResult {
            success: true,
            message: "项目已配置 codex hooks 且配置匹配".to_string(),
            skipped: true,
        };
    }

    host.log_info(&format!("Updating codex hooks (port={})", port));

    // 3. 复制 hook 脚本到项目 .codex/ 目录
    if let Err(e) = host.fs_copy(&source_script, &hook_script_path) {
        let msg = format!(
            "codex hook script copy failed: src={:?} dst={:?} err={}",
            source_script, hook_script_path, e
        );
        host.log_error(&format!("ensure_codex_hooks: {}", msg));
        host.mark_plugin_error(&format!("auto-task: {}", msg));
        return AgentIntegrationResult {
            success: false,
            message: format!("复制 codex hook 脚本失败: {}", e),
            skipped: false,
        };
    }

    // 4. 构建 hooks 配置并合并写入（保留用户自有 hooks 条目）
    let hooks_config = build_codex_hooks_config(port, &hook_script_path);
    let existing_hooks = hooks
        .get("hooks")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    hooks["hooks"] = merge_codex_hooks(&existing_hooks, &hooks_config);

    let content = match serde_json::to_string_pretty(&hooks) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("hooks.json serialize failed: {}", e);
            host.log_error(&format!("ensure_codex_hooks: {}", msg));
            host.mark_plugin_error(&format!("auto-task: {}", msg));
            return AgentIntegrationResult {
                success: false,
                message: format!("序列化 hooks.json 失败: {}", e),
                skipped: false,
            };
        }
    };

    if let Err(e) = host.fs_write(&hooks_path, &content) {
        let msg = format!("hooks.json write failed: path={:?} err={}", hooks_path, e);
        host.log_error(&format!("ensure_codex_hooks: {}", msg));
        host.mark_plugin_error(&format!("auto-task: {}", msg));
        return AgentIntegrationResult {
            success: false,
            message: format!("写入项目 hooks.json 失败: {}", e),
            skipped: false,
        };
    }

    host.log_info(&format!("Codex hooks configured in {}", hooks_path));

    AgentIntegrationResult {
        success: true,
        message: "项目 Codex hooks 已配置".to_string(),
        skipped: false,
    }
}

/// 清理指定项目的 Codex hooks（幂等，文件不存在视为已清理）
///
/// 1. 从 hooks.json 移除 BedCode 相关条目（保留用户自有 hooks 与其他配置）
/// 2. 删除项目 `.codex/codex_task_hook.py` 脚本
/// 不触碰 `.codex/` 下的用户配置（config.toml / AGENTS.md 等）。
pub fn cleanup_codex_hooks(host: &WasmHost, working_dir: &str) -> AgentIntegrationResult {
    let codex_dir = format!("{}/{}", working_dir, CODEX_CONFIG_DIR_NAME);
    let hooks_path = format!("{}/{}", codex_dir, CODEX_HOOKS_FILE);
    let hook_script_path = format!("{}/{}", codex_dir, CODEX_HOOK_SCRIPT_NAME);

    let mut hooks: serde_json::Value = match host.fs_read(&hooks_path) {
        Ok(Some(content)) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    let mut had_codex_hooks = false;
    if let Some(existing) = hooks.get("hooks").cloned() {
        if is_codex_hooks_configured(&existing) {
            had_codex_hooks = true;
            let cleaned = remove_codex_hooks(&existing);
            if cleaned
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(true)
            {
                hooks.as_object_mut().map(|o| o.remove("hooks"));
            } else {
                hooks["hooks"] = cleaned;
            }

            match serde_json::to_string_pretty(&hooks) {
                Ok(content) => {
                    if let Err(e) = host.fs_write(&hooks_path, &content) {
                        return AgentIntegrationResult {
                            success: false,
                            message: format!("写入 hooks.json 失败: {}", e),
                            skipped: false,
                        };
                    }
                }
                Err(e) => {
                    return AgentIntegrationResult {
                        success: false,
                        message: format!("序列化 hooks.json 失败: {}", e),
                        skipped: false,
                    };
                }
            }
        }
    }

    if let Err(e) = host.fs_delete(&hook_script_path) {
        host.log_warn(&format!(
            "Failed to delete codex hook script {}: {}",
            hook_script_path, e
        ));
    }

    if had_codex_hooks {
        host.log_info(&format!(
            "Cleaned codex hooks from project: {}",
            working_dir
        ));
        AgentIntegrationResult {
            success: true,
            message: "项目 Codex hooks 已清理".to_string(),
            skipped: false,
        }
    } else {
        AgentIntegrationResult {
            success: true,
            message: "项目无 Codex hooks".to_string(),
            skipped: true,
        }
    }
}

/// 模板版本标记：内容升级时递增模板内标记，旧部署副本据此自动重部署
/// （端口匹配检查无法发现脚本内容更新）
const CODEX_HOOK_TEMPLATE_VERSION: &str = "1";

/// 检查已部署脚本是否携带当前模板版本标记
fn codex_hook_version_matches(content: &str) -> bool {
    const MARKER: &str = "@bedcode-template-version ";
    content.find(MARKER).map_or(false, |start| {
        let value_start = start + MARKER.len();
        let digits: String = content[value_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits == CODEX_HOOK_TEMPLATE_VERSION
    })
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
    let pre_tool_use_cmd = format!("{}python \"{}\" pre-tool-use", env_prefix, hook_script_path);
    let post_tool_use_cmd = format!(
        "{}python \"{}\" post-tool-use",
        env_prefix, hook_script_path
    );
    let post_tool_use_fail_cmd = format!(
        "{}python \"{}\" post-tool-use-fail",
        env_prefix, hook_script_path
    );
    let notification_cmd = format!("{}python \"{}\" notification", env_prefix, hook_script_path);
    let stop_cmd = format!("{}python \"{}\" stop", env_prefix, hook_script_path);
    let subagent_stop_cmd = format!(
        "{}python \"{}\" subagent-stop",
        env_prefix, hook_script_path
    );
    let session_end_cmd = format!("{}python \"{}\" session-end", env_prefix, hook_script_path);

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

/// 构建 Codex hooks JSON 配置
///
/// 注册 Codex 生命周期事件，覆盖与 Claude 版一致的状态机：
/// SessionStart → UserPromptSubmit → PermissionRequest → PostToolUse
/// → Stop/SubagentStart/SubagentStop → SessionEnd
///
/// 与 Claude 版差异（对齐 Codex hooks 官方 schema）：
/// - 无 PostToolUseFailure / Notification：工具失败仍走 PostToolUse；
///   等待用户授权由 PermissionRequest 承担（asking + 自动放行）
/// - SessionStart matcher 限定 startup|resume|clear：compact 发生在任务运行
///   中途，推 idle 会把执行中的任务错误置空
/// - 新增 SubagentStart：维护子 agent 计数，替代 Claude Stop 载荷的
///   background_tasks 字段（Codex Stop 无该字段）
/// - SessionEnd 是 advisory 且超时上限 3 秒（文档规定），配置 3
fn build_codex_hooks_config(port: u16, hook_script_path: &str) -> serde_json::Value {
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
    let permission_request_cmd = format!(
        "{}python \"{}\" permission-request",
        env_prefix, hook_script_path
    );
    let post_tool_use_cmd = format!(
        "{}python \"{}\" post-tool-use",
        env_prefix, hook_script_path
    );
    let subagent_start_cmd = format!(
        "{}python \"{}\" subagent-start",
        env_prefix, hook_script_path
    );
    let subagent_stop_cmd = format!(
        "{}python \"{}\" subagent-stop",
        env_prefix, hook_script_path
    );
    let stop_cmd = format!("{}python \"{}\" stop", env_prefix, hook_script_path);
    let session_end_cmd = format!(
        "{}python \"{}\" session-end",
        env_prefix, hook_script_path
    );

    serde_json::json!({
        "SessionStart": [
            {
                "matcher": "startup|resume|clear",
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
        "PermissionRequest": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": permission_request_cmd,
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
        "SubagentStart": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": subagent_start_cmd,
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
        "SessionEnd": [
            {
                "matcher": "",
                "hooks": [
                    {
                        "type": "command",
                        "command": session_end_cmd,
                        "timeout": 3
                    }
                ]
            }
        ]
    })
}

/// 检查 hooks 配置是否包含插件 hook 命令
fn is_plugin_hooks_configured(hooks: &serde_json::Value) -> bool {
    is_script_hooks_configured(hooks, HOOK_SCRIPT_NAME)
}

/// 检查 hooks 配置是否包含 Codex 插件 hook 命令
fn is_codex_hooks_configured(hooks: &serde_json::Value) -> bool {
    is_script_hooks_configured(hooks, CODEX_HOOK_SCRIPT_NAME)
}

/// 检查 hooks 配置是否包含指定脚本名的插件 hook 命令
fn is_script_hooks_configured(hooks: &serde_json::Value, script_name: &str) -> bool {
    let hooks_obj = match hooks.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    // 检查任意事件类型中是否包含目标脚本
    for (_event_type, events) in hooks_obj {
        if let Some(events_arr) = events.as_array() {
            for event in events_arr {
                if let Some(hook_list) = event.get("hooks").and_then(|v| v.as_array()) {
                    for hook in hook_list {
                        if let Some(cmd) = hook.get("command").and_then(|v| v.as_str()) {
                            if cmd.contains(script_name) {
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
    remove_script_hooks(hooks, HOOK_SCRIPT_NAME)
}

/// 移除所有 Codex 插件相关的 hook 条目
fn remove_codex_hooks(hooks: &serde_json::Value) -> serde_json::Value {
    remove_script_hooks(hooks, CODEX_HOOK_SCRIPT_NAME)
}

/// 移除所有包含指定脚本名的插件 hook 条目（保留用户自有 hooks）
fn remove_script_hooks(hooks: &serde_json::Value, script_name: &str) -> serde_json::Value {
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
                                        .map(|cmd| !cmd.contains(script_name))
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
fn merge_hooks(
    existing: &serde_json::Value,
    plugin_hooks: &serde_json::Value,
) -> serde_json::Value {
    merge_script_hooks(existing, plugin_hooks, HOOK_SCRIPT_NAME)
}

/// 合并 Codex hooks 配置：保留非插件 hooks，替换 Codex 插件相关的 hooks
fn merge_codex_hooks(
    existing: &serde_json::Value,
    plugin_hooks: &serde_json::Value,
) -> serde_json::Value {
    merge_script_hooks(existing, plugin_hooks, CODEX_HOOK_SCRIPT_NAME)
}

/// 合并 hooks 配置：保留非目标脚本的 hooks，替换目标脚本相关的 hooks
fn merge_script_hooks(
    existing: &serde_json::Value,
    plugin_hooks: &serde_json::Value,
    script_name: &str,
) -> serde_json::Value {
    let mut result = serde_json::json!({});

    if let (Some(existing_obj), Some(plugin_obj)) = (existing.as_object(), plugin_hooks.as_object())
    {
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
                                        .map(|cmd| cmd.contains(script_name))
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
    is_script_hooks_port_matching(hooks, port, HOOK_SCRIPT_NAME)
}

/// 检查现有 Codex hooks 中的端口是否与当前值匹配
fn codex_hooks_port_matching(hooks: &serde_json::Value, port: u16) -> bool {
    is_script_hooks_port_matching(hooks, port, CODEX_HOOK_SCRIPT_NAME)
}

/// 检查现有 hooks 中指定脚本的端口是否与当前值匹配
///
/// 环境变量前缀格式：BEDCODE_PORT={port}
/// 解析 hook command 中的环境变量，与当前 port 比较
fn is_script_hooks_port_matching(hooks: &serde_json::Value, port: u16, script_name: &str) -> bool {
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
                            if cmd.contains(script_name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_constant_names() {
        assert_eq!(CODEX_CONFIG_DIR_NAME, ".codex");
        assert_eq!(CODEX_HOOKS_FILE, "hooks.json");
        assert_eq!(CODEX_HOOK_SCRIPT_NAME, "codex_task_hook.py");
    }

    #[test]
    fn build_codex_hooks_config_registers_full_state_machine() {
        let config = build_codex_hooks_config(
            9876,
            r"C:\proj\.codex\codex_task_hook.py",
        );
        let hooks = config.as_object().expect("hooks config must be an object");

        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PermissionRequest",
            "PostToolUse",
            "SubagentStart",
            "SubagentStop",
            "Stop",
            "SessionEnd",
        ] {
            assert!(hooks.contains_key(event), "missing event {}", event);
        }

        // compact 发生在任务运行中途，推 idle 会误伤任务状态，必须排除
        assert_eq!(
            hooks["SessionStart"][0]["matcher"],
            "startup|resume|clear"
        );
        // SessionEnd 是 advisory：Codex 上限 3 秒，超时会被杀死
        assert_eq!(hooks["SessionEnd"][0]["hooks"][0]["timeout"], 3);

        for (event, groups) in hooks {
            for group in groups.as_array().expect("hook group must be array") {
                for hook in group["hooks"].as_array().expect("hook list must be array") {
                    let cmd = hook["command"].as_str().unwrap_or_default();
                    assert!(
                        cmd.contains("codex_task_hook.py"),
                        "{}: command missing script: {}",
                        event,
                        cmd
                    );
                    assert!(
                        cmd.starts_with("BEDCODE_PORT=9876 "),
                        "{}: command missing env prefix: {}",
                        event,
                        cmd
                    );
                }
            }
        }
    }

    #[test]
    fn codex_hooks_configured_detection_is_script_specific() {
        let codex = serde_json::json!({
            "Stop": [{
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=8765 python \"C:\\proj\\.codex\\codex_task_hook.py\" stop"
                }]
            }]
        });
        assert!(is_codex_hooks_configured(&codex));
        assert!(!is_plugin_hooks_configured(&codex));

        let claude = serde_json::json!({
            "Stop": [{
                "hooks": [{
                    "type": "command",
                    "command": "BEDCODE_PORT=8765 python \"C:\\proj\\.claude\\auto_task_hook.py\" stop"
                }]
            }]
        });
        assert!(!is_codex_hooks_configured(&claude));
        assert!(is_plugin_hooks_configured(&claude));
    }

    #[test]
    fn remove_codex_hooks_keeps_user_and_claude_entries() {
        let hooks = serde_json::json!({
            "Stop": [
                { "hooks": [{ "type": "command", "command": "user-stop-hook" }] },
                { "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\proj\\.codex\\codex_task_hook.py\" stop" }] },
                { "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\proj\\.claude\\auto_task_hook.py\" stop" }] }
            ],
            "SessionStart": [
                { "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\proj\\.codex\\codex_task_hook.py\" session-start" }] }
            ]
        });

        let cleaned = remove_codex_hooks(&hooks);
        let stop = cleaned["Stop"].as_array().expect("Stop must remain");
        assert_eq!(stop.len(), 2);
        assert!(stop.iter().all(|group| {
            !group["hooks"][0]["command"]
                .as_str()
                .unwrap_or_default()
                .contains("codex_task_hook.py")
        }));
        assert!(!cleaned.as_object().unwrap().contains_key("SessionStart"));
    }

    #[test]
    fn merge_codex_hooks_preserves_user_entries() {
        let existing = serde_json::json!({
            "Stop": [
                { "hooks": [{ "type": "command", "command": "user-stop-hook" }] }
            ],
            "SessionEnd": [
                { "hooks": [{ "type": "command", "command": "user-end-hook" }] }
            ]
        });
        let plugin = build_codex_hooks_config(8765, r"C:\proj\.codex\codex_task_hook.py");

        let merged = merge_codex_hooks(&existing, &plugin);
        let stop = merged["Stop"].as_array().expect("Stop must remain");
        assert_eq!(stop.len(), 2);
        assert!(stop
            .iter()
            .any(|g| g["hooks"][0]["command"].as_str() == Some("user-stop-hook")));
        assert_eq!(merged["SessionEnd"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn claude_hooks_helpers_survive_shared_refactor() {
        // 参数化重构后 Claude 原有语义必须保持不变：识别、清理、合并均按
        // auto_task_hook.py 精确匹配，不误伤 codex 与用户 hooks
        let mixed = serde_json::json!({
            "Stop": [
                { "hooks": [{ "type": "command", "command": "user-stop-hook" }] },
                { "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\p\\.claude\\auto_task_hook.py\" stop" }] },
                { "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\p\\.codex\\codex_task_hook.py\" stop" }] }
            ]
        });
        assert!(is_plugin_hooks_configured(&mixed));

        let cleaned = remove_plugin_hooks(&mixed);
        let stop = cleaned["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert!(stop.iter().all(|group| {
            let cmd = group["hooks"][0]["command"].as_str().unwrap_or_default();
            !cmd.contains("auto_task_hook.py")
        }));

        let merged = merge_hooks(
            &mixed,
            &serde_json::json!({
                "Stop": [{
                    "hooks": [{ "type": "command", "command": "BEDCODE_PORT=8765 python \"C:\\p\\.claude\\auto_task_hook.py\" stop" }]
                }]
            }),
        );
        let stop = merged["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 3);
        assert!(stop
            .iter()
            .any(|g| g["hooks"][0]["command"].as_str() == Some("user-stop-hook")));
        assert!(stop
            .iter()
            .any(|g| g["hooks"][0]["command"].as_str().unwrap_or_default().contains("codex_task_hook.py")));
    }

    #[test]
    fn codex_hooks_port_matching_detects_current_port() {
        let hooks = build_codex_hooks_config(8765, r"C:\proj\.codex\codex_task_hook.py");
        assert!(codex_hooks_port_matching(&hooks, 8765));
        assert!(!codex_hooks_port_matching(&hooks, 9000));
    }

    #[test]
    fn codex_hook_version_marker_matching() {
        let content = "# @bedcode-template-version 1\nprint('ok')\n";
        assert!(codex_hook_version_matches(content));
        assert!(!codex_hook_version_matches("no marker here"));
    }
}
