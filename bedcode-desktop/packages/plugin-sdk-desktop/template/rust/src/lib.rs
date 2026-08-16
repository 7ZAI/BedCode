//! {{NAME}} 插件后端（WASM）
//!
//! 最小可编译实现：激活/停用日志，命令返回未知命令错误。
//! 通过 bedcode-plugin-api 的 WasmPlugin trait 实现，wasm_entry! 宏生成 ABI 导出。

use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};

struct {{STRUCT}};

impl WasmPlugin for {{STRUCT}} {
    const ID: &'static str = "{{ID}}";

    fn manifest() -> PluginManifest {
        // plugin.json 为 manifest 单一事实来源（bedcode-plugin-desktop build 时自动填充）
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        WasmHost.log_info("{{NAME}} plugin activated (wasm)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        WasmHost.log_info("{{NAME}} plugin deactivated (wasm)");
        Ok(())
    }

    fn invoke_command(_name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Err(anyhow::anyhow!("No commands implemented"))
    }
}

bedcode_plugin_api::wasm_entry!({{STRUCT}});
