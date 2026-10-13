//! File Transfer Plugin (WASM, 双端单产物) — 业务自持版（issue 13 Phase 3）
//!
//! ## 单产物架构（规格 `.scratch/2026-10-13-file-transfer-single-wasm/spec.md`）
//!
//! 本 crate 是双端**唯一**的 file-transfer wasm 产物源：按桌面 `world plugin`
//! 构建，桌面与移动宿主加载**同一份** `.wasm`。产品差异不消除、不靠两份产物
//! 表达，而是收敛为「同一端口 trait 的双实现 + 运行期 platform 分支」：
//! 激活时经 `host-platform.platform()` 探测端值（未知值 = 激活失败），命令面 /
//! 事件面 / 持久化 / 信任决策按平台选择桌面或移动行为（见 `platform.rs`、
//! `adapters.rs`、`adapters_mobile.rs`、`peer.rs` 的分支点）。
//!
//! ADR 0022 v2 后插件接管全部业务状态，宿主只提供无业务语义的引擎原语：
//!
//! - 设备缓存：前端 deviceState.ts 自持（found/lost/TTL/cap 位），本 crate 只做
//!   browse 生命周期、`mdns:*` 透传与快照持久化（见 device_bridge）；
//! - 共享根注册表：桌面 plugin-db 表 / 移动 storage 键（roots_registry，平台分支）
//!   + `set-shared-roots` 全量推送；
//! - 任务真源与历史（票 06 / 票 07 / 票 08）：双方向均以引擎原始事件归约——
//!   发送方向吃 `peer:transfer-event`，接收方向吃 `peer:receive-event`
//!   （offer-pending / pull-started 建行 + progress / terminal / paused /
//!   resumed 推进 + node-stopped 标注中断）；终态归档 + 200 封顶 + 重启
//!   interrupted 标注；重试回放判据（终态 + 有回放凭证）与发送闸门判据同在
//!   transfer_store 纯函数单点，回放前判据与闸门前置（不铸无主会话）；排队
//!   发送批派发失败落终态行、拉取意图先于引擎调用入队（见 peer.rs）；
//! - 发送并发闸门：插件自控（`PENDING_SENDS` 队列 + 设置 `concurrency`），
//!   宿主 `send-files` 收窄为「一次调用即发一会话」；
//! - 接收策略：插件 storage 真源，auto 分支在 on_message 自动应答，
//!   ask 弹窗留在前端；配置经保留原语推送宿主闸门（settings_store）；
//! - 加密参数化：send 载荷元素级 `{path, encrypt}`（步骤 5，零 ABI）。
//!
//! 桌面专属能力（旧快照双写期对账、节点电源、目录多选、reveal-in-dir、
//! 认证中心互调）只在桌面分支触达；移动分支误触即 fail-visible（桌面专属
//! import 在移动宿主是显性 unsupported 桩）。

mod adapters;
mod adapters_mobile;
mod auth_center;
mod device_bridge;
mod peer;
mod platform;
mod roots_registry;
mod settings_store;
mod transfer_store;

use bedcode_plugin_api::host::{
    mdns_event_topic, HostBus, HostEvents, HostLog, HostMdns, HostPeer, HostPlatform, MDNS_FOUND,
    MDNS_LOST,
};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{BusMessage, WasmPlugin};
use bedcode_file_transfer_core::ports::ConsentGate;
use platform::Platform;
use std::sync::Mutex;
use std::sync::OnceLock;

pub(crate) use peer::PLUGIN_ID;

/// 双端协商 ABI 版本（共享产物自报值）
///
/// 必须 ≤ 双端宿主 `AbiPolicy` 的下界（当前 = 移动端 ABI）：桌面 SDK 的
/// `ABI_VERSION` 高于该下界，故经 `wasm_entry_with_abi!` 显式传本常量。
/// 任一端 bump ABI 后需重估（契约锁断言 ≤ 双端 `ABI_VERSION`，见 T7）。
pub(crate) const SHARED_ABI_VERSION: u32 = 21;

