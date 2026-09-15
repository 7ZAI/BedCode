//! host-mdns v2 逻辑层 —— mDNS 基础能力服务（spec v2：
//! `.scratch/2026-09-10-mdns-service-plugin/spec-basic-capability-service.md`）
//!
//! 与桌面端同构（ticket 06）：全局唯一 `ServiceDaemon`（LazyLock 单例）+ 双句柄
//! 表（每条带属主）+ browse 事件定向投递 + advertise 原语 + 双表 purge；peer-net
//! 引擎与全部插件浏览/广播共享同一守护。移动端差异：
//!
//! - Android 多播锁随单守护首次使用获取、常驻持有（幂等，不再随浏览句柄增删——
//!   落地 spec v2 §8 注释计划「主动释放暂不做，随守护常开退役一并落地」）：宿主
//!   函数经 async command（tokio worker）驱动，禁止 block_on（manager.rs /
//!   peer.rs 2026-08-26 真机实证同款反模式 panic），fire-and-forget spawn 申请，
//!   失败只 warn（缺锁仅退化收包）；
//! - 事件透传循环跑在独立 std 线程（flume recv 阻塞语义，不经 tokio 避免运行时
//!   依赖），退出由 stop_browse 断开 channel 触发；
//! - 权限仲裁走 `granted_permissions`（插件 manifest 声明），与桌面 check_permission
//!   同语义。

use super::super::WasmPluginState;
use crate::plugin::android_plugins::multicast_lock_acquire;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

/// 全局唯一 mDNS 守护（本进程共享同一实例；mdns-sd 设计即为单守护多服务共享，
/// browse / register 各自独立订阅，互不干扰）。
///
/// `OnceLock`：首次任一原语访问时初始化（browse / advertise / shared_daemon）；
/// stop/query 路径只在「守护已初始化」时触网（单测表操作不强制拉起真实守护）
static DAEMON: OnceLock<ServiceDaemon> = OnceLock::new();

fn init_daemon() -> ServiceDaemon {
    tracing::info!("mdns service daemon initializing (single shared instance)");
    let daemon = ServiceDaemon::new().expect("mdns service daemon init failed");
    // Android：多播锁随守护常驻获取（幂等，不随浏览句柄增删）。宿主函数可能
    // 运行于 tokio worker，此处 block_on 属运行时内阻塞反模式——fire-and-forget
    // spawn；非 Android 平台 stub 立即返回，同走 spawn 保持单一代码路径（可测）
    tauri::async_runtime::spawn(async {
        if let Err(e) = multicast_lock_acquire().await {
            tracing::warn!("mdns daemon: multicast lock acquire failed ({e}); receive may be degraded");
        }
    });
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
/// 与 peer-net `REANNOUNCE_INTERVAL`（45s）同节奏——宿主身份广播的续期由
/// 引擎自己的 re-announce 循环负责（ticket 04/06），本间隔只服务插件句柄
const REANNOUNCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(45);

/// 单条浏览订阅：共享守护 + 服务类型（stop_browse 按类型退订）+ 属主。
/// 事件透传线程独立 std 线程（detached），随 channel 断连自行收尾
struct BrowserEntry {
    service_type: String,
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

/// 非属主操作的统一拒绝文案（属主仲裁，spec v2 §5.1）
const NOT_OWNER: &str = "not owner of mdns handle";

fn denied() -> String {
    "permission denied: mdns".to_string()
}

/// 权限门（manifest granted_permissions 仲裁，与桌面 check_permission 同语义）
fn check_permission(state: &WasmPluginState) -> bool {
    state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_MDNS)
}

/// 取全局共享守护句柄（clone 廉价）：peer-net 引擎接线用（ticket 06）
pub(crate) fn shared_daemon() -> ServiceDaemon {
    daemon().clone()
}

// ==================== 浏览原语 ====================

/// 浏览某服务类型：铸造 browser-id（`mdnsbr-<uuid>`）并启动事件定向投递线程
pub(crate) fn mdns_browse(state: &WasmPluginState, service_type: &str) -> Result<String, String> {
    let plugin_id = state.plugin_id.as_str();
    if !check_permission(state) {
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
    let app_handle = state.host_ctx.app_handle.clone();
    std::thread::spawn(move || {
        let browser_id = task_browser_id;
        loop {
            match receiver.recv() {
                Ok(event) => match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let txt_records: std::collections::BTreeMap<String, String> = info
                            .get_properties()
                            .iter()
                            .map(|p| (p.key().to_string(), p.val_str().to_string()))
                            .collect();
                        // 自播回显过滤：TXT `id` == 本机 NodeId 即自身，跳过
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
                },
                Err(_) => break, // channel disconnected = 已退订
            }
        }
        tracing::debug!(browser_id = %browser_id, "mdns browse loop exited");
    });

    BROWSERS.lock().expect("mdns browser table lock poisoned").insert(
        browser_id.clone(),
        BrowserEntry {
            service_type: service_type.clone(),
            owner: plugin_id.to_string(),
        },
    );
    tracing::info!(browser_id = %browser_id, service_type = %service_type, plugin = %plugin_id, "mdns browse started");
    Ok(browser_id)
}

/// 停止浏览并回收句柄：权限门 + 属主校验；事件线程随 channel 断开退出。
/// 返回是否存在该句柄（幂等：未知句柄 false）
pub(crate) fn mdns_stop_browse(state: &WasmPluginState, browser_id: &str) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    stop_browser(&state.plugin_id, browser_id)
}

