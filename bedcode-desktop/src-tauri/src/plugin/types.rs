//! Plugin Types (Desktop)
//!
//! 桌面端插件类型 — 仅保留桌面端特有的内部模型
//! 共享类型（PluginManifest, PluginContributes, PluginState 等）迁移到 bedcode-plugin-api

use bedcode_plugin_api::{
    PluginContributes, PluginManifest, PluginState, PluginType,
};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

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
    /// cdylib 动态库加载的 Rust+TS 插件
    Cdylib,
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
    /// cdylib 动态库文件名（仅 rust-ts 类型插件使用）
    pub rust_library: String,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
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
        }
    }
}
