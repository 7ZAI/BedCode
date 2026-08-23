//! Plugin Types (Mobile SDK)
//!
//! 移动端插件声明式描述类型 — 从宿主 types.rs 迁移的共享部分

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 插件类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// 纯 Rust 插件，无前端组件
    Rust,
    /// Rust + TypeScript 插件
    RustTs,
    /// 纯 TypeScript 插件
    TsOnly,
    /// WASM 插件，通过 wasmtime 动态加载
    Wasm,
}

impl Default for PluginType {
    fn default() -> Self {
        Self::TsOnly
    }
}

/// 插件运行时状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PluginState {
    Loaded,
    Activated,
    /// 插件请求的权限尚未获得用户批准（需在插件管理页人工审批后才能激活）
    NeedsApproval,
    Deactivated,
    Error { error: String },
}

impl Default for PluginState {
    fn default() -> Self {
        Self::Loaded
    }
}

/// 插件清单
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// 前端入口模块路径（相对于 dist/）
    #[serde(default)]
    pub main: String,
    #[serde(default)]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub contributes: PluginContributes,
    /// 插件图标：emoji、内联 <svg> 标记或相对插件目录的图片路径（如 "icon.png"/"icon.svg"）
    /// 缺省时前端按插件 id 生成字母头像回退
    #[serde(default)]
    pub icon: Option<String>,
    /// WASM 文件 SHA256 哈希，用于远程下载校验
    #[serde(default)]
    pub wasm_hash: String,
    /// Rust 库名（对应 WASM 文件名）
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

