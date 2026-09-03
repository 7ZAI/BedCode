//! OCR Plugin (WASM shell — Mobile)
//!
//! 最小壳实现（spec §6）：仅激活/停用日志，不实现识别命令路由——
//! 识别/取图命令由宿主命令直供（plugin_ocr_* / plugin_pick_image /
//! plugin_camera_capture），前端经 context.ocr.* 调用，全程不经 WASM。
//!
//! host 目标（cargo test/build）下 SDK 的 wasm_entry! 不生成任何导出
//! （仅 wasm32 目标），pub 命令面在 host 构建中被判 dead_code —— 属
//! SDK ABI v3 有意设计（见其 wasm.rs 注释），故仅对非 wasm32 目标
//! 放宽该 lint，wasm32 产物仍保留完整检查。
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bedcode_plugin_api_mobile::types::PluginManifest;
use bedcode_plugin_api_mobile::{HostLog, WasmHost, WasmPlugin};

struct OcrPlugin;

impl WasmPlugin for OcrPlugin {
    const ID: &'static str = "com.bedcode.ocr";

    fn manifest() -> PluginManifest {
        // plugin.json 为 manifest 单一事实来源（bedcode-plugin build/package 时自动填充）
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        WasmHost.log_info("OCR plugin activated (wasm shell; recognize via host commands)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        WasmHost.log_info("OCR plugin deactivated (wasm shell)");
        Ok(())
    }

    fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // 无命令面：前端全部经 context.ocr.* 直通宿主命令（spec §6）
        Err(anyhow::anyhow!("Unknown command: {}", name))
    }
}

bedcode_plugin_api_mobile::wasm_entry!(OcrPlugin);
