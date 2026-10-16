//! mDNS Commands
//!
//! Tauri 命令 — 服务发现与广播

use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::mdns::discovery::MdnsDiscovery;
use crate::mdns::types::DiscoveredService;
use crate::Result;

/// 获取全局 MdnsDiscovery 实例
///
/// 实例不可变（内部字段级锁），直接持有 Arc 调用，避免外层锁跨 await
fn get_discovery(app: &AppHandle) -> Arc<MdnsDiscovery> {
    app.state::<Arc<MdnsDiscovery>>().inner().clone()
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
