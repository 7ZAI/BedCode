//! Auto Task Plugin (WASM)
//!
//! Claude Code 任务状态同步与自动授权
//! 使用 bedcode-plugin-api WasmPlugin trait 实现，通过 wasm_entry! 宏生成导出
//!
//! 架构说明：WASM 模块作为触发器和状态管理器，实际文件 I/O（hooks 配置）
//! 由宿主侧 setup.rs 执行。PluginHost.invoke_rust_command() 对 auto-task 的
//! 文件操作命令做特殊路由，直接调用 setup.rs 函数而非走 WASM invoke_command。

use bedcode_plugin_api::{WasmHost, WasmPlugin};
use bedcode_plugin_api::types::PluginManifest;

struct AutoTaskPlugin;

impl WasmPlugin for AutoTaskPlugin {
    const ID: &'static str = "com.bedcode.auto-task";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": "com.bedcode.auto-task",
            "name": "Auto Task",
            "version": "1.0.0",
            "description": "Claude Code 任务状态同步与自动授权",
            "author": "BedCode",
            "main": "index.js",
            "sandbox": "inline",
            "pluginType": "rust-ts",
            "rustLibrary": "bedcode_plugin_auto_task",
            "permissions": ["storage", "terminal:input", "terminal:output", "session:read"],
            "contributes": {
                "commands": [
                    { "id": "auto-task.setup-project-hooks", "title": "Setup Project Hooks" },
                    { "id": "auto-task.cleanup-project-hooks", "title": "Cleanup Project Hooks" },
                    { "id": "auto-task.get-task-status", "title": "Get Task Status" },
                    { "id": "auto-task.set-auto-mode", "title": "Set Auto Mode" }
                ],
                "lifecycle": {
                    "onStartup": true,
                    "onShutdown": true
                }
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin activated");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin deactivated");
        Ok(())
    }

    fn invoke_command(name: &str, _args_json: &str) -> anyhow::Result<serde_json::Value> {
        // 实际文件操作由宿主侧 setup.rs 执行（WASM 无法访问文件系统）
        // 宿主 PluginHost.invoke_rust_command() 对 auto-task 命令做特殊路由
        // WASM 侧仅处理不需要文件 I/O 的命令
        match name {
            "auto-task.get-task-status" => {
                let host = WasmHost::new(Self::ID);
                host.log_info("get-task-status requested (routed to host)");
                Ok(serde_json::json!({ "routed": true }))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    fn on_startup() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin on_startup");
        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = WasmHost::new(Self::ID);
        host.log_info("Auto Task plugin on_shutdown");
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(AutoTaskPlugin);
