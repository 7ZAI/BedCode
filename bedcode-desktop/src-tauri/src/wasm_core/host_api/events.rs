//! 事件域宿主实现（前端事件 / 通知）

use tauri::Emitter;

/// 发送 Tauri 事件到前端
///
/// 无头上下文（测试）没有 AppHandle，事件无处投递，返回 Ok 保持幂等
pub(crate) fn emit_event(app: &dyn crate::wasm_core::host_api::context::AppHandleScope, event_name: &str, payload_json: &str) -> Result<(), String> {
    let json_payload: serde_json::Value = match serde_json::from_str(payload_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, event = %event_name, "emit_event: invalid JSON payload, using raw string");
            serde_json::Value::String(payload_json.to_string())
        }
    };
    let Some(app_handle) = app.app_handle() else {
        tracing::warn!(event = %event_name, "emit_event: app_handle not available in headless context");
        return Ok(());
    };
    app_handle
        .emit(event_name, json_payload)
        .map_err(|e| format!("event emit failed: {}", e))
}

/// 通过 Tauri 事件发送到前端 toast
pub(crate) fn notify(app: &dyn crate::wasm_core::host_api::context::AppHandleScope, plugin_id: &str, title: &str, body: &str) -> Result<(), String> {
    let Some(app_handle) = app.app_handle() else {
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

    /// 无头上下文（AppHandle=None）：事件无处投递但返回 Ok（幂等约定）
    #[test]
    fn emit_event_headless_returns_ok() {
        let ctx = build_host_ctx();
        assert!(emit_event(ctx.as_ref(), "plugin:event", r#"{"ok":true}"#).is_ok());
    }

    /// 非法 JSON 载荷降级为原始字符串；无头上下文同样 Ok（不因载荷失败）
    #[test]
    fn emit_event_headless_invalid_json_ok() {
        let ctx = build_host_ctx();
        assert!(emit_event(ctx.as_ref(), "plugin:event", "not-json").is_ok());
    }

    /// notify 与 emit 的降级约定不同：无头上下文明确报错（弹窗是强需求能力）
    #[test]
    fn notify_headless_rejected() {
        let ctx = build_host_ctx();
        let err = notify(ctx.as_ref(), "test-plugin", "title", "body").unwrap_err();
        assert!(err.contains("app_handle not available"), "got: {}", err);
    }
}