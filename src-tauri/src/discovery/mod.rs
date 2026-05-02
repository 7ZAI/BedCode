//! mDNS Device Discovery
//!
//! 提供设备发现和广播功能

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

/// mDNS 服务类型
pub const SERVICE_TYPE: &str = "_claude-remote._tcp.local.";

/// 发现的设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    /// 设备名称
    pub name: String,
    /// IP 地址
    pub address: String,
    /// 端口
    pub port: u16,
    /// 服务属性
    pub properties: HashMap<String, String>,
    /// 发现时间
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}

// ==================== Desktop Implementation ====================

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::net::IpAddr;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::time::Duration;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct DiscoveryService {
    /// mDNS 守护进程
    daemon: ServiceDaemon,
    /// 已发现的设备
    discovered: Arc<Mutex<HashMap<String, DiscoveredDevice>>>,
    /// 设备发现事件发送器
    device_tx: broadcast::Sender<DiscoveredDevice>,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl DiscoveryService {
    /// 创建新的发现服务
    pub fn new() -> crate::Result<Self> {
        let daemon = ServiceDaemon::new()?;
        let (device_tx, _) = broadcast::channel(64);

        Ok(Self {
            daemon,
            discovered: Arc::new(Mutex::new(HashMap::new())),
            device_tx,
        })
    }

    /// 开始广播服务
    pub fn start_broadcast(&self, service_name: &str, port: u16) -> crate::Result<()> {
        let mut properties = HashMap::new();
        properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
        properties.insert("platform".to_string(), get_platform_info());

        let info = ServiceInfo::new(
            SERVICE_TYPE,
            service_name,
            service_name,
            "", // 自动获取主机名
            port,
            properties,
        )?;

        self.daemon.register(info)?;

        tracing::info!("mDNS service broadcast started: {} on port {}", service_name, port);
        Ok(())
    }

    /// 开始发现服务
    pub fn start_discovery(&self) -> crate::Result<()> {
        let receiver = self.daemon.browse(SERVICE_TYPE)?;

        let discovered = self.discovered.clone();
        let device_tx = self.device_tx.clone();

        tokio::spawn(async move {
            loop {
                match receiver.recv_timeout(Duration::from_secs(1)) {
                    Ok(event) => match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let name = info.get_fullname().to_string();

                            // 获取 IP 地址
                            let address = info
                                .get_addresses()
                                .iter()
                                .filter(|ip| is_valid_address(ip))
                                .next()
                                .map(|ip| ip.to_string())
                                .unwrap_or_default();

                            if address.is_empty() {
                                continue;
                            }

                            let device = DiscoveredDevice {
                                name: name.clone(),
                                address,
                                port: info.get_port(),
                                properties: info
                                    .get_properties()
                                    .iter()
                                    .map(|p| (p.key().to_string(), p.val_str().to_string()))
                                    .collect(),
                                discovered_at: chrono::Utc::now(),
                            };

                            tracing::info!("Discovered device: {} at {}:{}", device.name, device.address, device.port);

                            if let Ok(mut d) = discovered.try_lock() {
                                d.insert(name, device.clone());
                            }

                            let _ = device_tx.send(device);
                        }
                        ServiceEvent::ServiceRemoved(_, name) => {
                            tracing::info!("Device removed: {}", name);
                            if let Ok(mut d) = discovered.try_lock() {
                                d.remove(&name);
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        // Timeout or other error
                        if !e.to_string().contains("timed out") {
                            tracing::error!("mDNS browse error: {}", e);
                        }
                        continue;
                    }
                }
            }
        });

        tracing::info!("mDNS discovery started");
        Ok(())
    }

    /// 获取已发现的设备列表
    pub async fn get_discovered_devices(&self) -> Vec<DiscoveredDevice> {
        let discovered = self.discovered.lock().await;
        discovered.values().cloned().collect()
    }

    /// 订阅设备发现事件
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveredDevice> {
        self.device_tx.subscribe()
    }

    /// 停止广播
    pub fn stop_broadcast(&self, service_name: &str) -> crate::Result<()> {
        self.daemon.unregister(service_name)?;
        tracing::info!("mDNS broadcast stopped: {}", service_name);
        Ok(())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Drop for DiscoveryService {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
/// 检查是否是有效的地址（过滤掉回环和链路本地地址）
fn is_valid_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            !ipv4.is_loopback() && !ipv4.is_link_local()
        }
        IpAddr::V6(ipv6) => {
            !ipv6.is_loopback()
        }
    }
}

// ==================== Mobile Implementation (Stub) ====================

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct DiscoveryService {
    /// 已发现的设备
    discovered: Arc<Mutex<HashMap<String, DiscoveredDevice>>>,
    /// 设备发现事件发送器
    device_tx: broadcast::Sender<DiscoveredDevice>,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl DiscoveryService {
    /// 创建新的发现服务
    pub fn new() -> crate::Result<Self> {
        let (device_tx, _) = broadcast::channel(64);

        Ok(Self {
            discovered: Arc::new(Mutex::new(HashMap::new())),
            device_tx,
        })
    }

    /// 开始广播服务 (mobile - uses WebSocket client instead)
    pub fn start_broadcast(&self, service_name: &str, port: u16) -> crate::Result<()> {
        tracing::info!("Mobile broadcast requested: {} on port {} (use WebSocket client instead)", service_name, port);
        // On mobile, we don't broadcast - we connect as a client
        Ok(())
    }

    /// 开始发现服务 (mobile - uses manual connection)
    pub fn start_discovery(&self) -> crate::Result<()> {
        tracing::info!("Mobile discovery started (manual IP entry mode)");
        // On mobile, users enter IP manually or use QR code
        Ok(())
    }

    /// 获取已发现的设备列表
    pub async fn get_discovered_devices(&self) -> Vec<DiscoveredDevice> {
        let discovered = self.discovered.lock().await;
        discovered.values().cloned().collect()
    }

    /// 订阅设备发现事件
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveredDevice> {
        self.device_tx.subscribe()
    }

    /// 停止广播
    pub fn stop_broadcast(&self, _service_name: &str) -> crate::Result<()> {
        Ok(())
    }

    /// 手动添加设备（用于移动端手动输入 IP）
    pub async fn add_device_manual(&self, name: String, address: String, port: u16) {
        let device = DiscoveredDevice {
            name: name.clone(),
            address,
            port,
            properties: HashMap::new(),
            discovered_at: chrono::Utc::now(),
        };

        let mut discovered = self.discovered.lock().await;
        discovered.insert(name, device.clone());
        let _ = self.device_tx.send(device);
    }
}

// ==================== Common ====================

/// 获取平台信息
fn get_platform_info() -> String {
    #[cfg(target_os = "windows")]
    {
        "windows".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "linux".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "macos".to_string()
    }
    #[cfg(target_os = "android")]
    {
        "android".to_string()
    }
    #[cfg(target_os = "ios")]
    {
        "ios".to_string()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos", target_os = "android", target_os = "ios")))]
    {
        "unknown".to_string()
    }
}

/// 简单的设备发现函数（用于测试）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn discover_devices(timeout_secs: u64) -> crate::Result<Vec<DiscoveredDevice>> {
    let service = DiscoveryService::new()?;
    service.start_discovery()?;

    tokio::time::sleep(Duration::from_secs(timeout_secs)).await;

    Ok(service.get_discovered_devices().await)
}
