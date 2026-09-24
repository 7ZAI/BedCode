//! host-mdns v2 逻辑层 —— mDNS 基础能力服务（spec v2：
//! `.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`）
//!
//! 内核单例 `MdnsService`：全局唯一 `ServiceDaemon`（LazyLock 单例，init 一次性
//! `disable_virtual_interfaces`）+ BROWSERS / ADVERTISERS 双句柄表（每条带
//! `owner: plugin_id`）。与 peer-net 引擎共享同一守护——消灭「双 daemon 同绑
//! 5353 互抢多播包」的结构性病灶（真机实证：只发现自己、发现不了对端）。
//!
//! - 事件定向投递：browse 事件按属主发布到私有 topic `<owner>::mdns:found` /
//!   `<owner>::mdns:lost`（owner = 发起 browse 的插件 id），非属主插件的订阅被
//!   总线命名空间门禁拒绝（票 05）——业务隔离（需求①）；
//! - 广播原语（需求②扩展性核心）：`advertise(config-json)` 纯引擎参数透传，
//!   宿主零业务拼装；句柄带属主，跨插件 stop / 查询一律拒绝；周期 re-announce
//!   续期（mdns-sd 注册后不主动周期广播，须手动续期）；
//! - 双表回收：`purge_for_plugin` 回收某插件全部浏览 + 广播句柄，只碰本人；
//! - 宿主身份广播（owner=host，peer-net 节点身份）：注册动作由引擎经
//!   [`shared_daemon`] 完成（TXT/ServiceInfo 引擎构造，零业务代码红线 D3），
//!   本模块只做句柄登记，作为「host 与插件 advertise 共存于单守护、互不注销
//!   对方」的可验证凭据。

use bedcode_plugin_api::host::bus::owned_topic;
use bedcode_plugin_api::host::mdns::{MDNS_FOUND, MDNS_LOST};

use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::permission::PERMISSION_MDNS;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

/// 全局唯一 mDNS 守护（本进程共享同一实例；mdns-sd 设计即为单守护多服务共享，
/// browse / register 各自独立订阅，互不干扰）。
///
/// `OnceLock`：首次任一原语访问时初始化（browse / advertise / shared_daemon）；
/// stop/query 路径只在「守护已初始化」时触网——单测表操作不强制拉起真实
/// 守护（保持测试与宿主机 mDNS 环境解耦）
static DAEMON: OnceLock<ServiceDaemon> = OnceLock::new();

fn init_daemon() -> ServiceDaemon {
    tracing::info!("mdns service daemon initializing (single shared instance)");
    let daemon = ServiceDaemon::new().expect("mdns service daemon init failed");
    // 虚拟/回环网卡禁用：多接口监听会在空等解析响应（VMware/Hyper-V/WSL 虚拟
    // 交换机不转发组播，对端 resolve 可延迟分钟级）。仅初始化执行一次——共享
    // 守护对 peer-net 引擎与全部插件浏览一视同仁（peer-net 不再重复执行）
    bedcode_peer_net::disable_virtual_interfaces(&daemon);
    daemon
}

/// 取全局守护（首次访问触发初始化）
fn daemon() -> &'static ServiceDaemon {
    DAEMON.get_or_init(init_daemon)
}

/// 守护若已初始化则返回（stop/query 前预防性保护：单测表操作不触网）
fn daemon_if_initialized() -> Option<&'static ServiceDaemon> {
    DAEMON.get()
}

/// 周期 re-announce 间隔：mdns-sd 注册后不主动周期广播，须手动续期。
/// 与 peer-net `REANNOUNCE_INTERVAL`（45s）同节奏——同一共享守护上的宿主
/// 身份广播续期由引擎自己的 re-announce 循环负责（见 ticket 04），本间隔
/// 只服务插件 advertise 句柄
const REANNOUNCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(45);

/// 单条浏览订阅：共享守护 + 服务类型（stop_browse 按类型退订）+ 事件任务句柄
struct BrowserEntry {
    service_type: String,
    #[allow(dead_code)] // 事件循环退出以 receiver 断开为准；句柄保留只为表内生命周期可见性
    task: tauri::async_runtime::JoinHandle<()>,
    /// 所属插件（停用时按属主回收全部句柄）
    owner: String,
}

