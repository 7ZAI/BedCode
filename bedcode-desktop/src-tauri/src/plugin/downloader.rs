//! Plugin Downloader / Installer（桌面端）
//!
//! 插件包（zip）本地安装：选择 zip → 解压 → manifest/身份校验 → 路径穿越防护
//! → wasm 存在性校验 → 写来源标记 → 移动到用户插件目录（app_data_dir/plugins）
//!
//! 分发单元为单个 zip（内含 plugin.json、index.js 与可选的 .wasm），与
//! scripts/package-plugins.mjs 产物（dist/plugin-packages/<target>/<id>.zip）一致。
//! 参考移动端 downloader.rs（wasm_hash 校验因桌面端 PluginManifest 无该字段而省略，
//! 仅校验 rust_library 声明的 wasm 文件存在）。

use crate::plugin::manager::validation::validate_plugin_id;
use crate::system::constants::plugin::{
    PLUGIN_DOWNLOAD_TEMP_DIR, PLUGIN_MANIFEST_FILE, PLUGIN_SOURCE_MARKER, SOURCE_FILE_INSTALL, WASM_FILE_EXT,
};
use crate::Result;
use bedcode_plugin_api::PluginManifest;
use std::io::Read;
use std::path::Path;

/// 插件下载安装器
pub struct PluginDownloader;

impl PluginDownloader {
    /// 从本地 zip 插件包安装
    ///
    /// 1. 打开 zip，校验 plugin.json 必填字段与 id 格式
    /// 2. 解压到临时目录（路径穿越防护）
    /// 3. wasm 文件存在性校验（manifest 声明 rust_library 时）
    /// 4. 写来源标记（file-install）
    /// 5. 移动到 user_plugins_dir/{plugin_id}/
    ///
    /// 拒绝覆盖已存在的同 id 插件（无签名链时无法区分「同作者更新」与
    /// 「冒名顶替替换」，升级需先卸载旧版本，与移动端一致）。
    pub fn install_from_file(zip_path: &str, user_plugins_dir: &Path) -> Result<String> {
        let zip_path = Path::new(zip_path);
        if !zip_path.exists() {
            return Err(crate::AppError::Plugin(format!(
                "Plugin package not found: {}",
                zip_path.display()
            )));
        }
        Self::install_zip(zip_path, user_plugins_dir)
    }

