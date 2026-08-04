//! Auto Task Plugin (WASM — Mobile)
//!
//! 移动端自动任务队列管理的 Rust 后端
//! 极简实现：仅 activate/deactivate 日志，业务逻辑由 TS 前端通过 HTTP API 完成

use bedcode_plugin_api_mobile::{HostLog, WasmHost, WasmPlugin};
use bedcode_plugin_api_mobile::types::PluginManifest;

struct AutoTaskPlugin;

impl WasmPlugin for AutoTaskPlugin {
    const ID: &'static str = "com.bedcode.auto-task";

    fn manifest() -> PluginManifest {
        // plugin.json 为 manifest 单一事实来源（bedcode-plugin build/package 时自动填充）
        serde_json::from_str(include_str!("../../plugin.json")).expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin activated (mobile)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Auto Task plugin deactivated (mobile)");
        Ok(())
    }

    fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // 移动端不执行命令，业务逻辑由 TS 前端通过 HTTP API 完成
        Err(anyhow::anyhow!("Unknown command: {}", name))
    }
}

bedcode_plugin_api_mobile::wasm_entry!(AutoTaskPlugin);
