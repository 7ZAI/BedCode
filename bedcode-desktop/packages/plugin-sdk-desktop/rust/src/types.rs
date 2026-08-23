//! Plugin Types
//!
//! 插件声明式描述类型、状态枚举 — 从桌面端 types.rs 迁移的共享部分

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件描述文件 (plugin.json) 的完整结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// 入口文件路径（相对于插件根目录，TS-only 插件使用）
    #[serde(default)]
    pub main: String,
    /// 沙箱模式：MVP 仅支持 "inline"
    #[serde(default = "default_sandbox")]
    pub sandbox: String,
    /// 请求的权限列表
    #[serde(default)]
    pub permissions: Vec<String>,
    /// 对外可互调 api 清单（ADR-0017，插件互调机制）
    ///
    /// 全限定名数组（如 `com.bedcode.scheduler.add`）；宿主在插件激活时
    /// 登记到 api 注册表，`bedcode.api.*` 请求 topic 的目标 api 必须命中
    /// 某已激活插件的声明清单，否则被总线门禁拒绝。缺省空数组 = 不对外
    /// 提供互调 api（现有插件不受影响）。
    #[serde(default)]
    pub api: Vec<String>,
    /// 扩展点声明
    #[serde(default)]
    pub contributes: PluginContributes,
    /// 插件类型：rust / rust-ts / ts-only
    #[serde(default = "default_plugin_type")]
    pub plugin_type: PluginType,
    /// WASM 库文件名（不含路径，相对于插件目录）
    /// 仅 rust-ts 类型插件使用，宿主根据平台自动添加后缀
    #[serde(default)]
    pub rust_library: String,
    /// 插件图标：图片路径（相对插件目录）或内联 SVG 标记
    #[serde(default)]
    pub icon: Option<String>,
}

fn default_sandbox() -> String {
    "inline".to_string()
}

fn default_plugin_type() -> PluginType {
    PluginType::TsOnly
}

/// 插件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// 纯 Rust 插件，无前端组件
    Rust,
    /// Rust + TypeScript 插件，Rust 提供后端能力，TS 提供 UI
    RustTs,
    /// 纯 TypeScript 插件，仅前端组件
    TsOnly,
}

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfiguration {
    /// 配置区域标题
    pub title: String,
    /// 配置属性映射（key → 属性定义）
    pub properties: HashMap<String, ConfigProperty>,
}

/// 配置属性定义
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
    /// 生命周期钩子声明
    #[serde(default)]
    pub lifecycle: Option<LifecycleContribution>,
    /// 声明此插件会发布的消息 topic（文档性质，不做强制校验）
    #[serde(default)]
    pub provides: Vec<String>,
    /// 声明此插件感兴趣的消息 topic（宿主据此路由消息）
    #[serde(default)]
    pub subscribes: Vec<String>,
}

/// 生命周期扩展点声明
///
/// 插件通过此声明告知宿主它需要接收应用启动/关闭事件。
/// Rust 插件通过 `BedcodePlugin` trait 的 `on_startup`/`on_shutdown` 方法实现回调；
/// TS-only 插件通过前端事件 `lifecycle:startup`/`lifecycle:shutdown` 接收。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleContribution {
    /// 是否注册 onStartup 回调
    #[serde(default)]
    pub on_startup: bool,
    /// 是否注册 onShutdown 回调
    #[serde(default)]
    pub on_shutdown: bool,
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct TerminalContribution {
    #[serde(default)]
    pub input_handlers: Vec<String>,
    #[serde(default)]
    pub output_parsers: Vec<String>,
}

/// 外部工具扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolProviderContribution {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// 文件处理扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// 插件请求的权限尚未获得用户批准（需在插件管理页人工审批后才能激活）
    NeedsApproval,
    Error(String),
    Deactivated,
}