/// 按平台取「信任决策面」端口（差异面⑦的唯一选择点）。
///
/// 桌面 = 经互调认证中心（`DesktopConsentGate`），移动 = 直答宿主原语
/// （`MobilePorts`）。**编排层不再出现 `if platform == …` 的信任分支**——
/// 差异全部落在两个实现内，与其余端口面同纪律。
fn consent_gate<'a>(platform: Platform, h: &'a WasmHost) -> Box<dyn ConsentGate + 'a> {
    match platform {
        Platform::Desktop => Box::new(adapters::DesktopConsentGate(h)),
        Platform::Mobile => Box::new(adapters_mobile::MobilePorts(h)),
    }
}

/// 对等网络 mDNS 服务类型（与 peer-net crate `SERVICE_TYPE` 同值；插件不直接
/// 依赖 peer-net crate，此处常量对齐 spec v2 §4.2）
const PEER_MDNS_SERVICE_TYPE: &str = "_bedcode-peer._tcp.local.";

/// 定向发现事件 topic（spec v2 §5.2 + 双端共享 lib spec M3：事件按属主投递，
/// owner = 本插件 id；topic 用 `<owner>::` 属主命名空间终态，与桌面/SDK
/// `owned_topic` 规则逐字一致）。LazyLock 而非 concat!：PLUGIN_ID 是 const
/// `&str` 而非字面量，concat! 只收字面量，故运行时拼一次
/// （bus_subscribe / on_message 每消息复用它）
static MDNS_FOUND_TOPIC: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| mdns_event_topic(MDNS_FOUND, PLUGIN_ID));
static MDNS_LOST_TOPIC: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| mdns_event_topic(MDNS_LOST, PLUGIN_ID));