/// 取出并停止单条浏览订阅（属主校验：非属主拒绝；幂等：未知句柄 false）
///
/// 共享守护不 shutdown——`stop_browse` 断开事件 channel 即完成退订
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
pub(crate) fn mdns_advertise(state: &WasmPluginState, config_json: &str) -> Result<String, String> {
    if !check_permission(state) {
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
        _ => default_instance_name(&state.plugin_id),
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
    advertise_inner(&state.plugin_id, service_type, service_info)
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
/// announce，不 probe、不发 goodbye——对端不会误判离线，规避查询指数退避
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
pub(crate) fn mdns_stop_advertise(state: &WasmPluginState, advertise_id: &str) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    stop_advertise(&state.plugin_id, advertise_id)
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
                // 注销确认在后台等待（异步完成）；宿主 host 调用同步返回不阻塞
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
pub(crate) fn mdns_is_advertising(state: &WasmPluginState, advertise_id: &str) -> Result<bool, String> {
    if !check_permission(state) {
        return Err(denied());
    }
    let table = ADVERTISERS.lock().expect("mdns advertiser table lock poisoned");
    match table.get(advertise_id) {
        Some(entry) if entry.owner == state.plugin_id => Ok(true),
        Some(_) => Err(NOT_OWNER.to_string()),
        None => Ok(false),
    }
}

// ==================== 宿主身份广播登记（owner=host，D3）====================

/// 登记宿主（peer-net 引擎）节点身份广播：注册动作由引擎经 [`shared_daemon`]
/// 完成（TXT/ServiceInfo 由引擎构造，本模块零业务拼装）；本函数只做 ADVERTISERS
/// 句柄登记（owner=host）。节点身份广播的周期续期由引擎自己的 re-announce
/// 循环负责，本登记行不挂续期任务，避免双续期
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

/// 回收指定插件的全部浏览 + 广播句柄（插件停用/卸载时由插件管理器调用）。
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
    let topic = if found {
        format!("mdns:found.{owner}")
    } else {
        format!("mdns:lost.{owner}")
    };
    publish_mdns(&topic, payload);
}

/// 发布到插件消息总线（管理器未就绪静默跳过：与 peer 事件桥接同口径）
fn publish_mdns(topic: &str, payload: serde_json::Value) {
    if let Some(pm) = crate::state::try_get_plugin_manager() {
        pm.message_bus().publish(topic, "host", payload);
    }
}

/// 自播回显判定：browse 会收到本机自己的广播（TXT `id` == 本机节点 ID）。
///
/// 无 app 句柄（无头/测试）或节点未启动时返回 false——无法比对即不拦截
fn is_self_broadcast(
    app_handle: &Option<Arc<tauri::AppHandle>>,
    txt: &std::collections::BTreeMap<String, String>,
) -> bool {
    let Some(app) = app_handle.as_ref() else {
        return false;
    };
    let Some(own) = crate::peer_net::current_node_id(app) else {
        return false;
    };
    txt.get("id").map(|v| v.as_str()) == Some(own.as_str())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use crate::plugin::wasm_runtime::WasmHostContext;
    use std::collections::HashSet;
    use std::sync::Arc;

    fn mdns_enabled_state(plugin_id: &str, runtime_handle: &tokio::runtime::Handle) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let tmp = Box::leak(Box::new(tempfile::tempdir().expect("tempdir")));
        let storage = Arc::new(PluginStorage::new(&tmp.path().to_path_buf()));
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(WasmHostContext::new(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: runtime_handle.clone(),
            granted_permissions: HashSet::from([bedcode_plugin_api_mobile::permission::PERMISSION_MDNS.to_string()]),
            on_message_binary: None,
        }
    }

    fn fake_browser(owner: &str, id: &str, service_type: &str) {
        BROWSERS.lock().unwrap().insert(
            id.to_string(),
            BrowserEntry {
                service_type: service_type.to_string(),
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

    /// stop 未知句柄幂等 false（browse / advertise 双表）
    #[test]
    fn stop_unknown_handles_are_idempotent_false() {
        assert!(!super::stop_browser("any-plugin", "mdnsbr-nonexistent").unwrap_or(true));
        assert!(!super::stop_advertise("any-plugin", "mdnsad-nonexistent").unwrap_or(true));
    }

    /// 多播锁申请迁入守护 init 后（ticket 06）：浏览在 tokio worker 上下文内
    /// 调用（含首次守护初始化）不得 panic、不得嵌套 block_on（生产形态：async
    /// command 在 tokio worker 上驱动 wasm activate 调入宿主函数）
    #[test]
    fn mdns_browse_within_runtime_worker_does_not_panic() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = "com.bedcode.mdns-test.worker".to_string();
        let state = mdns_enabled_state(&plugin_id, &handle);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(async {
                let joined =
                    tokio::spawn(async move { mdns_browse(&state, "_bedcode-selftest-worker._tcp.local.") }).await;
                joined.expect("mdns_browse must not panic inside runtime worker")
            })
        }));
        purge_for_plugin(&plugin_id);
        assert!(
            result.is_ok(),
            "mdns_browse must not panic when called inside a tokio runtime worker"
        );
    }

    /// 纯 std 线程（无 runtime 上下文）：fire-and-forget spawn 回退全局 runtime，
    /// 同样不得 panic 或阻塞当前线程
    #[test]
    fn mdns_browse_outside_runtime_does_not_panic() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = "com.bedcode.mdns-test.plain".to_string();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let state = mdns_enabled_state(&plugin_id, &handle);
            let _ = mdns_browse(&state, "_bedcode-selftest-plain._tcp.local.");
        }));
        purge_for_plugin(&plugin_id);
        assert!(result.is_ok(), "mdns_browse must not panic outside any runtime context");
    }

    /// 属主仲裁：非属主 stop / is-advertising 拒绝；属主可操作
    #[test]
    fn cross_plugin_stop_and_query_rejected() {
        let owner = "com.bedcode.mdns-test.owner";
        let other = "intruder-plugin";
        let bid = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_browser(owner, &bid, "_t1._cp.local.");
        fake_advertiser(owner, &aid, "_t1._cp.local.", "x._t1._cp.local.");

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
        assert!(super::stop_browser(owner, &bid).expect("owner stop browse"));
        assert!(super::stop_advertise(owner, &aid).expect("owner stop advertise"));
        let _ = super::stop_browser(owner, &bid);
        let _ = super::stop_advertise(owner, &aid);
    }

    /// purge 双表：只回收本人，host（owner=host）与它插件登记不动
    #[test]
    fn purge_dual_table_and_preserves_others() {
        let victim = "com.bedcode.mdns-test.victim";
        let bystander = "com.bedcode.mdns-test.bystander";
        let bid1 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid2 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid3 = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let host_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let bystander_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_browser(victim, &bid1, "_t2._cp.local.");
        fake_browser(victim, &bid2, "_t2._cp.local.");
        fake_browser(bystander, &bid3, "_t2._cp.local.");
        fake_advertiser(victim, &aid, "_t2._cp.local.", "a._t2._cp.local.");
        fake_advertiser("host", &host_aid, "_t2._cp.local.", "h._t2._cp.local.");
        fake_advertiser(bystander, &bystander_aid, "_t2._cp.local.", "b._t2._cp.local.");

        assert_eq!(super::purge_for_plugin(victim), 3);
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
        let _ = super::stop_browser(bystander, &bid3);
        let _ = super::stop_advertise("host", &host_aid);
        let _ = super::stop_advertise(bystander, &bystander_aid);
    }

    /// advertise 状态翻转：登记 → is-advertising true → stop → false（幂等）
    #[test]
    fn advertise_state_flip_roundtrip() {
        let owner = "com.bedcode.mdns-test.flip";
        let aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        fake_advertiser(owner, &aid, "_t3._cp.local.", "f._t3._cp.local.");
        {
            let table = ADVERTISERS.lock().unwrap();
            assert_eq!(table.get(&aid).map(|e| e.owner.as_str()), Some(owner));
        }
        assert!(super::stop_advertise(owner, &aid).expect("stop converts to false"));
        assert!(!super::stop_advertise(owner, &aid).expect("repeat stop idempotent false"));
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

    /// 集成：双插件同服务类型 browse 隔离（ticket 08，需求①物理隔离）
    ///
    /// 物理隔离 = 句柄级（BROWSERS 按 owner 独立）+ topic 级（事件定向
    /// `mdns:found.<owner>`）+ 总线精确 topic 分发（bus.rs dispatch_publish
    /// 按 topic 精确匹配，既有测试覆盖）三层组合：A 的事件 topic 集合不含 B
    /// 的实例消息。此处断言句柄表与 topic 构造两层
    #[test]
    fn browse_owner_isolation_across_plugins() {
        let owner_a = "com.bedcode.mdns-test.iso-a";
        let owner_b = "com.bedcode.mdns-test.iso-b";
        let bid_a = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        let bid_b = format!("mdnsbr-{}", uuid::Uuid::new_v4());
        // A、B 各 browse 同一服务类型（spec §4.2：重复浏览同一类型允许）
        fake_browser(owner_a, &bid_a, "_iso._cp.local.");
        fake_browser(owner_b, &bid_b, "_iso._cp.local.");
        {
            let table = BROWSERS.lock().unwrap();
            assert_eq!(table.get(&bid_a).map(|e| e.owner.as_str()), Some(owner_a));
            assert_eq!(table.get(&bid_b).map(|e| e.owner.as_str()), Some(owner_b));
            assert_ne!(bid_a, bid_b, "distinct browser handles");
        }
        // A 的定向 topic 与 B 互斥（物理隔离的 topic 层）
        assert_ne!(format!("mdns:found.{owner_a}"), format!("mdns:found.{owner_b}"));
        assert_ne!(format!("mdns:lost.{owner_a}"), format!("mdns:lost.{owner_b}"));
        // purge A 只回收 A 的句柄，B 的浏览不受影响
        assert_eq!(super::purge_for_plugin(owner_a), 1);
        {
            let table = BROWSERS.lock().unwrap();
            assert!(!table.contains_key(&bid_a), "A purged");
            assert!(table.contains_key(&bid_b), "B survives plugin A purge");
        }
        let _ = super::stop_browser(owner_b, &bid_b);
    }

    /// 集成：host（owner=host）与插件 advertise 共存于单守护（ticket 08）
    ///
    /// 共享守护上宿主节点身份广播与插件广播并行登记，互不注销对方：
    /// - 插件 purge / stop 只碰自己，host 登记存活；
    /// - stop_host_service（节点停机）只移除 host 登记，插件广播存活
    #[test]
    fn host_and_plugin_advertise_coexist_on_shared_daemon() {
        let plugin = "com.bedcode.mdns-test.coexist";
        let host_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        let plugin_aid = format!("mdnsad-{}", uuid::Uuid::new_v4());
        // host 登记（register_host_service 同源：owner=host）
        let host_reg =
            super::register_host_service("_co._cp.local.", "h._co._cp.local.").expect("host registration books handle");
        fake_advertiser(plugin, &plugin_aid, "_co._cp.local.", "p._co._cp.local.");
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(table.contains_key(&host_reg), "host row present");
            assert!(table.contains_key(&plugin_aid), "plugin row present");
        }
        // 插件 purge 只回收本人，host 登记不动
        assert_eq!(super::purge_for_plugin(plugin), 1);
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(table.contains_key(&host_reg), "host registration survives plugin purge");
        }
        // 宿主停机（stop_host_service）只移除 host 登记；插件行保留（重插后断言）
        fake_advertiser(plugin, &plugin_aid, "_co._cp.local.", "p._co._cp.local.");
        assert!(super::stop_host_service(&host_reg).expect("host registration removable"));
        {
            let table = ADVERTISERS.lock().unwrap();
            assert!(!table.contains_key(&host_reg), "host row removed on node stop");
            assert!(table.contains_key(&plugin_aid), "plugin advertise survives host stop");
        }
        let _ = super::stop_advertise(plugin, &plugin_aid);
        let _ = super::stop_host_service(&host_aid);
    }

    /// 自播回显：无 app 句柄（无头/测试）时不拦截
    #[test]
    fn self_broadcast_filter_needs_app_handle() {
        let txt = std::collections::BTreeMap::from([("id".to_string(), "some-node".to_string())]);
        assert!(!super::is_self_broadcast(&None, &txt));
    }

    /// browse 停用循环保持表有界（回归既有语义）：真实 daemon 可能因沙箱/
    /// 无多播网络启动失败，失败路径同样不留表项
    #[test]
    fn browse_stop_cycles_keep_table_bounded() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = "com.bedcode.mdns-test.cycle".to_string();
        let state = mdns_enabled_state(&plugin_id, &handle);
        for i in 0..3 {
            let Ok(id) = mdns_browse(&state, "_bedcode-selftest-cycle._tcp.local.") else {
                continue;
            };
            assert!(
                mdns_stop_browse(&state, &id).expect("stop browse"),
                "cycle {i}: fresh handle must report stopped"
            );
            assert!(
                !mdns_stop_browse(&state, &id).expect("stop browse again"),
                "cycle {i}: repeat stop must be idempotent false"
            );
        }
        assert_eq!(
            purge_for_plugin(&plugin_id),
            0,
            "stop_browse must recycle handles; BROWSERS must not grow across cycles"
        );
    }
}
