//! mDNS Service Discovery
//!
//! 浏览局域网内的 _bedcode._tcp.local. 服务

use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

use mdns_sd::ServiceEvent;

use super::types::{DiscoveredService, SERVICE_TYPE};
use crate::system::constants::mdns::RECV_TIMEOUT_SECS;

/// mDNS 发现管理器
pub struct MdnsDiscovery {
    /// 已发现的服务缓存
    services: Arc<RwLock<HashMap<String, DiscoveredService>>>,
    /// 是否正在扫描
    scanning: Arc<RwLock<bool>>,
}

impl MdnsDiscovery {
    /// 创建新的发现管理器
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            scanning: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动服务发现（生产入口：向前端 emit 发现事件）
    pub async fn start(&self, app_handle: AppHandle) -> crate::Result<()> {
        self.start_inner(Some(app_handle)).await
    }

    /// 无头装配口（跨端互连测试）：与 [`Self::start`] **同一套浏览 / 解析 / 缓存逻辑**，
    /// 只是没有前端可 emit（`app_handle = None` 时前端事件跳过）。
    ///
    /// 存在的理由：`AppHandle` 的运行时类型是 `Wry`，无头进程里构造不出来，而发现链路的
    /// 真实可观测面是服务缓存（`get_services`）——不值得为测试改生产行为。
    pub async fn start_headless(&self) -> crate::Result<()> {
        self.start_inner(None).await
    }

    /// 启动服务发现
    pub async fn start_inner(&self, app_handle: Option<AppHandle>) -> crate::Result<()> {
        let mut scanning = self.scanning.write().await;
        if *scanning {
            tracing::warn!("[MdnsDiscovery] Already scanning");
            return Ok(());
        }

        // 浏览经全局共享守护（crate::mdns::engine，票 03）：不再自建 daemon——
        // 消灭双 daemon 同绑 5353 互抢多播包病灶；共享守护常驻，stop 只退订不 shutdown
        let receiver = crate::mdns::engine::daemon()
            .browse(SERVICE_TYPE)
            .map_err(|e| crate::AppError::Internal(format!("Failed to browse mDNS: {e}")))?;

        *scanning = true;

        tracing::info!("[MdnsDiscovery] Started browsing {}", SERVICE_TYPE);

        // 在后台任务中接收 mDNS 事件并转发到前端
        let services = self.services.clone();
        let scanning_flag = self.scanning.clone();
        tokio::spawn(async move {
            // 前端事件出口（无头装配时 `app_handle = None` → 只填缓存，不 emit）
            let emit = |event: &str, payload: serde_json::Value| {
                if let Some(ah) = app_handle.as_ref() {
                    let _ = ah.emit(event, payload);
                }
            };
            loop {
                // 检查是否应该停止
                if !*scanning_flag.read().await {
                    tracing::info!("[MdnsDiscovery] Stopped scanning, exiting event loop");
                    break;
                }

                match receiver.recv_timeout(std::time::Duration::from_secs(RECV_TIMEOUT_SECS)) {
                    Ok(event) => {
                        match event {
                            ServiceEvent::ServiceFound(service_type, instance_name) => {
                                tracing::debug!("[MdnsDiscovery] Found: {} ({})", instance_name, service_type);
                                emit(
                                    "mdns_service_found",
                                    serde_json::json!({
                                        "instance_name": instance_name,
                                    }),
                                );
                            }
                            ServiceEvent::ServiceResolved(info) => {
                                let instance_name = info.get_fullname().to_string();
                                let host_name = info.get_hostname().to_string();
                                let port = info.get_port();
                                // 优先使用 IPv4 地址，避免 IPv6 导致连接失败
                                let address = info
                                    .get_addresses()
                                    .iter()
                                    .find(|a| a.is_ipv4())
                                    .map(|a| a.to_string())
                                    .or_else(|| info.get_addresses().iter().next().map(|a| a.to_string()))
                                    .unwrap_or_default();

                                let txt_records: HashMap<String, String> = info
                                    .get_properties()
                                    .iter()
                                    .map(|p| (p.key().to_string(), p.val_str().to_string()))
                                    .collect();

                                let platform = txt_records
                                    .get("platform")
                                    .cloned()
                                    .unwrap_or_else(|| "unknown".to_string());
                                let device_name = txt_records
                                    .get("device_name")
                                    .cloned()
                                    .unwrap_or_else(|| host_name.clone());

                                let service = DiscoveredService {
                                    instance_name: instance_name.clone(),
                                    host_name,
                                    address,
                                    port,
                                    txt_records,
                                    platform,
                                    device_name,
                                };

                                tracing::info!(
                                    "[MdnsDiscovery] Resolved: {} at {}:{} (platform={})",
                                    service.device_name,
                                    service.address,
                                    service.port,
                                    service.platform
                                );

                                // 更新缓存
                                services.write().await.insert(instance_name.clone(), service.clone());

                                emit(
                                    "mdns_service_resolved",
                                    serde_json::to_value(&service).unwrap_or(serde_json::Value::Null),
                                );
                            }
                            ServiceEvent::ServiceRemoved(service_type, instance_name) => {
                                tracing::debug!("[MdnsDiscovery] Removed: {} ({})", instance_name, service_type);
                                services.write().await.remove(&instance_name);
                                emit(
                                    "mdns_service_removed",
                                    serde_json::json!({
                                        "instance_name": instance_name,
                                    }),
                                );
                            }
                            ServiceEvent::SearchStarted(service_type) => {
                                tracing::debug!("[MdnsDiscovery] Search started: {}", service_type);
                            }
                            ServiceEvent::SearchStopped(service_type) => {
                                tracing::debug!("[MdnsDiscovery] Search stopped: {}", service_type);
                                break;
                            }
                            // ServiceEvent 是 non-exhaustive，忽略未知变体
                            _ => {}
                        }
                    }
                    Err(flume::RecvTimeoutError::Timeout) => {
                        // 正常超时，继续等待
                    }
                    Err(flume::RecvTimeoutError::Disconnected) => {
                        tracing::warn!("[MdnsDiscovery] Receiver disconnected");
                        break;
                    }
                }
            }

            *scanning_flag.write().await = false;
            tracing::info!("[MdnsDiscovery] Event loop exited");
        });

        Ok(())
    }

    /// 停止服务发现
    pub async fn stop(&self) -> crate::Result<()> {
        let mut scanning = self.scanning.write().await;
        if !*scanning {
            return Ok(());
        }
        *scanning = false;
        drop(scanning); // 释放锁，让后台任务能读到 false

        // 停止守护进程（共享守护常驻不 shutdown，票 03）：只退订本浏览
        if let Some(daemon) = crate::mdns::engine::daemon_if_initialized() {
            let _ = daemon.stop_browse(SERVICE_TYPE);
        }

        self.services.write().await.clear();
        tracing::info!("[MdnsDiscovery] Stopped");
        Ok(())
    }

    /// 获取当前已发现的服务列表
    pub async fn get_services(&self) -> Vec<DiscoveredService> {
        self.services.read().await.values().cloned().collect()
    }

    /// 是否正在扫描
    pub async fn is_scanning(&self) -> bool {
        *self.scanning.read().await
    }
}
