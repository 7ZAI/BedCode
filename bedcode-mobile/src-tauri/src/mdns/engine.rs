//! mDNS 全局共享守护（全进程唯一 `ServiceDaemon`）
//!
//! 从 `plugin/wasm_runtime/host_impl/mdns.rs` 提升（票 03）：守护是**引擎资产**，
//! 三个消费方共用同一实例——
//!
//! 1. 宿主命令面（`mdns/discovery.rs` / `mdns/advertiser.rs`，前端设备发现/广播）；
//! 2. 插件面（`host_impl/mdns.rs` host-mdns 原语，单守护 + 双句柄表）；
//! 3. peer-net 引擎（节点身份广播，经 [`shared_daemon`] 接线）。
//!
//! **全仓共享守护的创建点只有 [`init_daemon`] 一处**——切勿在别处另建守护：
//! 两个守护会争抢同一组播端口（真机实证：只发现自己、发现不了对端）。
//!
//! Android 多播锁随守护常驻获取（幂等，不随浏览句柄增删）：宿主函数可能运行于
//! tokio worker，此处 block_on 属运行时内阻塞反模式——fire-and-forget spawn；
//! 非 Android 平台 stub 立即返回，同走 spawn 保持单一代码路径（可测）。

use std::sync::OnceLock;

use mdns_sd::ServiceDaemon;

use crate::plugin::android_plugins::multicast_lock_acquire;

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
pub(crate) fn daemon() -> &'static ServiceDaemon {
    DAEMON.get_or_init(init_daemon)
}

/// 守护若已初始化则返回（stop/query 前预防性保护：单测表操作不触网）
pub(crate) fn daemon_if_initialized() -> Option<&'static ServiceDaemon> {
    DAEMON.get()
}

/// 周期 re-announce 间隔：mdns-sd 注册后不主动周期广播，须手动续期。
/// 与 peer-net `REANNOUNCE_INTERVAL`（45s）同节奏——宿主身份广播的续期由
/// 引擎自己的 re-announce 循环负责（ticket 04/06），本间隔只服务插件句柄
pub(crate) const REANNOUNCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(45);

/// 取全局共享守护句柄（clone 廉价）：peer-net 引擎接线用（ticket 06）
pub(crate) fn shared_daemon() -> ServiceDaemon {
    daemon().clone()
}
