//! Plugin Loader（移动端）
//!
//! APK assets 内置插件解压 + app_data_dir 插件扫描
//! 解析 plugin.json，编译并实例化 WASM 模块

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
    /// 解压内置插件到 app_data_dir/plugins
    ///
    /// Android：经 Kotlin PluginAssetExtractor 从 APK assets 解压（按来源标记跳过已解压）。
    /// 非 Android（桌面 dev 窗口）：从源码 resources/plugins/mobile 复制（仅 debug 构建）。
    pub async fn extract_apk_plugins(
        app_data_dir: &Path,
        app_version: &str,
    ) -> crate::Result<()> {
        let plugins_data_dir = app_data_dir.join(PLUGIN_DATA_DIR);
        fs::create_dir_all(&plugins_data_dir)?;

        #[cfg(target_os = "android")]
        {
            crate::plugin::android_plugins::extract_bundled_plugins(app_version).await?;
        }

        #[cfg(not(target_os = "android"))]
        {
            Self::dev_copy_plugins(&plugins_data_dir, app_version)?;
        }

        Ok(())
    }

    /// 桌面 dev 模式：从源码资源目录复制内置插件（仅 debug 构建）
    ///
    /// 移动端应用以桌面窗口开发时没有 APK assets，
    /// 从 CARGO_MANIFEST_DIR/resources/plugins/mobile 复制，标记逻辑与 Android 一致。
    #[cfg(not(target_os = "android"))]
    fn dev_copy_plugins(plugins_data_dir: &Path, app_version: &str) -> crate::Result<()> {
        if !cfg!(debug_assertions) {
            return Ok(());
        }
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| crate::AppError::Plugin("CARGO_MANIFEST_DIR not set".to_string()))?;
        let src_root = Path::new(&manifest_dir)
            .join("resources")
            .join("plugins")
            .join("mobile");
        if !src_root.exists() {
            return Ok(());
        }

        let expected = format!("{}:{}", SOURCE_APK_ASSET, app_version);
        let mut copied = 0;
        for entry in fs::read_dir(&src_root)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().to_string();
            if id.starts_with('.') || !entry.path().is_dir() {
                continue;
            }
            let dest = plugins_data_dir.join(&id);
            let marker = dest.join(PLUGIN_SOURCE_MARKER);
            if marker.exists()
                && fs::read_to_string(&marker)
                    .unwrap_or_default()
                    .trim()
                    == expected
            {
                continue;
            }
            if dest.exists() {
                fs::remove_dir_all(&dest)?;
            }
            copy_dir_all(&entry.path(), &dest)?;
            fs::write(&marker, &expected)?;
            copied += 1;
            tracing::info!("[PluginLoader] Dev-copied builtin plugin: {}", id);
        }
        tracing::info!(copied, "[PluginLoader] Dev plugin copy complete");
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

                    let source = Self::detect_source(&path);

                    // WASM 插件：编译 + 实例化
                    if manifest.plugin_type == PluginType::Wasm && !manifest.rust_library.is_empty() {
                        let wasm_file = path.join(format!("{}{}", manifest.rust_library, WASM_FILE_EXT));
                        if wasm_file.exists() {
                            match wasm_runtime.compile_module_from_file(&wasm_file) {
                                Ok(module) => {
                                    // 与 SDK PermissionManager::grant_permissions 语义一致：storage 默认授予
                                    let mut granted: std::collections::HashSet<String> =
                                        manifest.permissions.iter().cloned().collect();
                                    granted.insert(
                                        bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE
                                            .to_string(),
                                    );
                                    match wasm_runtime.instantiate(
                                        &module,
                                        &plugin_id,
                                        wasm_host_ctx.clone(),
                                        granted,
                                    ) {
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

    /// 根据 .bedcode-source 标记判断插件来源
    fn detect_source(plugin_dir: &Path) -> PluginSource {
        let marker = plugin_dir.join(PLUGIN_SOURCE_MARKER);
        if let Ok(content) = fs::read_to_string(&marker) {
            let content = content.trim();
            if content.starts_with(SOURCE_APK_ASSET) {
                return PluginSource::ApkAsset;
            }
            if content == SOURCE_FILE_INSTALL {
                return PluginSource::FileInstall;
            }
            if content == SOURCE_REMOTE_DOWNLOAD {
                return PluginSource::RemoteDownload;
            }
        }
        // 无标记（历史产物）按内置处理
        PluginSource::ApkAsset
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

/// 递归复制目录（dev 插件复制用）
fn copy_dir_all(src: &Path, dest: &Path) -> crate::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
