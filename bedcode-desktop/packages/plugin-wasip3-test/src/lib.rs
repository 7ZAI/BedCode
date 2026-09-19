//! wasip3 编译链测试插件（wasm32-wasip3 目标）
//!
//! 与组件共存插件（wasm32-unknown-unknown）的区别：本插件编译到 WASI 0.3
//! target（`rustup target add wasm32-wasip3 --toolchain <pinned-nightly>`），
//! cdylib 产物**直接是 Component**（magic `\0asm` + `0d 00 01 00`，免
//! componentize/wit-component 步骤）——见 `docs/knowledge/wasip3-toolchain.md`。
//!
//! 用途：
//! 1. 工具链健康基线（票 01）：pinned nightly + wasm32-wasip3 target 下可编译、
//!    产物为组件（`scripts/wasip3-toolchain.sh fixture` 校验）。
//! 2. 宿主 async 化门禁（票 02）将基于本 fixture 扩展：import `wasi:random`
//!    （async `get-random-bytes`）与 `wasi:clocks` 的解析/实例化闭环。
//!
//! 仅供宿主测试套件加载验证，不进 resources/plugins/ 分发。

use bedcode_plugin_api::types::{PluginKind, PluginManifest, PluginType};
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_entry;

struct Wasip3TestPlugin;

impl WasmPlugin for Wasip3TestPlugin {
    const ID: &'static str = "com.bedcode.wasip3-test";

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: Self::ID.to_string(),
            name: "wasip3-test".to_string(),
            version: "0.1.0".to_string(),
            description: "wasip3 编译链测试插件".to_string(),
            author: String::new(),
            main: String::new(),
            sandbox: "inline".to_string(),
            permissions: Vec::new(),
            api: Vec::new(),
            contributes: Default::default(),
            plugin_type: PluginType::Rust,
            rust_library: String::new(),
            icon: None,
            wasi_preopen_dirs: Vec::new(),
            kind: PluginKind::Application,
            dependencies: Vec::new(),
            resource_overrides: None,
        }
    }

    fn activate() -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        Ok(())
    }

    fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            // wasip3 时钟可读性探测：std::time 在 WASI 0.3 下走 wasi:clocks 导入
            // （async 语义的时钟接口由宿主在 A0-3 async linker 中提供；p2 sync
            // 宿主不可实例化本组件，此命令仅作为编译链 + import 面的静态证明）
            "wasip3-test.read-clock" => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                Ok(serde_json::json!({ "unix_ms": now }))
            }
            // wasip3 熵源探测：async `wasi:random` get-random-bytes 返回真随机字节
            // （宿主测试断言非零 + 跨调用不等，票 02 A1 闭环）
            "wasip3-test.get-random" => {
                let mut buf = [0u8; 32];
                getrandom::fill(&mut buf)
                    .map_err(|e| anyhow::anyhow!("getrandom failed: {}", e))?;
                let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
                Ok(serde_json::json!({ "hex": hex }))
            }
            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }
}

wasm_entry!(Wasip3TestPlugin);