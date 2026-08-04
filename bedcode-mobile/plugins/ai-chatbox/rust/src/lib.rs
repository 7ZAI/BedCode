//! AI Chatbox Plugin (WASM, Mobile)
//!
//! AI 大模型对话与终端提示词优化
//! 使用 bedcode-plugin-api-mobile WasmPlugin trait 实现，通过 wasm_entry! 宏生成导出

mod ai_client;
mod commands;
mod db;

use bedcode_plugin_api_mobile::{HostLog, WasmPlugin, WasmHost};
use bedcode_plugin_api_mobile::types::PluginManifest;

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        // plugin.json 为 manifest 单一事实来源（bedcode-plugin build/package 时自动填充）
        serde_json::from_str(include_str!("../../plugin.json")).expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        db::init(&host)?;
        host.log_info("Plugin activated (wasm, mobile)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Plugin deactivated (wasm, mobile)");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            "ai-chatbox.chat-stream" => commands::chat_stream(&args),
            "ai-chatbox.chat-complete" => commands::chat_complete(&args),
            "ai-chatbox.optimize-prompt" => commands::optimize_prompt(&args),
            "ai-chatbox.list-conversations" => commands::list_conversations(&args),
            "ai-chatbox.get-messages" => commands::get_messages(&args),
            "ai-chatbox.save-conversation" => commands::save_conversation(&args),
            "ai-chatbox.save-message" => commands::save_message(&args),
            "ai-chatbox.delete-conversation" => commands::delete_conversation(&args),
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }

    fn on_terminal_input(_session_id: &str, _text: &str) -> Option<String> {
        None
    }

    fn on_terminal_output(_session_id: &str, _data: &str) -> Option<String> {
        None
    }
}

bedcode_plugin_api_mobile::wasm_entry!(AiChatboxPlugin);
