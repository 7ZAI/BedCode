//! mDNS Commands
//!
//! Tauri 命令 — 服务发现与广播

use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::mdns::advertiser::MdnsAdvertiser;
use crate::mdns::discovery::MdnsDiscovery;
use crate::mdns::types::{AdvertiseConfig, DiscoveredService};
use crate::Result;

/// 获取全局 MdnsDiscovery 实例
///
/// 实例不可变（内部字段级锁），直接持有 Arc 调用，避免外层锁跨 await
fn get_discovery(app: &AppHandle) -> Arc<MdnsDiscovery> {
    app.state::<Arc<MdnsDiscovery>>().inner().clone()
}

/// 获取全局 MdnsAdvertiser 实例
fn get_advertiser(app: &AppHandle) -> Arc<MdnsAdvertiser> {
    app.state::<Arc<MdnsAdvertiser>>().inner().clone()
}

/// 启动 mDNS 服务发现
#[tauri::command]
pub async fn mdns_start_discovery(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_start_discovery] Starting discovery");
    get_discovery(&app_handle).start(app_handle).await
}

/// 停止 mDNS 服务发现
#[tauri::command]
pub async fn mdns_stop_discovery(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_stop_discovery] Stopping discovery");
    get_discovery(&app_handle).stop().await
}

/// 获取当前已发现的服务列表
#[tauri::command]
pub async fn mdns_get_discovered_services(app_handle: AppHandle) -> Result<Vec<DiscoveredService>> {
    Ok(get_discovery(&app_handle).get_services().await)
}

/// 启动 mDNS 服务广播
#[tauri::command]
pub async fn mdns_start_advertise(
    app_handle: AppHandle,
    port: u16,
    device_name: String,
) -> Result<()> {
    tracing::info!("[mdns_start_advertise] Starting advertise: {} on port {}", device_name, port);
    let mut txt_records = std::collections::HashMap::new();
    txt_records.insert("platform".to_string(), "mobile".to_string());
    txt_records.insert("device_name".to_string(), device_name.clone());
    txt_records.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());

    let config = AdvertiseConfig {
        service_name: device_name,
        port,
        txt_records,
    };

    get_advertiser(&app_handle).start(config).await
}

/// 停止 mDNS 服务广播
#[tauri::command]
pub async fn mdns_stop_advertise(app_handle: AppHandle) -> Result<()> {
    tracing::info!("[mdns_stop_advertise] Stopping advertise");
    get_advertiser(&app_handle).stop().await
}
