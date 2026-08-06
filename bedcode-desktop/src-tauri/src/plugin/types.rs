//! Plugin Types (Desktop)
//!
//! 桌面端插件类型 — 仅保留桌面端特有的内部模型
//! 共享类型（PluginManifest, PluginContributes, PluginState 等）迁移到 bedcode-plugin-api

use bedcode_plugin_api::{
    PluginContributes, PluginManifest, PluginState, PluginType,
};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::Path;

/// 已加载插件的内部表示
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub granted_permissions: HashSet<String>,
    pub extension_path: String,
    pub activated_at: Option<DateTime<Utc>>,
    /// 插件来源：静态注册或文件扫描
    pub source: PluginSource,
}

/// 插件来源
#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// 静态注册的 Rust 插件（通过 inventory::collect）
    StaticRegistry,
    /// 文件系统扫描的 TS-only 插件
    FileScan,
    /// WASM 模块加载的 Rust+TS 插件
    Wasm,
}

impl PluginSource {
    /// 序列化为前端友好字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaticRegistry => "builtin",
            Self::FileScan => "scanned",
            Self::Wasm => "wasm",
        }
    }
}

/// 插件信息（返回给前端的精简版本）
///
/// 从 bedcode_plugin_api::PluginInfo 扩展，添加桌面端特有字段
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub sandbox: String,
    pub plugin_type: PluginType,
    /// WASM 模块文件名（仅 rust-ts 类型插件使用）
    pub rust_library: String,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
    /// 插件图标（manifest.icon 透传，可为空）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 插件来源
    pub source: String,
    /// 插件目录总大小（字节）
    pub size_bytes: u64,
    /// 安装时间（unix 毫秒，plugin.json mtime）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<i64>,
}

impl From<&LoadedPlugin> for DesktopPluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        DesktopPluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            main: p.manifest.main.clone(),
            sandbox: p.manifest.sandbox.clone(),
            plugin_type: p.manifest.plugin_type.clone(),
            rust_library: p.manifest.rust_library.clone(),
            permissions: p.manifest.permissions.clone(),
            state: p.state.clone(),
            extension_path: p.extension_path.clone(),
            contributes: p.manifest.contributes.clone(),
            icon: p.manifest.icon.clone(),
            source: p.source.as_str().to_string(),
            size_bytes: dir_size(Path::new(&p.extension_path)),
            installed_at: manifest_installed_at(&p.extension_path),
        }
    }
}

/// 递归计算目录总大小（字节），路径不存在或不可读时返回 0
fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(ft) if ft.is_file() => {
                    total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                }
                Ok(ft) if ft.is_dir() => {
                    total += dir_size(&entry.path());
                }
                _ => {}
            }
        }
    }
    total
}

/// 以 plugin.json 的 mtime 近似安装时间（unix 毫秒）
fn manifest_installed_at(extension_path: &str) -> Option<i64> {
    let manifest_path = Path::new(extension_path).join("plugin.json");
    std::fs::metadata(manifest_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_millis() as i64)
        })
}
