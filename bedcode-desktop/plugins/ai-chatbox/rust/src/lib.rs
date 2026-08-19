//! AI Chatbox Plugin (WASM)
//!
//! 纯 AI 对话插件：JSONL 对话日志落盘 + 多方言供应商协议（请求构建与 SSE 解析
//! 在前端适配层 src/adapters/，Rust 仅透传 http_fetch 载荷）。
//! 激活时集中目录授权（宿主 fs_auth 弹窗）：同意 → 初始化数据目录 → 激活成功；
//! 拒绝/超时 → 激活失败（Error 状态），重新启用可重试。
//! 数据目录由插件配置 defaultDir 指定（空 = 默认 {home}/.bedcode/ai-chatbox）；
//! useSelfFileAccess 开启时一并预授权 WASI 预打开目录（宿主 WASI 接线后生效）。

mod client;
mod commands;
mod store;

use bedcode_plugin_api::host::{HostConfig, HostFs, HostLog, HostStorage};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::{WasmHost, WasmPlugin};
use std::sync::RwLock;

/// 数据目录（activate 时解析；deactivate 时清空，支持同一进程内停用后重新激活）
static DATA_DIR: RwLock<Option<String>> = RwLock::new(None);

/// 插件配置 storage key（与前端 PLUGIN_CONFIG_STORAGE_KEY 约定一致，见 SDK）
const PLUGIN_CONFIG_STORAGE_KEY: &str = "config";

struct AiChatboxPlugin;

impl WasmPlugin for AiChatboxPlugin {
    const ID: &'static str = "com.bedcode.ai-chatbox";

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;

        // 数据目录默认：{HomeDir}/.bedcode/ai-chatbox/（插件目录外，卸载不清用户数据）；
        // 插件配置 defaultDir 非空时优先（读取 storage key `config`，与宿主配置页同一 key）
        let home = host
            .config_get(bedcode_plugin_api::host::ConfigKey::HomeDir)?
            .ok_or_else(|| anyhow::anyhow!("activate: home_dir config unavailable"))?;

        // 文件访问配置：读取失败/缺失不阻断激活，按默认值走（宿主 fs 路径）
        let file_cfg = match host.storage_get(PLUGIN_CONFIG_STORAGE_KEY) {
            Ok(Some(v)) => store::parse_file_access_config(Some(&v)),
            Ok(None) => store::parse_file_access_config(None),
            Err(e) => {
                host.log_warn(&format!("activate: config read failed, use defaults: {}", e));
                store::parse_file_access_config(None)
            }
        };
        let data_dir = store::resolve_data_dir(&home, file_cfg.default_dir.as_deref());

        // 集中目录授权：数据目录 + 自身文件访问（WASI 预打开）目录一并授权；
        // 未同意（拒绝/30s 超时）→ 激活失败，重新启用可再次弹窗
        let mut auth_paths = vec![data_dir.clone()];
        if file_cfg.use_self_file_access {
            if let Some(dir) = file_cfg.file_access_dir {
                host.log_info(&format!(
                    "activate: self file access enabled, preopen dir: {}",
                    dir
                ));
                auth_paths.push(dir);
            }
        }
        let allowed = host
            .fs_request_auth(&auth_paths)
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
