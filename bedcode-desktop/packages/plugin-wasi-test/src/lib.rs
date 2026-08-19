//! WASI preopen 测试插件（wasm32-wasip2 目标）
//!
//! 与组件共存插件（wasm32-unknown-unknown）的区别：本插件编译到
//! WASI preview2 目标（rustup target add wasm32-wasip2），std::fs 直连
//! WASI 文件系统——宿主在实例化时按插件配置预打开目录 `/data` 后，
//! 插件即拥有对该目录的直接读写能力（无需宿主 fs_* 转发）。
//!
//! 仅供宿主测试套件加载验证（宿主侧 E2E：preopen → std::fs 写 → 宿主
//! 侧校验落盘文件）。

use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};

struct WasiTestPlugin;

impl WasmPlugin for WasiTestPlugin {
    const ID: &'static str = "com.bedcode.wasi-test";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            // 经 WASI preopen `/data` 直接写文件（宿主侧校验落盘）
            "wasi-test.write-file" => {
                std::fs::write("/data/demo.txt", "hello-from-wasi")
                    .map_err(|e| anyhow::anyhow!("wasi write failed: {}", e))?;
                Ok(serde_json::json!({ "ok": true }))
            }
            // 读回（内容应为上一次写入的字面量）
            "wasi-test.read-file" => {
                let content = std::fs::read_to_string("/data/demo.txt")
                    .map_err(|e| anyhow::anyhow!("wasi read failed: {}", e))?;
                Ok(serde_json::json!({ "content": content }))
            }
            // 列举预打开目录（证明 preopen 对 guest 可见）
            "wasi-test.list" => {
                let entries: Vec<String> = std::fs::read_dir("/data")
                    .map_err(|e| anyhow::anyhow!("wasi read_dir failed: {}", e))?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect();
                Ok(serde_json::json!({ "entries": entries }))
            }
            // 访问预打开根之外的路径必须失败（WASI 沙箱边界断言）
            "wasi-test.outside-root" => {
                match std::fs::metadata("/tmp/outside.txt") {
                    Ok(_) => Ok(serde_json::json!({ "leaked": true })),
                    Err(e) => Ok(serde_json::json!({ "leaked": false, "error": e.to_string() })),
                }
            }
            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }
}

bedcode_plugin_api::wasm_entry!(WasiTestPlugin);