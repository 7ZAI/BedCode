//! Device Connection Commands

use crate::Result;
use std::sync::Arc;

/// 已连接设备清单（**引擎事实**：连接注册表原始记录）
///
/// host-business-decarriage 收尾：本命令只回连接注册表事实（addr / device_id /
/// fingerprint），不再拼装「设备派生视图」（在线判定 + 真实会话数 + 任务状态合并）。
/// 派生视图是业务，归属 `com.bedcode.terminal-session` 插件（api `devices-connect-list` /
/// 命令面 `session.devices.connect-list`，供插件设备中心消费）；宿主侧的消费方
/// （`useGlobalNotifications` 启动期指纹种子化）只需要事实字段，`session_count`
/// 是已退役设备页的产物。
///
/// 因此本命令**无插件依赖**：插件未激活时依然可用（`session_count` 字段不再
/// 由本路径产出，保持 0 以维持前端类型形状）。
#[tauri::command]
pub async fn get_connected_devices(
    _host: tauri::State<'_, Arc<crate::plugin::PluginHost>>,
) -> Result<Vec<crate::server::DeviceConnectionInfo>> {
    let manager = crate::server::ws::WebSocketManager::global();
    let clients = manager.list_clients().await;
    let devices = clients
        .into_iter()
        .map(|c| crate::server::DeviceConnectionInfo {
            addr: c.addr,
            device_id: c.client_id,
            fingerprint: c.fingerprint,
            // 派生计数在插件侧；本命令只回引擎事实
            session_count: 0,
        })
        .collect();
    Ok(devices)
}
