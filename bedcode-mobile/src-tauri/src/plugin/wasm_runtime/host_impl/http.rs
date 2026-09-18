//! host_http_fetch — HTTP 请求（逻辑层）

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use crate::plugin::wasm_host;
use tauri::Emitter;

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

    let request: serde_json::Value =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request JSON: {}", e))?;

    // ---------- Egress 校验（D9：插件网络原语与宿主代理同一校验，fail-closed） ----------
    // 未命中 L1/L2/L3 记忆 → 弹授权窗（懒触发；超时/拒绝 → DENIED，请求不发）
    check_egress(state, &request)?;

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
            if let Err(e) =
                wasm_host::execute_streaming_http(&request, &app_handle, &stream_event_clone, &plugin_id).await
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
        || tokio::task::block_in_place(|| state.runtime_handle.block_on(wasm_host::execute_http_request(&request))),
    )
    .map_err(|e| format!("HTTP request failed: {}", e))?;

    serde_json::to_string(&response)
        .map(Some)
        .map_err(|e| format!("response serialization failed: {}", e))
}

/// Egress 三层判定（URL 级；未命中声明 → 授权弹窗，fail-closed）
///
/// 与宿主 `http_request` 共用 `egress::policy()`（spec §5.6 机制要点 6）：
/// L1 桌面端目标 / L2 声明（宿主内置 + 插件 preauthUrls）/ L3 记忆 → 放行；
/// NeedConsent → 弹窗（30s 超时视为拒绝）；Deny / 弹窗拒绝 → 返回含错误码的错误。
fn check_egress(state: &WasmPluginState, request: &serde_json::Value) -> Result<(), String> {
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'url' in HTTP request".to_string())?;
    let source = format!("plugin:{}", state.plugin_id);

    match crate::egress::policy().decide(url, &source) {
        crate::egress::EgressDecision::Allow(_) => Ok(()),
        crate::egress::EgressDecision::Deny(e) => {
            tracing::warn!(
                plugin_id = %state.plugin_id,
                url = %url,
                code = %e.code,
                "egress: plugin url denied"
            );
            Err(format!("{}: {}", e.code, e.url))
        }
        crate::egress::EgressDecision::NeedConsent(mut req) => {
            // 弹窗事务 id：插件路径无前端 request_id，宿主生成 UUID
            req.id = uuid::Uuid::new_v4().to_string();
            let Some(app_handle) = state.host_ctx.app_handle.clone() else {
                return Err("egress consent required but app_handle unavailable (headless/test)".to_string());
            };
            let allowed = tokio::task::block_in_place(|| {
                state
                    .runtime_handle
                    .block_on(async { crate::egress::policy().request_consent(&app_handle, req).await })
            })
            .map_err(|e| format!("egress consent flow failed: {}", e))?;
            if allowed {
                Ok(())
            } else {
                Err(format!("{}: {}", crate::egress::ERROR_URL_DENIED, url))
            }
        }
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::fs_auth::FsAuthChecker;
    use crate::plugin::message_bus::MessageBus;
    use crate::plugin::storage::PluginStorage;
    use crate::plugin::wasm_runtime::{WasmHostContext, WasmPluginState};
    use std::collections::HashSet;
    use std::sync::Arc;

    fn test_state(plugin_id: &str, runtime_handle: &tokio::runtime::Handle) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        ));
        let tmp = Box::leak(Box::new(tempfile::tempdir().expect("tempdir")));
        let storage = Arc::new(PluginStorage::new(&tmp.path().to_path_buf()));
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(WasmHostContext::new(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: runtime_handle.clone(),
            granted_permissions: HashSet::from([
                bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP.to_string()
            ]),
        }
    }

    fn req(url: &str) -> serde_json::Value {
        serde_json::json!({ "method": "GET", "url": url })
    }

    /// L2 插件声明：register_plugin_urls 后放行
    #[tokio::test]
    async fn declared_plugin_url_allowed() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.ai-chatbox", &rt);
        crate::egress::policy()
            .register_plugin_urls("com.bedcode.ai-chatbox", &["https://api.openai.com/*".to_string()]);
        assert!(check_egress(&st, &req("https://api.openai.com/v1/chat")).is_ok());
        crate::egress::policy().unregister_plugin_urls("com.bedcode.ai-chatbox");
    }

    /// L1 桌面端目标放行（插件请求桌面端端点）
    #[tokio::test]
    async fn desktop_target_allowed_for_plugin() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.file-transfer", &rt);
        crate::egress::policy().add_desktop_target("192.168.1.5", 4455);
        assert!(check_egress(&st, &req("http://192.168.1.5:4455/api/health")).is_ok());
        crate::egress::policy().clear_desktop_targets();
    }

    /// 未声明 + 无 app_handle（无头/测试）→ fail-closed 拒绝
    #[tokio::test]
    async fn undeclared_without_app_handle_denied() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.ai-chatbox", &rt);
        let err = check_egress(&st, &req("https://custom.example.com/v1")).unwrap_err();
        assert!(
            err.contains("consent required") || err.contains("EXTERNAL_URL"),
            "got: {err}"
        );
    }

    /// 非法 URL → Deny（fail-closed，非弹窗）
    #[tokio::test]
    async fn malformed_url_denied() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.demo", &rt);
        let err = check_egress(&st, &req("not-a-url")).unwrap_err();
        assert!(err.contains("EXTERNAL_URL_NOT_DECLARED"), "got: {err}");
    }

    /// 缺 url 字段 → 拒绝
    #[tokio::test]
    async fn missing_url_rejected() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.demo", &rt);
        let err = check_egress(&st, &serde_json::json!({ "method": "GET" })).unwrap_err();
        assert!(err.contains("missing 'url'"), "got: {err}");
    }
}
