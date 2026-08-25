//! AI Chatbox Plugin (WASM, wasm32-wasip2)
//!
//! 纯 AI 对话插件：JSONL 对话日志落盘 + 多方言供应商协议（请求构建与 SSE 解析
//! 在前端适配层 src/adapters/，Rust 仅透传 http_fetch 载荷）。
//! 文件访问全部经 WASI：宿主实例化时按 manifest `wasiPreopenDirs` 声明
//! （${home} 展开 + 授权校验）预打开数据目录到 `/data`，本插件 std::fs 直连，
//! 不经宿主 fs_* 转发。激活时集中目录授权（fs_auth 弹窗）：同意 → 持久化授权
//! 记录（下次实例化据此建立预打开）→ 初始化数据目录；拒绝/超时 → 激活失败
//! （Error 状态），重新启用可重试。首次启用需停用再启用一次完成预打开挂载。

mod client;
mod commands;
mod store;

use bedcode_plugin_api::host::{HostConfig, HostFs, HostLog};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};

/// WASI 预打开根路径（guest 视角）：宿主按 manifest wasiPreopenDirs 首项挂载
const DATA_ROOT: &str = "/data";

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;

        // 数据目录固定：{HomeDir}/.bedcode/ai-chatbox/（与 manifest wasiPreopenDirs
        // 声明一致；此处仅用于授权弹窗展示与持久化授权记录）
        let home = host
            .config_get(bedcode_plugin_api::host::ConfigKey::HomeDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: home_dir config unavailable"))?;
        let data_dir = format!(
            "{}/.bedcode/ai-chatbox",
            home.trim_end_matches(['/', '\\'])
        );

        // 集中目录授权：同意 → 授权记录持久化（宿主下次实例化据此建立 WASI 预打开）；
        // 未同意（拒绝/30s 超时）→ 激活失败，重新启用可再次弹窗
        let allowed = host
            .fs_request_auth(&[data_dir.clone()])
            .map_err(|e| anyhow::anyhow!("activate: fs_request_auth failed: {}", e))?;
        if !allowed {
            return Err(anyhow::anyhow!(
                "目录授权被拒绝：{}，请在插件设置中重新启用以再次授权",
                data_dir
            ));
        }

        // WASI 预打开自检：若本次实例化时该目录尚未授权（首次启用），
        // /data 未挂载——授权已随上方弹窗落库，停用再启用即生效
        if std::fs::metadata(DATA_ROOT).is_err() {
            return Err(anyhow::anyhow!(
                "WASI 预打开目录未就绪：{} 的授权已保存，请停用后重新启用插件完成初始化",
                data_dir
            ));
        }

        store::init(&host, DATA_ROOT)?;

        host.log_info("Plugin activated (wasm, wasi file access)");
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        WasmHost.log_info("Plugin deactivated (wasm)");
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