/// 自建 browse 句柄（host-mdns，spec v2 / ticket 07：本插件自建浏览、事件
/// 定向投递 `<PLUGIN_ID>::mdns:found|lost`；None = 未激活/降级态）
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
            h.log_warn(&format!("mdns self-browse deferred (non-fatal): {e}"));
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
        // 双端宿主均从**文件** plugin.json 读 manifest（本导出仅测试消费）；
        // 内嵌的是桌面侧副本——移动侧 resources 的 plugin.json 才是加载真源
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let h = host();
        // 平台探测先行（fail-visible）：未知/空取值 = 拒绝激活——猜错平台会把
        // 桌面分支静默跑在移动宿主上，比启动失败严重得多
        let platform = platform::detect(&h)?;
        h.log_info(&format!(
            "File Transfer plugin activating (self-hosted, single-artifact; platform = {platform:?})"
        ));
        // 发现事件（mdns:*）改经 host-mdns 自建 browse 收定向 topic（spec v2 /
        // ticket 07 + M3：`<owner>::` 终态）：不再订阅全局 `mdns:found` /
        // `mdns:lost`（全局桥接已退役 D1）——本插件自建浏览、事件按属主投递到
        // `<PLUGIN_ID>::mdns:found|lost`
        let mut topics = vec![
            MDNS_FOUND_TOPIC.as_str(),
            MDNS_LOST_TOPIC.as_str(),
            "peer:receive-event",
            "peer:transfer-event",
            "peer:consent",
            "peer:connection",
        ];
        // 旧快照双写期对账 topic：仅桌面仍订阅（移动已整条退役——宿主早无发布者，
        // 订阅只是把死 topic 写进文档）
        if platform.uses_legacy_snapshot() {
            topics.extend(["peer:devices", "peer:transfer", "peer:receive"]);
        }
        for topic in topics {
            // 订阅失败不留痕 = 该 topic 的事件永不回流，且无任何线索可查；宿主不因
            // 单个订阅失败拒绝激活，故此处只记日志不翻成激活失败
            if let Err(e) = h.bus_subscribe(topic) {
                h.log_error(&format!("activate: bus_subscribe {topic} failed: {e}"));
            }
        }
        // 自建 browse 对等网络服务类型（共享守护，事件定向投递）；引擎未就绪
        // 时降级（空列表 + 提示），刷新命令补建
        ensure_mdns_browse(&h);
        // 引擎电源：桌面由本插件显式请求起停（审计票 12：peer-net 节点由插件
        // 自己经 host-peer 原语请求，内核不按硬编码产品 id 开关）；移动由宿主
        // 外壳驱动（plugin/manager.rs 接线），插件不持有电源
        if platform == Platform::Desktop {
            // 放在 activate 后段，等价于旧外壳的 `result.is_ok()` 门——前面任何
            // 一步失败都不会把节点带起来。起节点失败不翻成激活失败（与旧行为一致：
            // 旧外壳只 log_error，激活结果由 inner 决定），否则一次引擎故障会变成
            // 插件整体不可用
            match h.peer_start_node() {
                Ok(changed) => h.log_info(&format!(
                    "peer-net node requested by this activation (state changed: {changed})"
                )),
                Err(e) => h.log_error(&format!("peer-net node start failed (plugin stays active): {e}")),
            }
        }
        // 注册表层初始化：桌面建 plugin-db 表（幂等，尽早暴露持久化故障）；
        // 移动 storage KV 无表可建（函数内按平台短路）
        if let Err(e) = roots_registry::ensure_table(&h) {
            h.log_warn(&format!("shared_roots table init deferred: {e}"));
        }
        // 首屏兜底（传输编排下沉票 2 开放点 4）：激活晚于事件时经
        // active-transfers 原语查询宿主在册活跃批补占位行；节点未起等场景
        // 内部降级日志，不阻断激活
        peer::rebuild_from_active_transfers(&h);
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let h = host();
        let platform = platform::current();
        // 关闭插件 = 服务下线：先自建 browse 回收句柄（宿主 purge 兜底不泄漏）
        if let Some(id) = mdns_browser().lock().expect("mdns browser slot lock").take() {
            // 回收失败不阻断停用：宿主 purge 是兜底，不泄漏句柄——但要留痕，
            // 否则「插件关了却仍有 browse 残留」这类问题无从排查
            if let Err(e) = h.mdns_stop_browse(&id) {
                h.log_warn(&format!("deactivate: mdns_stop_browse {id} failed: {e}"));
            }
        }
        // 再断开全部活跃对等连接（对端即时感知断开，否则 TLS 连接残留、对端
        // 仍显示在线——「关闭插件 对方无感知」），再清空句柄表与订阅
        for handle in device_bridge::drain_sessions() {
            if let Err(e) = h.peer_close(&handle) {
                h.log_warn(&format!("deactivate: peer_close {handle} failed (non-fatal): {e}"));
            }
        }
        // endpoint memo 随会话一并清空（与移动端同构）：跨启停残留旧地址会在
        // 对端换 IP/端口后让数据面重拨命中过期 memo
        device_bridge::clear_peer_state();
        // 进程内意图随停用作废（票 08）：排队发送批 / 待挂载拉取凭证都只存在
        // 内存，无行可查、用户无从重试——留到下次激活就是凭空冒出的旧任务
        let (queued, intents) = peer::reset_volatile_intents();
        if queued > 0 || intents > 0 {
            h.log_info(&format!(
                "deactivate: dropped {queued} queued sends, {intents} pending pull intents"
            ));
        }
        let mut topics = vec![
            MDNS_FOUND_TOPIC.as_str(),
            MDNS_LOST_TOPIC.as_str(),
            "peer:receive-event",
            "peer:transfer-event",
            "peer:consent",
            "peer:connection",
        ];
        if platform.uses_legacy_snapshot() {
            topics.extend(["peer:devices", "peer:transfer", "peer:receive"]);
        }
        for topic in topics {
            // 退订失败同样留痕：残留订阅会让停用后的插件仍收到事件
            if let Err(e) = h.bus_unsubscribe(topic) {
                h.log_warn(&format!("deactivate: bus_unsubscribe {topic} failed: {e}"));
            }
        }
        // 引擎电源（审计票 12）：桌面插件下线 → 自己关停节点（会话已在上方关闭，
        // 对端经断流事件即时感知）。放在最后一步，与旧外壳的 `result.is_ok()` 门
        // 同形。宿主侧另有一条按属主的兜底清理，本步失败或被漏调都不会让节点泄漏
        // 在已停用的插件名下；非属主调用会报错，这里只记录不翻成停用失败。
        // 移动端节点由宿主外壳驱动停息，插件不调
        if platform == Platform::Desktop {
            match h.peer_stop_node() {
                Ok(_) => h.log_info("peer-net node stopped with plugin"),
                Err(e) => h.log_warn(&format!("peer-net node stop skipped/failed (non-fatal): {e}")),
            }
        }
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let h = host();
        let platform = platform::current();
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
                // 空 node_id 拒绝的判据单点在核内（SessionTable::remember_endpoint_checked），
                // 前端/桌面/移动共用一份，端内不再自带副本
                device_bridge::remember_endpoint(&endpoint)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // ==================== 发送 ====================
            "file-transfer.pick-files" => Ok(serde_json::to_value(h.platform_pick_files()?)?),
            // 下载完成「打开本地目录」（桌面形态命令）：走 ABI v22 平台原语
            // `host-platform-desktop.reveal-in-dir`（与 pick-files 同域、同口径，
            // 不叠加权限门）。原先经宿主前端 `context.system.revealInDir` +
            // `system:open` 权限 + Tauri 命令 `plugin_reveal_in_dir` 的桥已随
            // 原语化退役（票 04）。移动端无此命令（plugin.json 不声明、前端不调用）；
            // 显式分支比「调用打到移动宿主的 unsupported 桩」更早给出可读错误
            "file-transfer.reveal-in-dir" => {
                if platform == Platform::Mobile {
                    return Err(anyhow::anyhow!(
                        "unsupported on mobile: reveal-in-dir is a desktop platform primitive"
                    ));
                }
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if path.is_empty() {
                    return Err(anyhow::anyhow!("reveal-in-dir: empty path rejected"));
                }
                h.platform_reveal_in_dir(&path)?;
                Ok(serde_json::json!({ "revealed": true }))
            }
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

            // ==================== 信任层（差异面⑦ 走端口） ====================
            // 桌面：consent / 信任列表经互调会话中心（auth.decide-consent /
            // auth.list-trusted-devices；认证中心不可用时双轨降级直答宿主，
            // 见 auth_center 模块文档）。移动：直答宿主原语（无认证中心接入）。
            // 两条路径由 `consent_gate()` 按平台选实现——**此处不再分支**。
            // 撤销无互调 api，两端均维持宿主原语直通（与信任视图的 peer 段共享
            // 同一宿主 trust store），故不属本端口面。
            "file-transfer.respond-consent" => {
                let request_id = require_str(&args, "requestId")?;
                let accepted = args.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false);
                let hit = consent_gate(platform, &h)
                    .decide_consent(&request_id, accepted)
                    .map_err(anyhow::Error::msg)?;
                Ok(serde_json::json!({ "hit": hit }))
            }
            "file-transfer.list-trusted" => consent_gate(platform, &h)
                .list_trusted()
                .map_err(anyhow::Error::msg),
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
            // 接收落点选择（桌面形态）：移动端固定 MediaStore.Downloads，
            // 命令显性报错（与退役前移动端行为逐字一致）
            "file-transfer.pick-download-dir" => {
                if platform == Platform::Mobile {
                    return Err(anyhow::anyhow!(
                        "unsupported on mobile: downloads land in MediaStore.Downloads"
                    ));
                }
                peer::pick_download_dir(&h)
            }
            "file-transfer.mount-local" => peer::mount_local(&h, &args),
            "file-transfer.update-roots" => peer::update_roots(&h, &args),

            _ => Err(anyhow::anyhow!("unknown command: {}", name)),
        }
    }

    // ==================== 总线消息（双词汇汇） ====================
    //
    // 桌面宿主要求 `events.on-message(topic, sender, payload)`；移动宿主要求
    // `events.on-bus-message(topic, payload)`（WIT events 并集，v38）。两者路由
    // 到同一内部处理（file-transfer 逻辑不消费 sender）。

    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        dispatch_bus_message(&host(), &msg.topic, &msg.payload)
    }

    fn on_bus_message(msg: &BusMessage) -> anyhow::Result<()> {
        dispatch_bus_message(&host(), &msg.topic, &msg.payload)
    }

    fn on_shutdown() -> anyhow::Result<()> {
        host().log_info("File Transfer plugin shut down (self-hosted, single-artifact)");
        Ok(())
    }
}