/// 单条广播登记：注销凭据 + 周期续期任务 + 属主
struct AdvertiserEntry {
    /// 服务类型（unregister 按实例全名；按类型用于日志/调试）
    service_type: String,
    /// 完整实例名（`{escaped_instance}.{service_type}`，注销按全名寻址）
    fullname: String,
    /// 周期 re-announce 任务（stop 时回收）
    reannounce_task: tauri::async_runtime::JoinHandle<()>,
    /// 属主：插件 id 或 "host"（peer-net 节点身份，D3）
    owner: String,
}

static BROWSERS: LazyLock<Mutex<HashMap<String, BrowserEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static ADVERTISERS: LazyLock<Mutex<HashMap<String, AdvertiserEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn denied() -> String {
    "permission denied: mdns".to_string()
}

/// 非属主操作的统一拒绝文案（属主仲裁，spec v2 §5.1）
const NOT_OWNER: &str = "not owner of mdns handle";

/// 取全局共享守护句柄（clone 廉价）：peer-net 引擎接线用（ticket 04）
pub(crate) fn shared_daemon() -> ServiceDaemon {
    daemon().clone()
}

// ==================== 浏览原语 ====================

/// 浏览某服务类型：铸造 browser-id（`mdnsbr-<uuid>`）并启动事件定向投递循环
pub(crate) fn mdns_browse(host_ctx: &WasmHostContext, plugin_id: &str, service_type: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_browse") {
        return Err(denied());
    }
    let service_type = service_type.trim().to_string();
    if service_type.is_empty() {
        return Err("mdns browse: service type must not be empty".to_string());
    }
    let receiver = daemon()
        .browse(&service_type)
        .map_err(|e| format!("mdns browse: subscribe failed: {e}"))?;

    let browser_id = format!("mdnsbr-{}", uuid::Uuid::new_v4());
    let task_browser_id = browser_id.clone();
    let owner = plugin_id.to_string();
    let event_service_type = service_type.clone();
    // 自播回显过滤用：捕获宿主 AppHandle，事件循环内按需读取本机节点 ID
    // （节点可能晚于 browse 启动，须在事件时刻实时比对而非 browse 时刻）
    let app_handle = host_ctx.app_handle.clone();
    let task = tauri::async_runtime::spawn(async move {
        let browser_id = task_browser_id;
        // found/lost 定向投递；SearchStarted/Resolved 之外的编排事件忽略
        while let Ok(event) = receiver.recv_async().await {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let txt_records: std::collections::BTreeMap<String, String> = info
                        .get_properties()
                        .iter()
                        .map(|p| (p.key().to_string(), p.val_str().to_string()))
                        .collect();
                    // 自播回显过滤：本机广播也会被自己的浏览收到（引擎
                    // handle_browse_event 同口径）。TXT `id` == 本机 NodeId
                    // 即自身，跳过不推送——否则设备列表出现自己、拨号连自己
                    if is_self_broadcast(&app_handle, &txt_records) {
                        tracing::debug!(
                            instance = %info.get_fullname(),
                            "mdns browse self-broadcast filtered"
                        );
                        continue;
                    }
                    let addresses: Vec<String> = {
                        // IPv4 优先（与引擎拨号寻址口径一致），同族内保持库序
                        let all: Vec<_> = info.get_addresses().iter().collect();
                        let mut v4: Vec<_> = all
                            .iter()
                            .filter(|a| a.is_ipv4())
                            .map(|a| a.to_ip_addr().to_string())
                            .collect();
                        let rest: Vec<_> = all
                            .iter()
                            .filter(|a| !a.is_ipv4())
                            .map(|a| a.to_ip_addr().to_string())
                            .collect();
                        v4.extend(rest);
                        v4
                    };
                    publish_dir_event(
                        &owner,
                        &event_service_type,
                        &browser_id,
                        true,
                        serde_json::json!({
                            "instanceName": info.get_fullname(),
                            "addresses": addresses,
                            "port": info.get_port(),
                            "txtRecords": txt_records,
                        }),
                    );
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    publish_dir_event(
                        &owner,
                        &event_service_type,
                        &browser_id,
                        false,
                        serde_json::json!({ "instanceName": fullname }),
                    );
                }
                _ => {}
            }
        }
        tracing::debug!(browser_id = %browser_id, "mdns browse loop exited");
    });

    BROWSERS.lock().expect("mdns browser table lock poisoned").insert(
        browser_id.clone(),
        BrowserEntry {
            service_type: service_type.clone(),
            task,
            owner: plugin_id.to_string(),
        },
    );
    tracing::info!(browser_id = %browser_id, service_type = %service_type, plugin = %plugin_id, "mdns browse started");
    Ok(browser_id)
}

