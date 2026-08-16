//! Plugin Loader（移动端）
//!
//! APK assets 内置插件解压 + app_data_dir 插件扫描
//! 解析 plugin.json，编译并实例化 WASM 组件（Component Model，迁移 ticket 06）

use crate::plugin::types::*;
use crate::plugin::validation::{validate_dir_binding, validate_plugin_id};
use crate::plugin::wasm_runtime::{LoadedComponentPlugin, WasmHostContext, WasmRuntime};
use crate::system::constants::plugin::*;
use std::collections::{HashMap, HashSet};
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
    /// 对 pluginType: "wasm" 的插件，编译 + 实例化 WASM 组件（Component Model，
    /// 一次性切割无共存：产物必须为组件，core module 加载报错即检查员）
    /// 对 pluginType: "ts-only" 的插件，仅注册 manifest
    pub(crate) fn load_all(
        plugins_dir: &Path,
        wasm_runtime: &WasmRuntime,
        wasm_host_ctx: &Arc<WasmHostContext>,
    ) -> (HashMap<String, LoadedPlugin>, HashMap<String, LoadedComponentPlugin>) {
        tracing::info!("[PluginLoader] Scanning plugin directory: {:?}", plugins_dir);

        if !plugins_dir.exists() {
            tracing::warn!("[PluginLoader] Plugin directory does not exist: {:?}", plugins_dir);
            return (HashMap::new(), HashMap::new());
        }

        let mut plugins = HashMap::new();
        let mut wasm_plugins = HashMap::new();
        // 已加载 id 集合：重复 id 先到先得，后出现的目录拒绝加载，
        // 防止冒名插件顶替已加载插件（HashMap insert 覆盖语义是漏洞本体）
        let mut seen_ids: HashSet<String> = HashSet::new();

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

                    // ==================== 身份校验（防冒名顶替） ====================
                    // 1. id 必须为反向域名格式（拒绝大写/下划线/单段等非约定格式）
                    if !validate_plugin_id(&plugin_id) {
                        tracing::error!(
                            "[PluginLoader] Rejecting plugin from {:?}: invalid id format {:?}",
                            dir_name, plugin_id
                        );
                        continue;
                    }
                    // 2. 目录名必须与 manifest id 一致（卸载/文件服务路径依赖此约定）
                    if !validate_dir_binding(&dir_name, &plugin_id) {
                        tracing::error!(
                            "[PluginLoader] Rejecting plugin {:?} from {:?}: dir name does not match manifest id (possible impersonation)",
                            plugin_id, dir_name
                        );
                        continue;
                    }
                    // 3. 重复 id：先到先得，后到目录拒绝（防静默覆盖已加载插件）
                    if !seen_ids.insert(plugin_id.clone()) {
                        tracing::error!(
                            "[PluginLoader] Rejecting duplicate plugin id {:?} from {:?}: already loaded from another directory",
                            plugin_id, dir_name
                        );
                        continue;
                    }

                    let source = Self::detect_source(&path);

                    // WASM 插件：编译 + 实例化（组件路径；core module 在此编译失败，
                    // 报错信息明确——旧 ABI 产物残留在切换期即暴露）
                    if manifest.plugin_type == PluginType::Wasm && !manifest.rust_library.is_empty() {
                        let wasm_file = path.join(format!("{}{}", manifest.rust_library, WASM_FILE_EXT));
                        if wasm_file.exists() {
                            match wasm_runtime.compile_component_from_file(&wasm_file) {
                                Ok(component) => {
                                    // 与 SDK PermissionManager::grant_permissions 语义一致：storage 默认授予
                                    let mut granted: std::collections::HashSet<String> =
                                        manifest.permissions.iter().cloned().collect();
                                    granted.insert(
                                        bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE
                                            .to_string(),
                                    );
                                    match wasm_runtime.instantiate_component(
                                        &component,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;

    /// 迁移 ticket 06 验收：生产加载路径（PluginLoader::load_all）直接吃组件产物
    ///
    /// 覆盖 manifest 解析 → 组件编译（AOT 缓存）→ instantiate_component → 实例落表；
    /// 再经实例调用 activate 导出（SDK wasm_entry! 宏产物）。
    /// 与 component.rs 的 instantiate 直测互补：此处验证 loader/manager 接线本身。
    #[test]
    fn test_load_all_loads_component_plugin() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let tmp = tempfile::tempdir().expect("tempdir");

            // 插件目录布局：{plugins_dir}/{plugin_id}/plugin.json + {rustLibrary}.wasm
            // （与 APK assets / dev 资源目录解压后的布局一致）
            let plugin_dir = tmp.path().join("com.bedcode.auto-task");
            std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
            std::fs::write(
                plugin_dir.join(PLUGIN_MANIFEST_FILE),
                r#"{
                    "id": "com.bedcode.auto-task",
                    "name": "Auto Task",
                    "version": "1.0.0-beta",
                    "pluginType": "wasm",
                    "rustLibrary": "bedcode_plugin_auto_task"
                }"#,
            )
            .expect("write plugin.json");
            // 真实 SDK 宏产物（wasm_entry! 8 组导出），组件形态字节已在 build 助手断言
            std::fs::write(
                plugin_dir.join("bedcode_plugin_auto_task.wasm"),
                &crate::plugin::wasm_runtime::component::tests::build_auto_task_component(),
            )
            .expect("write component wasm");

            let runtime = WasmRuntime::new(Some(tmp.path().join("aot"))).expect("wasm runtime");
            let host_ctx = crate::plugin::wasm_runtime::component::tests::build_host_ctx(&tmp);

            let (plugins, wasm_plugins) = PluginLoader::load_all(tmp.path(), &runtime, &host_ctx);

            // manifest 注册 + 组件实例落表
            assert!(plugins.contains_key("com.bedcode.auto-task"));
            let mut loaded = wasm_plugins
                .into_iter()
                .find(|(id, _)| id == "com.bedcode.auto-task")
                .expect("auto-task 组件必须被 loader 实例化")
                .1;

            // 实例可用：activate 导出调用（宏内 HostLog 接线走真实 host 日志）
            assert_eq!(loaded.activate().expect("activate"), 0);

            // AOT 缓存产物落在宿主 cache 目录（组件缓存 `c` 前缀）
            let cache_files: Vec<_> = std::fs::read_dir(tmp.path().join("aot"))
                .expect("aot dir")
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "cwasm").unwrap_or(false))
                .collect();
            assert!(
                cache_files.iter().any(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with('c')),
                "组件 AOT 缓存应以 c 前缀命名"
            );
        });
    }
}
