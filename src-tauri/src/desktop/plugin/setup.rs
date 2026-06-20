//! Plugin Setup
//!
//! 应用启动时自动检查并配置 bedcode-plugin：
//! 1. 校验/生成 token
//! 2. 从 bundle resource 复制插件文件到 ~/.claude/plugins/bedcode/
//! 3. 注入 BEDCODE_PORT 和 BEDCODE_TOKEN 环境变量到 hooks.json

use crate::shared::system::config::AppConfig;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 插件配置结果，通过 Tauri event 发送到前端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSetupResult {
    /// 是否成功
    pub success: bool,
    /// 提示消息
    pub message: String,
    /// 是否新生成了 token
    pub token_generated: bool,
}

/// 插件安装目标目录（相对于用户 home）
const PLUGIN_DIR_NAME: &str = "bedcode";

/// 获取插件安装目标路径：~/.claude/plugins/bedcode/
fn plugin_install_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("plugins").join(PLUGIN_DIR_NAME))
}

/// 执行插件自动配置
///
/// 返回 PluginSetupResult 描述配置结果
pub fn setup_plugin(
    config: &mut AppConfig,
    config_path: &PathBuf,
    resource_dir: &PathBuf,
) -> PluginSetupResult {
    tracing::info!("setup_plugin called, resource_dir={}", resource_dir.display());

    // 1. 校验/生成 token
    let token_generated = config.ensure_valid_token();
    if token_generated {
        if let Err(e) = config.save_to(config_path) {
            tracing::warn!("Failed to save config after token generation: {}", e);
        }
    }

    let token = config.plugin.token.clone();
    let port = config.network.port;

    // 2. 获取插件安装目标路径
    let install_dir = match plugin_install_dir() {
        Some(dir) => dir,
        None => {
            return PluginSetupResult {
                success: false,
                message: "无法获取用户主目录".to_string(),
                token_generated,
            };
        }
    };

    // 3. 从 bundle resource 复制插件文件
    // Tauri 2.x 将 ../ 路径映射为 _up_ 子目录，资源实际位于 resource_dir/_up_/scripts/bedcode-plugin/
    let plugin_resource = resource_dir.join("_up_").join("scripts").join("bedcode-plugin");
    tracing::info!("Looking for plugin resources at: {}", plugin_resource.display());
    if !plugin_resource.exists() {
        return PluginSetupResult {
            success: false,
            message: format!("插件资源目录不存在: {}", plugin_resource.display()),
            token_generated,
        };
    }

    // 4. 检查是否需要安装（版本比对）
    let needs_install = needs_plugin_install(&install_dir, &plugin_resource);

    if needs_install {
        tracing::info!("Installing bedcode-plugin to {}", install_dir.display());

        // 清除旧安装
        if install_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&install_dir) {
                tracing::warn!("Failed to remove old plugin dir: {}", e);
            }
        }

        // 创建目标目录
        if let Err(e) = fs::create_dir_all(&install_dir) {
            return PluginSetupResult {
                success: false,
                message: format!("创建插件目录失败: {}", e),
                token_generated,
            };
        }

        // 复制插件文件
        if let Err(e) = copy_dir_recursive(&plugin_resource, &install_dir) {
            return PluginSetupResult {
                success: false,
                message: format!("复制插件文件失败: {}", e),
                token_generated,
            };
        }

        tracing::info!("Plugin files copied successfully");
    } else {
        tracing::info!("Plugin already installed with matching version, skipping copy");
    }

    // 5. 注入环境变量到 hooks.json
    if let Err(e) = inject_env_to_hooks(&install_dir, port, &token) {
        tracing::warn!("Failed to inject env to hooks.json: {}", e);
        return PluginSetupResult {
            success: false,
            message: format!("注入环境变量失败: {}", e),
            token_generated,
        };
    }

    PluginSetupResult {
        success: true,
        message: "插件已配置".to_string(),
        token_generated,
    }
}

/// 比对版本号判断是否需要安装
fn needs_plugin_install(install_dir: &PathBuf, resource_dir: &PathBuf) -> bool {
    let installed_manifest = install_dir.join(".claude-plugin").join("plugin.json");
    let resource_manifest = resource_dir.join(".claude-plugin").join("plugin.json");

    // 已安装的 manifest 不存在 → 需要安装
    if !installed_manifest.exists() {
        return true;
    }

    // 读取版本号
    let installed_version = read_plugin_version(&installed_manifest);
    let resource_version = read_plugin_version(&resource_manifest);

    match (installed_version, resource_version) {
        (Some(v1), Some(v2)) => v1 != v2,
        _ => true,
    }
}

/// 从 plugin.json 读取 version 字段
fn read_plugin_version(path: &PathBuf) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("version").and_then(|v| v.as_str()).map(String::from)
}

