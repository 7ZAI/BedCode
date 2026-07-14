//! Plugin Loader（移动端）
//!
//! APK assets 内置插件解压 + app_data_dir 插件扫描
//! 解析 plugin.json，编译并实例化 WASM 模块

use crate::plugin::storage::PluginStorage;
use crate::plugin::types::*;
use crate::plugin::wasm_runtime::{LoadedWasmPlugin, WasmHostContext, WasmRuntime};
use crate::system::constants::plugin::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// 插件加载器
pub struct PluginLoader;

impl PluginLoader {
    /// 解压 APK assets 中的内置插件到 app_data_dir
    ///
    /// 仅在首次启动或版本变更时解压，已存在的文件跳过
    pub fn extract_apk_plugins(app_data_dir: &Path, _app_handle: &tauri::AppHandle) -> crate::Result<()> {
        let plugins_data_dir = app_data_dir.join(PLUGIN_DATA_DIR);
        fs::create_dir_all(&plugins_data_dir)?;

        // Android 平台：通过 AssetManager 解压 assets/plugins/ 到 plugins_data_dir
        #[cfg(target_os = "android")]
        {
            // Android AssetManager 访问需要通过 JNI 或 tauri asset protocol
            // 预留接口，后续 Android 集成时完善
            tracing::info!("[PluginLoader] Android asset extraction (placeholder)");
        }

        Ok(())
    }

    /// 扫描插件目录并加载所有 plugin.json
    ///
    /// 对 pluginType: "wasm" 的插件，编译 + 实例化 WASM 模块
    /// 对 pluginType: "ts-only" 的插件，仅注册 manifest
    pub fn load_all(
        plugins_dir: &Path,
        wasm_runtime: &WasmRuntime,
        wasm_host_ctx: &Arc<WasmHostContext>,
    ) -> (HashMap<String, LoadedPlugin>, HashMap<String, LoadedWasmPlugin>) {
        tracing::info!("[PluginLoader] Scanning plugin directory: {:?}", plugins_dir);

        if !plugins_dir.exists() {
            tracing::warn!("[PluginLoader] Plugin directory does not exist: {:?}", plugins_dir);
            return (HashMap::new(), HashMap::new());
        }

        let mut plugins = HashMap::new();
        let mut wasm_plugins = HashMap::new();

        let entries = match fs::read_dir(plugins_dir) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::error!("[PluginLoader] Failed to read plugin directory: {}", e);
                return (HashMap::new(), HashMap::new());
            }
        };

        let mut dir_count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // 跳过临时目录和缓存目录
            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            if dir_name.starts_with('_') || dir_name.starts_with('.') {
                continue;
            }
            dir_count += 1;

            let manifest_path = path.join(PLUGIN_MANIFEST_FILE);
            if !manifest_path.exists() {
                tracing::debug!("[PluginLoader] Skipping {:?}: no plugin.json", path);
                continue;
            }

            match Self::load_manifest(&manifest_path) {
                Ok(manifest) => {
                    let plugin_id = manifest.id.clone();
                    let extension_path = path.to_string_lossy().to_string();

                    let source = PluginSource::ApkAsset;

                    // WASM 插件：编译 + 实例化
                    if manifest.plugin_type == PluginType::Wasm && !manifest.rust_library.is_empty() {
                        let wasm_file = path.join(format!("{}{}", manifest.rust_library, WASM_FILE_EXT));
                        if wasm_file.exists() {
                            match wasm_runtime.compile_module_from_file(&wasm_file) {
                                Ok(module) => {
                                    match wasm_runtime.instantiate(&module, &plugin_id, wasm_host_ctx.clone()) {
                                        Ok(loaded_wasm) => {
                                            tracing::info!(
                                                "[PluginLoader] WASM plugin loaded: {} v{}",
                                                manifest.id, manifest.version
                                            );
                                            wasm_plugins.insert(plugin_id.clone(), loaded_wasm);
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                "[PluginLoader] WASM instantiation failed for '{}': {}",
                                                manifest.id, e
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "[PluginLoader] WASM compilation failed for '{}': {}",
                                        manifest.id, e
                                    );
                                }
                            }
                        } else {
                            tracing::warn!(
                                "[PluginLoader] WASM file not found for '{}': {:?}",
                                manifest.id, wasm_file
                            );
                        }
                    }

                    let permissions: std::collections::HashSet<String> =
                        manifest.permissions.iter().cloned().collect();

                    let loaded = LoadedPlugin {
                        manifest,
                        state: PluginState::Loaded,
                        granted_permissions: permissions,
                        source,
                        extension_path,
                    };

                    plugins.insert(plugin_id, loaded);
                }
                Err(e) => {
                    let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                    tracing::error!("[PluginLoader] Failed to load plugin from {:?}: {}", dir_name, e);
                }
            }
        }

        tracing::info!(
            "[PluginLoader] Scanned {} dir(s), loaded {} plugin(s), {} WASM instance(s)",
            dir_count, plugins.len(), wasm_plugins.len()
        );

        (plugins, wasm_plugins)
    }

    /// 解析单个 plugin.json
    fn load_manifest(path: &std::path::PathBuf) -> crate::Result<PluginManifest> {
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

        // WASM 插件必须有 rustLibrary 字段
        if manifest.plugin_type == PluginType::Wasm && manifest.rust_library.is_empty() {
            return Err(crate::AppError::Plugin(
                "WASM plugin.json missing rustLibrary field".to_string(),
            ));
        }

        Ok(manifest)
    }
}
