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
    // 启动早期（AppContext::init 完成前，auto-activate 的插件可能已广播）
    // 无同步通道可用：静默丢弃（与 MessageBus 无订阅者同语义），不 panic
    let Some(ctx) = crate::system::app_context::AppContext::try_global() else {
        return Err("broadcast error: AppContext not initialized yet".to_string());
    };
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

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};

    /// 无头上下文（AppHandle=None）：事件无处投递但返回 Ok（幂等约定）
    #[test]
    fn emit_event_headless_returns_ok() {
        let ctx = build_host_ctx();
        assert!(emit_event(&ctx, "plugin:event", r#"{"ok":true}"#).is_ok());
    }

    /// 非法 JSON 载荷降级为原始字符串；无头上下文同样 Ok（不因载荷失败）
    #[test]
    fn emit_event_headless_invalid_json_ok() {
        let ctx = build_host_ctx();
        assert!(emit_event(&ctx, "plugin:event", "not-json").is_ok());
    }

    /// notify 与 emit 的降级约定不同：无头上下文明确报错（弹窗是强需求能力）
    #[test]
    fn notify_headless_rejected() {
        let ctx = build_host_ctx();
        let err = notify(&ctx, "test-plugin", "title", "body").unwrap_err();
        assert!(err.contains("app_handle not available"), "got: {}", err);
    }

    /// 无 broadcast 权限：同步广播被权限门禁拒绝（AppContext 全局未初始化也不 panic）
    #[test]
    fn broadcast_sync_permission_denied() {
        let ctx = build_host_ctx();
        let err = broadcast_sync(&ctx, "test-plugin", "{}").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 有权限但载荷畸形：类型化 SyncEvent 解析拒绝（未知/畸形事件不静默丢弃）
    #[test]
    fn broadcast_sync_malformed_payload_rejected() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_BROADCAST]);
        let err = broadcast_sync(&ctx, "test-plugin", "not-json").unwrap_err();
        assert!(err.contains("unknown or malformed sync event"), "got: {}", err);
    }

    // 成功路径（反序列化 → AppContext::global().sync_tx 广播）依赖应用启动时初始化的
    // AppContext 全局单例：测试环境未初始化会 panic，交由集成/手动测试覆盖，
    // 此处只测可独立验证的权限门禁与载荷校验
}
