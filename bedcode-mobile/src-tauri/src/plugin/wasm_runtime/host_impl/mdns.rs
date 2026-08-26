//! host-mdns 逻辑层 —— mDNS 浏览纯能力（ADR 0022 v2，issue 13 Phase 2）
//!
//! 与桌面端同构：每条浏览订阅独立 daemon + 事件透传循环，found/lost 原样
//! 推送到插件总线 `mdns:found` / `mdns:lost`。移动端差异：Android 多播锁在
//! 首条浏览建立时 best-effort 申请（幂等）；主动释放暂不做——发现守护自节点
//! 启动即持锁且不释放，本接口的引用计数释放随 Phase 4 守护常开浏览退役一并落地。

use super::super::WasmPluginState;
use crate::plugin::android_plugins::multicast_lock_acquire;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::sync::LazyLock;

/// 单条浏览订阅：daemon + 服务类型（stop_browse 按类型退订）
struct BrowserEntry {
    daemon: ServiceDaemon,
    service_type: String,
    /// 所属插件（停用时按属主回收全部句柄）
    owner: String,
}

static BROWSERS: LazyLock<std::sync::Mutex<HashMap<String, BrowserEntry>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// 浏览某服务类型：铸造 browser-id（`mdnsbr-<uuid>`）并启动事件透传循环
pub(crate) fn mdns_browse(state: &WasmPluginState, service_type: &str) -> Result<String, String> {
    let plugin_id = state.plugin_id.as_str();
    if !state.granted_permissions.contains(bedcode_plugin_api_mobile::permission::PERMISSION_MDNS) {
        return Err("permission denied: mdns".to_string());
    }
    let service_type = service_type.trim().to_string();
    if service_type.is_empty() {
        return Err("mdns browse: service type must not be empty".to_string());
    }
    let daemon = ServiceDaemon::new()
        .map_err(|e| format!("mdns browse: daemon start failed: {e}"))?;
    let receiver = daemon
        .browse(&service_type)
        .map_err(|e| format!("mdns browse: subscribe failed: {e}"))?;

    // Android：首条浏览前确保多播锁（幂等；失败只 warn 不阻断——缺锁仅退化收包）
    #[cfg(target_os = "android")]
    if let Err(e) = tauri::async_runtime::block_on(multicast_lock_acquire()) {
        tracing::warn!("mdns browse: multicast lock acquire failed ({e}); receive may be degraded");
    }
    #[cfg(not(target_os = "android"))]
    let _ = multicast_lock_acquire;

    let browser_id = format!("mdnsbr-{}", uuid::Uuid::new_v4());
    let task_browser_id = browser_id.clone();
    std::thread::spawn(move || {
        let browser_id = task_browser_id;
        // 事件透传循环跑在独立线程（flume recv 阻塞语义；不经 tokio 避免运行时依赖），
        // 退出由 stop_browse 断开 channel 触发
        loop {
            match receiver.recv() {
                Ok(event) => match event {
                    ServiceEvent::ServiceResolved(info) => {
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
                        let txt_records: std::collections::BTreeMap<String, String> = info
                            .get_properties()
                            .iter()
                            .map(|p| (p.key().to_string(), p.val_str().to_string()))
                            .collect();
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
                },
                Err(_) => break, // channel disconnected = 已退订
            }
        }
        tracing::debug!(browser_id = %browser_id, "mdns browse loop exited");
    });

    BROWSERS
        .lock()
        .expect("mdns browser table lock poisoned")
        .insert(
            browser_id.clone(),
            BrowserEntry { daemon, service_type: service_type.clone(), owner: plugin_id.to_string() },
        );
    tracing::info!(browser_id = %browser_id, service_type = %service_type, plugin = %plugin_id, "mdns browse started");
    Ok(browser_id)
}

/// 停止浏览并回收句柄：退订 → 后台关停守护线程；事件线程随 channel 断开退出。
/// 返回是否存在该句柄（幂等：未知句柄 false）。
pub(crate) fn mdns_stop_browse(state: &WasmPluginState, browser_id: &str) -> Result<bool, String> {
    let _plugin_id = state.plugin_id.as_str();
    if !state.granted_permissions.contains(bedcode_plugin_api_mobile::permission::PERMISSION_MDNS) {
        return Err("permission denied: mdns".to_string());
    }
    stop_browser(browser_id)
}

/// 回收指定插件的全部浏览句柄（插件停用/卸载时由插件管理器调用）
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
    Ok(true)
}

/// 发布到插件消息总线（管理器未就绪静默跳过：与 peer 事件桥接同口径）
fn publish_mdns(topic: &str, payload: serde_json::Value) {
    if let Some(pm) = crate::state::try_get_plugin_manager() {
        pm.message_bus().publish(topic, "host", payload);
    }
}
