//! 组件形态测试插件（迁移阶段 A 验证）
//!
//! 与 `plugin-test`（core module 形态）平行：用 WIT 契约 + wit-bindgen 构建
//! 组件，验证宿主 Component Model 路径（阶段 A 共存）的完整往返：
//! - export：全部 7 个接口（command / lifecycle / events / terminal-hooks /
//!   upload-hook / manifest / abi）
//! - import：host-storage（命令调用内做读往返）、host-log（生命周期激活打日志）
//!
//! 构建产物是 core module（wit-bindgen 绑定），宿主测试用
//! `wit_component::ComponentEncoder` 编码为组件后加载（等价于
//! `wasm-tools component new`，阶段 B SDK 构建脚本将内置该步骤）。

wit_bindgen::generate!({
    path: "../plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin-phase-a",
});

use crate::bedcode::plugin::host_log;
use crate::bedcode::plugin::host_storage;
use crate::exports::bedcode::plugin::{abi, command, events, lifecycle, manifest, terminal_hooks, upload_hook};

struct Guest;

impl command::Guest for Guest {
    fn invoke(name: String, args: String) -> String {
        // 调用宿主 import：storage 读往返（key 由宿主测试预先写入）
        match host_storage::get("component-test-key") {
            Ok(Some(v)) => format!("{{\"name\":\"{}\",\"args\":{},\"stored\":{}}}", name, args, v),
            Ok(None) => format!("{{\"name\":\"{}\",\"args\":{},\"stored\":null}}", name, args),
            Err(e) => format!("{{\"error\":\"{}\"}}", e),
        }
    }
}

impl lifecycle::Guest for Guest {
    fn activate() -> Result<(), String> {
        host_log::info("component test plugin activated");
        Ok(())
    }

    fn deactivate() -> Result<(), String> {
        Ok(())
    }

    fn on_startup() {}

    fn on_shutdown() {}
}

impl events::Guest for Guest {
    fn on_message(topic: String, _sender: String, _payload: String) -> Result<(), String> {
        host_log::info(&format!("component test plugin on_message: {}", topic));
        Ok(())
    }

    fn on_session_lifecycle(_payload: String) -> Result<(), String> {
        Ok(())
    }

    fn on_input_submitted(_payload: String) -> Result<(), String> {
        Ok(())
    }
}

impl terminal_hooks::Guest for Guest {
    // 与 core 形态 plugin-test 行为对齐（大写转换），宿主测试断言同一语义
    fn on_terminal_input(_session_id: String, text: String) -> Option<String> {
        Some(text.to_uppercase())
    }

    fn on_terminal_output(_session_id: String, data: String) -> Option<String> {
        Some(data.to_uppercase())
    }
}

impl upload_hook::Guest for Guest {
    // fail-closed 语义由宿主保持；测试插件固定拒绝并附原因
    fn on_upload_request(meta_json: String) -> String {
        format!(
            "{{\"allow\":false,\"reason\":\"component-test deny ({})\"}}",
            meta_json.len()
        )
    }
}

impl manifest::Guest for Guest {
    fn get() -> String {
        r#"{"id":"com.bedcode.component-test","version":"0.1.0","name":"Component Test"}"#
            .to_string()
    }
}

impl abi::Guest for Guest {
    // 与 SDK abi::ABI_VERSION（当前 v6）保持一致；宿主按 `abi.form()==1` 识别组件形态
    fn version() -> u32 {
        6
    }

    fn form() -> u32 {
        1
    }
}

export!(Guest);
