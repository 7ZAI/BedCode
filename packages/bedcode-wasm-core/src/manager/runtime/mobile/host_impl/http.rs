//! host_http_fetch — HTTP 请求（逻辑层）
//!
//! egress 安全闸门（D5）整体留宿主：三层判定（L1 桌面端目标 / L2 声明 /
//! L3 记忆）+ NeedConsent 授权弹窗经 [`crate::host_api::ports::HostEnginePorts::egress_check`]
//! 一次完成，crate 侧只见「放行 / 拒绝（含错误码文本）」——判定与弹窗编排
//! 是宿主安全闸门的内聚实现，不拆过界。

use std::sync::Arc;

use super::super::WasmPluginState;
use super::support::guarded_host_call;
use crate::host_api::http_engine;
use crate::host_api::ports::HostEnginePorts;
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
    // 三层判定 + 需授权弹窗在宿主引擎内一次完成（端口投影；无 app_handle 的
    // 无头/测试上下文直接拒绝——弹窗事务无宿主载体，fail-closed）
    check_egress(state, &request)?;

    let is_stream = request.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    let ports = state.host_ctx.ports.clone();

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
                http_engine::execute_streaming_http(&request, &ports, &app_handle, &stream_event_clone, &plugin_id).await
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
        || tokio::task::block_in_place(|| state.runtime_handle.block_on(http_engine::execute_http_request(&request, &ports))),
    )
    .map_err(|e| format!("HTTP request failed: {}", e))?;

    serde_json::to_string(&response)
        .map(Some)
        .map_err(|e| format!("response serialization failed: {}", e))
}

/// Egress 闸门调用（端口投影；宿主实现 = 三层判定 + NeedConsent 弹窗）
///
/// 无 app_handle（无头/测试）→ fail-closed 直接拒绝：判定引擎可以无头放行
/// 已声明目标，但「需授权」分支无弹窗载体，整体收紧为拒绝是安全等价形态
/// （与旧实现 NeedConsent 分支缺 app_handle 报错同语义，判定提前到入口）。
fn check_egress(state: &WasmPluginState, request: &serde_json::Value) -> Result<(), String> {
    let url = request
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'url' in HTTP request".to_string())?;
    let source = format!("plugin:{}", state.plugin_id);

    let Some(app_handle) = state.host_ctx.app_handle.clone() else {
        tracing::warn!(
            plugin_id = %state.plugin_id,
            url = %url,
            "egress: consent-capable context unavailable (headless), denying"
        );
        return Err("egress denied: app_handle unavailable (headless/test)".to_string());
    };
    let ports: Arc<dyn HostEnginePorts> = state.host_ctx.ports.clone();
    let url_owned = url.to_string();
    tokio::task::block_in_place(|| {
        state
            .runtime_handle
            .block_on(async move { ports.egress_check(&app_handle, &url_owned, &source).await })
    })
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_api::ports::UnimplementedPorts;
    use crate::security::fs_auth::FsAuthChecker;
    use crate::storage::PluginStorage;
    use std::collections::HashSet;

    fn test_state(plugin_id: &str, runtime_handle: &tokio::runtime::Handle) -> WasmPluginState {
        let db = Arc::new(std::sync::Mutex::new(crate::db::Database::from_connection(
            rusqlite::Connection::open_in_memory().expect("open in-memory db"),
        )));
        let storage = PluginStorage::test_storage();
        let fs_auth = Arc::new(FsAuthChecker::new(storage.clone(), None, Vec::new()));
        let status_reporter: Arc<dyn Fn(&str, &str) + Send + Sync> = Arc::new(|_, _| {});
        WasmPluginState {
            plugin_id: plugin_id.to_string(),
            host_ctx: Arc::new(crate::manager::runtime::WasmHostContext::new_headless(
                db,
                storage,
                None,
                fs_auth,
                Arc::new(crate::bus::MessageBus::new()),
                status_reporter,
            )),
            runtime_handle: runtime_handle.clone(),
            granted_permissions: HashSet::from([
                bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP.to_string()
            ]),
            on_message_binary: None,
        }
    }

    /// 权限门 fail-closed（票 13 显性锁）：未授予 `network:http` → 请求在
    /// 权限层即拒绝（不进入 egress 判定，也不发起网络请求）
    #[tokio::test]
    async fn permission_denied_without_network_http() {
        let rt = tokio::runtime::Handle::current();
        let mut st = test_state("com.bedcode.demo", &rt);
        st.granted_permissions.clear();
        let err = http_fetch(
            &st,
            &serde_json::json!({ "method": "GET", "url": "http://192.168.1.5:4455/api/health" })
                .to_string(),
        )
        .unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
    }

    /// 缺 url 字段 → 权限门后、egress 前拒绝（入参校验）
    #[tokio::test]
    async fn missing_url_rejected() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.demo", &rt);
        let err = http_fetch(&st, &serde_json::json!({ "method": "GET" }).to_string()).unwrap_err();
        assert!(err.contains("missing 'url'"), "got: {err}");
    }

    /// 非法 JSON 请求体 → 拒绝（入参校验）
    #[tokio::test]
    async fn invalid_request_json_rejected() {
        let rt = tokio::runtime::Handle::current();
        let st = test_state("com.bedcode.demo", &rt);
        let err = http_fetch(&st, "not json").unwrap_err();
        assert!(err.contains("invalid request JSON"), "got: {err}");
    }

    /// 无头上下文（无 app_handle）→ egress fail-closed 拒绝（禁「查不到就放行」）
    #[test]
    fn headless_context_denied_by_egress() {
        // 测试内自建 runtime 且 state 持其 handle：block_in_place 桥内 drop
        // runtime 会 panic（tokio 禁止异步上下文 drop runtime），故 forget 保活
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
        let handle = rt.handle().clone();
        std::mem::forget(rt);
        let st = test_state("com.bedcode.demo", &handle);
        let err = http_fetch(
            &st,
            &serde_json::json!({ "method": "GET", "url": "http://192.168.1.5:4455/api/health" }).to_string(),
        )
        .unwrap_err();
        assert!(
            err.contains("egress denied") && err.contains("app_handle unavailable"),
            "got: {err}"
        );
    }

}
