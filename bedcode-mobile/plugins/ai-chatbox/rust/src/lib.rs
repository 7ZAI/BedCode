//! AI Chatbox Plugin (WASM, Mobile)
//!
//! 纯 AI 对话插件：JSONL 对话日志落盘 + 单一 OpenAI 兼容供应商协议。
//! 激活时集中目录授权（宿主 fs_auth 弹窗）：同意 → 初始化数据目录 → 激活成功；
//! 拒绝/超时 → 激活失败（Error 状态），重新启用可重试。
//! 数据目录：`{AppDownloadsDir}/ai-chatbox/`（插件目录之外，卸载不清用户数据）。
//! host 目标（cargo test/build）下 SDK 的 wasm_entry! 不生成任何导出（仅 wasm32 目标），
//! pub 命令面在 host 构建中被判 dead_code —— 属 SDK ABI v3 有意设计（见其 wasm.rs 注释），
//! 故仅对非 wasm32 目标放宽该 lint，wasm32 产物仍保留完整检查。
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod client;
mod commands;
mod store;

use bedcode_plugin_api_mobile::host::{HostConfig, HostFs, HostLog};
use bedcode_plugin_api_mobile::types::PluginManifest;
use bedcode_plugin_api_mobile::{WasmHost, WasmPlugin};
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

        // 数据目录：{AppDownloadsDir}/ai-chatbox/（Android 外部私有下载目录，免权限）
        let downloads = host
            .config_get(bedcode_plugin_api_mobile::host::ConfigKey::AppDownloadsDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: app downloads dir config unavailable"))?;
        let data_dir = format!("{}/ai-chatbox", downloads.trim_end_matches(['/', '\\']));

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

        host.log_info("Plugin activated (wasm, mobile)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        // 清空数据目录：同一进程内停用后重新激活可再次初始化（宿主复用 WASM 实例）
        if let Ok(mut guard) = DATA_DIR.write() {
            *guard = None;
        }
        host.log_info("Plugin deactivated (wasm, mobile)");
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

bedcode_plugin_api_mobile::wasm_entry!(AiChatboxPlugin);