/// 递归复制目录
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// 注入 BEDCODE_PORT 和 BEDCODE_TOKEN 到 hooks.json
///
/// 修改 hooks.json 中所有 command 字段，在脚本路径前添加环境变量
fn inject_env_to_hooks(install_dir: &PathBuf, port: u16, token: &str) -> Result<()> {
    let hooks_path = install_dir.join("hooks").join("hooks.json");

    if !hooks_path.exists() {
        return Err(crate::AppError::Internal(
            format!("hooks.json not found: {}", hooks_path.display())
        ));
    }

    let content = fs::read_to_string(&hooks_path)?;
    let mut hooks: serde_json::Value = serde_json::from_str(&content)?;

    // 遍历所有 hook，注入环境变量到 command 字段
    inject_env_to_commands(&mut hooks, port, token);

    let updated = serde_json::to_string_pretty(&hooks)?;
    fs::write(&hooks_path, updated)?;

    tracing::info!("Injected BEDCODE_PORT={} and BEDCODE_TOKEN into hooks.json", port);
    Ok(())
}

/// 递归遍历 JSON，找到所有 command 字段并注入环境变量
fn inject_env_to_commands(value: &mut serde_json::Value, port: u16, token: &str) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(command) = map.get_mut("command") {
                if let Some(cmd_str) = command.as_str() {
                    // 跳过已注入的命令（幂等性），未注入则注入环境变量
                    if !cmd_str.contains("BEDCODE_PORT=") {
                        let new_cmd = if cmd_str.ends_with(".sh") || cmd_str.contains(".sh ") {
                            // Unix: VAR=value VAR2=value script.sh
                            format!("BEDCODE_PORT={} BEDCODE_TOKEN={} {}", port, token, cmd_str)
                        } else if cmd_str.ends_with(".cmd") || cmd_str.contains(".cmd ") {
                            // Windows: set VAR=value && set VAR2=value && script.cmd
                            format!("set BEDCODE_PORT={} && set BEDCODE_TOKEN={} && {}", port, token, cmd_str)
                        } else {
                            // 未知格式，使用 Unix 风格
                            format!("BEDCODE_PORT={} BEDCODE_TOKEN={} {}", port, token, cmd_str)
                        };
                        *command = serde_json::Value::String(new_cmd);
                    }
                }
            }
            // 递归处理嵌套对象
            for (_, v) in map.iter_mut() {
                inject_env_to_commands(v, port, token);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                inject_env_to_commands(item, port, token);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_env_to_commands_unix() {
        let mut hooks = serde_json::json!({
            "hooks": {
                "SessionStart": [{
                    "matcher": "*",
                    "hooks": [{
                        "type": "command",
                        "command": "${CLAUDE_PLUGIN_ROOT}/scripts/session-start.sh",
                        "timeout": 5
                    }]
                }]
            }
        });

        inject_env_to_commands(&mut hooks, 8765, "testtoken123456");

        let command = hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"].as_str().unwrap();
        assert!(command.starts_with("BEDCODE_PORT=8765"));
        assert!(command.contains("BEDCODE_TOKEN=testtoken123456"));
        assert!(command.ends_with(".sh"));
    }

    #[test]
    fn test_inject_env_to_commands_windows() {
        let mut hooks = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{
                        "type": "command",
                        "command": "${CLAUDE_PLUGIN_ROOT}/scripts/write-event.cmd",
                        "timeout": 5
                    }]
                }]
            }
        });

        inject_env_to_commands(&mut hooks, 8765, "testtoken123456");

        let command = hooks["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap();
        assert!(command.starts_with("set BEDCODE_PORT=8765"));
        assert!(command.contains("set BEDCODE_TOKEN=testtoken123456"));
        assert!(command.ends_with(".cmd"));
    }

    #[test]
    fn test_inject_env_idempotent() {
        let mut hooks = serde_json::json!({
            "command": "BEDCODE_PORT=8765 BEDCODE_TOKEN=old ${CLAUDE_PLUGIN_ROOT}/scripts/session-start.sh"
        });

        inject_env_to_commands(&mut hooks, 9999, "newtoken");

        // 已有 BEDCODE_PORT 不应重复注入
        let command = hooks["command"].as_str().unwrap();
        assert!(!command.contains("9999"));
        assert!(command.contains("old"));
    }

    #[test]
    fn test_read_plugin_version() {
        let json = r#"{"version": "1.0.0", "name": "bedcode"}"#;
        let dir = std::env::temp_dir().join("bedcode_test_version");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("plugin.json");
        fs::write(&path, json).unwrap();

        let version = read_plugin_version(&path);
        assert_eq!(version, Some("1.0.0".to_string()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_needs_plugin_install_missing() {
        let dir = std::env::temp_dir().join("bedcode_test_install_missing");
        let _ = fs::remove_dir_all(&dir);
        // 目录不存在 → 需要安装
        assert!(needs_plugin_install(&dir.join("nonexistent"), &dir));
    }
}
