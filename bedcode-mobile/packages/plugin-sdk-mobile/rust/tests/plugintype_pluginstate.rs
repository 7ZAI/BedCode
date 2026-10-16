//! PluginType / PluginState — 公共 API 集成测试（自 bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs 迁出）

use bedcode_plugin_api_mobile::types::*;

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
        serde_json::to_value(PluginState::Activating).unwrap(),
        serde_json::json!({ "state": "activating" })
    );
    assert_eq!(
        serde_json::to_value(PluginState::Degraded { error: "init fail".into() }).unwrap(),
        serde_json::json!({ "state": "degraded", "error": "init fail" })
    );
    assert_eq!(
        serde_json::to_value(PluginState::Error { error: "boom".into() }).unwrap(),
        serde_json::json!({ "state": "error", "error": "boom" })
    );
    let back: PluginState =
        serde_json::from_value(serde_json::json!({ "state": "error", "error": "x" })).unwrap();
    assert_eq!(back, PluginState::Error { error: "x".into() });
    let degraded_back: PluginState =
        serde_json::from_value(serde_json::json!({ "state": "degraded", "error": "d" })).unwrap();
    assert_eq!(degraded_back, PluginState::Degraded { error: "d".into() });
}
#[test]
fn test_plugin_state_default() {
    assert_eq!(PluginState::default(), PluginState::Loaded);
}
