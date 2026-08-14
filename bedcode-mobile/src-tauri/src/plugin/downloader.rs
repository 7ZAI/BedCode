//! Plugin Downloader / Installer（移动端）
//!
//! 插件包（zip）统一安装：本地文件或远程 URL → 解压 → wasm_hash 校验 → 写来源标记 → 移动到插件目录
//! 分发单元为单个 zip（内含 plugin.json、index.js 与可选的 .wasm）

use crate::plugin::validation::validate_plugin_id;
use crate::system::constants::plugin::*;
use crate::Result;
use std::io::Read;
use std::path::Path;
use tokio::io::AsyncWriteExt;

/// 插件下载安装器
pub struct PluginDownloader;

impl PluginDownloader {
    /// 从本地 zip 插件包安装
    ///
    /// 1. 打开 zip，校验 plugin.json 必填字段
    /// 2. 解压到临时目录（路径穿越防护）
    /// 3. wasm_hash 校验（manifest 声明时）
    /// 4. 写来源标记（file-install）
    /// 5. 移动到 app_data_dir/plugins/{plugin_id}/
    pub async fn install_from_file(zip_path: &str, plugins_dir: &Path) -> Result<String> {
        let zip_path = Path::new(zip_path);
        if !zip_path.exists() {
            return Err(crate::AppError::Plugin(format!(
                "Plugin package not found: {}",
                zip_path.display()
            )));
        }
        Self::install_zip(zip_path, plugins_dir, SOURCE_FILE_INSTALL).await
    }

    /// 下载并安装远程 zip 插件包
    ///
    /// 下载 zip 到临时文件后复用 install_zip，来源标记为 remote-download
    pub async fn download_and_install(zip_url: &str, plugins_dir: &Path) -> Result<String> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(PLUGIN_DOWNLOAD_CONNECT_TIMEOUT_SECS))
            .read_timeout(std::time::Duration::from_secs(PLUGIN_DOWNLOAD_READ_TIMEOUT_SECS))
            .build()
            .map_err(|e| crate::AppError::Plugin(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!("[PluginDownloader] Downloading plugin package from: {}", zip_url);
        let response = client
            .get(zip_url)
            .send()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to download plugin package: {}", e)))?;
        if !response.status().is_success() {
            return Err(crate::AppError::Plugin(format!(
                "Download '{}' returned status {}",
                zip_url,
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to read download response: {}", e)))?;

        // 写入临时文件后走统一安装流程
        let temp_zip = plugins_dir
            .join(PLUGIN_DOWNLOAD_TEMP_DIR)
            .join("download.zip");
        if let Some(parent) = temp_zip.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(&temp_zip).await?;
        file.write_all(&bytes).await?;
        drop(file);

        let result = Self::install_zip(&temp_zip, plugins_dir, SOURCE_REMOTE_DOWNLOAD).await;
        let _ = tokio::fs::remove_file(&temp_zip).await;
        result
    }

    /// zip 解压安装（file-install / remote-download 共用）
    async fn install_zip(
        zip_path: &Path,
        plugins_dir: &Path,
        source: &str,
    ) -> Result<String> {
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
        let manifest: crate::plugin::types::PluginManifest = serde_json::from_str(&manifest_str)
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
        let temp_dir = plugins_dir.join(PLUGIN_DOWNLOAD_TEMP_DIR).join(&plugin_id);

        // 3. 解压到临时目录
        if temp_dir.exists() {
            tokio::fs::remove_dir_all(&temp_dir).await?;
        }
        tokio::fs::create_dir_all(&temp_dir).await?;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| {
                crate::AppError::Plugin(format!("Failed to read plugin package entry: {}", e))
            })?;
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
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| {
                crate::AppError::Plugin(format!("Failed to create '{}': {}", name, e))
            })?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| crate::AppError::Plugin(format!("Failed to extract '{}': {}", name, e)))?;
        }

        // 4. wasm_hash 校验（manifest 声明时）
        if !manifest.rust_library.is_empty() && !manifest.wasm_hash.is_empty() {
            let wasm_path = temp_dir.join(format!("{}{}", manifest.rust_library, WASM_FILE_EXT));
            if !wasm_path.exists() {
                return Err(crate::AppError::Plugin(format!(
                    "Plugin package missing WASM file: {}{}",
                    manifest.rust_library, WASM_FILE_EXT
                )));
            }
            Self::verify_sha256(&wasm_path, &manifest.wasm_hash).await?;
        }

        // 5. 写来源标记
        let marker = temp_dir.join(PLUGIN_SOURCE_MARKER);
        tokio::fs::write(&marker, source).await?;

        // 6. 移动到最终目录
        //
        // 拒绝覆盖已存在的同 id 插件：无签名链时无法区分「同作者更新」与
        // 「冒名顶替替换」，静默替换会让既有审批（哈希钉扎）与权限继续
        // 作用于被替换后的新代码，是审批门禁的旁路。升级需先卸载旧版本。
        let final_dir = plugins_dir.join(&plugin_id);
        if final_dir.exists() {
            tokio::fs::remove_dir_all(&temp_dir).await?;
            return Err(crate::AppError::Plugin(format!(
                "Plugin '{}' is already installed. Uninstall it first to install a new version.",
                plugin_id
            )));
        }
        tokio::fs::rename(&temp_dir, &final_dir).await?;

        tracing::info!(
            "[PluginDownloader] Plugin '{}' installed to {:?} (source: {})",
            plugin_id, final_dir, source
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
        if normalized
            .split('/')
            .any(|seg| seg == ".." || seg == ".")
        {
            return false;
        }
        true
    }

    /// SHA256 校验
    async fn verify_sha256(file_path: &Path, expected_hash: &str) -> Result<()> {
        let bytes = tokio::fs::read(file_path).await?;
        let hash = sha256_hex(&bytes);
        let expected = expected_hash.strip_prefix(SHA256_PREFIX).unwrap_or(expected_hash);

        if hash != expected {
            return Err(crate::AppError::Plugin(format!(
                "SHA256 verification failed for {:?}: expected {}, got {}",
                file_path, expected, hash
            )));
        }

        tracing::info!("[PluginDownloader] SHA256 verified for {:?}", file_path);
        Ok(())
    }
}

/// 计算 SHA256 哈希（hex 编码）
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    let hash = <sha2::Sha256 as sha2::Digest>::digest(data);
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        write!(hex, "{:02x}", byte).unwrap();
    }
    hex
}