/// 总线消息内部路由（双词汇共用）：发现透传 → consent（桌面认证中心预检）→
/// 连接态 → 引擎事件归约（receive 方向走 on_receive_event）→ 桌面旧快照对账
fn dispatch_bus_message(
    h: &WasmHost,
    topic: &str,
    payload: &serde_json::Value,
) -> anyhow::Result<()> {
    // 定向发现事件透传（wire 形状不过翻译）；设备缓存状态机在前端。
    // 注：payload 增量追加 serviceType / browserId（spec v2 §5.2），
    // 前端按「忽略未知字段」增量原则兼容，这里原样透传
    if topic == *MDNS_FOUND_TOPIC || topic == *MDNS_LOST_TOPIC {
        let event = if topic == *MDNS_FOUND_TOPIC { "mdns-found" } else { "mdns-lost" };
        h.emit_event(&format!("plugin:file-transfer:{event}"), payload);
        return Ok(());
    }

    // 首连确认：桌面决策权归会话中心（票 10，auth.decide-consent 阶段 1 信任
    // 预检）——已信任 → 免确认自动放行（不弹窗）；未知 peer / 认证中心不可用
    // → 照旧转发前端弹窗询问（双轨兜底与迁移前行为等价）。移动端无预检路径
    // （`MobilePorts::evaluate_consent` 恒 false = 交前端弹窗），同样走端口，
    // 此处不再按平台分支
    if topic == "peer:consent" {
        let platform = platform::current();
        if consent_gate(platform, h)
            .evaluate_consent(payload)
            .map_err(anyhow::Error::msg)?
        {
            return Ok(());
        }
        h.emit_event("plugin:file-transfer:consent-requested", payload);
        return Ok(());
    }
    if topic == "peer:connection" {
        if payload.get("connected").and_then(|v| v.as_bool()) == Some(false) {
            if let Some(node_id) = payload.get("nodeId").and_then(|v| v.as_str()) {
                device_bridge::forget_session(node_id);
            }
        }
        h.emit_event("plugin:file-transfer:connection-changed", payload);
        return Ok(());
    }

    // 引擎原始事件归约（传输编排下沉票 2）：peer:transfer-event = send 方向
    // （本端发起批 + 供流记账批）、peer:receive-event = receive 方向（含
    // node-stopped 中断标注，双端共用入口）。事件归约是 store 主写。
    if topic == "peer:transfer-event" {
        peer::reduce_and_emit(h, "send", payload);
        return Ok(());
    }

    if topic == "peer:receive-event" {
        peer::on_receive_event(h, payload);
        return Ok(());
    }

    // ==================== 桌面旧快照双写期对账（移动不可达） ====================

    // 双写期对账源：宿主发现快照 vs 前端自建缓存规模差异仅记日志
    if topic == "peer:devices" {
        if let Some(n) = payload.as_array().map(|a| a.len()) {
            h.log_info(&format!("reconcile: host discovery cache size = {n}"));
        }
        return Ok(());
    }

    if topic == "peer:transfer" {
        if let Some(arr) = payload.as_array() {
            peer::merge_and_emit(h, arr, "send");
        }
        return Ok(());
    }

    if topic == "peer:receive" {
        if let Some(arr) = payload.as_array() {
            // auto 分支先应答（accept/reject 策略下无弹窗；ask 留给前端倒计时）
            peer::auto_answer_pending(h, arr);
            peer::merge_and_emit(h, arr, "receive");
        }
        return Ok(());
    }

    Ok(())
}

fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}

// 协商 ABI（双端下界）：桌面宿主持 38 ≥ 21、移动宿主 21 ≥ 21 均放行
bedcode_plugin_api::wasm_entry_with_abi!(FileTransferPlugin, SHARED_ABI_VERSION);
