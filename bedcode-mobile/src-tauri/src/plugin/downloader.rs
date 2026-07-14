//! Plugin Downloader（移动端）
//!
//! 远程插件下载 + SHA256 校验 + 安装到 app_data_dir

use crate::system::constants::plugin::*;
use crate::Result;
use std::path::Path;
use tokio::io::AsyncWriteExt;

/// 插件下载器
pub struct PluginDownloader;

impl PluginDownloader {
    /// 下载并安装远程插件
    ///
    /// 1. 下载 manifest JSON
    /// 2. 校验必填字段
    /// 3. 下载 WASM 文件和前端资源到临时目录
    /// 4. SHA256 校验
    /// 5. 移动到 app_data_dir/plugins/{plugin_id}/
    pub async fn download_and_install(
        manifest_url: &str,
        plugins_dir: &Path,
    ) -> Result<String> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(PLUGIN_DOWNLOAD_CONNECT_TIMEOUT_SECS))
            .read_timeout(std::time::Duration::from_secs(PLUGIN_DOWNLOAD_READ_TIMEOUT_SECS))
            .build()
            .map_err(|e| crate::AppError::Plugin(format!("Failed to create HTTP client: {}", e)))?;

        // 1. 下载 manifest
        tracing::info!("[PluginDownloader] Downloading manifest from: {}", manifest_url);
        let manifest_str = client
            .get(manifest_url)
            .send()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to download manifest: {}", e)))?
            .text()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to read manifest response: {}", e)))?;

        let manifest: crate::plugin::types::PluginManifest = serde_json::from_str(&manifest_str)
            .map_err(|e| crate::AppError::Plugin(format!("Failed to parse manifest JSON: {}", e)))?;

        // 2. 校验必填字段
        if manifest.id.is_empty() {
            return Err(crate::AppError::Plugin("Remote manifest missing id".to_string()));
        }
        if manifest.name.is_empty() {
            return Err(crate::AppError::Plugin("Remote manifest missing name".to_string()));
        }

        let plugin_id = manifest.id.clone();
        let base_url = Self::base_url(manifest_url);

        // 3. 创建临时目录
        let temp_dir = plugins_dir.join(PLUGIN_DOWNLOAD_TEMP_DIR).join(&plugin_id);
        tokio::fs::create_dir_all(&temp_dir).await?;

        // 4. 下载 WASM 文件
        if !manifest.rust_library.is_empty() {
            let wasm_filename = format!("{}{}", manifest.rust_library, WASM_FILE_EXT);
            let wasm_url = format!("{}/{}", base_url, wasm_filename);
            let wasm_dest = temp_dir.join(&wasm_filename);

            tracing::info!("[PluginDownloader] Downloading WASM: {}", wasm_url);
            Self::download_file(&client, &wasm_url, &wasm_dest).await?;

            // SHA256 校验
            if !manifest.wasm_hash.is_empty() {
                Self::verify_sha256(&wasm_dest, &manifest.wasm_hash).await?;
            }
        }

        // 5. 下载前端资源
        if !manifest.main.is_empty() {
            let js_url = format!("{}/{}", base_url, manifest.main);
            let js_dest = temp_dir.join(&manifest.main);

            if let Some(parent) = js_dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            tracing::info!("[PluginDownloader] Downloading JS: {}", js_url);
            Self::download_file(&client, &js_url, &js_dest).await?;
        }

        // 6. 写入 manifest
        let manifest_dest = temp_dir.join(PLUGIN_MANIFEST_FILE);
        tokio::fs::write(&manifest_dest, &manifest_str).await?;

        // 7. 移动到最终目录
        let final_dir = plugins_dir.join(&plugin_id);
        if final_dir.exists() {
            tokio::fs::remove_dir_all(&final_dir).await?;
        }
        tokio::fs::rename(&temp_dir, &final_dir).await?;

        tracing::info!("[PluginDownloader] Plugin '{}' installed to {:?}", plugin_id, final_dir);
        Ok(plugin_id)
    }

    /// 下载单个文件
    async fn download_file(client: &reqwest::Client, url: &str, dest: &Path) -> Result<()> {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to download '{}': {}", url, e)))?;

        if !response.status().is_success() {
            return Err(crate::AppError::Plugin(format!(
                "Download '{}' returned status {}",
                url,
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| crate::AppError::Plugin(format!("Failed to read download response: {}", e)))?;

        let mut file = tokio::fs::File::create(dest).await?;
        file.write_all(&bytes).await?;

        Ok(())
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

    /// 从 manifest URL 推导基础 URL
    fn base_url(manifest_url: &str) -> String {
        if let Some(pos) = manifest_url.rfind('/') {
            manifest_url[..pos].to_string()
        } else {
            manifest_url.to_string()
        }
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
