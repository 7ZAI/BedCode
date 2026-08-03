//! {{NAME}} Plugin (WASM — Mobile)
//!
//! 最小可编译实现：激活/停用日志，命令返回未知命令错误

use bedcode_plugin_api_mobile::{HostLog, WasmHost, WasmPlugin};
use bedcode_plugin_api_mobile::types::PluginManifest;

struct {{STRUCT}};

impl WasmPlugin for {{STRUCT}} {
    const ID: &'static str = "{{ID}}";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": "{{ID}}",
            "name": "{{NAME}}",
            "version": "0.1.0",
            "description": "",
            "author": "{{AUTHOR}}",
            "main": "index.js",
            "pluginType": "wasm",
            "rustLibrary": "{{CRATE}}",
            "permissions": ["storage"],
            "contributes": {}
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        WasmHost.log_info("{{NAME}} plugin activated (mobile)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        WasmHost.log_info("{{NAME}} plugin deactivated (mobile)");
        Ok(())
    }

    fn invoke_command(_name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Err(anyhow::anyhow!("No commands implemented"))
    }
}

bedcode_plugin_api_mobile::wasm_entry!({{STRUCT}});
