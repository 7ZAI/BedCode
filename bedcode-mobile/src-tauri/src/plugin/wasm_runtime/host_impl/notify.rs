//! host_notify — 系统通知

use super::super::WasmPluginState;
use super::support::{guarded_host_call, read_wasm_string};

/// 通知：移动端特有，转发 Kotlin TaskNotificationPlugin（host_notify）
///
/// 同步 host 函数内部经 tokio block_in_place + block_on 执行异步插件调用
pub(crate) fn host_notify(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    title_ptr: u32,
    title_len: u32,
    body_ptr: u32,
    body_len: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();

    let title = match read_wasm_string(&mut caller, title_ptr, title_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_notify: failed to read title");
            return -1;
        }
    };

    let body = match read_wasm_string(&mut caller, body_ptr, body_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_notify: failed to read body");
            return -1;
        }
    };

    #[cfg(target_os = "android")]
    {
        use crate::plugin::android_plugins::notification_plugin_handle;

        let handle = match notification_plugin_handle() {
            Some(h) => h,
            None => {
                tracing::error!(plugin_id = %plugin_id, "host_notify: TaskNotificationPlugin not registered");
                return -1;
            }
        };
        let runtime_handle = caller.data().runtime_handle.clone();
        let payload = serde_json::json!({ "title": title, "body": body });
        match guarded_host_call(
            &plugin_id,
            "host_notify",
            Err::<serde_json::Value, _>(anyhow::anyhow!("host_notify panicked")),
            || {
                tokio::task::block_in_place(|| {
                    runtime_handle
                        .block_on(handle.run_mobile_plugin_async("showPluginNotification", payload))
                        .map_err(|e| anyhow::anyhow!("{e}"))
                })
            },
        ) {
            Ok(_) => 0,
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, "host_notify: notification failed");
                -1
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (&title, &body);
        tracing::warn!(plugin_id = %plugin_id, "host_notify: only supported on Android");
        -1
    }
}