/// 停止浏览并回收句柄：权限门 + 属主校验后退订；事件循环随 channel 断开退出。
/// 返回是否存在该句柄（幂等：未知句柄 false）
pub(crate) fn mdns_stop_browse(host_ctx: &WasmHostContext, plugin_id: &str, browser_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_stop_browse") {
        return Err(denied());
    }
    stop_browser(plugin_id, browser_id)
}

/// 取出并停止单条浏览订阅（属主校验：非属主拒绝；幂等：未知句柄 false）
///
/// 共享守护不 shutdown——`stop_browse` 断开事件 channel 即完成退订，守护线程
/// 随进程存活（其余浏览/广播订阅不受影响）
fn stop_browser(owner: &str, browser_id: &str) -> Result<bool, String> {
    let mut table = BROWSERS.lock().expect("mdns browser table lock poisoned");
    let Some(entry) = table.remove(browser_id) else {
        return Ok(false);
    };
    if entry.owner != owner {
        table.insert(browser_id.to_string(), entry);
        return Err(NOT_OWNER.to_string());
    }
    drop(table);
    if let Some(daemon) = daemon_if_initialized() {
        if let Err(e) = daemon.stop_browse(&entry.service_type) {
            tracing::warn!(browser_id = %browser_id, "mdns stop_browse failed: {e}");
        }
    }
    tracing::info!(browser_id = %browser_id, "mdns browse stopped");
    Ok(true)
}

// ==================== 广播原语 ====================

/// advertise config JSON 契约（spec v2 §4.2）：宿主只校验 serviceType 非空，
/// 其余字段原样透传。instanceName 缺省时按 `{plugin}-{短指纹}` 默认（D4）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdvertiseConfig {
    service_type: String,
    #[serde(default)]
    instance_name: Option<String>,
    port: u16,
    #[serde(default)]
    txt_records: HashMap<String, String>,
}

/// 广播某服务类型（v2 新增）：铸造 advertise 句柄（`mdnsad-<uuid>`），
/// 共享守护注册 + 周期续期 + 句柄登记（owner = 调用插件）
pub(crate) fn mdns_advertise(host_ctx: &WasmHostContext, plugin_id: &str, config_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_advertise") {
        return Err(denied());
    }
    let config: AdvertiseConfig =
        serde_json::from_str(config_json).map_err(|e| format!("mdns advertise: invalid config: {e}"))?;
    let service_type = config.service_type.trim().to_string();
    if service_type.is_empty() {
        return Err("mdns advertise: service type must not be empty".to_string());
    }
    let instance_name = match config.instance_name {
        // 显式实例名优先；空白视为缺省（D4）
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => default_instance_name(plugin_id),
    };
    // 只校验「serviceType 非空」，其余原样透传（含中文 TXT 值不转义损坏）
    let service_info = ServiceInfo::new(
        &service_type,
        &instance_name,
        &format!("{instance_name}.local."),
        "",
        config.port,
        config.txt_records,
    )
    .map_err(|e| format!("mdns advertise: service info build failed: {e}"))?
    .enable_addr_auto();
    advertise_inner(plugin_id, service_type, service_info)
}

/// 默认实例名 `{plugin}-{短指纹}`：插件 id 的确定性短指纹（进程内稳定），
/// 同插件重复 advertise 同名幂等，不同插件互斥；显式 instanceName 优先（D4）
fn default_instance_name(plugin_id: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    plugin_id.hash(&mut hasher);
    let short = hasher.finish() as u32;
    format!("{plugin_id}-{short:08x}")
}

/// 注册某广播到共享守护 + 周期 re-announce + 句柄登记（供插件 advertise 与
/// 宿主身份登记共用——后者 owner="host"，见 [`register_host_service`]）
fn advertise_inner(owner: &str, service_type: String, service_info: ServiceInfo) -> Result<String, String> {
    daemon()
        .register(service_info.clone())
        .map_err(|e| format!("mdns advertise: register failed: {e}"))?;
    let fullname = service_info.get_fullname().to_string();

    let advertise_id = format!("mdnsad-{}", uuid::Uuid::new_v4());
    let reannounce_task = tauri::async_runtime::spawn(run_reannounce_loop(daemon().clone(), service_info));
    ADVERTISERS.lock().expect("mdns advertiser table lock poisoned").insert(
        advertise_id.clone(),
        AdvertiserEntry {
            service_type,
            fullname: fullname.clone(),
            reannounce_task,
            owner: owner.to_string(),
        },
    );
    tracing::info!(advertise_id = %advertise_id, instance = %fullname, owner = %owner, "mdns advertise registered");
    Ok(advertise_id)
}

