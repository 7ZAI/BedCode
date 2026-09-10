//! host-mdns 逻辑层 —— mDNS 浏览纯能力（ADR 0022 v2，issue 13 Phase 2）
//!
//! browse-only：每个 browser 句柄对应一条独立的 mdns-sd 浏览订阅，发现/离开
//! 事件经消息总线 `mdns:found` / `mdns:lost` **原样透传**（instance-name /
//! addresses / port / txt-records），宿主不做任何加工——设备列表等派生视图
//! 由消费插件自建缓存。
//!
//! 自我广播（register/advertise）不进本接口：留在 peer-net 启动流程自动完成，
//! 无插件激活时主机照样可被发现。虚拟/回环网卡的禁用逻辑复用引擎的
//! [`bedcode_peer_net::disable_virtual_interfaces`]（桌面多网卡解析延迟问题
//! 同样作用于独立浏览）。

use crate::plugin::permission::PERMISSION_MDNS;
use crate::plugin::wasm_runtime::WasmHostContext;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// 单条浏览订阅：daemon + 服务类型（stop_browse 按类型退订）+ 事件任务句柄
struct BrowserEntry {
    daemon: ServiceDaemon,
    service_type: String,
    task: tauri::async_runtime::JoinHandle<()>,
    /// 所属插件（停用时按属主回收全部句柄）
    owner: String,
}

static BROWSERS: LazyLock<std::sync::Mutex<HashMap<String, BrowserEntry>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn denied() -> String {
    "permission denied: mdns".to_string()
}

/// 浏览某服务类型：铸造 browser-id（`mdnsbr-<uuid>`）并启动事件透传循环
pub(crate) fn mdns_browse(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    service_type: &str,
) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_browse") {
        return Err(denied());
    }
    let service_type = service_type.trim().to_string();
    if service_type.is_empty() {
        return Err("mdns browse: service type must not be empty".to_string());
    }
    let daemon = ServiceDaemon::new()
        .map_err(|e| format!("mdns browse: daemon start failed: {e}"))?;
    bedcode_peer_net::disable_virtual_interfaces(&daemon);
    let receiver = daemon
        .browse(&service_type)
        .map_err(|e| format!("mdns browse: subscribe failed: {e}"))?;

    let browser_id = format!("mdnsbr-{}", uuid::Uuid::new_v4());
    let task_browser_id = browser_id.clone();
    // 自播回显过滤用：捕获宿主 AppHandle，事件循环内按需读取本机节点 ID
    // （节点可能晚于 browse 启动，须在事件时刻实时比对而非 browse 时刻）
    let app_handle = host_ctx.app_handle.clone();
    let task = tauri::async_runtime::spawn(async move {
        let browser_id = task_browser_id;
        // found/lost 原样透传；SearchStarted/Resolved 之外的编排事件忽略
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
                    publish_mdns(
                        "mdns:found",
                        serde_json::json!({
                            "instanceName": info.get_fullname(),
                            "addresses": addresses,
                            "port": info.get_port(),
                            "txtRecords": txt_records,
                        }),
                    );
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    publish_mdns(
                        "mdns:lost",
                        serde_json::json!({ "instanceName": fullname }),
                    );
                }
                _ => {}
            }
        }
        tracing::debug!(browser_id = %browser_id, "mdns browse loop exited");
    });

    BROWSERS
        .lock()
        .expect("mdns browser table lock poisoned")
        .insert(
            browser_id.clone(),
            BrowserEntry { daemon, service_type: service_type.clone(), task, owner: plugin_id.to_string() },
        );
    tracing::info!(browser_id = %browser_id, service_type = %service_type, plugin = %plugin_id, "mdns browse started");
    Ok(browser_id)
}

/// 停止浏览并回收句柄：退订 → 后台关停守护线程；事件循环随 channel 断开退出
pub(crate) fn mdns_stop_browse(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    browser_id: &str,
) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_MDNS, "host_mdns_stop_browse") {
        return Err(denied());
    }
    stop_browser(browser_id)
}

/// 回收指定插件的全部浏览句柄（插件停用/卸载时由 PluginHost 调用）
pub(crate) fn purge_browsers_for_plugin(plugin_id: &str) -> usize {
    let ids: Vec<String> = BROWSERS
        .lock()
        .expect("mdns browser table lock poisoned")
        .iter()
        .filter(|(_, e)| e.owner == plugin_id)
        .map(|(id, _)| id.clone())
        .collect();
    let mut purged = 0;
    for id in ids {
        if stop_browser(&id).unwrap_or(false) {
            purged += 1;
        }
    }
    purged
}

/// 取出并停止单条浏览订阅（幂等：未知句柄返回 false）
fn stop_browser(browser_id: &str) -> Result<bool, String> {
    let Some(entry) = BROWSERS
        .lock()
        .expect("mdns browser table lock poisoned")
        .remove(browser_id)
    else {
        return Ok(false);
    };
    if let Err(e) = entry.daemon.stop_browse(&entry.service_type) {
        tracing::warn!(browser_id = %browser_id, "mdns stop_browse failed: {e}");
    }
    // shutdown 消费 self 并等待守护线程确认（秒级）：移到后台避免阻塞 host 调用；
    // 事件任务随 receiver 断开自行收尾，无需 abort
    tauri::async_runtime::spawn(async move {
        let _ = entry.task;
        let _ = entry.daemon.shutdown();
    });
    tracing::info!(browser_id = %browser_id, "mdns browse stopped");
    Ok(true)
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
/// 与现状一致（引擎 handle_browse_event 的同口径过滤在独立浏览缺失，此处补齐）。
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
    /// stop_browser 对未知句柄幂等返回 false（表为空时不 panic）
    #[test]
    fn stop_unknown_browser_is_idempotent_false() {
        assert!(!super::stop_browser("mdnsbr-nonexistent").unwrap_or(true));
    }
}
