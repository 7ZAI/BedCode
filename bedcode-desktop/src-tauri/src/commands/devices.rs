//! Device Connection Commands

use crate::Result;
use std::sync::Arc;

/// 已连接设备列表
///
/// 票 11：`session_count` 不再硬编码 0——插件可用时经 `devices_bridge` 走会话中心
/// 插件的**设备派生视图**（在线判定 + 真实会话数 + 任务状态合并，spec D3）；插件
/// 未激活 / 互调失败时降级宿主旧路径（原始连接 + `session_count = 0`），无单点。
#[tauri::command]
pub async fn get_connected_devices(
    host: tauri::State<'_, Arc<crate::plugin::PluginHost>>,
) -> Result<Vec<crate::server::DeviceConnectionInfo>> {
    if let Some(devices) = crate::utils::devices_bridge::connected_devices_via_plugin(host.wasm_host_ctx()).await? {
        return Ok(devices);
    }
    // 降级：宿主旧路径（原始连接；会话数维持 0——插件未激活时无会话数真源）
    let manager = crate::server::ws::WebSocketManager::global();
    let clients = manager.list_clients().await;
    let devices = clients
        .into_iter()
        .map(|c| crate::server::DeviceConnectionInfo {
            addr: c.addr,
            device_id: c.client_id,
            fingerprint: c.fingerprint,
            session_count: 0,
        })
        .collect();
    Ok(devices)
}
