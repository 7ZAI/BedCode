//! mDNS Commands
//!
//! Tauri 命令 — 服务发现与广播

use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::{AppHandle, Manager};

use crate::mdns::advertiser::MdnsAdvertiser;
use crate::mdns::discovery::MdnsDiscovery;
use crate::mdns::types::{AdvertiseConfig, DiscoveredService};
use crate::Result;

/// 获取全局 MdnsDiscovery 实例
fn get_discovery(app: &AppHandle) -> Arc<RwLock<MdnsDiscovery>> {
    app.state::<Arc<RwLock<MdnsDiscovery>>>().inner().clone()
}

/// 获取全局 MdnsAdvertiser 实例
fn get_advertiser(app: &AppHandle) -> Arc<RwLock<MdnsAdvertiser>> {
    app.state::<Arc<RwLock<MdnsAdvertiser>>>().inner().clone()
}

/// 启动 mDNS 服务发现
#[tauri::command]
pub async fn mdns_start_discovery(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_start_discovery] Starting discovery");
    let discovery = get_discovery(&app_handle);
    let d = discovery.read().await;
    d.start(app_handle).await
}

/// 停止 mDNS 服务发现
#[tauri::command]
pub async fn mdns_stop_discovery(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_stop_discovery] Stopping discovery");
    let discovery = get_discovery(&app_handle);
    let d = discovery.read().await;
    d.stop().await
}

/// 获取当前已发现的服务列表
#[tauri::command]
pub async fn mdns_get_discovered_services(app_handle: AppHandle) -> Result<Vec<DiscoveredService>> {
    let discovery = get_discovery(&app_handle);
    let d = discovery.read().await;
    Ok(d.get_services().await)
}

/// 启动 mDNS 服务广播
#[tauri::command]
pub async fn mdns_start_advertise(
    app_handle: AppHandle,
    port: u16,
    device_name: String,
) -> Result<()> {
    tracing::info!("[mdns_start_advertise] Starting advertise: {} on port {}", device_name, port);
    let advertiser = get_advertiser(&app_handle);
    let mut txt_records = std::collections::HashMap::new();
    txt_records.insert("platform".to_string(), "mobile".to_string());
    txt_records.insert("device_name".to_string(), device_name.clone());
    txt_records.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());

    let config = AdvertiseConfig {
        service_name: device_name,
        port,
        txt_records,
    };

    let a = advertiser.read().await;
    a.start(config).await
}

/// 停止 mDNS 服务广播
#[tauri::command]
pub async fn mdns_stop_advertise(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_stop_advertise] Stopping advertise");
    let advertiser = get_advertiser(&app_handle);
    let a = advertiser.read().await;
    a.stop().await
}
