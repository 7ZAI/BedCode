//! host-mdns 逻辑层 —— mDNS 浏览纯能力（ADR 0022 v2，issue 13 Phase 2）
//!
//! 与桌面端同构：每条浏览订阅独立 daemon + 事件透传循环，found/lost 原样
//! 推送到插件总线 `mdns:found` / `mdns:lost`。移动端差异：Android 多播锁在
//! 首条浏览建立时 fire-and-forget 异步申请（幂等，宿主函数不阻塞等待——
//! 宿主函数可能运行于 tokio worker，同步 block_on 属运行时重入反模式）；
//! 主动释放暂不做——发现守护自节点启动即持锁且不释放，本接口的引用计数
//! 释放随 Phase 4 守护常开浏览退役一并落地。

use super::super::WasmPluginState;
use crate::plugin::android_plugins::multicast_lock_acquire;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

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

    // Android：首条浏览前确保多播锁。宿主函数经 async command（tokio worker）
    // 驱动 wasm activate 调入，此处禁止 block_on——项目约束「运行时内 block_on
    // 会 panic」（manager.rs，peer.rs 2026-08-26 真机实证同款反模式）。改为
    // tauri::async_runtime::spawn fire-and-forget：不阻塞当前线程、不嵌套
    // block_on、不依赖「当前是否已在 runtime」判断；每次 mdns_browse 调用
    // （= 一条浏览订阅）至多触发一次获取，Kotlin 侧 acquire 幂等。多播锁获取
    // 为 best-effort：缺锁仅退化收包，失败只 warn 不阻断 browse。非 Android
    // 平台 stub 立即返回，同走 spawn 保持单一代码路径（可测）。
    tauri::async_runtime::spawn(async {
        if let Err(e) = multicast_lock_acquire().await {
            tracing::warn!("mdns browse: multicast lock acquire failed ({e}); receive may be degraded");
        }
    });

    let browser_id = format!("mdnsbr-{}", uuid::Uuid::new_v4());
    let task_browser_id = browser_id.clone();
    // 自播回显过滤用：捕获宿主 AppHandle，事件循环内按需读取本机节点 ID
    // （节点可能晚于 browse 启动，须在事件时刻实时比对而非 browse 时刻）
    let app_handle = state.host_ctx.app_handle.clone();
    std::thread::spawn(move || {
        let browser_id = task_browser_id;
        // 事件透传循环跑在独立线程（flume recv 阻塞语义；不经 tokio 避免运行时依赖），
        // 退出由 stop_browse 断开 channel 触发
        loop {
            match receiver.recv() {
                Ok(event) => match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let txt_records: std::collections::BTreeMap<String, String> = info
                            .get_properties()
                            .iter()
                            .map(|p| (p.key().to_string(), p.val_str().to_string()))
                            .collect();
                        // 自播回显过滤：本机广播也会被自己的浏览收到（引擎
                        // handle_browse_event 同口径）。TXT `id` == 本机 NodeId
                        // 即自身，跳过不推送——否则设备列表出现自己、拨号连
                        // 自己（真机实证：移动端向桌面端连接变成连自己）
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
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use crate::plugin::wasm_runtime::WasmHostContext;
    use std::collections::HashSet;
    use std::sync::Arc;

    /// 测试插件 ID 前缀（各用例拼唯一后缀，owner 隔离 + purge 精确回收）
    const TEST_PLUGIN_PREFIX: &str = "com.bedcode.mdns-selftest";

    /// 最小宿主状态（授予 mdns 权限）：内存库 + tempdir storage、app_handle=None
    /// 无头形态，与 component.rs 测试 build_host_ctx 同构。tempdir 刻意泄漏保活
    /// （storage 目录随进程存活；mdns 路径不触 storage，防悬空即可）。
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
            granted_permissions: HashSet::from([
                bedcode_plugin_api_mobile::permission::PERMISSION_MDNS.to_string(),
            ]),
        }
    }

    #[test]
    fn mdns_browse_within_runtime_worker_does_not_panic() {
        // 复刻生产上下文：async command 在 tokio worker 上驱动 wasm activate 调入
        // 宿主函数。改动前此处 tauri::async_runtime::block_on 属「运行时内阻塞」
        // 反模式（同 peer.rs 2026-08-26 真机实证 panic 形态）；改动后 fire-and-forget
        // spawn，worker 上下文内调用必须不 panic、不嵌套 block_on
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = format!("{TEST_PLUGIN_PREFIX}.worker");
        let state = mdns_enabled_state(&plugin_id, &handle);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(async {
                // tokio::spawn 落到 worker 线程执行，与生产宿主函数同上下文
                let joined = tokio::spawn(async move {
                    mdns_browse(&state, "_bedcode-selftest-worker._tcp.local.")
                })
                .await;
                joined.expect("mdns_browse must not panic inside runtime worker")
            })
        }));
        // 回收句柄（成功路径守护线程随 stop 退出；daemon 启动失败路径无句柄）
        purge_browsers_for_plugin(&plugin_id);
        assert!(
            result.is_ok(),
            "mdns_browse must not panic when called inside a tokio runtime worker"
        );
    }

    #[test]
    fn mdns_browse_outside_runtime_does_not_panic() {
        // 纯 std 线程（无 runtime 上下文）：fire-and-forget spawn 回退全局 runtime，
        // 同样不得 panic 或阻塞当前线程
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = format!("{TEST_PLUGIN_PREFIX}.plain");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let state = mdns_enabled_state(&plugin_id, &handle);
            let _ = mdns_browse(&state, "_bedcode-selftest-plain._tcp.local.");
        }));
        purge_browsers_for_plugin(&plugin_id);
        assert!(
            result.is_ok(),
            "mdns_browse must not panic outside any runtime context"
        );
    }

    #[test]
    fn browse_stop_cycles_keep_browser_table_bounded() {
        // browse → stop_browse 连续多次：句柄必须被回收、stop 幂等（重复 stop
        // 返回 false），BROWSERS 表跨循环不增长。沙箱/无多播网络下 daemon 可能
        // 启动失败（browse 返回 Err）：失败路径同样不得残留表项。
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build test runtime");
        let handle = rt.handle().clone();
        let plugin_id = format!("{TEST_PLUGIN_PREFIX}.cycle");
        let state = mdns_enabled_state(&plugin_id, &handle);
        for i in 0..3 {
            let Ok(id) = mdns_browse(&state, "_bedcode-selftest-cycle._tcp.local.") else {
                continue;
            };
            assert!(mdns_stop_browse(&state, &id).expect("stop browse"), "cycle {i}: fresh handle must report stopped");
            assert!(
                !mdns_stop_browse(&state, &id).expect("stop browse again"),
                "cycle {i}: repeat stop must be idempotent false"
            );
        }
        assert_eq!(
            purge_browsers_for_plugin(&plugin_id),
            0,
            "stop_browse must recycle handles; BROWSERS must not grow across cycles"
        );
    }
}