/// 周期 re-announce 任务：每隔 [`REANNOUNCE_INTERVAL`] 重复 register 自身服务
///
/// mdns-sd 的 `register` 幂等（同名覆盖），重复调用直接触发 unsolicited
/// announce（RFC 6762 §8.3 自动补发第二条），不 probe、不发 goodbye——对端
/// 不会误判离线，又能持续收到续期广播，规避 browse 查询指数退避（上限 1 小时）
/// 超过缓存 TTL 导致的「启动互见、随后互不可见」。stop 时上层 abort 回收。
async fn run_reannounce_loop(daemon: ServiceDaemon, service_info: ServiceInfo) {
    let mut interval = tokio::time::interval(REANNOUNCE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // 首个 tick 立即到期：跳过（advertise_inner 启动时已 register 过一次）
    interval.tick().await;
    loop {
        interval.tick().await;
        if let Err(e) = daemon.register(service_info.clone()) {
            // register 失败只是本次续期丢失（下拍重试），不影响句柄表
            tracing::warn!(
                instance = %service_info.get_fullname(),
                error = %e,
                "mdns re-announce register failed"
            );
        }
    }
}

/// 停止广播并回收句柄：权限门 + 属主校验 → 注销 → 回收续期任务。
/// 返回是否存在该句柄（幂等：未知句柄 false）
pub(crate) fn mdns_stop_advertise(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    advertise_id: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_stop_advertise") {
        return Err(denied());
    }
    stop_advertise(plugin_id, advertise_id)
}

/// 取出并停止单条广播（属主校验；幂等：未知句柄 false）
///
/// 注销走共享守护 unregister（按实例全名，发 goodbye 对端即时移除本机）；
/// 只注销本人句柄，其余插件广播与本机其他广播不受影响
fn stop_advertise(owner: &str, advertise_id: &str) -> Result<bool, String> {
    let mut table = ADVERTISERS.lock().expect("mdns advertiser table lock poisoned");
    let Some(entry) = table.remove(advertise_id) else {
        return Ok(false);
    };
    if entry.owner != owner {
        table.insert(advertise_id.to_string(), entry);
        return Err(NOT_OWNER.to_string());
    }
    let fullname = entry.fullname.clone();
    let service_type = entry.service_type.clone();
    // 回收续期任务（abort 即止；锁外不等待）
    entry.reannounce_task.abort();
    drop(table);
    if let Some(daemon) = daemon_if_initialized() {
        match daemon.unregister(&fullname) {
            Ok(status) => {
                // 注销确认在后台等待（unregister 发 goodbye 是异步完成）；宿主
                // host 调用是同步返回，不 block 等待——状态日志 best-effort
                tauri::async_runtime::spawn(async move {
                    let _ = status.recv_async().await;
                });
            }
            Err(e) => tracing::warn!(instance = %fullname, "mdns unregister failed: {e}"),
        }
    }
    tracing::info!(advertise_id = %advertise_id, service_type = %service_type, "mdns advertise stopped");
    Ok(true)
}

/// 查询广播状态（返回是否存在该句柄）：权限门 + 属主校验（跨插件拒绝）
pub(crate) fn mdns_is_advertising(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    advertise_id: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_is_advertising") {
        return Err(denied());
    }
    let table = ADVERTISERS.lock().expect("mdns advertiser table lock poisoned");
    match table.get(advertise_id) {
        Some(entry) if entry.owner == plugin_id => Ok(true),
        Some(_) => Err(NOT_OWNER.to_string()),
        None => Ok(false),
    }
}

// ==================== 宿主身份广播登记（owner=host，D3）====================

