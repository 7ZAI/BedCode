//! host_notify — 系统通知（逻辑层）
//!
//! 移动端特有能力：组件路径归属 WIT `host-events.notify`（SDK HostEvents trait 现状即
//! emit+notify 同组，spec §3.1 如实映射）

use super::super::WasmPluginState;

/// 逻辑层：发送系统通知（title/body → Kotlin TaskNotificationPlugin）
///
/// 非 Android 平台（桌面 dev 场景）不支持，返回 Err（与旧 func_wrap 同语义）
pub(crate) fn notify(state: &WasmPluginState, title: &str, body: &str) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use crate::plugin::android_plugins::notification_plugin_handle;
        use super::support::guarded_host_call;

        let Some(handle) = notification_plugin_handle() else {
            return Err("TaskNotificationPlugin not registered".to_string());
        };
        let payload = serde_json::json!({ "title": title, "body": body });
        guarded_host_call(
            &state.plugin_id,
            "host_notify",
            Err::<serde_json::Value, _>(anyhow::anyhow!("host_notify panicked")),
            || {
                tokio::task::block_in_place(|| {
                    state
                        .runtime_handle
                        .block_on(handle.run_mobile_plugin_async("showPluginNotification", payload))
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
            },
        )
        .map(|_| ())
        .map_err(|e| format!("notification failed: {}", e))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (title, body);
        let _ = state;
        Err("only supported on Android".to_string())
    }
}
