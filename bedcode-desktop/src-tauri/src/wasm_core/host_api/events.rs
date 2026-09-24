//! 事件域宿主实现（前端事件 / 移动端同步广播 / 通知）

use crate::wasm_core::host_api::context::WasmHostContext;
use crate::wasm_core::permission::PERMISSION_BROADCAST;
use tauri::Emitter;

/// 发送 Tauri 事件到前端
///
/// 无头上下文（测试）没有 AppHandle，事件无处投递，返回 Ok 保持幂等
pub(crate) fn emit_event(host_ctx: &WasmHostContext, event_name: &str, payload_json: &str) -> Result<(), String> {
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
/// 载荷为 SDK 类型化 `SyncEvent`（专项票 02 起其 serde 表示与出站 `SyncPayload`
/// 同 wire）：宿主反序列化 → 包成 `HostSyncEvent` → 经统一 `events::publish` 入口
/// 进入广播面。这里**没有**按会话变体的转换分支，宿主只剩「解析 + 投递」两件事。
///
/// 失败一律显性留痕，禁止静默丢弃：解析失败（含票 02 换格式后未重建的旧产物）、
/// 载荷折不成出站形状（`validate`）、事件源未注册（装配缺失）都走 `Err`。
/// 注意 WIT `host-events.broadcast-sync` 无返回值（D5 不动 ABI），所以 `Err` 的
/// 可观测点是 `runtime/component.rs` 导入壳打的 `error!`，不是插件侧的异常。
pub(crate) fn broadcast_sync(host_ctx: &WasmHostContext, plugin_id: &str, event_json: &str) -> Result<(), String> {
    // 权限校验：broadcast 权限门控移动端同步通道
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_BROADCAST, "host_broadcast_sync") {
        return Err("permission denied".to_string());
    }
    // 未知/畸形事件在此被拒绝（含票 02 之前的旧内部标签格式）：静默收下就是
    // 「线还在、推送永远是空」的断链
    let sdk_event: bedcode_plugin_api::events::SyncEvent = serde_json::from_str(event_json)
        .map_err(|e| format!("broadcast error: unknown or malformed sync event: {}", e))?;
    let event = crate::events::HostSyncEvent::from(sdk_event);
    // 统一发布入口（校验 + 投递）：host function 是同步的，经既有桥驱动 async matcher
    crate::wasm_core::runtime_util::block_on_async(crate::events::publish(event))
        .map_err(|e| format!("broadcast error: {e}"))
}

/// 通过 Tauri 事件发送到前端 toast
pub(crate) fn notify(host_ctx: &WasmHostContext, plugin_id: &str, title: &str, body: &str) -> Result<(), String> {
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
    use crate::wasm_core::host_api::tests::{build_host_ctx, grant_permissions};

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

    /// 票 02 换格式后的**旧产物**拒绝锁：内部标签 PascalCase + 字段平铺的载荷
    /// （专项前插件 `broadcast_sync` 出的形状）必须是点名可见的解析错误，
    /// 不能被判成「没有事件」——静默收下就是「线还在、推送永远是空」的断链。
    #[test]
    fn broadcast_sync_rejects_pre_alignment_internal_tag_format() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "test-plugin", &[PERMISSION_BROADCAST]);
        let legacy = r#"{"type":"TaskQueueChanged","session_id":"s1","queue_count":1,"action":"add"}"#;
        let err = broadcast_sync(&ctx, "test-plugin", legacy).unwrap_err();
        assert!(
            err.contains("unknown or malformed sync event"),
            "旧格式应被显性拒绝，实际: {err}"
        );
        // 反控：新格式（adjacently tagged）必须能过解析这一关
        let current =
            r#"{"type":"task_queue_changed","data":{"session_id":"s1","queue_count":1,"action":"add"}}"#;
        match broadcast_sync(&ctx, "test-plugin", current) {
            // 无头 harness 里事件源未注册（publish 的 NoSource）是唯一允许的下游错误，
            // 解析本身必须已通过——否则就是新格式也被拒了
            Err(e) => assert!(
                !e.contains("malformed sync event"),
                "新格式不该被判成畸形: {e}"
            ),
            Ok(()) => {}
        }
    }

    // 成功路径（解析 → HostSyncEvent → events::publish → 全局事件源）依赖启动期
    // 注册的 HostSyncEvent 事件源：无头 harness 里 `publish` 显性报 NoSource，
    // 端到端投递由集成测试（broadcast_shutdown / pty_session_chain）覆盖，
    // 此处只测可独立验证的权限门禁与载荷校验
}