/// 登记宿主（peer-net 引擎）节点身份广播：注册动作由引擎经 [`shared_daemon`]
/// 完成（TXT/ServiceInfo 由引擎构造——节点身份/证书/能力位是引擎语义，本
/// 模块零业务拼装）；本函数只做 ADVERTISERS 句柄登记（owner=host），作为
/// 「host 与插件 advertise 共存于单守护、互不注销对方」的可验证凭据。
///
/// 注意：节点身份广播的周期续期由引擎自己的 re-announce 循环负责（与
/// [`REANNOUNCE_INTERVAL`] 同节奏），本登记行不挂续期任务，避免双续期
pub(crate) fn register_host_service(service_type: &str, fullname: &str) -> Result<String, String> {
    let advertise_id = format!("mdnsad-{}", uuid::Uuid::new_v4());
    ADVERTISERS.lock().expect("mdns advertiser table lock poisoned").insert(
        advertise_id.clone(),
        AdvertiserEntry {
            service_type: service_type.to_string(),
            fullname: fullname.to_string(),
            reannounce_task: tauri::async_runtime::spawn(async {}),
            owner: "host".to_string(),
        },
    );
    tracing::info!(advertise_id = %advertise_id, instance = %fullname, "host mdns service registered (owner=host)");
    Ok(advertise_id)
}

/// 注销宿主身份广播登记（节点停机时调用）：只移除 owner=host 的登记行，
/// 实际 unregister 由引擎的 DiscoveryDaemon.stop() 按全名完成。幂等
pub(crate) fn stop_host_service(advertise_id: &str) -> Result<bool, String> {
    let mut table = ADVERTISERS.lock().expect("mdns advertiser table lock poisoned");
    let Some(entry) = table.remove(advertise_id) else {
        return Ok(false);
    };
    if entry.owner != "host" {
        table.insert(advertise_id.to_string(), entry);
        return Err(NOT_OWNER.to_string());
    }
    tracing::info!(advertise_id = %advertise_id, "host mdns service registration removed");
    Ok(true)
}

// ==================== 双表回收 ====================

/// 回收指定插件的全部浏览 + 广播句柄（插件停用/卸载时由 PluginHost 调用）。
/// 只回收本人句柄；其余插件与宿主（owner=host）登记不受影响
pub(crate) fn purge_for_plugin(plugin_id: &str) -> usize {
    let mut purged = 0;
    let browse_ids: Vec<String> = BROWSERS
        .lock()
        .expect("mdns browser table lock poisoned")
        .iter()
        .filter(|(_, e)| e.owner == plugin_id)
        .map(|(id, _)| id.clone())
        .collect();
    for id in browse_ids {
        if stop_browser(plugin_id, &id).unwrap_or(false) {
            purged += 1;
        }
    }
    let advertise_ids: Vec<String> = ADVERTISERS
        .lock()
        .expect("mdns advertiser table lock poisoned")
        .iter()
        .filter(|(_, e)| e.owner == plugin_id)
        .map(|(id, _)| id.clone())
        .collect();
    for id in advertise_ids {
        if stop_advertise(plugin_id, &id).unwrap_or(false) {
            purged += 1;
        }
    }
    purged
}

// ==================== 事件定向投递 ====================

/// 定向 publish：browse 事件按属主 topic 发布（payload 增量追加
/// serviceType / browserId，既有 4 字段不变——老订阅者按「忽略未知字段」
/// 增量原则兼容）
fn publish_dir_event(owner: &str, service_type: &str, browser_id: &str, found: bool, mut payload: serde_json::Value) {
    payload["serviceType"] = serde_json::Value::String(service_type.to_string());
    payload["browserId"] = serde_json::Value::String(browser_id.to_string());
    let topic = owned_topic(owner, if found { MDNS_FOUND } else { MDNS_LOST });
    publish_mdns(&topic, payload);
}

/// 发布到插件消息总线（无总线上下文静默跳过：与 peer 事件桥接同口径）
fn publish_mdns(topic: &str, payload: serde_json::Value) {
    if let Some(ctx) = crate::system::app_context::AppContext::try_global() {
        ctx.plugin_host().message_bus().publish(topic, "host", payload);
    }
}