    /// zip 解压安装（本地文件）
    fn install_zip(zip_path: &Path, user_plugins_dir: &Path) -> Result<String> {
        // 1. 打开 zip
        let file = std::fs::File::open(zip_path)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to open plugin package: {}", e)))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| crate::AppError::Plugin(format!("Invalid plugin package: {}", e)))?;

        // 2. 读取并校验 manifest
        let mut manifest_str = String::new();
        archive
            .by_name(PLUGIN_MANIFEST_FILE)
            .map_err(|e| crate::AppError::Plugin(format!("Plugin package missing plugin.json: {}", e)))?
            .read_to_string(&mut manifest_str)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to read plugin.json: {}", e)))?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_str)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to parse plugin.json: {}", e)))?;
        // 身份校验：id 必须为反向域名格式（防伪造 id 冒名顶替/路径注入）
        if !validate_plugin_id(&manifest.id) {
            return Err(crate::AppError::Plugin(format!(
                "plugin.json id {:?} is invalid: must be a reverse-domain name like com.example.plugin",
                manifest.id
            )));
        }
        if manifest.name.is_empty() {
            return Err(crate::AppError::Plugin("plugin.json missing name field".to_string()));
        }
        if manifest.version.is_empty() {
            return Err(crate::AppError::Plugin("plugin.json missing version field".to_string()));
        }

        let plugin_id = manifest.id.clone();
        let temp_dir = user_plugins_dir.join(PLUGIN_DOWNLOAD_TEMP_DIR).join(&plugin_id);

        // 3. 解压到临时目录
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to clear temp dir: {}", e)))?;
        }
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to create temp dir: {}", e)))?;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to read plugin package entry: {}", e)))?;
            let name = entry.name().to_string();
            if entry.is_dir() {
                continue;
            }
            if !Self::is_safe_zip_name(&name) {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin package contains unsafe path: {}",
                    name
                )));
            }
            let dest = temp_dir.join(&name);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| crate::AppError::Plugin(format!("Failed to create '{}': {}", name, e)))?;
            }
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to create '{}': {}", name, e)))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to extract '{}': {}", name, e)))?;
        }

        // 4. wasm 存在性校验（manifest 声明 rust_library 时；桌面端无 wasm_hash 字段，
        //    只保证声明的模块文件存在，避免激活期才暴露缺失）
        if !manifest.rust_library.is_empty() {
            let wasm_path = temp_dir.join(format!("{}{}", manifest.rust_library, WASM_FILE_EXT));
            if !wasm_path.exists() {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin package missing WASM file: {}{}",
                    manifest.rust_library, WASM_FILE_EXT
                )));
            }
        }

        // 5. 写来源标记（供 dev 副本刷新/未来磁盘审计区分安装来源）
        let marker = temp_dir.join(PLUGIN_SOURCE_MARKER);
        std::fs::write(&marker, SOURCE_FILE_INSTALL)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to write source marker: {}", e)))?;

        // 6. 移动到最终目录
        //
        // 拒绝覆盖已存在的同 id 插件：无签名链时无法区分「同作者更新」与
        // 「冒名顶替替换」，静默替换会让既有权限继续作用于被替换后的新代码，
        // 是权限门禁的旁路。升级需先卸载旧版本。
        //
        // 例外：目录内无 plugin.json 的「孤儿残留」不构成有效安装——这类目录
        // 通常是历史安装/激活遗留的插件私有数据库（plugin.db），loader 因缺
        // plugin.json 会跳过它（UI 不可见、卸载按 extension_path 也够不着），
        // 但磁盘查重会误判为已安装，卡死同 id 重装。此处识别后清除再安装。
        let final_dir = user_plugins_dir.join(&plugin_id);
        if final_dir.exists() {
            if final_dir.join(PLUGIN_MANIFEST_FILE).exists() {
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(crate::AppError::Plugin(format!(
                    "Plugin '{}' is already installed. Uninstall it first to install a new version.",
                    plugin_id
                )));
            }
            tracing::warn!(
                plugin_id = %plugin_id,
                dir = %final_dir.display(),
                "[PluginDownloader] Removing orphan residue dir (no plugin.json) before install"
            );
            std::fs::remove_dir_all(&final_dir).map_err(|e| {
                crate::AppError::Plugin(format!("Failed to remove orphan dir '{}': {}", final_dir.display(), e))
            })?;
        }
        std::fs::rename(&temp_dir, &final_dir)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to move plugin into place: {}", e)))?;

        tracing::info!(
            "[PluginDownloader] Plugin '{}' installed to {:?} (source: {})",
            plugin_id,
            final_dir,
            SOURCE_FILE_INSTALL
        );
        Ok(plugin_id)
    }

    /// zip 条目路径安全校验：拒绝绝对路径、盘符、.. 路径穿越
    fn is_safe_zip_name(name: &str) -> bool {
        if name.starts_with('/') || name.starts_with('\\') {
            return false;
        }
        if name.contains(':') {
            return false;
        }
        // 规范化后检查是否有 .. 段
        let normalized = name.replace('\\', "/");
        if normalized.split('/').any(|seg| seg == ".." || seg == ".") {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// 构造一个合法插件 zip：plugin.json + index.js + 可选 wasm 文件
    /// `declares_wasm` 控制 manifest 是否声明 rust_library；`includes_wasm_file` 控制 zip 是否含该文件
    fn build_plugin_zip(dir: &Path, id: &str, declares_wasm: bool, includes_wasm_file: bool) -> PathBuf {
        let zip_path = dir.join(format!("{}.zip", id));
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);

        let wasm_name = format!("lib_{}", id.replace('.', "_"));
        let manifest = serde_json::json!({
            "id": id,
            "name": "Test Plugin",
            "version": "1.0.0",
            "main": "index.js",
            "pluginType": if declares_wasm { "rust-ts" } else { "ts-only" },
            "rustLibrary": if declares_wasm { wasm_name.clone() } else { String::new() },
            "permissions": [],
            "contributes": {}
        })
        .to_string();
        writer
            .start_file(PLUGIN_MANIFEST_FILE, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer
            .start_file("index.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"console.log('test')").unwrap();
        if includes_wasm_file {
            writer
                .start_file(format!("{}.wasm", wasm_name), zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&[0u8; 8]).unwrap();
        }
        writer.finish().unwrap();
        zip_path
    }

    #[test]
    fn install_from_file_ok_and_writes_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let zip_path = build_plugin_zip(tmp.path(), "com.test.install-ok", false, false);

        let id = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap();
        assert_eq!(id, "com.test.install-ok");

        let final_dir = plugins_dir.join(&id);
        assert!(final_dir.join("plugin.json").exists());
        assert!(final_dir.join("index.js").exists());
        // 来源标记写入（file-install）
        assert_eq!(
            std::fs::read_to_string(final_dir.join(PLUGIN_SOURCE_MARKER)).unwrap(),
            SOURCE_FILE_INSTALL
        );
        // 临时目录已清理（rename 而非残留）
        assert!(!plugins_dir.join(PLUGIN_DOWNLOAD_TEMP_DIR).join(&id).exists());
    }

    #[test]
    fn install_requires_wasm_file_when_declared() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        // 声明了 rust_library 但 zip 缺失 wasm 文件 → 拒绝安装
        let zip_path = build_plugin_zip(tmp.path(), "com.test.missing-wasm", true, false);

        let err = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap_err();
        assert!(err.to_string().contains("missing WASM"));
        assert!(!plugins_dir.join("com.test.missing-wasm").exists());
    }

    #[test]
    fn install_accepts_wasm_plugin_with_file() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let zip_path = build_plugin_zip(tmp.path(), "com.test.with-wasm", true, true);

        let id = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap();
        assert_eq!(id, "com.test.with-wasm");
        let installed = std::fs::read_dir(plugins_dir.join(&id)).unwrap();
        assert!(installed
            .into_iter()
            .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".wasm")));
    }

    #[test]
    fn install_rejects_duplicate_id() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let zip_path = build_plugin_zip(tmp.path(), "com.test.dup", false, false);

        PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap();
        let err = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap_err();
        assert!(err.to_string().contains("already installed"));
    }

    /// 孤儿残留目录（无 plugin.json，如历史安装遗留的私有数据库 plugin.db）不阻塞
    /// 重装：安装前识别为无效安装，清除后继续安装新版本
    #[test]
    fn install_replaces_orphan_dir_without_plugin_json() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let id = "com.test.orphan-replace";

        // 预置孤儿残留：只有私有数据库文件，无 plugin.json（loader 会跳过它）
        let orphan_dir = plugins_dir.join(id);
        std::fs::create_dir_all(&orphan_dir).unwrap();
        std::fs::write(orphan_dir.join("plugin.db"), b"orphan db").unwrap();

        let zip_path = build_plugin_zip(tmp.path(), id, false, false);
        let installed = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap();
        assert_eq!(installed, id);

        // 孤儿残留被清除：plugin.db 消失，新插件文件就位
        assert!(!orphan_dir.join("plugin.db").exists());
        assert!(orphan_dir.join("plugin.json").exists());
        assert!(orphan_dir.join("index.js").exists());
    }

    /// 有效安装目录（含 plugin.json）仍拒绝覆盖：无签名链时无法区分同作者更新与
    /// 冒名顶替，静默替换是权限门禁旁路，升级必须先卸载
    #[test]
    fn install_still_rejects_valid_existing_install() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        let id = "com.test.valid-exists";

        // 预置含 plugin.json 的同 id 安装目录（模拟已安装的旧版本）
        let existing = plugins_dir.join(id);
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("plugin.json"), "{\"id\":\"com.test.valid-exists\"}").unwrap();
        std::fs::write(existing.join("index.js"), "old").unwrap();

        let zip_path = build_plugin_zip(tmp.path(), id, false, false);
        let err = PluginDownloader::install_from_file(zip_path.to_str().unwrap(), &plugins_dir).unwrap_err();
        assert!(err.to_string().contains("already installed"));

        // 原安装目录保持原样，未被替换；临时目录已清理
        assert_eq!(std::fs::read_to_string(existing.join("index.js")).unwrap(), "old");
        assert!(!plugins_dir.join(PLUGIN_DOWNLOAD_TEMP_DIR).join(id).exists());
    }

    #[test]
    fn is_safe_zip_name_rejects_traversal() {
        assert!(PluginDownloader::is_safe_zip_name("index.js"));
        assert!(PluginDownloader::is_safe_zip_name("sub/dir/file.js"));
        assert!(!PluginDownloader::is_safe_zip_name("../evil.js"));
        assert!(!PluginDownloader::is_safe_zip_name("/abs/path.js"));
        assert!(!PluginDownloader::is_safe_zip_name("C:\\windows\\evil.exe"));
        assert!(!PluginDownloader::is_safe_zip_name("sub/../evil.js"));
    }

    #[test]
    fn install_rejects_invalid_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_dir = tmp.path().join("plugins");
        // 无 plugin.json 的 zip
        let bad_zip = tmp.path().join("bad.zip");
        let file = std::fs::File::create(&bad_zip).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("index.js", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();

        let err = PluginDownloader::install_from_file(bad_zip.to_str().unwrap(), &plugins_dir).unwrap_err();
        assert!(err.to_string().contains("missing plugin.json"));
    }
}
