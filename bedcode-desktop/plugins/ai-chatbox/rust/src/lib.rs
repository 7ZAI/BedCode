//! AI Chatbox Plugin (WASM)
//!
//! AI 大模型对话与终端提示词优化
//! 使用 bedcode-plugin-api WasmPlugin trait 实现，通过 wasm_entry! 宏生成导出

mod ai_client;
mod commands;
mod db;
mod terminal;

use bedcode_plugin_api::host::HostLog;
use bedcode_plugin_api::{WasmPlugin, WasmHost};
use bedcode_plugin_api::types::PluginManifest;

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        let json = serde_json::json!({
            "id": "com.bedcode.ai-chatbox",
            "name": "AI Chatbox",
            "version": "1.0.0",
            "description": "AI 大模型对话与终端提示词优化",
            "author": "BedCode",
            "main": "index.js",
            "sandbox": "inline",
            "pluginType": "rust-ts",
            "rustLibrary": "bedcode_plugin_ai_chatbox",
            "permissions": ["ui:sidebar", "ui:input", "storage", "terminal:input", "terminal:output", "session:read"],
            "contributes": {
                "commands": [
                    { "id": "ai-chatbox.chat-stream", "title": "AI Chat Stream" },
                    { "id": "ai-chatbox.chat-complete", "title": "AI Chat Complete" },
                    { "id": "ai-chatbox.optimize-prompt", "title": "Optimize Prompt" },
                    { "id": "ai-chatbox.list-conversations", "title": "List Conversations" },
                    { "id": "ai-chatbox.get-messages", "title": "Get Messages" },
                    { "id": "ai-chatbox.save-conversation", "title": "Save Conversation" },
                    { "id": "ai-chatbox.save-message", "title": "Save Message" },
                    { "id": "ai-chatbox.delete-conversation", "title": "Delete Conversation" }
                ],
                "views": [
                    { "id": "ai-chatbox.sidebar", "type": "sidebar", "title": "AI 对话", "component": "ChatView" }
                ],
                "terminal": {
                    "inputHandlers": ["on_terminal_input"],
                    "outputParsers": []
                },
                "configuration": {
                    "title": "AI Chatbox Settings",
                    "properties": {
                        "apiProviders": { "type": "string", "title": "API Providers (JSON)", "description": "JSON array of API provider configs", "default": "[]" },
                        "activeProvider": { "type": "string", "title": "Active Provider ID", "default": "" },
                        "activeModel": { "type": "string", "title": "Active Model", "default": "" }
                    }
                }
            }
        });
        serde_json::from_value(json).expect("Invalid manifest JSON")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        db::init(&host)?;
        host.log_info("Plugin activated (wasm)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Plugin deactivated (wasm)");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            "ai-chatbox.chat-stream" => commands::chat_stream(args),
            "ai-chatbox.chat-complete" => commands::chat_complete(args),
            "ai-chatbox.optimize-prompt" => commands::optimize_prompt(args),
            "ai-chatbox.list-conversations" => commands::list_conversations(args),
            "ai-chatbox.get-messages" => commands::get_messages(args),
            "ai-chatbox.save-conversation" => commands::save_conversation(args),
            "ai-chatbox.save-message" => commands::save_message(args),
            "ai-chatbox.delete-conversation" => commands::delete_conversation(args),
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

bedcode_plugin_api::wasm_entry!(AiChatboxPlugin);
