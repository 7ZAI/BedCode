//! File Transfer Plugin (WASM, Desktop)
//!
//! 内网文件传输插件 — 任务状态机、队列调度、持久化、续传握手、传输编排。
//! 下载方向全链路打通；上传方向依赖对端 JWT（另一 worker 补全中）。

mod commands;
mod handshake;
mod peer;
mod queue;
mod state;

use bedcode_plugin_api::host::{HostBus, HostFileService, HostLog, HostTransfer};
use bedcode_plugin_api::types::{
    PluginManifest, TransferRequestMeta, UploadHookDecision,
    UploadRequestMeta,
};
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, WasmPlugin};
use commands::PluginState;
use peer::{PeerStore, MOUNT_PATH, PLUGIN_ID};
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
        host.log_info("File Transfer plugin activating (wasm, desktop)");

        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

        // 1. 加载设置
        s.settings = commands::load_settings(&host);
        let concurrency = s.settings.concurrency;
        s.queue.set_concurrency(concurrency);

        // 2. 挂载文件服务（roots 非空时）
        if !s.settings.roots.is_empty() {
            let options = commands::build_mount_options(&s.settings.roots, &commands::resolve_download_dir(&s, &host).ok());
            match host.filesrv_mount(&options) {
                Ok(result) => {
                    s.mounted = true;
                    host.log_info(&format!("mounted at {}", result.base_path));
                    // v2：挂载后同步批准超时（宿主 pending 批 TTL 扫描配置）
                    commands::sync_approval_timeout(&s, &host);
                }
                Err(e) => {
                    host.log_warn(&format!("mount failed (will retry later): {}", e));
                }
            }
        }

        // 3. 加载持久化任务（保留 paused/resumable，传输中残留降级为 resumable；
        //    v2：WaitingApproval 丢弃——批上下文不可恢复）与传输历史
        s.tasks.load(&host);
        s.history.load(&host);

        // 4. 初始化对端存储（is_peer_desktop=false：桌面插件的对端是移动端，
        //    base 无 /api/plugins 前缀；对端列表由 peer_changed 事件驱动增删）
        s.peer = PeerStore::new(false);

        // 5. 订阅总线 topics（v2 新增接收端 4 topic + 发送端应答 topic）
        let _ = host.bus_subscribe("filesrv:peer_changed");
        let _ = host.bus_subscribe("filesrv:transfer_request");
        let _ = host.bus_subscribe("filesrv:transfer_resolved");
        let _ = host.bus_subscribe("filesrv:receiving_started");
        let _ = host.bus_subscribe("filesrv:receiving_done");
        let _ = host.bus_subscribe("filesrv:transfer_approval");

        // 6. 主动探测对端（修复插件激活晚于认证导致的总线事件丢失：
        //    activate 完成即广播 Query，对端回复后宿主推送 peer_changed）
        if let Err(e) = host.filesrv_query_peer("") {
            host.log_warn(&format!(
                "peer probe on activate failed (will recover on next event/query): {}",
                e
            ));
        }

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
        let _ = host.bus_unsubscribe("filesrv:transfer_request");
        let _ = host.bus_unsubscribe("filesrv:transfer_resolved");
        let _ = host.bus_unsubscribe("filesrv:receiving_started");
        let _ = host.bus_unsubscribe("filesrv:receiving_done");
        let _ = host.bus_unsubscribe("filesrv:transfer_approval");

        // v2 接收状态清空：WASM 静态 state 跨 deactivate/activate 存活，残留
        // 批卡/接收任务会在下次激活时陈旧复现（spec §9.5：接收状态不跨生命周期）
        s.batches.clear();
        s.pending_batches.clear();
        s.receiving.clear();

        // flush 历史（终态归档不丢）
        s.history.save(&host);

        host.log_info("File Transfer plugin deactivated (wasm, desktop)");
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let host = host();
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());

        match name {
            "file-transfer.list-tasks" => Ok(commands::list_tasks(&s)),

            "file-transfer.query-peer" => commands::query_peer(&host),

            "file-transfer.list-peers" => Ok(commands::list_peers(&s)),

            "file-transfer.set-active-peer" => {
                let peer_id = args
                    .get("peerId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing peerId"))?
                    .to_string();
                let result = commands::set_active_peer(&mut s, &host, &peer_id)?;
                Ok(result)
            }

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

            "file-transfer.remove-task" => {
                let task_id = args
                    .get("taskId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing taskId"))?
                    .to_string();
                let result = commands::remove_task(&mut s, &host, &task_id)?;
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

            "file-transfer.get-settings" => Ok(commands::get_settings(&s)),

            "file-transfer.set-settings" => {
                let result = commands::set_settings(&mut s, &host, &args)?;
                commands::schedule_and_start(&mut s, &host);
                Ok(result)
            }

            "file-transfer.list-batches" => Ok(commands::list_batches(&s)),

            "file-transfer.approve-batch" => {
                let batch_id = args
                    .get("batchId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing batchId"))?
                    .to_string();
                commands::approve_batch(&mut s, &host, &batch_id)
            }

            "file-transfer.reject-batch" => {
                let batch_id = args
                    .get("batchId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing batchId"))?
                    .to_string();
                commands::reject_batch(&mut s, &host, &batch_id)
            }

            "file-transfer.list-receiving" => Ok(commands::list_receiving(&s)),

            "file-transfer.cancel-receiving" => {
                let session_id = args
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing sessionId"))?
                    .to_string();
                commands::cancel_receiving(&mut s, &host, &session_id)
            }

            "file-transfer.list-history" => Ok(commands::list_history(&s)),

            "file-transfer.clear-history" => commands::clear_history(&mut s, &host),

            "file-transfer.pick-download-dir" => commands::pick_download_dir(),

            "file-transfer.mount-local" => commands::mount_local(&mut s, &host, &args),

            "file-transfer.update-roots" => commands::update_roots(&mut s, &host, &args),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        let host = host();

        if msg.topic.starts_with("transfer:") {
            let progress: bedcode_plugin_api::types::TransferProgress =
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

        // v2 接收端：批量传输请求 / 已解决（pending 批卡数据源）
        if msg.topic == "filesrv:transfer_request" {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_transfer_request(&mut s, &host, &msg.payload);
            return Ok(());
        }
        if msg.topic == "filesrv:transfer_resolved" {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_transfer_resolved(&mut s, &host, &msg.payload);
            return Ok(());
        }
        // v2 接收端：接收任务开始/结束（正在接收 tab + 历史归档）
        if msg.topic == "filesrv:receiving_started" {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_receiving_started(&mut s, &host, &msg.payload);
            return Ok(());
        }
        if msg.topic == "filesrv:receiving_done" {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_receiving_done(&mut s, &host, &msg.payload);
            return Ok(());
        }
        // v2 发送端：传输批应答（批准后批内任务重新调度 / 拒绝终态）
        if msg.topic == "filesrv:transfer_approval" {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            commands::handle_transfer_approval(&mut s, &host, &msg.payload);
            commands::schedule_and_start(&mut s, &host);
            return Ok(());
        }

        Ok(())
    }

    fn on_upload_request(meta: &UploadRequestMeta) -> UploadHookDecision {
        let host = host();
        let s = state().lock().unwrap_or_else(|e| e.into_inner());
        commands::handle_upload_request(&s, &host, meta)
    }

    fn on_transfer_request(meta: &TransferRequestMeta) -> UploadHookDecision {
        // v2：按接收策略分流（accept → allow；reject → deny；ask → ask）
        let _ = meta;
        let s = state().lock().unwrap_or_else(|e| e.into_inner());
        commands::handle_transfer_request_hook(&s)
    }

    fn on_shutdown() -> anyhow::Result<()> {
        let host = host();
        let s = state().lock().unwrap_or_else(|e| e.into_inner());
        s.tasks.save(&host);
        host.log_info("File Transfer: tasks flushed on shutdown");
        Ok(())
    }
}

bedcode_plugin_api::wasm_entry!(FileTransferPlugin);
