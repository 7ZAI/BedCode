//! host_emit_event — 事件推送（逻辑层）
//!
//! 载荷严格解析语义在共享核 `bedcode-host-api-core::events`（票 18 批次 4，
//! **以桌面机制为准**）：非法 JSON **拒绝投递** + warn——此前移动把非法载荷
//! 宽松降级为字符串投递，会让前端收到形状不同的载荷、监听方按原 schema 解析
//! 失败且无信号（静默降级的断链形态）。WIT `host-events.emit` 无错误返回，
//! 拒绝以 warn 落日志（与既有 emit 失败仅日志同语义）。

use super::super::WasmPluginState;
use tauri::Emitter;

/// 逻辑层：向前端发送事件（WIT host-events.emit，无错误返回；
/// 失败仅记录日志——与旧 func_wrap 同语义）
pub(crate) fn emit_event(state: &WasmPluginState, event_name: &str, payload_str: &str) {
    let json_payload = match bedcode_host_api_core::events::parse_event_payload(payload_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "host_emit_event: invalid JSON payload rejected");
            return;
        }
    };

    // 无头/测试上下文（app_handle 为 None）：广播事件降级为仅日志
    if let Some(app) = &state.host_ctx.app_handle {
        if let Err(e) = app.emit(event_name, json_payload) {
            tracing::error!(error = %e, event = %event_name, "host_emit_event: emit failed");
        }
    }
}