/// 命令扩展点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// 插件配置声明
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfiguration {
    pub title: String,
    pub properties: HashMap<String, ConfigProperty>,
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

/// 生命周期扩展点声明
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleContribution {
    #[serde(default)]
    pub on_startup: bool,
    #[serde(default)]
    pub on_shutdown: bool,
    #[serde(default)]
    pub on_auth_success: bool,
    #[serde(default)]
    pub on_disconnect: bool,
    #[serde(default)]
    pub on_session_created: bool,
    #[serde(default)]
    pub on_session_stopped: bool,
    #[serde(default)]
    pub on_terminal_input: bool,
    #[serde(default)]
    pub on_terminal_output: bool,
}

impl LifecycleContribution {
    /// 检查是否声明了指定事件
    pub fn is_declared(&self, event_name: &str) -> bool {
        match event_name {
            "onStartup" => self.on_startup,
            "onShutdown" => self.on_shutdown,
            "onAuthSuccess" => self.on_auth_success,
            "onDisconnect" => self.on_disconnect,
            "onSessionCreated" => self.on_session_created,
            "onSessionStopped" => self.on_session_stopped,
            "onTerminalInput" => self.on_terminal_input,
            "onTerminalOutput" => self.on_terminal_output,
            _ => false,
        }
    }

    /// 检查是否有任何声明
    pub fn has_any_declared(&self) -> bool {
        self.on_startup
            || self.on_shutdown
            || self.on_auth_success
            || self.on_disconnect
            || self.on_session_created
            || self.on_session_stopped
            || self.on_terminal_input
            || self.on_terminal_output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PluginManifest ====================

    #[test]
    fn test_manifest_parse_with_defaults() {
        // 缺省字段（description/author/main/pluginType/permissions/contributes/
        // icon/wasmHash/rustLibrary）全部走 default，宿主加载最小化 plugin.json 不应失败
        let json = serde_json::json!({
            "id": "com.bedcode.demo",
            "name": "Demo",
            "version": "0.1.0",
            "permissions": ["storage", "terminal:input"]
        });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.id, "com.bedcode.demo");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.description, "");
        assert_eq!(m.author, "");
        assert_eq!(m.main, "");
        assert_eq!(m.plugin_type, PluginType::TsOnly);
        assert_eq!(m.permissions, vec!["storage", "terminal:input"]);
        assert_eq!(m.icon, None);
        assert_eq!(m.wasm_hash, "");
        assert_eq!(m.rust_library, "");
    }

    #[test]
    fn test_manifest_round_trip_fills_defaults() {
        // 宿主加载最小化 plugin.json 后序列化回写：缺省字段应已填充默认值
        let json = serde_json::json!({ "id": "com.bedcode.x", "name": "X", "version": "1.0.0" });
        let m: PluginManifest = serde_json::from_value(json).unwrap();
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["pluginType"], serde_json::json!("ts-only"));
        assert_eq!(back["wasmHash"], serde_json::json!(""));
        // contributes 序列化时带全部字段（serde(default) 只影响反序列化）
        assert_eq!(back["contributes"]["commands"], serde_json::json!([]));
        assert_eq!(back["contributes"]["navTab"], serde_json::Value::Null);
        assert_eq!(back["contributes"]["settings"], serde_json::Value::Null);
    }

    // ==================== PluginType / PluginState ====================

    #[test]
    fn test_plugin_type_kebab_case() {
        // 线协议 kebab-case：宿主按字面量解析 plugin.json 的 pluginType 字段；
        // 移动端比桌面端多 Wasm 变体
        assert_eq!(serde_json::to_value(PluginType::Rust).unwrap(), serde_json::json!("rust"));
        assert_eq!(serde_json::to_value(PluginType::RustTs).unwrap(), serde_json::json!("rust-ts"));
        assert_eq!(serde_json::to_value(PluginType::TsOnly).unwrap(), serde_json::json!("ts-only"));
        assert_eq!(serde_json::to_value(PluginType::Wasm).unwrap(), serde_json::json!("wasm"));
        assert_eq!(
            serde_json::from_value::<PluginType>(serde_json::json!("wasm")).unwrap(),
            PluginType::Wasm
        );
        assert!(serde_json::from_value::<PluginType>(serde_json::json!("rust_ts")).is_err());
    }

    #[test]
    fn test_plugin_type_default() {
        assert_eq!(PluginType::default(), PluginType::TsOnly);
    }

    #[test]
    fn test_plugin_state_camel_case_tag() {
        // state 内部标签 + camelCase 变体名（与桌面端 PascalCase 不同，移动端线协议如此）
        assert_eq!(
            serde_json::to_value(PluginState::Loaded).unwrap(),
            serde_json::json!({ "state": "loaded" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Activated).unwrap(),
            serde_json::json!({ "state": "activated" })
        );
        assert_eq!(
            serde_json::to_value(PluginState::Error { error: "boom".into() }).unwrap(),
            serde_json::json!({ "state": "error", "error": "boom" })
        );
        let back: PluginState =
            serde_json::from_value(serde_json::json!({ "state": "error", "error": "x" })).unwrap();
        assert_eq!(back, PluginState::Error { error: "x".into() });
    }

    #[test]
    fn test_plugin_state_default() {
        assert_eq!(PluginState::default(), PluginState::Loaded);
    }

    // ==================== 移动端特有扩展点 ====================

    #[test]
    fn test_nav_tab_contribution_wire_format() {
        // 底部导航 Tab（移动端特有）：order 缺省为 0
        let t = NavTabContribution {
            id: "tab1".into(),
            title: "Tasks".into(),
            icon: "tasks.svg".into(),
            component: "TasksView".into(),
            order: 2,
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            serde_json::json!({
                "id": "tab1",
                "title": "Tasks",
                "icon": "tasks.svg",
                "component": "TasksView",
                "order": 2
            })
        );
        let minimal =
            serde_json::json!({ "id": "t", "title": "T", "icon": "i", "component": "C" });
        let back: NavTabContribution = serde_json::from_value(minimal).unwrap();
        assert_eq!(back.order, 0);
    }

    #[test]
    fn test_settings_contribution_wire_format() {
        let s = SettingsContribution {
            section: "network".into(),
            component: "NetSettings".into(),
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({ "section": "network", "component": "NetSettings" })
        );
    }

    #[test]
    fn test_view_contribution_type_field() {
        // view_type 序列化为 "type"（与前端 vscode 风格扩展点一致）
        let v = ViewContribution {
            id: "v1".into(),
            view_type: "toolbox".into(),
            title: "Toolbox".into(),
            component: "Panel".into(),
        };
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({
                "id": "v1",
                "type": "toolbox",
                "title": "Toolbox",
                "component": "Panel"
            })
        );
    }

    #[test]
    fn test_terminal_contribution_toolbar_items() {
        // 终端扩展点含工具栏按钮（ui:input 权限对应）
        let t = TerminalContribution {
            input_handlers: vec!["in1".into()],
            output_parsers: vec!["out1".into()],
            toolbar_items: vec![TerminalToolbarItemContribution {
                id: "tb1".into(),
                title: "Send".into(),
                icon: "send.svg".into(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&t).unwrap(),
            serde_json::json!({
                "inputHandlers": ["in1"],
                "outputParsers": ["out1"],
                "toolbarItems": [{ "id": "tb1", "title": "Send", "icon": "send.svg" }]
            })
        );
    }

    #[test]
    fn test_contributes_defaults_and_full_parse() {
        // 全量贡献点解析：commands/views/terminal/navTab/settings/configuration/lifecycle
        let json = serde_json::json!({
            "commands": [{ "id": "c1", "title": "C1" }],
            "views": [{ "id": "v1", "type": "toolbox", "title": "V1", "component": "C" }],
            "terminal": { "inputHandlers": ["in1"], "outputParsers": ["out1"] },
            "navTab": { "id": "t1", "title": "T", "icon": "i.svg", "component": "C" },
            "settings": { "section": "net", "component": "S" },
            "configuration": {
                "title": "Config",
                "properties": { "key": { "type": "string", "title": "Key" } }
            },
            "lifecycle": { "onStartup": true, "onAuthSuccess": true }
        });
        let c: PluginContributes = serde_json::from_value(json).unwrap();
        assert_eq!(c.commands[0].id, "c1");
        assert_eq!(c.views[0].view_type, "toolbox");
        assert_eq!(c.terminal.as_ref().unwrap().input_handlers, vec!["in1"]);
        assert_eq!(c.nav_tab.as_ref().unwrap().id, "t1");
        assert_eq!(c.settings.as_ref().unwrap().section, "net");
        assert_eq!(c.configuration.as_ref().unwrap().properties.len(), 1);
        assert!(c.lifecycle.as_ref().unwrap().on_startup);
        assert!(c.lifecycle.as_ref().unwrap().on_auth_success);
        assert!(!c.lifecycle.as_ref().unwrap().on_disconnect);
    }

    #[test]
    fn test_config_property_type_field() {
        // 属性类型字段序列化为 "type"，缺省字段（description/default）为 null
        let p = ConfigProperty {
            prop_type: "string".into(),
            title: "API Key".into(),
            description: None,
            default: None,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "type": "string",
                "title": "API Key",
                "description": null,
                "default": null
            })
        );
    }

    // ==================== LifecycleContribution ====================

    #[test]
    fn test_lifecycle_is_declared_mapping() {
        // 宿主按 camelCase 事件名查询声明；未声明/未知事件一律 false
        let mut lc = LifecycleContribution::default();
        assert!(!lc.is_declared("onStartup"));
        lc.on_startup = true;
        lc.on_auth_success = true;
        lc.on_session_stopped = true;
        assert!(lc.is_declared("onStartup"));
        assert!(lc.is_declared("onAuthSuccess"));
        assert!(lc.is_declared("onSessionStopped"));
        assert!(!lc.is_declared("onShutdown"));
        assert!(!lc.is_declared("onDisconnect"));
        // 未知事件名拒绝，防止宿主拼写漂移静默通过
        assert!(!lc.is_declared("onPaused"));
        assert!(!lc.is_declared(""));
    }

    #[test]
    fn test_lifecycle_has_any_declared() {
        // 全空默认 = 无任何生命周期钩子声明
        let lc = LifecycleContribution::default();
        assert!(!lc.has_any_declared());
        // 任一钩子置位即视为有声明（宿主据此决定是否注册回调）
        let mut lc2 = LifecycleContribution::default();
        lc2.on_terminal_input = true;
        assert!(lc2.has_any_declared());
    }

}
