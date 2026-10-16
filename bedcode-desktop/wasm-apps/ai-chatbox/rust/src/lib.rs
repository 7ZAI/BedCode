//! AI Chatbox Plugin (WASM, wasm32-wasip3)
//!
//! 纯 AI 对话插件：JSONL 对话日志落盘 + 多方言供应商协议（请求构建与 SSE 解析
//! 在前端适配层 src/adapters/，Rust 仅透传 http_fetch 载荷）。
//!
//! **文件访问经宿主 `host-fs` 原语**（`store.rs` 全部 IO 走它，不经 WASI）：
//! WASI 0.3 的 filesystem 方法是 `async func`，而插件导出（`activate` /
//! `invoke_command`…）是 sync-lifted——wasmtime 进入 sync 导出时会清掉 task 的
//! `may_block` 标志，guest 一旦等待 async import 即 trap
//! `wasm trap: cannot block a synchronous task before returning`
//! （CannotBlockSyncTask），故 wasip3 目标下必须走宿主 fs 原语。
//!
//! 数据根 = `{HomeDir}/.bedcode/ai-chatbox`；激活时经 `fs_request_auth` 集中申请
//! 一次目录授权（同意后宿主持久化记住，后续逐调用免弹窗），拒绝/超时 → 激活失败
//! （Error 状态），重新启用可重试。

mod client;
mod commands;
mod store;

use std::sync::OnceLock;

use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostFs, HostLog};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};

/// 插件数据根（宿主绝对路径）：`activate` 解析成功后缓存，命令面复用。
///
/// 单一事实来源：路径只在 activate 里算一次；命令面读缓存，未激活时显性报错
/// （宿主只在激活成功后才派发插件命令，该分支属防御性 fail-visible）。
static DATA_ROOT: OnceLock<String> = OnceLock::new();

/// 数据根访问（activate 未完成 → `None`）
pub(crate) fn data_root() -> Option<&'static str> {
    DATA_ROOT.get().map(|s| s.as_str())
}

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;

        // 数据目录固定：{HomeDir}/.bedcode/ai-chatbox/
        let home = host
            .config_get(ConfigKey::HomeDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: home_dir config unavailable"))?;
        let data_dir = format!("{}/.bedcode/ai-chatbox", home.trim_end_matches(['/', '\\']));

        // 集中目录授权（host-fs 三层校验的「记住」层）：同意一次 → 宿主持久化，
        // 后续逐调用免弹窗；拒绝/超时 → 激活失败，重新启用可再次弹窗
        let allowed = host
            .fs_request_auth(&[data_dir.clone()])
            .map_err(|e| anyhow::anyhow!("activate: fs_request_auth failed: {}", e))?;
        if !allowed {
            return Err(anyhow::anyhow!(
                "目录授权被拒绝：{}，请在插件设置中重新启用以再次授权",
                data_dir
            ));
        }

        store::init(&host, &data_dir)?;
        // 缓存供命令面复用（重复 set 只可能是同值重入，幂等，保留首次值）
        let _ = DATA_ROOT.set(data_dir);

        host.log_info("Plugin activated (wasm, host-fs access)");
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
