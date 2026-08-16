//! AI Chatbox Plugin (WASM)
//!
//! 纯 AI 对话插件：JSONL 对话日志落盘 + 多方言供应商协议（请求构建与 SSE 解析
//! 在前端适配层 src/adapters/，Rust 仅透传 http_fetch 载荷）。
//! 激活时集中目录授权（宿主 fs_auth 弹窗）：同意 → 初始化数据目录 → 激活成功；
//! 拒绝/超时 → 激活失败（Error 状态），重新启用可重试。

mod client;
mod commands;
mod store;

use bedcode_plugin_api::host::{HostConfig, HostFs, HostLog};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};
use std::sync::RwLock;

/// 数据目录（activate 时解析；deactivate 时清空，支持同一进程内停用后重新激活）
static DATA_DIR: RwLock<Option<String>> = RwLock::new(None);

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;

        // 数据目录：{HomeDir}/.bedcode/ai-chatbox/（插件目录外，卸载不清用户数据）
        let home = host
            .config_get(bedcode_plugin_api::host::ConfigKey::HomeDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: home_dir config unavailable"))?;
        let data_dir = format!("{}/.bedcode/ai-chatbox", home.trim_end_matches(['/', '\\']));

        // 集中目录授权：未同意（拒绝/30s 超时）→ 激活失败，重新启用可再次弹窗
        let allowed = host
            .fs_request_auth(&[data_dir.clone()])
            .map_err(|e| anyhow::anyhow!("activate: fs_request_auth failed: {}", e))?;
        if !allowed {
            return Err(anyhow::anyhow!(
                "目录授权被拒绝：{}，请在插件设置中重新启用以再次授权",
                data_dir
            ));
        }

        *DATA_DIR
            .write()
            .map_err(|e| anyhow::anyhow!("activate: data_dir lock poisoned: {}", e))? = Some(data_dir.clone());
        store::init(&host, &data_dir)?;

        host.log_info("Plugin activated (wasm)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        // 清空数据目录：同一进程内停用后重新激活可再次初始化（宿主复用 WASM 实例）
        if let Ok(mut guard) = DATA_DIR.write() {
            *guard = None;
        }
        host.log_info("Plugin deactivated (wasm)");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            "ai-chatbox.chat-stream" => commands::chat_stream(args),
            "ai-chatbox.chat-complete" => commands::chat_complete(args),
            "ai-chatbox.fetch-models" => commands::fetch_models(args),
            "ai-chatbox.list-conversations" => commands::list_conversations(args),
            "ai-chatbox.get-messages" => commands::get_messages(args),
            "ai-chatbox.save-conversation" => commands::save_conversation(args),
            "ai-chatbox.save-message" => commands::save_message(args),
            "ai-chatbox.delete-conversation" => commands::delete_conversation(args),
            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }
}

bedcode_plugin_api::wasm_entry!(AiChatboxPlugin);
