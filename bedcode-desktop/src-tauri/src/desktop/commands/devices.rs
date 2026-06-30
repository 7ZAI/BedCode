//! Device Connection Commands

use crate::Result;

#[tauri::command]
pub async fn get_connected_devices() -> Result<Vec<crate::desktop::server::DeviceConnectionInfo>> {
    let manager = crate::desktop::websocket_manager::WebSocketManager::global();
    let clients = manager.list_clients().await;
    let devices = clients
        .into_iter()
        .map(|c| crate::desktop::server::DeviceConnectionInfo {
            addr: c.addr,
            device_id: c.client_id,
            fingerprint: c.fingerprint,
            session_count: 0,
        })
        .collect();
    Ok(devices)
}
