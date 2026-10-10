//! 事件域宿主实现（前端事件 / 通知）

use tauri::Emitter;

/// 发送 Tauri 事件到前端
///
/// 无头上下文（测试）没有 AppHandle，事件无处投递，返回 Ok 保持幂等
///
/// 非法 JSON 载荷直接 `Err`（H-05）：降级成字符串会让 guest 以为已投递、前端
/// 收到形状不同的载荷、监听方按原 schema 解析运行时失败且无信号——静默降级
/// 的断链形态。插件侧载荷错误应在来源处可见。
/// 发送 Tauri 事件到前端
///
/// 无头上下文（测试）没有 AppHandle，事件无处投递，返回 Ok 保持幂等
///
/// 非法 JSON 载荷直接 `Err`（H-05）：降级成字符串会让 guest 以为已投递、前端
/// 收到形状不同的载荷、监听方按原 schema 解析运行时失败且无信号——静默降级
/// 的断链形态。插件侧载荷错误应在来源处可见。实现层（严格解析语义）在共享核
/// `bedcode-host-api-core::events`（票 18 批次 4）。
pub(crate) fn emit_event(
    app: &dyn crate::host_api::context::AppHandleScope,
    event_name: &str,
    payload_json: &str,
) -> Result<(), String> {
    let json_payload = bedcode_host_api_core::events::parse_event_payload(payload_json)?;
    let Some(app_handle) = app.app_handle() else {
        tracing::warn!(event = %event_name, "emit_event: app_handle not available in headless context");
        return Ok(());
    };
    app_handle
        .emit(event_name, json_payload)
        .map_err(|e| format!("event emit failed: {}", e))
}

/// 通过 Tauri 事件发送到前端 toast 的实现（`host-events-desktop.notify`）已随
/// WIT impl 迁宿主 `src-tauri/src/plugin/events.rs`（票 02 批次 06 / v36 交集切片：
/// 域函数 + 单测随域走宿主，内核不再持有 `plugin:notify` 事件发射面）。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::tests::build_host_ctx;

    /// 无头上下文（AppHandle=None）：事件无处投递但返回 Ok（幂等约定）
    #[test]
    fn emit_event_headless_returns_ok() {
        let ctx = build_host_ctx();
        assert!(emit_event(ctx.as_ref(), "plugin:event", r#"{"ok":true}"#).is_ok());
    }

    /// 非法 JSON 载荷直接报错（H-05，fail-visible）：降级成字符串会让 guest
    /// 误以为已投递而前端收到形状不同的载荷
    #[test]
    fn emit_event_rejects_invalid_json() {
        let ctx = build_host_ctx();
        let err = emit_event(ctx.as_ref(), "plugin:event", "not-json").expect_err("非法 JSON 必须拒绝");
        assert!(err.contains("not valid JSON"), "错误须点名 JSON 解析失败: {err}");
        // 合法 JSON 在无头上下文仍按幂等约定 Ok
        assert!(emit_event(ctx.as_ref(), "plugin:event", r#"{"ok":true}"#).is_ok());
    }
}