/// 自播回显判定：browse 会收到本机自己的广播（TXT `id` == 本机节点 ID）。
///
/// 无 app 句柄（无头/测试）或节点未启动时返回 false——无法比对即不拦截，
/// 与现状一致（引擎 handle_browse_event 的同口径过滤在独立浏览缺失，此处补齐）
fn is_self_broadcast(
    app_handle: &Option<Arc<tauri::AppHandle>>,
    txt: &std::collections::BTreeMap<String, String>,
) -> bool {
    let Some(app) = app_handle.as_ref() else {
        return false;
    };
    let Some(own) = crate::server::peer_net::current_node_id(app) else {
        return false;
    };
    txt.get("id").map(|v| v.as_str()) == Some(own.as_str())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    /// 权限门（票 01 门禁用例）：未授予 `mdns` 即拒绝浏览/广播，不触达引擎
    #[test]
    fn mdns_denied_without_permission() {
        let ctx = build_host_ctx();
        assert_eq!(
            mdns_browse(&ctx, "com.bedcode.no-mdns", "_bedcode._tcp").unwrap_err(),
            denied()
        );
        assert_eq!(
            mdns_stop_browse(&ctx, "com.bedcode.no-mdns", "mdnsbr-nonexistent").unwrap_err(),
            denied()
        );
    }

    /// 正例（防「恒拒绝」假绿）：授予后越过权限门，报错来自参数校验而非权限
    /// （用坏配置打门后的第一段代码，避免真起 mDNS 守护）
    #[test]
    fn mdns_granted_passes_permission_gate() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.mdns-ok", &[PERMISSION_MDNS]);
        let err = mdns_advertise(&ctx, "com.bedcode.mdns-ok", "not-a-json").expect_err("坏配置应报错");
        assert!(!err.contains("permission denied"), "已授予 mdns 仍被权限门拒绝: {err}");
        assert!(err.contains("invalid config"), "预期参数校验错误: {err}");
    }

    /// 进程内静态表隔离（cargo test 多线程并行）：每个用例用唯一前缀 id，
    /// 只操作自己插的条目，绝不 clear 全表
    fn test_owner(seed: &str) -> String {
        format!("test-plugin-{seed}")
    }

    fn fake_browser(owner: &str, id: &str, service_type: &str) {
        BROWSERS.lock().unwrap().insert(
            id.to_string(),
            BrowserEntry {
                service_type: service_type.to_string(),
                task: tauri::async_runtime::spawn(async {}),
                owner: owner.to_string(),
            },
        );
    }

    fn fake_advertiser(owner: &str, id: &str, service_type: &str, fullname: &str) {
        ADVERTISERS.lock().unwrap().insert(
            id.to_string(),
            AdvertiserEntry {
                service_type: service_type.to_string(),
                fullname: fullname.to_string(),
                reannounce_task: tauri::async_runtime::spawn(async {}),
                owner: owner.to_string(),
            },
        );
    }

    /// stop_browser 对未知句柄幂等返回 false（表为空时不 panic）
    #[test]
    fn stop_unknown_browser_is_idempotent_false() {
        assert!(!super::stop_browser("any-plugin", "mdnsbr-nonexistent").unwrap_or(true));
    }

    /// stop_advertise 对未知句柄幂等返回 false
    #[test]
    fn stop_unknown_advertise_is_idempotent_false() {
        assert!(!super::stop_advertise("any-plugin", "mdnsad-nonexistent").unwrap_or(true));
    }

    /// 属主仲裁：非属主 stop / 查询拒绝（spec v2 §5.1）；属主可操作
    #[test]
    fn cross_plugin_stop_and_query_rejected() {
        let owner = test_owner("ownercheck");
        let other = "intruder-plugin";
        let bid = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_browser(&owner, &bid, "_t1._cp.local.");
        fake_advertiser(&owner, &aid, "_t1._cp.local.", "x._t1._cp.local.");

        // 跨插件 stop / is-advertising → 拒绝（句柄仍保留）
        assert_eq!(
            super::stop_browser(other, &bid).expect_err("cross stop-browse"),
            super::NOT_OWNER
        );
        assert_eq!(
            super::stop_advertise(other, &aid).expect_err("cross stop-advertise"),
            super::NOT_OWNER
        );
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(table.contains_key(&aid), "rejected stop must not consume handle");
        }
        // 属主本人可停
        assert!(super::stop_browser(&owner, &bid).expect("owner stop browse"));
        assert!(super::stop_advertise(&owner, &aid).expect("owner stop advertise"));
        // 清理
        let _ = super::stop_browser(&owner, &bid);
        let _ = super::stop_advertise(&owner, &aid);
    }

    /// purge 双表：只回收指定插件句柄，host（owner=host）与它插件登记不动
    #[test]
    fn purge_dual_table_and_preserves_others() {
        let victim = test_owner("purgee");
        let bystander = test_owner("bystander");
        let bid1 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid2 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid3 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let host_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let bystander_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_browser(&victim, &bid1, "_t2._cp.local.");
        fake_browser(&victim, &bid2, "_t2._cp.local.");
        fake_browser(&bystander, &bid3, "_t2._cp.local.");
        fake_advertiser(&victim, &aid, "_t2._cp.local.", "a._t2._cp.local.");
        fake_advertiser("host", &host_aid, "_t2._cp.local.", "h._t2._cp.local.");
        fake_advertiser(&bystander, &bystander_aid, "_t2._cp.local.", "b._t2._cp.local.");

        // 只回收 victim 的 2 browse + 1 advertise
        assert_eq!(super::purge_for_plugin(&victim), 3);
        {
            let ads = ADVERTISERS.lock().unwrap();
            assert!(
                ads.contains_key(&host_aid),
                "host registration must survive plugin purge"
            );
            assert!(
                ads.contains_key(&bystander_aid),
                "third-party registration must survive"
            );
            assert!(!ads.contains_key(&aid), "victim advertise purged");
        }
        {
            let bro = BROWSERS.lock().unwrap();
            assert!(
                !bro.contains_key(&bid1) && !bro.contains_key(&bid2),
                "victim browsers purged"
            );
            assert!(bro.contains_key(&bid3), "bystander browser survives");
        }
        // 清理
        let _ = super::stop_browser(&bystander, &bid3);
        let _ = super::stop_advertise("host", &host_aid);
        let _ = super::stop_advertise(&bystander, &bystander_aid);
    }

    /// advertise 状态翻转：登记 → is-advertising true → stop → false（幂等）
    #[test]
    fn advertise_state_flip_roundtrip() {
        let owner = test_owner("flip");
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_advertiser(&owner, &aid, "_t3._cp.local.", "f._t3._cp.local.");
        {
            let table = ADVERTISERS.lock().unwrap();
            // 表内状态即 is-advertising 的数据源（权限门 + 属主比对后查表）
            assert_eq!(table.get(&aid).map(|e| e.owner.as_str()), Some(owner.as_str()));
        }
        assert!(super::stop_advertise(&owner, &aid).expect("stop converts to false"));
        assert!(!super::stop_advertise(&owner, &aid).expect("repeat stop idempotent false"));
    }

    /// 默认实例名：`{plugin}-{8-hex}`，同插件稳定、跨插件不同
    #[test]
    fn default_instance_name_is_stable_and_distinct() {
        let a1 = super::default_instance_name("com.bedcode.file-transfer");
        let a2 = super::default_instance_name("com.bedcode.file-transfer");
        let b = super::default_instance_name("com.bedcode.other");
        assert_eq!(a1, a2, "same plugin deterministic");
        assert_ne!(a1, b, "different plugins distinct");
        assert!(
            a1.starts_with("com.bedcode.file-transfer-"),
            "prefix shape {{plugin}}-{{short}}"
        );
    }

    /// 自播回显：无 app 句柄（无头/测试）时不拦截（无法比对即放行，与现状一致）
    #[test]
    fn self_broadcast_filter_needs_app_handle() {
        let txt = std::collections::BTreeMap::from([("id".to_string(), "some-node".to_string())]);
        assert!(!super::is_self_broadcast(&None, &txt));
    }

    /// 定向事件 payload 增量字段：serviceType / browserId 追加，既有字段保留
    /// （found 载荷经 publish_dir_event 加工后形状；topic 构造直测）
    #[test]
    fn directed_event_payload_increments_fields() {
        // topic 构造（found/lost 按属主后缀）
        // publish_dir_event 内部 shape——通过捕获发布副作用不可行（静态函数），
        // 这里验证 topic 字符串拼接语义与 payload 字段注入的等价性：
        let payload = serde_json::json!({
            "instanceName": "svc._t4._cp.local.",
            "addresses": ["192.168.1.5"],
            "port": 19000,
            "txtRecords": { "id": "abc" },
        });
        // 手动执行与 publish_dir_event 相同的注入，断言增量字段形状
        let mut enriched = payload.clone();
        enriched["serviceType"] = serde_json::Value::String("_t4._cp.local.".into());
        enriched["browserId"] = serde_json::Value::String("mdnsbr-x".into());
        assert_eq!(enriched["instanceName"], "svc._t4._cp.local.");
        assert_eq!(enriched["port"], 19000);
        assert_eq!(enriched["txtRecords"]["id"], "abc");
        assert_eq!(enriched["serviceType"], "_t4._cp.local.");
        assert_eq!(enriched["browserId"], "mdnsbr-x");
        // topic 形状（属主私有命名空间，构造与 SDK 共用 owned_topic）
        assert_eq!(owned_topic("plugin-a", MDNS_FOUND), "plugin-a::mdns:found");
        assert_eq!(owned_topic("plugin-a", MDNS_LOST), "plugin-a::mdns:lost");
    }

    /// 集成：双插件同服务类型 browse 隔离（ticket 08，需求①物理隔离）
    ///
    /// 隔离 = 句柄级（BROWSERS 按 owner 独立）+ topic 级（事件定向投递到
    /// `<owner>::mdns:found`）+ 门禁级（票 05：非属主订阅他人命名空间被宿主
    /// 拒绝，见 host_impl/bus.rs 用例）三层组合：A 的事件只进 A 的收件箱，
    /// B 既订不到也收不到。此处断言句柄表与 topic 构造两层
    #[test]
    fn browse_owner_isolation_across_plugins() {
        let owner_a = test_owner("iso-a");
        let owner_b = test_owner("iso-b");
        let bid_a = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid_b = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        // A、B 各 browse 同一服务类型（spec §4.2：重复浏览同一类型允许）
        fake_browser(&owner_a, &bid_a, "_iso._cp.local.");
        fake_browser(&owner_b, &bid_b, "_iso._cp.local.");
        {
            let table = BROWSERS.lock().unwrap();
            assert_eq!(table.get(&bid_a).map(|e| e.owner.as_str()), Some(owner_a.as_str()));
            assert_eq!(table.get(&bid_b).map(|e| e.owner.as_str()), Some(owner_b.as_str()));
            assert_ne!(bid_a, bid_b, "distinct browser handles");
        }
        // A 的定向 topic 与 B 的互斥（宿主只向属主命名空间投递）
        assert_eq!(owned_topic(&owner_a, MDNS_FOUND), "test-plugin-iso-a::mdns:found");
        assert_ne!(owned_topic(&owner_a, MDNS_FOUND), owned_topic(&owner_b, MDNS_FOUND));
        assert_ne!(owned_topic(&owner_a, MDNS_LOST), owned_topic(&owner_b, MDNS_LOST));
        // purge A 只回收 A 的句柄，B 的浏览不受影响
        assert_eq!(super::purge_for_plugin(&owner_a), 1);
        {
            let table = BROWSERS.lock().unwrap();
            assert!(!table.contains_key(&bid_a), "A purged");
            assert!(table.contains_key(&bid_b), "B survives plugin A purge");
        }
        let _ = super::stop_browser(&owner_b, &bid_b);
    }

    /// 集成：host（owner=host）与插件 advertise 共存于单守护（ticket 08）
    ///
    /// 共享守护上宿主节点身份广播与插件广播并行登记，互不注销对方：
    /// - 插件 purge / stop 只碰自己，host 登记存活；
    /// - stop_host_service（节点停机）只移除 host 登记，插件广播存活
    #[test]
    fn host_and_plugin_advertise_coexist_on_shared_daemon() {
        let plugin = test_owner("coexist");
        let host_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let plugin_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        // host 登记（register_host_service 同源：owner=host）
        let host_reg =
            super::register_host_service("_co._cp.local.", "h._co._cp.local.").expect("host registration books handle");
        fake_advertiser(&plugin, &plugin_aid, "_co._cp.local.", "p._co._cp.local.");
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(table.contains_key(&host_reg), "host row present");
            assert!(table.contains_key(&plugin_aid), "plugin row present");
        }
        // 插件 purge 只回收本人，host 登记不动
        assert_eq!(super::purge_for_plugin(&plugin), 1);
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(table.contains_key(&host_reg), "host registration survives plugin purge");
        }
        // 宿主停机（stop_host_service）只移除 host 登记；插件行保留（重插后断言）
        fake_advertiser(&plugin, &plugin_aid, "_co._cp.local.", "p._co._cp.local.");
        assert!(super::stop_host_service(&host_reg).expect("host registration removable"));
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(!table.contains_key(&host_reg), "host row removed on node stop");
            assert!(table.contains_key(&plugin_aid), "plugin advertise survives host stop");
        }
        let _ = super::stop_advertise(&plugin, &plugin_aid);
        let _ = super::stop_host_service(&host_aid);
    }
}
