//! 移动端特有扩展点 — 公共 API 集成测试（自 bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs 迁出）

use bedcode_plugin_api_mobile::types::*;

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
    // 终端扩展点只含工具栏按钮（ui:input 权限对应）；
    // inputHandlers/outputParsers 已随票 15 阶段 B 退役（terminal-hooks 删面）
    let t = TerminalContribution {
        toolbar_items: vec![TerminalToolbarItemContribution {
            id: "tb1".into(),
            title: "Send".into(),
            icon: "send.svg".into(),
        }],
    };
    assert_eq!(
        serde_json::to_value(&t).unwrap(),
        serde_json::json!({
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
