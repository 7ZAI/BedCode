//! Mobile Plugin Types
//!
//! 移动端插件系统所有公开类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 插件类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// 纯 Rust 插件，通过 inventory 静态注册（已废弃，保留兼容）
    Rust,
    /// Rust + TS 双层插件（已废弃，保留兼容）
    RustTs,
    /// 纯前端插件
    TsOnly,
    /// WASM 插件，通过 wasmtime 动态加载
    Wasm,
}

/// 插件运行状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PluginState {
    Loaded,
    Activated,
    Deactivated,
    Error { error: String },
}

impl Default for PluginState {
    fn default() -> Self {
        Self::Loaded
    }
}

/// 插件清单
///
/// 移动端 manifest 不从文件系统读取，而是在 Rust 端编译期硬编码
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    /// 前端入口模块路径（相对于 dist/，如 "plugins/com.bedcode.ai-chatbox/index.js"）
    pub main: String,
    pub plugin_type: PluginType,
    pub permissions: Vec<String>,
    pub contributes: PluginContributes,
    /// WASM 文件 SHA256 哈希（格式: "sha256-abc123..."），用于远程下载校验
    #[serde(default)]
    pub wasm_hash: String,
    /// Rust 库名（对应 WASM 文件名，如 "ai_chatbox" -> ai_chatbox.wasm）
    #[serde(default)]
    pub rust_library: String,
}

/// 插件扩展点声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContributes {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub views: Vec<ViewContribution>,
    #[serde(default)]
    pub terminal: Option<TerminalContribution>,
    #[serde(default)]
    pub nav_tab: Option<NavTabContribution>,
    #[serde(default)]
    pub settings: Option<SettingsContribution>,
    #[serde(default)]
    pub configuration: Option<PluginConfiguration>,
    #[serde(default)]
    pub lifecycle: Option<LifecycleContribution>,
}

/// 视图扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewContribution {
    pub id: String,
    #[serde(rename = "type")]
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 底部导航 Tab 扩展点（移动端特有）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavTabContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub component: String,
    #[serde(default)]
    pub order: i32,
}

/// 设置页扩展点（移动端特有）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsContribution {
    pub section: String,
    pub component: String,
}

/// 终端扩展点
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalContribution {
    #[serde(default)]
    pub input_handlers: Vec<String>,
    #[serde(default)]
    pub output_parsers: Vec<String>,
    #[serde(default)]
    pub toolbar_items: Vec<TerminalToolbarItemContribution>,
}

/// 终端工具栏按钮
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalToolbarItemContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
}

/// 命令扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfiguration {
    pub title: String,
    pub properties: std::collections::HashMap<String, ConfigProperty>,
}

/// 配置属性
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProperty {
    #[serde(rename = "type")]
    pub prop_type: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// 生命周期声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleContribution {
    #[serde(default)]
    pub on_startup: bool,
    #[serde(default)]
    pub on_shutdown: bool,
}

/// 插件来源
#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    /// APK assets 内置插件
    ApkAsset,
    /// 远程下载插件
    RemoteDownload,
    /// 仅前端注册（无 WASM 模块）
    FrontendOnly,
}

/// 已加载插件的内部表示
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub granted_permissions: HashSet<String>,
    /// 插件来源
    pub source: PluginSource,
    /// 插件目录路径（包含 plugin.json 的目录）
    pub extension_path: String,
}

/// 返回给前端的插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobilePluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub plugin_type: PluginType,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub contributes: PluginContributes,
    /// 插件来源
    pub source: String,
}

impl From<&LoadedPlugin> for MobilePluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        let source_str = match &p.source {
            PluginSource::ApkAsset => "apk-asset",
            PluginSource::RemoteDownload => "remote-download",
            PluginSource::FrontendOnly => "frontend-only",
        };
        MobilePluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            main: p.manifest.main.clone(),
            plugin_type: p.manifest.plugin_type.clone(),
            permissions: p.manifest.permissions.clone(),
            state: p.state.clone(),
            contributes: p.manifest.contributes.clone(),
            source: source_str.to_string(),
        }
    }
}
