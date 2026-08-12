//! host_http_fetch — HTTP 请求

use crate::plugin::wasm_host;
use tauri::Emitter;
use super::super::WasmPluginState;
use super::support::{guarded_host_call, has_permission, read_wasm_string, write_result_to_out_ptr, write_wasm_string};

/// HTTP 代理：发起 HTTP 请求
pub(crate) fn host_http_fetch(
    mut caller: wasmtime::Caller<'_, WasmPluginState>,
    req_ptr: u32,
    req_len: u32,
    out_ptr: u32,
) -> i32 {
    let plugin_id = caller.data().plugin_id.clone();
    if !has_permission(&caller, bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP) {
        tracing::warn!(plugin_id = %plugin_id, "host_http_fetch: permission denied (network:http)");
        return -1;
    }
    let host_ctx = caller.data().host_ctx.clone();

    let request_json = match read_wasm_string(&mut caller, req_ptr, req_len) {
        Some(s) => s,
        None => {
            tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to read request JSON");
            return -1;
        }
    };

    let request: serde_json::Value = match serde_json::from_str(&request_json) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: invalid request JSON");
            return -1;
        }
    };

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        let stream_id = uuid::Uuid::new_v4().to_string();
        let stream_event = request
            .get("streamEvent")
            .and_then(|v| v.as_str())
            .unwrap_or(&stream_id)
            .to_string();

        let app_handle = host_ctx.app_handle.clone();
        let plugin_id_clone = plugin_id.clone();
        let stream_event_clone = stream_event.clone();

        tokio::spawn(async move {
            if let Err(e) = wasm_host::execute_streaming_http(
                &request,
                &app_handle,
                &stream_event_clone,
                &plugin_id_clone,
            )
            .await
            {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id_clone,
                    "Streaming HTTP request failed"
                );
                let _ = app_handle.emit(
                    &stream_event_clone,
                    serde_json::json!({ "error": e.to_string(), "done": true }),
                );
            }
        });

        let result_json = serde_json::json!({
            "streamId": stream_id,
            "streamEvent": stream_event,
        });
        let result_str = serde_json::to_string(&result_json).unwrap_or_default();
        match write_wasm_string(&mut caller, &result_str) {
            Some((ptr, len)) => {
                if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                    0
                } else {
                    -1
                }
            }
            None => -1,
        }
    } else {
        let handle = caller.data().runtime_handle.clone();
        match guarded_host_call(
            &plugin_id,
            "host_http_fetch",
            Err(anyhow::anyhow!("host_http_fetch panicked")),
            || tokio::task::block_in_place(|| {
                handle.block_on(wasm_host::execute_http_request(&request))
            }),
        ) {
            Ok(response) => {
                let result_str = match serde_json::to_string(&response) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: response serialization failed");
                        return -1;
                    }
                };
                match write_wasm_string(&mut caller, &result_str) {
                    Some((ptr, len)) => {
                        if write_result_to_out_ptr(&mut caller, out_ptr, ptr, len) {
                            0
                        } else {
                            -1
                        }
                    }
                    None => {
                        tracing::error!(plugin_id = %plugin_id, "host_http_fetch: failed to write result to WASM memory");
                        -1
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, plugin_id = %plugin_id, "host_http_fetch: HTTP request failed");
                -1
            }
        }
    }
}
