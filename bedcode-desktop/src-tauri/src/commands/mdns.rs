//! mDNS Commands
//!
//! Tauri 命令 — 桌面端 mDNS 广播控制

use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::{AppHandle, Manager};

use crate::mdns::advertiser::MdnsAdvertiser;
use crate::system::constants::mdns;
use crate::Result;

/// 获取全局 MdnsAdvertiser 实例
fn get_advertiser(app: &AppHandle) -> Arc<RwLock<MdnsAdvertiser>> {
    app.state::<Arc<RwLock<MdnsAdvertiser>>>().inner().clone()
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
    txt_records.insert(mdns::TXT_KEY_PLATFORM.to_string(), mdns::TXT_VALUE_PLATFORM.to_string());
    txt_records.insert(mdns::TXT_KEY_DEVICE_NAME.to_string(), device_name.clone());
    txt_records.insert(mdns::TXT_KEY_VERSION.to_string(), env!("CARGO_PKG_VERSION").to_string());

    let config = crate::mdns::types::AdvertiseConfig {
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

/// 获取 mDNS 广播状态
#[tauri::command]
pub async fn mdns_is_advertising(app_handle: AppHandle) -> Result<bool> {
    let advertiser = get_advertiser(&app_handle);
    let a = advertiser.read().await;
    Ok(a.is_advertising().await)
}
