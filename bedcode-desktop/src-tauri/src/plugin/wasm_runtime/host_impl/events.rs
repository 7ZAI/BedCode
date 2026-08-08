//! 事件域宿主实现（前端事件 / 移动端同步广播 / 通知）

use crate::plugin::permission::PERMISSION_BROADCAST;
use crate::plugin::wasm_runtime::WasmHostContext;
use tauri::Emitter;

/// 发送 Tauri 事件到前端
///
/// 无头上下文（测试）没有 AppHandle，事件无处投递，返回 Ok 保持幂等
pub(crate) fn emit_event(
    host_ctx: &WasmHostContext,
    event_name: &str,
    payload_json: &str,
) -> Result<(), String> {
    let json_payload: serde_json::Value = match serde_json::from_str(payload_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_json.to_string())
        }
    };
    let Some(app_handle) = host_ctx.app_handle.as_ref() else {
        tracing::warn!(event = %event_name, "emit_event: app_handle not available in headless context");
        return Ok(());
    };
    app_handle
        .emit(event_name, json_payload)
        .map_err(|e| format!("event emit failed: {}", e))
}

/// 广播同步事件到所有客户端（移动端同步通道）
///
/// 载荷为 SDK 类型化 `SyncEvent`（serde 表示即线协议），
/// 宿主反序列化后经 `From` 穷尽转换为内部事件 —— 未知类型在编译期即不可能出现
pub(crate) fn broadcast_sync(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    event_json: &str,
) -> Result<(), String> {
    // 权限校验：broadcast 权限门控移动端同步通道
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_BROADCAST, "host_broadcast_sync") {
        return Err("permission denied".to_string());
    }
    // 载荷直接反序列化为 SDK 类型化 SyncEvent（与插件侧同一类型，serde 表示即线协议）
    // 未知/畸形事件在此被拒绝，不再静默丢弃：类型化后插件侧也无法构造未知变体
    let sdk_event: bedcode_plugin_api::events::SyncEvent = serde_json::from_str(event_json)
        .map_err(|e| format!("broadcast error: unknown or malformed sync event: {}", e))?;
    // 穷尽转换：SyncEvent 新增变体时 From 实现编译失败，强制同步
    let sync_event = crate::events::DesktopSyncEvent::from(sdk_event);
    let ctx = crate::system::app_context::AppContext::global();
    let sync_tx = ctx.sync_tx();
    sync_tx
        .send(sync_event)
        .map(|_| ())
        .map_err(|e| format!("broadcast error: {}", e))
}

/// 通过 Tauri 事件发送到前端 toast
pub(crate) fn notify(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    title: &str,
    body: &str,
) -> Result<(), String> {
    let Some(app_handle) = host_ctx.app_handle.as_ref() else {
        return Err("notify error: app_handle not available in headless context".to_string());
    };
    app_handle
        .emit(
            "plugin:notify",
            serde_json::json!({
                "plugin_id": plugin_id,
                "title": title,
                "body": body,
            }),
        )
        .map_err(|e| format!("notify error: emit failed: {}", e))
}
