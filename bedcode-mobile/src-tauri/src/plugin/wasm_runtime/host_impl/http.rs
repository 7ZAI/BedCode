//! host_http_fetch — HTTP 请求（逻辑层）

use crate::plugin::wasm_host;
use tauri::Emitter;
use super::super::WasmPluginState;
use super::support::guarded_host_call;

/// 逻辑层：发起 HTTP 请求（request 为 JSON；stream=true 走流式分支）
///
/// 流式分支：注册 streamId 立即返回（宿主后台推流到 streamEvent；
/// 进度经 app_handle.emit 广播），非流式同步返回响应 JSON
pub(crate) fn http_fetch(state: &WasmPluginState, request_json: &str) -> Result<Option<String>, String> {
    if !state
        .granted_permissions
        .contains(bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP)
    {
        return Err("permission denied: network:http".to_string());
    }

    let request: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|e| format!("invalid request JSON: {}", e))?;

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_stream {
        let stream_id = uuid::Uuid::new_v4().to_string();
        let stream_event = request
            .get("streamEvent")
            .and_then(|v| v.as_str())
            .unwrap_or(&stream_id)
            .to_string();

        // 无头/测试上下文（app_handle 为 None）：流式 HTTP 不可用，直接拒绝
        let Some(app_handle) = state.host_ctx.app_handle.clone() else {
            return Err("app_handle unavailable, streaming http rejected".to_string());
        };
        let plugin_id = state.plugin_id.clone();
        let stream_event_clone = stream_event.clone();

        tokio::spawn(async move {
            if let Err(e) = wasm_host::execute_streaming_http(
                &request,
                &app_handle,
                &stream_event_clone,
                &plugin_id,
            )
            .await
            {
                tracing::error!(
                    error = %e,
                    plugin_id = %plugin_id,
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
        return Ok(Some(
            serde_json::to_string(&result_json).map_err(|e| format!("serialize failed: {}", e))?,
        ));
    }

    let response = guarded_host_call(
        &state.plugin_id,
        "host_http_fetch",
        Err(anyhow::anyhow!("host_http_fetch panicked")),
        || {
            tokio::task::block_in_place(|| {
                state.runtime_handle.block_on(wasm_host::execute_http_request(&request))
            })
        },
    )
    .map_err(|e| format!("HTTP request failed: {}", e))?;

    serde_json::to_string(&response)
        .map(Some)
        .map_err(|e| format!("response serialization failed: {}", e))
}
