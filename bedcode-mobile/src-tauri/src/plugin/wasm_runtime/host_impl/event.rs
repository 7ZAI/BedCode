//! host_emit_event — 事件推送（逻辑层）

use tauri::Emitter;
use super::super::WasmPluginState;

/// 逻辑层：向前端发送事件（WIT host-events.emit，无错误返回；
/// 失败仅记录日志——与旧 func_wrap 同语义）
pub(crate) fn emit_event(state: &WasmPluginState, event_name: &str, payload_str: &str) {
    let json_payload: serde_json::Value = match serde_json::from_str(payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "host_emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_str.to_string())
        }
    };

    // 无头/测试上下文（app_handle 为 None）：广播事件降级为仅日志
    if let Some(app) = &state.host_ctx.app_handle {
        if let Err(e) = app.emit(event_name, json_payload) {
            tracing::error!(error = %e, event = %event_name, "host_emit_event: emit failed");
        }
    }
}
