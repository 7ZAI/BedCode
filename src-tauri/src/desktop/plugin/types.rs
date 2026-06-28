//! Plugin Types
//!
//! 插件声明式描述类型、状态枚举和内部加载模型

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

/// 插件描述文件 (plugin.json) 的完整结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// 唯一标识（反向域名格式，如 com.bedcode.quick-snippets）
    pub id: String,
    /// 显示名称
    pub name: String,
    /// 语义化版本号
    pub version: String,
    /// 插件描述
    #[serde(default)]
    pub description: String,
    /// 作者
    #[serde(default)]
    pub author: String,
    /// 入口文件路径（相对于插件根目录）
    pub main: String,
    /// 沙箱模式：MVP 仅支持 "inline"
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    /// 请求的权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 扩展点声明
    #[serde(default)]
    pub contributes: PluginContributes,
}

fn default_sandbox() -> String {
    "inline".to_string()
}

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfiguration {
    /// 配置区域标题
    pub title: String,
    /// 配置属性映射（key → 属性定义）
    pub properties: HashMap<String, ConfigProperty>,
}

/// 配置属性定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProperty {
    /// 属性类型：string / number / boolean
    #[serde(rename = "type")]
    pub prop_type: String,
    /// 显示标题
    pub title: String,
    /// 帮助描述
    #[serde(default)]
    pub description: Option<String>,
    /// 默认值
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// 枚举选项（type 为 string 时使用）
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
}

/// 插件扩展点声明
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginContributes {
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    #[serde(default)]
    pub views: Vec<ViewContribution>,
    #[serde(default)]
    pub terminal: Option<TerminalContribution>,
    #[serde(default)]
    pub tool_providers: Vec<ToolProviderContribution>,
    #[serde(default)]
    pub file_handlers: Vec<FileHandlerContribution>,
    /// 配置声明
    #[serde(default)]
    pub configuration: Option<PluginConfiguration>,
}

/// 命令扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 视图扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewContribution {
    pub id: String,
    /// "sidebar" | "toolbox" | "statusbar"
    #[serde(rename = "type")]
    pub view_type: String,
    pub title: String,
    pub component: String,
}

/// 终端扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalContribution {
    #[serde(default)]
    pub input_handlers: Vec<String>,
    #[serde(default)]
    pub output_parsers: Vec<String>,
}

/// 外部工具扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProviderContribution {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// 文件处理扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHandlerContribution {
    pub id: String,
    pub extensions: Vec<String>,
    pub viewer: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// 插件运行时状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "error")]
pub enum PluginState {
    Loaded,
    Activated,
    Error(String),
    Deactivated,
}

/// 已加载插件的内部表示
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub granted_permissions: HashSet<String>,
    pub extension_path: String,
    pub activated_at: Option<DateTime<Utc>>,
}

/// 插件信息（返回给前端的精简版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub sandbox: String,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
}

impl From<&LoadedPlugin> for PluginInfo {
    fn from(p: &LoadedPlugin) -> Self {
        PluginInfo {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            version: p.manifest.version.clone(),
            description: p.manifest.description.clone(),
            author: p.manifest.author.clone(),
            main: p.manifest.main.clone(),
            sandbox: p.manifest.sandbox.clone(),
            permissions: p.manifest.permissions.clone(),
            state: p.state.clone(),
            extension_path: p.extension_path.clone(),
            contributes: p.manifest.contributes.clone(),
        }
    }
}
