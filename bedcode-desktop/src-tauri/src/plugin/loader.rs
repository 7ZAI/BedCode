//! Plugin Loader
//!
//! 扫描插件目录，解析所有 plugin.json
//! 验证必填字段和权限合法性，返回已加载的插件列表
//! 仅处理文件扫描加载，Rust+TS cdylib 插件由 PluginHost 通过 CdylibLoader 加载

use crate::plugin::permission::PermissionManager;
use crate::plugin::types::{LoadedPlugin, PluginSource};
use bedcode_plugin_api::{PluginManifest, PluginState, PluginType};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 插件加载器
pub struct PluginLoader;

impl PluginLoader {
    /// 扫描插件目录并加载所有 plugin.json
    ///
    /// 目录约定：`plugins/desktop/{plugin-id}/plugin.json`
    /// 解析失败的插件跳过并记录警告，不影响其他插件
    pub fn load_all(plugins_dir: &Path, permission_mgr: &PermissionManager) -> HashMap<String, LoadedPlugin> {
        if !plugins_dir.exists() {
            tracing::info!("Plugin directory does not exist: {:?}", plugins_dir);
            return HashMap::new();
        }

        let mut plugins = HashMap::new();
        let entries = match fs::read_dir(plugins_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!("Failed to read plugin directory: {}", e);
                return HashMap::new();
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("plugin.json");
            if !manifest_path.exists() {
                tracing::debug!("Skipping {:?}: no plugin.json", path);
                continue;
            }

            match Self::load_manifest(&manifest_path) {
                Ok(manifest) => {
                    let plugin_id = manifest.id.clone();
                    let extension_path = path.to_string_lossy().to_string();

                    // TS-only 插件强制设置 plugin_type
                    let manifest = manifest;
                    if manifest.plugin_type == PluginType::TsOnly && !manifest.main.is_empty() {
                        // 保留 manifest 中的 plugin_type，若未指定则默认 TsOnly
                    }

                    // 授权并过滤非法权限
                    let granted = permission_mgr.grant_permissions(
                        &plugin_id,
                        &manifest.permissions,
                    );

                    // 根据 rust_library 字段判断来源：有 cdylib 则为 Cdylib，否则为 FileScan
                    let source = if !manifest.rust_library.is_empty() {
                        PluginSource::Cdylib
                    } else {
                        PluginSource::FileScan
                    };

                    let loaded = LoadedPlugin {
                        manifest,
                        state: PluginState::Loaded,
                        granted_permissions: granted,
                        extension_path,
                        activated_at: None,
                        source,
                    };

                    tracing::info!("Plugin loaded: {} v{}", loaded.manifest.id, loaded.manifest.version);
                    plugins.insert(plugin_id, loaded);
                }
                Err(e) => {
                    let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                    tracing::warn!("Failed to load plugin from {:?}: {}", dir_name, e);
                }
            }
        }

        tracing::info!("Loaded {} file-based plugin(s)", plugins.len());
        plugins
    }

    /// 解析单个 plugin.json
    fn load_manifest(path: &PathBuf) -> crate::Result<PluginManifest> {
        let content = fs::read_to_string(path)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to read plugin.json: {}", e)))?;

        let manifest: PluginManifest = serde_json::from_str(&content)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to parse plugin.json: {}", e)))?;

        if manifest.id.is_empty() {
            return Err(crate::AppError::Plugin("plugin.json missing id field".to_string()));
        }
        if manifest.name.is_empty() {
            return Err(crate::AppError::Plugin("plugin.json missing name field".to_string()));
        }
        if manifest.version.is_empty() {
            return Err(crate::AppError::Plugin("plugin.json missing version field".to_string()));
        }

        // TS-only 插件必须有 main 字段
        if manifest.plugin_type == PluginType::TsOnly && manifest.main.is_empty() {
            return Err(crate::AppError::Plugin(
                "TS-only plugin.json missing main field".to_string(),
            ));
        }

        // MVP 只支持 inline 模式
        if manifest.sandbox != "inline" {
            return Err(crate::AppError::Plugin(format!(
                "Unsupported sandbox mode: {}, MVP only supports inline",
                manifest.sandbox
            )));
        }

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_manifest_valid() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let plugin_dir = tmp_dir.path().join("com.test.plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = serde_json::json!({
            "id": "com.test.plugin",
            "name": "Test Plugin",
            "version": "1.0.0",
            "main": "index.ts",
            "permissions": ["terminal:input", "storage"],
            "contributes": {
                "commands": [{
                    "id": "test.hello",
                    "title": "Hello"
                }]
            }
        });

        let manifest_path = plugin_dir.join("plugin.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest_json).unwrap()).unwrap();

        let manifest = PluginLoader::load_manifest(&manifest_path).unwrap();
        assert_eq!(manifest.id, "com.test.plugin");
        assert_eq!(manifest.permissions.len(), 2);
        assert_eq!(manifest.contributes.commands.len(), 1);
    }

    #[test]
    fn test_load_manifest_missing_id() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let plugin_dir = tmp_dir.path().join("bad-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = serde_json::json!({
            "name": "No ID",
            "version": "1.0.0",
            "main": "index.ts"
        });

        let manifest_path = plugin_dir.join("plugin.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest_json).unwrap()).unwrap();

        let result = PluginLoader::load_manifest(&manifest_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_manifest_unsupported_sandbox() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let plugin_dir = tmp_dir.path().join("isolated-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let manifest_json = serde_json::json!({
            "id": "com.test.isolated",
            "name": "Isolated Plugin",
            "version": "1.0.0",
            "main": "index.ts",
            "sandbox": "isolated"
        });

        let manifest_path = plugin_dir.join("plugin.json");
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest_json).unwrap()).unwrap();

        let result = PluginLoader::load_manifest(&manifest_path);
        assert!(result.is_err());
    }
}
