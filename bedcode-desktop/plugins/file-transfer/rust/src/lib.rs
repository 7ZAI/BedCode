//! File Transfer Plugin (WASM, Desktop) — 业务自持版（issue 13 Phase 3）
//!
//! ADR 0022 v2 后插件接管全部业务状态，宿主只提供无业务语义的引擎原语：
//!
//! - 设备缓存：前端 deviceState.ts 自持（found/lost/TTL/cap 位），本 crate 只做
//!   browse 生命周期、`mdns:*` 透传与快照持久化（见 device_bridge）；
//! - 共享根注册表：plugin-database 真源 + `set-shared-roots` 全量推送（roots_registry）；
//! - 任务队列与历史：`peer:transfer`/`peer:receive` 快照驱动的自有存储
//!   （transfer_store），终态归档 + 200 封顶 + 重启 interrupted 标注 + retryMeta 回放；
//! - 接收策略：插件 storage 真源，auto 分支在 on_message 自动应答，
//!   ask 弹窗留在前端；配置经保留原语推送宿主闸门（settings_store，A1）；
//! - 加密参数化：send 载荷元素级 `{path, encrypt}`（步骤 5，零 ABI）。
//!
//! 双写期（Phase 3）：旧命令面保持可用；`peer:devices` 订阅降级为日志对账源。

use bedcode_plugin_api::host::{HostBus, HostEvents, HostLog, HostMdns, HostPeer, HostPlatform};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, WasmPlugin};
use std::sync::Mutex;
use std::sync::OnceLock;

mod device_bridge;
mod peer;
mod roots_registry;
mod settings_store;
mod transfer_store;

pub(crate) use peer::PLUGIN_ID;

/// 对等网络 mDNS 服务类型（与 peer-net crate `SERVICE_TYPE` 同值；插件不直接
/// 依赖 peer-net crate，此处常量对齐 spec v2 §4.2）
const PEER_MDNS_SERVICE_TYPE: &str = "_bedcode-peer._tcp.local.";

/// 定向发现事件 topic（spec v2 §5.2：事件按属主投递，owner = 本插件 id）。
/// LazyLock 而非 concat!：PLUGIN_ID 是 const `&str` 而非字面量，concat! 只收
/// 字面量，故运行时拼一次（bus_subscribe / on_message 每消息复用它）
static MDNS_FOUND_TOPIC: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| format!("mdns:found.{PLUGIN_ID}"));
static MDNS_LOST_TOPIC: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| format!("mdns:lost.{PLUGIN_ID}"));

/// 自建 browse 句柄（host-mdns，spec v2 / ticket 05：本插件自建浏览、事件
/// 定向投递 `mdns:found.<PLUGIN_ID>`；None = 未激活/翻修失败降级态）
static MDNS_BROWSER: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn mdns_browser() -> &'static Mutex<Option<String>> {
    MDNS_BROWSER.get_or_init(|| Mutex::new(None))
}

/// 自建 mDNS browse（幂等：已有句柄不动；失败降级返回 None，刷新时补建）
fn ensure_mdns_browse(h: &WasmHost) -> Option<String> {
    let mut slot = mdns_browser().lock().expect("mdns browser slot lock");
    if slot.is_some() {
        return slot.clone();
    }
    match h.mdns_browse(PEER_MDNS_SERVICE_TYPE) {
        Ok(id) => {
            h.log_info(&format!("mdns self-browse started: {id}"));
            *slot = Some(id.clone());
            Some(id)
        }
        Err(e) => {
            // 引擎未就绪/停用降级：空列表 + 提示，不阻断插件激活（spec v2 §9）
            h.log_info(&format!("mdns self-browse deferred (non-fatal): {e}"));
            None
        }
    }
}

/// 当前选中的对端节点 ID（单对端 UX：桌面端 ⇄ 移动端）
static ACTIVE_NODE: OnceLock<Mutex<String>> = OnceLock::new();

