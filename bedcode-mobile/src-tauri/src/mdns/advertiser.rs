//! mDNS Service Advertisement
//!
//! 广播本设备的 _bedcode._tcp.local. 服务

use std::sync::Arc;
use tokio::sync::RwLock;
use mdns_sd::{ServiceDaemon, ServiceInfo};

use super::types::{AdvertiseConfig, SERVICE_TYPE};

/// mDNS 广播管理器
pub struct MdnsAdvertiser {
    /// mdns-sd 守护进程
    daemon: Arc<RwLock<Option<ServiceDaemon>>>,
    /// 已注册的服务名
    registered_name: Arc<RwLock<Option<String>>>,
    /// 是否正在广播
    advertising: Arc<RwLock<bool>>,
}

impl MdnsAdvertiser {
    /// 创建新的广播管理器
    pub fn new() -> Self {
        Self {
            daemon: Arc::new(RwLock::new(None)),
            registered_name: Arc::new(RwLock::new(None)),
            advertising: Arc::new(RwLock::new(false)),
        }
    }

    /// 启动服务广播
    pub async fn start(&self, config: AdvertiseConfig) -> crate::Result<()> {
        let mut advertising = self.advertising.write().await;
        if *advertising {
            tracing::warn!("[MdnsAdvertiser] Already advertising");
            return Ok(());
        }

        let daemon = ServiceDaemon::new()
            .map_err(|e| crate::AppError::Internal(format!("Failed to create mDNS daemon: {}", e)))?;

        // 构造服务信息
        let service_type = SERVICE_TYPE;
        let instance_name = &config.service_name;

        // TXT 记录的 key 在 mdns-sd 中自动转小写
        let properties: Vec<(String, String)> = config.txt_records
            .into_iter()
            .map(|(k, v)| (k, v))
            .collect();

        let service_info = ServiceInfo::new(
            service_type,
            instance_name,
            &format!("{}.local.", instance_name),
            "",
            config.port,
            &*properties,
        )
        .map_err(|e| crate::AppError::Internal(format!("Failed to create ServiceInfo: {}", e)))?
        .enable_addr_auto();

        daemon.register(service_info)
            .map_err(|e| crate::AppError::Internal(format!("Failed to register mDNS service: {}", e)))?;

        *self.daemon.write().await = Some(daemon);
        *self.registered_name.write().await = Some(instance_name.clone());
        *advertising = true;

        tracing::info!("[MdnsAdvertiser] Advertising {} as {} on port {}", SERVICE_TYPE, instance_name, config.port);
        Ok(())
    }

    /// 停止服务广播
    pub async fn stop(&self) -> crate::Result<()> {
        let mut advertising = self.advertising.write().await;
        if !*advertising {
            return Ok(());
        }
        *advertising = false;

        if let Some(daemon) = self.daemon.write().await.take() {
            if let Some(name) = self.registered_name.write().await.take() {
                // unregister 接受完整的全限定名
                let fullname = format!("{}.{}", name, SERVICE_TYPE);
                let _ = daemon.unregister(&fullname);
            }
            let _ = daemon.shutdown();
        }

        tracing::info!("[MdnsAdvertiser] Stopped advertising");
        Ok(())
    }

    /// 是否正在广播
    pub async fn is_advertising(&self) -> bool {
        *self.advertising.read().await
    }
}