/// 插件信息（返回给前端的精简版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub main: String,
    pub sandbox: String,
    pub plugin_type: PluginType,
    pub permissions: Vec<String>,
    pub state: PluginState,
    pub extension_path: String,
    pub contributes: PluginContributes,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PluginManifest ====================

    #[test]
    fn test_manifest_parse_with_defaults() {
        // 缺省字段（description/author/main/icon/contributes）全部走 default，
        // 宿主加载最小化 plugin.json 不应失败
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "sandbox": "inline",
            "permissions": ["storage", "terminal:input"],
            "pluginType": "rust-ts",
            "contributes": {
                "commands": [{ "id": "run", "title": "Run", "icon": "run.svg" }],
                "subscribes": ["task:status-changed"]
            }
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.id, "com.bedcode.demo");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.description, "");
        assert_eq!(m.sandbox, "inline");
        assert_eq!(m.plugin_type, PluginType::RustTs);
        assert_eq!(m.permissions, vec!["storage", "terminal:input"]);
        assert_eq!(m.contributes.commands.len(), 1);
        assert_eq!(m.contributes.commands[0].id, "run");
        assert_eq!(m.contributes.commands[0].title, "Run");
        assert_eq!(m.contributes.commands[0].icon.as_deref(), Some("run.svg"));
        assert_eq!(m.contributes.subscribes, vec!["task:status-changed"]);
    }

    #[test]
    fn test_manifest_round_trip_fills_defaults() {
        // 宿主加载最小化 plugin.json 后序列化回写：缺省字段应已填充默认值
        // （serde default 在反序列化时生效，序列化反映内存中的实际值）
        let json = serde_json::json!({ "id": "com.bedcode.x", "name": "X", "version": "1.0.0" });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["pluginType"], serde_json::json!("ts-only"));
        assert_eq!(back["sandbox"], serde_json::json!("inline"));
        assert_eq!(back["description"], serde_json::json!(""));
        // contributes 序列化时带全部字段（serde(default) 只影响反序列化）
        assert_eq!(back["contributes"]["commands"], serde_json::json!([]));
        assert_eq!(back["contributes"]["subscribes"], serde_json::json!([]));
    }

    // ==================== PluginType / PluginState ====================

    #[test]
    fn test_plugin_type_kebab_case() {
        // 线协议 kebab-case：宿主按字面量解析 plugin.json 的 pluginType 字段
        assert_eq!(serde_json::to_value(PluginType::Rust).unwrap(), serde_json::json!("rust"));
        assert_eq!(serde_json::to_value(PluginType::RustTs).unwrap(), serde_json::json!("rust-ts"));
        assert_eq!(serde_json::to_value(PluginType::TsOnly).unwrap(), serde_json::json!("ts-only"));
        assert_eq!(serde_json::from_value::<PluginType>(serde_json::json!("rust-ts")).unwrap(), PluginType::RustTs);
        assert!(serde_json::from_value::<PluginType>(serde_json::json!("rust_ts")).is_err());
    }

    #[test]
    fn test_plugin_state_adjacent_tagging() {
        // state/error 相邻标签：unit 变体无 error 字段，Error 携带消息
        assert_eq!(
            serde_json::to_value(PluginState::Activated).unwrap(),
            serde_json::json!({ "state": "Activated" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Error("boom".into())).unwrap(),
            serde_json::json!({ "state": "Error", "error": "boom" })
        );
        let back: PluginState =
            serde_json::from_value(serde_json::json!({ "state": "Error", "error": "x" })).unwrap();
        assert_eq!(back, PluginState::Error("x".into()));
    }

    // ==================== 扩展点声明 ====================

    #[test]
    fn test_view_contribution_type_field() {
        // view_type 序列化为 "type"（与前端 vscode 风格扩展点一致）
        let v = ViewContribution {
            id: "v1".into(),
            view_type: "sidebar".into(),
            title: "Side".into(),
            component: "SidePanel".into(),
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({
                "id": "v1",
                "type": "sidebar",
                "title": "Side",
                "component": "SidePanel"
            })
        );
    }

    #[test]
    fn test_config_property_type_field() {
        // 属性类型字段序列化为 "type"，缺省字段（description/default/enumValues）为 null
        let p = ConfigProperty {
            prop_type: "string".into(),
            title: "API Key".into(),
            description: None,
            default: None,
            enum_values: None,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "type": "string",
                "title": "API Key",
                "description": null,
                "default": null,
                "enumValues": null
            })
        );
    }

    #[test]
    fn test_contributes_defaults_and_full_parse() {
        // 全量贡献点解析：terminal/toolProviders/fileHandlers/configuration/lifecycle
        let json = serde_json::json!({
            "commands": [{ "id": "c1", "title": "C1" }],
            "views": [{ "id": "v1", "type": "toolbox", "title": "V1", "component": "C" }],
            "terminal": {
                "inputHandlers": ["in1"],
                "outputParsers": ["out1"]
            },
            "toolProviders": [{ "id": "tp1", "name": "N", "endpoint": "http://x" }],
            "fileHandlers": [{ "id": "fh1", "extensions": ["md"], "viewer": "V" }],
            "configuration": {
                "title": "Config",
                "properties": {
                    "key": { "type": "string", "title": "Key" }
                }
            },
            "lifecycle": { "onStartup": true, "onShutdown": false },
            "provides": ["topic:a"],
            "subscribes": ["topic:b"]
        });
        let c: PluginContributes = serde_json::from_value(json).unwrap();
        assert_eq!(c.terminal.as_ref().unwrap().input_handlers, vec!["in1"]);
        assert_eq!(c.tool_providers[0].endpoint, "http://x");
        assert_eq!(c.file_handlers[0].extensions, vec!["md"]);
        assert_eq!(c.configuration.as_ref().unwrap().properties.len(), 1);
        assert!(c.lifecycle.as_ref().unwrap().on_startup);
        assert!(!c.lifecycle.as_ref().unwrap().on_shutdown);
        assert_eq!(c.provides, vec!["topic:a"]);
    }

}