fn active_node() -> &'static Mutex<String> {
    ACTIVE_NODE.get_or_init(|| Mutex::new(String::new()))
}

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
        let h = host();
        h.log_info("File Transfer plugin activating (self-hosted, desktop)");
        // 发现事件（mdns:*）改经 host-mdns 自建 browse 收定向 topic（spec v2 /
        // ticket 05，D2 一期迁移）：不再订阅全局 `mdns:found` / `mdns:lost`
        // （全局桥接已退役 D1）——本插件自建浏览、事件按属主投递到
        // `mdns:found.<PLUGIN_ID>` / `mdns:lost.<PLUGIN_ID>`
        for topic in [
            MDNS_FOUND_TOPIC.as_str(),
            MDNS_LOST_TOPIC.as_str(),
            "peer:devices",
            "peer:transfer",
            "peer:receive",
            "peer:consent",
            "peer:connection",
        ] {
            let _ = h.bus_subscribe(topic);
        }
        // 自建 browse 对等网络服务类型（共享守护，事件定向投递）；引擎未就绪
        // 时降级（空列表 + 提示），刷新命令补建
        ensure_mdns_browse(&h);
        // 引擎电源：节点/mDNS 生命周期由宿主 activate/deactivate 外壳直接驱动
        // （plugin/host.rs 接线 ensure_node_started），插件侧无需声明
        // 激活即推送注册表镜像（重启后引擎广播面由本插件重建）：
        // 引擎未启动时静默——首次增删共享目录时会再推
        if let Err(e) = roots_registry::ensure_table(&h) {
            h.log_info(&format!("shared_roots table init deferred: {e}"));
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let h = host();
        // 关闭插件 = 服务下线：先自建 browse 回收句柄（宿主 purge 兜底不泄漏）
        if let Some(id) = mdns_browser().lock().expect("mdns browser slot lock").take() {
            let _ = h.mdns_stop_browse(&id);
        }
        // 再断开全部活跃对等连接（对端即时感知断开，否则 TLS 连接残留、对端
        // 仍显示在线——「关闭插件 对方无感知」）
        for handle in device_bridge::drain_sessions() {
            if let Err(e) = h.peer_close(&handle) {
                h.log_info(&format!("deactivate: peer_close {handle} failed (non-fatal): {e}"));
            }
        }
        // endpoint memo 随会话一并清空（与移动端同构）：跨启停残留旧地址会在
        // 对端换 IP/端口后让数据面重拨命中过期 memo
        device_bridge::clear_peer_state();
        for topic in [
            MDNS_FOUND_TOPIC.as_str(),
            MDNS_LOST_TOPIC.as_str(),
            "peer:devices",
            "peer:transfer",
            "peer:receive",
            "peer:consent",
            "peer:connection",
        ] {
            let _ = h.bus_unsubscribe(topic);
        }
        // 引擎电源：本插件下线 → 节点/mDNS 服务下线由宿主 deactivate 外壳直接
        // 驱动（plugin/host.rs 接线 stop_node_for_plugin），会话已在上方关闭，
        // 对端经断流事件即时感知
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let h = host();
        match name {
            // ==================== 设备（自建缓存的宿主侧出口） ====================
            // 「探索发现」直达后端：确保自建 browse 存活（激活时引擎未就绪则在此
            // 补建）后经总线请求宿主即时重查 + 连接态重发（宿主 peer_net 静态
            // 订阅 peer:discovery-refresh 消费；mdns:found 缓存重发已退役 D1，
            // 设备列表由自建 browse 的定向事件流自行收敛）
            "file-transfer.refresh-devices" => {
                ensure_mdns_browse(&h);
                h.bus_publish(
                    "peer:discovery-refresh",
                    &serde_json::json!({ "requestedBy": PLUGIN_ID }),
                )?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.get-device-snapshot" => Ok(serde_json::json!({
                "devices": device_bridge::load_snapshot(&h)?
            })),
            "file-transfer.save-device-snapshot" => {
                let entries: Vec<device_bridge::DeviceSnapshotEntry> =
                    serde_json::from_value(args.get("devices").cloned().unwrap_or_default())
                        .map_err(|e| anyhow::anyhow!("invalid device snapshot: {e}"))?;
                device_bridge::save_snapshot(&h, &entries)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 连接 ====================
            "file-transfer.dial-peer" => peer::dial_peer(&h, &args),
            "file-transfer.disconnect-peer" => peer::disconnect_peer(&h, &args),
            "file-transfer.set-active-peer" => {
                let id = args.get("peerId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                *active_node().lock().expect("active node lock") = id;
                Ok(serde_json::json!({ "ok": true }))
            }
            // 入站连接的对端寻址登记：被连侧没有拨号 memo，数据面命令经它重拨
            // （endpoint 来源 = 前端自建设备缓存，connection-changed 时回填）
            "file-transfer.remember-peer-endpoint" => {
                let endpoint: device_bridge::DialEndpoint =
                    serde_json::from_value(args.get("endpoint").cloned().unwrap_or_default())
                        .map_err(|e| anyhow::anyhow!("invalid endpoint: {e}"))?;
                // 空 node_id 拒绝：写入污染 memo（后续任意 node_id 查询均不命中但
                // 长期驻留静态表）且掩盖前端缺字段 bug
                if endpoint.node_id.is_empty() {
                    return Err(anyhow::anyhow!("remember-peer-endpoint: empty node_id rejected"));
                }
                device_bridge::remember_endpoint(&endpoint);
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 发送 ====================
            "file-transfer.pick-files" => Ok(serde_json::to_value(h.platform_pick_files()?)?),
            "file-transfer.enqueue" => peer::enqueue(&h, &args, active_node()),
            "file-transfer.list-tasks" => peer::list_tasks(&h),
            "file-transfer.cancel" => peer::cancel_task(&h, &args),
            "file-transfer.retry" => peer::retry_task(&h, &args),
            "file-transfer.pause" => peer::pause_task(&h, &args),
            "file-transfer.resume" => peer::resume_task(&h, &args),
            "file-transfer.resume-all" => peer::resume_all_tasks(&h),
            "file-transfer.remove-task" => Err(anyhow::anyhow!(
                "unsupported: task removal is not part of the transfer lifecycle"
            )),

            // ==================== 接收端 ====================
            "file-transfer.list-batches" => peer::list_batches(&h),
            "file-transfer.list-receiving" => peer::list_receiving(&h),
            "file-transfer.list-history" => peer::list_history(&h),
            "file-transfer.approve-batch" => {
                let batch_id = require_str(&args, "batchId")?;
                h.peer_respond_transfer(&batch_id, true)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.reject-batch" => {
                let batch_id = require_str(&args, "batchId")?;
                h.peer_respond_transfer(&batch_id, false)?;
                Ok(serde_json::json!({ "ok": true }))
            }
            "file-transfer.cancel-receiving" => peer::cancel_receiving(&h, &args),
            "file-transfer.clear-history" => peer::clear_history(&h),

            // ==================== 信任层（原语直通） ====================
            "file-transfer.respond-consent" => {
                let request_id = require_str(&args, "requestId")?;
                let accepted = args.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false);
                let hit = h.peer_respond_consent(&request_id, accepted)?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            "file-transfer.list-trusted" => Ok(h.peer_list_trusted()?),
            "file-transfer.revoke-trusted" => {
                let node_id = require_str(&args, "nodeId")?;
                let removed = h.peer_revoke_trusted(&node_id)?;
                Ok(serde_json::json!({ "removed": removed }))
            }

            // ==================== 远端浏览 / 拉取 ====================
            "file-transfer.list-remote" => peer::list_remote(&h, &args, active_node()),
            "file-transfer.pull-files" => peer::pull_files(&h, &args, active_node()),

            // ==================== 设置（真源 = 插件 storage + 注册表） ====================
            "file-transfer.get-settings" => peer::get_settings(&h),
            "file-transfer.set-settings" => peer::set_settings(&h, &args),
            "file-transfer.pick-download-dir" => peer::pick_download_dir(&h),
            "file-transfer.mount-local" => peer::mount_local(&h, &args),
            "file-transfer.update-roots" => peer::update_roots(&h, &args),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        let h = host();

        // 定向发现事件透传（wire 形状不过翻译）；设备缓存状态机在前端。
        // 注：payload 增量追加 serviceType / browserId（spec v2 §5.2），
        // 前端按「忽略未知字段」增量原则兼容，这里原样透传
        if msg.topic == *MDNS_FOUND_TOPIC || msg.topic == *MDNS_LOST_TOPIC {
            let event = if msg.topic == *MDNS_FOUND_TOPIC { "mdns-found" } else { "mdns-lost" };
            h.emit_event(&format!("plugin:file-transfer:{event}"), &msg.payload);
            return Ok(());
        }

        // 首连确认 / 连接态：原样转发；断开同时摘除会话句柄映射
        if msg.topic == "peer:consent" {
            h.emit_event("plugin:file-transfer:consent-requested", &msg.payload);
            return Ok(());
        }
        if msg.topic == "peer:connection" {
            if msg.payload.get("connected").and_then(|v| v.as_bool()) == Some(false) {
                if let Some(node_id) = msg.payload.get("nodeId").and_then(|v| v.as_str()) {
                    device_bridge::forget_session(node_id);
                }
            }
            h.emit_event("plugin:file-transfer:connection-changed", &msg.payload);
            return Ok(());
        }

        // 双写期对账源：宿主发现快照 vs 前端自建缓存规模差异仅记日志
        if msg.topic == "peer:devices" {
            if let Some(n) = msg.payload.as_array().map(|a| a.len()) {
                h.log_info(&format!("reconcile: host discovery cache size = {n}"));
            }
            return Ok(());
        }

        if msg.topic == "peer:transfer" {
            if let Some(arr) = msg.payload.as_array() {
                peer::merge_and_emit(&h, arr, "send");
            }
            return Ok(());
        }

        if msg.topic == "peer:receive" {
            if let Some(arr) = msg.payload.as_array() {
                // auto 分支先应答（accept/reject 策略下无弹窗；ask 留给前端倒计时）
                peer::auto_answer_pending(&h, arr);
                peer::merge_and_emit(&h, arr, "receive");
            }
            return Ok(());
        }

        Ok(())
    }

    fn on_shutdown() -> anyhow::Result<()> {
        host().log_info("File Transfer plugin shut down (self-hosted, desktop)");
        Ok(())
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}

bedcode_plugin_api::wasm_entry!(FileTransferPlugin);
