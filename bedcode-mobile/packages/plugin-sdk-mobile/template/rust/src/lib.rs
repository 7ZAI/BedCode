//! {{NAME}} Plugin (WASM — Mobile)
//!
//! 最小可编译实现：激活/停用日志，命令返回未知命令错误

use bedcode_plugin_api_mobile::{HostLog, WasmHost, WasmPlugin};
use bedcode_plugin_api_mobile::types::PluginManifest;

struct {{STRUCT}};

impl WasmPlugin for {{STRUCT}} {
    const ID: &'static str = "{{ID}}";

    fn manifest() -> PluginManifest {
        // plugin.json 为 manifest 单一事实来源（bedcode-plugin build/package 时自动填充）
        serde_json::from_str(include_str!("../../plugin.json")).expect("plugin.json must be valid PluginManifest")
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
