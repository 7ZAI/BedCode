//! File Transfer Plugin (WASM, Mobile)
//!
//! 内网文件传输插件 — 任务状态机、队列调度、持久化、续传握手、传输编排。
//! 下载方向全链路打通；上传方向依赖对端 JWT（另一 worker 补全中）。
//!
//! 与桌面端实现同构，仅 trait 差异：移动端 WasmPlugin 用 `on_bus_message`
//! 接收总线消息（桌面端叫 `on_message`）。

mod commands;
mod handshake;
mod peer;
mod queue;
mod state;

use bedcode_plugin_api_mobile::host::{HostBus, HostFileService, HostLog, HostTransfer};
use bedcode_plugin_api_mobile::types::{
    FileOperation, MountOptions, PluginManifest, UploadHookDecision, UploadRequestMeta,
};
use bedcode_plugin_api_mobile::wasm_host::WasmHost;
use bedcode_plugin_api_mobile::{BusMessage, WasmPlugin};
use commands::PluginState;
use peer::{MOUNT_PATH, PLUGIN_ID};
use state::TaskState;
use std::sync::{Mutex, OnceLock};

/// 全局插件状态（WASM 单线程，Mutex 保护回调间的并发）
static STATE: OnceLock<Mutex<PluginState>> = OnceLock::new();

fn state() -> &'static Mutex<PluginState> {
    STATE.get_or_init(|| Mutex::new(PluginState::new()))
}

/// 便捷 host 构造
fn host() -> WasmHost {
    WasmHost
}

struct FileTransferPlugin;

impl WasmPlugin for FileTransferPlugin {
    const ID: &'static str = PLUGIN_ID;

    fn manifest() -> PluginManifest {
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = host();
        host.log_info("File Transfer plugin activating (wasm, mobile)");

        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

        // 1. 加载设置
        s.settings = commands::load_settings(&host);
        let concurrency = s.settings.concurrency;
        s.queue.set_concurrency(concurrency);

        // 2. 挂载文件服务（roots 非空时）
        if !s.settings.roots.is_empty() {
            let options = MountOptions {
                mount_path: MOUNT_PATH.to_string(),
                roots: s.settings.roots.clone(),
                operations: vec![
                    FileOperation::List,
                    FileOperation::Download,
                    FileOperation::Upload,
                ],
            };
            match host.filesrv_mount(&options) {
                Ok(result) => {
                    s.mounted = true;
                    host.log_info(&format!("mounted at {}", result.base_path));
                }
                Err(e) => {
                    host.log_warn(&format!("mount failed (will retry later): {}", e));
                }
            }
        }

        // 3. 加载持久化任务（保留 paused/resumable，传输中残留降级为 resumable）
        s.tasks.load(&host);

        // 4. 初始化对端缓存（is_peer_desktop=true：移动插件的对端是桌面端，
        //    base 带 /api/plugins 前缀 + JWT token；peer_id 留空，由 peer_changed 首次上线自动采纳）
        s.peer.init(&host, "", true);

        // 5. 订阅总线 topics
        let _ = host.bus_subscribe("filesrv:peer_changed");

        host.log_info(&format!(
            "File Transfer activated: {} tasks loaded, {} roots, concurrency={}",
            s.tasks.len(),
            s.settings.roots.len(),
            s.settings.concurrency
        ));

        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = host();
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

        // flush 任务存储
        s.tasks.save(&host);

        // 取消所有活跃宿主任务
        let active_ids: Vec<String> = s
            .tasks
            .values()
            .filter(|t| t.state == TaskState::Transferring)
            .filter_map(|t| t.host_task_id.clone())
            .collect();
        for htid in &active_ids {
            let _ = host.transfer_cancel(htid);
        }

        // 卸载挂载
        if s.mounted {
            let _ = host.filesrv_unmount(MOUNT_PATH);
            s.mounted = false;
        }

        // 取消订阅
        let _ = host.bus_unsubscribe("filesrv:peer_changed");

        host.log_info("File Transfer plugin deactivated (wasm, mobile)");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = host();
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

        match name {
            "file-transfer.list-tasks" => Ok(commands::list_tasks(&s)),

            "file-transfer.query-peer" => commands::query_peer(&host),

            "file-transfer.list-remote" => commands::list_remote(&s, &host, &args),

            "file-transfer.enqueue" => {
                let result = commands::enqueue(&mut s, &host, &args)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.pause" => {
                let task_id = args
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing taskId"))?
                    .to_string();
                let result = commands::pause(&mut s, &host, &task_id)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.resume" => {
                let task_id = args
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing taskId"))?
                    .to_string();
                let result = commands::resume(&mut s, &host, &task_id)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.cancel" => {
                let task_id = args
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing taskId"))?
                    .to_string();
                let result = commands::cancel(&mut s, &host, &task_id)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.resume-all" => {
                let result = commands::resume_all(&mut s, &host)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.retry" => {
                let task_id = args
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing taskId"))?
                    .to_string();
                let result = commands::retry(&mut s, &host, &task_id)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.set-concurrency" => commands::set_concurrency(&mut s, &host, &args),

            "file-transfer.get-settings" => Ok(commands::get_settings(&s, &host)),

            "file-transfer.set-settings" => {
                let result = commands::set_settings(&mut s, &host, &args)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.pick-download-dir" => commands::pick_download_dir(),

            "file-transfer.mount-local" => commands::mount_local(&mut s, &host, &args),

            "file-transfer.update-roots" => commands::update_roots(&mut s, &host, &args),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    fn on_bus_message(msg: &BusMessage) -> anyhow::Result<()> {
        let host = host();

        if msg.topic.starts_with("transfer:") {
            let progress: bedcode_plugin_api_mobile::types::TransferProgress =
                serde_json::from_value(msg.payload.clone()).map_err(|e| {
                    anyhow::anyhow!("invalid transfer progress payload: {}", e)
                })?;
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_transfer_progress(&mut s, &host, &progress);
            commands::schedule_and_start(&mut s, &host);
            return Ok(());
        }

        if msg.topic == "filesrv:peer_changed" {
            let peer_id = msg
                .payload
                .get("peerId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let online = msg
                .payload
                .get("online")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_peer_changed(&mut s, &host, peer_id, online);
            return Ok(());
        }

        Ok(())
    }

    fn on_upload_request(meta: &UploadRequestMeta) -> UploadHookDecision {
        let host = host();
        let s = state().lock().unwrap_or_else(|e| e.into_inner());
        commands::handle_upload_request(&s, &host, meta)
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = host();
        let s = state().lock().unwrap_or_else(|e| e.into_inner());
        s.tasks.save(&host);
        host.log_info("File Transfer: tasks flushed on shutdown");
        Ok(())
    }
}

bedcode_plugin_api_mobile::wasm_entry!(FileTransferPlugin);
