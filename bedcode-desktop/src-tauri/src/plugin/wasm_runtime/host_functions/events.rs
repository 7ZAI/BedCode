//! 事件域 Host Functions（前端事件 / 移动端同步广播 / 通知）

use super::memory::read_wasm_string_consume;
use crate::plugin::wasm_runtime::WasmPluginState;
use crate::plugin::permission::PERMISSION_BROADCAST;
use tauri::Emitter;

/// 事件：发送 Tauri 事件到前端
///
/// 参数：(event_name_ptr, event_name_len, payload_ptr, payload_len)
pub(super) fn host_emit_event(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    name_ptr: u32,
    name_len: u32,
    payload_ptr: u32,
    payload_len: u32,
) {
    let event_name = match read_wasm_string_consume(&mut caller, name_ptr, name_len) {
        Some(s) => s,
        None => {
            tracing::error!("host_emit_event: failed to read event_name");
            return;
        }
    };

    let payload_str = match read_wasm_string_consume(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!(event = %event_name, "host_emit_event: failed to read payload");
            return;
        }
    };

    let json_payload: serde_json::Value = match serde_json::from_str(&payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "host_emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str)
        }
    };

    let host_ctx = caller.data().host_ctx.clone();
    // 无头上下文（测试）没有 AppHandle，事件无处投递
    let Some(app_handle) = host_ctx.app_handle.as_ref() else {
        tracing::warn!(event = %event_name, "host_emit_event: app_handle not available in headless context");
        return;
    };
    if let Err(e) = app_handle.emit(&event_name, json_payload) {
        tracing::error!(error = %e, event = %event_name, "host_emit_event: emit failed");
    }
}

/// 广播同步事件到所有客户端（移动端同步通道）
///
/// 插件通过此函数将状态变更推送到 DesktopSyncEvent 广播通道，
/// 由 SyncEventHandler 转发给所有已认证的 WebSocket 客户端（移动端）。
///
/// 载荷为 SDK 类型化 `SyncEvent`（serde 表示即线协议），
/// 宿主反序列化后经 `From` 穷尽转换为内部事件 —— 未知类型在编译期即不可能出现
pub(super) fn host_broadcast_sync(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    payload_ptr: u32,
    payload_len: u32,
) {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    // 权限校验：broadcast 权限门控移动端同步通道
    if !super::check_permission(&host_ctx, &plugin_id, PERMISSION_BROADCAST, "host_broadcast_sync") {
        return;
    }

    let payload_str = match read_wasm_string_consume(&mut caller, payload_ptr, payload_len) {
        Some(s) => s,
        None => {
            tracing::error!("[plugin:{}] host_broadcast_sync: failed to read payload", plugin_id);
            return;
        }
    };

    // 载荷直接反序列化为 SDK 类型化 SyncEvent（与插件侧同一类型，serde 表示即线协议）
    // 未知/畸形事件在此被拒绝，不再静默丢弃：类型化后插件侧也无法构造未知变体
    let sdk_event: bedcode_plugin_api::events::SyncEvent = match serde_json::from_str(&payload_str) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "[plugin:{}] host_broadcast_sync: unknown or malformed sync event", plugin_id);
            return;
        }
    };

    // 穷尽转换：SyncEvent 新增变体时 From 实现编译失败，强制同步
    let sync_event = crate::events::DesktopSyncEvent::from(sdk_event);

    let ctx = crate::system::app_context::AppContext::global();
    let sync_tx = ctx.sync_tx();
    if let Err(e) = sync_tx.send(sync_event) {
        tracing::error!(error = %e, "[plugin:{}] host_broadcast_sync: broadcast failed", plugin_id);
    }
}

/// 通知：通过 Tauri 事件发送到前端 toast
///
/// 参数：(title_ptr, title_len, body_ptr, body_len)
/// 返回：0 成功，-1 失败
pub(super) fn host_notify(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    title_ptr: u32,
    title_len: u32,
    body_ptr: u32,
    body_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    let host_ctx = caller.data().host_ctx.clone();

    let title = match read_wasm_string_consume(&mut caller, title_ptr, title_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_notify: failed to read title");
            return -1;
        }
    };

    let body = match read_wasm_string_consume(&mut caller, body_ptr, body_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, title = %title, "host_notify: failed to read body");
            return -1;
        }
    };

    let Some(app_handle) = host_ctx.app_handle.as_ref() else {
        tracing::warn!(plugin_id = %plugin_id, "host_notify: app_handle not available in headless context");
        return -1;
    };
    match app_handle.emit("plugin:notify", serde_json::json!({
        "plugin_id": plugin_id,
        "title": title,
        "body": body,
    })) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_notify: emit failed");
            -1
        }
    }
}
