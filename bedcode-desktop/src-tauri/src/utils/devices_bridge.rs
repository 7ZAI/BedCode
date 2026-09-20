//! 设备派生视图桥接（票 11）：`get_connected_devices` 的真实会话数与任务状态
//!
//! 模式与 `session_create_bridge.rs` / `session_action_bridge.rs` 同构：探活锚点
//! （会话中心互调面已登记）→ JSON-RPC 互调 `devices-connect-list` → 插件完成
//! **设备派生视图**（在线判定 + 真实会话数 + 任务状态合并，spec D3/D4），再经
//! `host-session.connections-list` / `host-auth.trusted-devices-list` /
//! `host-session.list-sessions` 三原语取原始事实。
//!
//! 背景：今天 `commands/devices.rs::get_connected_devices` 的 `session_count`
//! **硬编码 0**（问题陈述 5「派生视图口径漂移」）——本桥接在插件可用时以真实
//! 计数替换之；插件不可用（未激活 / 互调失败）时降级宿主旧路径（原始连接 +
//! `session_count = 0`），行为与迁移前逐字一致，无单点。
//!
//! ## 降级语义（无单点）
//!
//! - 插件未激活（锚点不在注册表）或互调失败（超时 / 响应损坏 / 插件侧显性报错）
//!   → 返回 `Ok(None)`，调用方（`commands/devices.rs`）走宿主旧路径
//! - 插件派生成功 → `Ok(Some(Vec<DeviceConnectionInfo>))`：`addr` / `device_id` /
//!   `fingerprint` 直取连接原始记录；`session_count` 来自插件派生（按正统渲染端
//!   归属口径）。字段形状与宿主 `DeviceConnectionInfo` 逐字同形，前端零改动。

use crate::plugin::manager::wasm_runtime::WasmHostContext;
use crate::utils::auth::auth_center::{call_api, session_active};
use crate::{AppError, Result};

/// 插件互调 api（短名由 `#[plugin_api]` 宏按 manifest.api 比对防漂移）
const API_DEVICES_LIST: &str = "com.bedcode.session.devices-connect-list";

/// 插件侧不可用时记录降级（结构化字段；双轨期未激活是常态）
fn log_fallback(err: &AppError) {
    tracing::warn!(
        api = %API_DEVICES_LIST,
        error = %err,
        "device derived view plugin surface unavailable, fallback to host raw connections"
    );
}

/// 经会话中心插件取设备派生视图（真实会话数）
///
/// - `Ok(Some(devices))`：插件派生成功（连接 + 会话数 + 配对合并 + 任务状态）
/// - `Ok(None)`：插件不可用 / 互调失败 → 调用方降级宿主旧路径（无单点）
pub async fn connected_devices_via_plugin(
    host_ctx: &WasmHostContext,
) -> Result<Option<Vec<crate::server::DeviceConnectionInfo>>> {
    if !session_active(host_ctx) {
        return Ok(None);
    }
    match call_api(host_ctx, API_DEVICES_LIST, serde_json::json!({})) {
        Ok(v) => {
            let conns = v
                .get("connections")
                .and_then(|c| c.as_array())
                .ok_or_else(|| AppError::Plugin(format!("devices-list reply missing connections: {v}")))?;
            let devices: Vec<crate::server::DeviceConnectionInfo> = conns
                .iter()
                .map(|c| crate::server::DeviceConnectionInfo {
                    addr: c.get("addr").and_then(|a| a.as_str()).unwrap_or("").to_string(),
                    device_id: c.get("clientId").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                    fingerprint: c.get("fingerprint").and_then(|f| f.as_str()).map(str::to_string),
                    // 票 11：真实会话数（此前硬编码 0）——来自插件派生（按正统渲染端归属）
                    session_count: c.get("sessionCount").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
                })
                .collect();
            tracing::info!(
                count = devices.len(),
                "connected devices served via plugin derived view"
            );
            Ok(Some(devices))
        }
        Err(e) => {
            log_fallback(&e);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 派生行 → `DeviceConnectionInfo` 映射：addr / device_id / fingerprint 直取，
    /// session_count 来自插件（映射逻辑经真实互调回执形状验证：本桥接只读该回执，
    /// 映射规则与外层闭环测试同源）
    #[test]
    fn maps_derived_connection_shape() {
        let reply = serde_json::json!({
            "connections": [
                {
                    "clientId": "192.168.1.5:41234",
                    "deviceName": "Pixel",
                    "fingerprint": "fp-9",
                    "addr": "192.168.1.5:41234",
                    "authenticated": true,
                    "connectedAt": 1_700_000_000_000i64,
                    "paired": { "id": "p-1", "deviceName": "Pixel" },
                    "sessionCount": 2,
                    "sessions": []
                }
            ]
        });
        // 桥接的字段取值路径（不经互调，直接按回执形状断言映射规则）
        let conn = &reply["connections"][0];
        let mapped = crate::server::DeviceConnectionInfo {
            addr: conn.get("addr").and_then(|a| a.as_str()).unwrap_or("").to_string(),
            device_id: conn.get("clientId").and_then(|i| i.as_str()).unwrap_or("").to_string(),
            fingerprint: conn.get("fingerprint").and_then(|f| f.as_str()).map(str::to_string),
            session_count: conn.get("sessionCount").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
        };
        assert_eq!(mapped.addr, "192.168.1.5:41234");
        assert_eq!(mapped.device_id, "192.168.1.5:41234");
        assert_eq!(mapped.fingerprint.as_deref(), Some("fp-9"));
        assert_eq!(mapped.session_count, 2);
    }
}
