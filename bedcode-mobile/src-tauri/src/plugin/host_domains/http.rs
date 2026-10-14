//! host_http_fetch — HTTP 出站（转发：`bedcode-server-http` 能力域）
//!
//! **2026-10-10 wasm-core-mobile-purity 票 05**：与票 03 的 peer-net 同款手法——
//! 出站 HTTP 的执行本体（非流式请求 / SSE 流 / 上游限广大 byte / 重定向再校验）
//! 在能力域 crate `bedcode-server-http`（`plugin_binding::egress`），本文件只把
//! 本端宿主服务接到能力域的 [`HttpPorts`]。
//!
//! **为什么 egress 判定（D9）不随之下沉**：三层判定（桌面端目标 / 声明 / 记忆）
//! 与 NeedConsent 授权弹窗是**宿主安全闸门**的内聚实现，能力域只见「放行 / 拒绝 /
//! 判定失败」三态结论（[`OutboundAuth`]）——拆过界会让弹窗编排脱离宿主载体。
//! 无 app_handle 的无头 / 测试上下文走 `CheckFailed`（fail-closed：弹窗事务没有
//! 载体，收紧为「判定失败」而非「查不到就放行」）。
//!
//! ## 行为 Variant（相对迁移前，均为错误文本而非判定结果）
//!
//! - 权限/egress/流畅能力的**判定**不变，只是生产者从本文件换成能力域；
//! - 错误前缀由 `HTTP request failed: ` → `http error: `（能力域统一字眼）；
//! - 缺 `url` 的拒绝从本文件的入参校验下沉到执行层（错误仍含 `missing 'url'`）。

use std::sync::Arc;

use bedcode_host_kit::WasmPluginState;
use super::{current_handle, host_ctx};
use super::support::guarded_host_call;
use bedcode_server_http::plugin_binding::egress::http_fetch as core_http_fetch;
use bedcode_server_http::plugin_binding::ports::{
    BoxedBlocked, HttpEventSink, HttpPorts, OutboundAuth,
};
use tauri::Emitter;

use crate::plugin::bindings::bedcode;

impl bedcode::plugin::host_http::Host for WasmPluginState {
    fn fetch(&mut self, request_json: String) -> Result<Option<String>, String> {
        http_fetch(self, &request_json)
    }
}


// ==================== 能力域端口 adapter ====================

/// 统一 state → 能力域 [`HttpPorts`] 的移动 adapter
///
/// 能力域的端口契约带 `'static`（它的异步桥要求 future 可 `'static`），故本
/// adapter **持有副本**而非借 state：调用点在入口处一次性快照宿主服务的句柄
/// （权限快照 / 引擎端口 / AppHandle），不需要在此持 `&WasmPluginState`。
struct StateHttpPorts {
    granted: std::collections::HashSet<String>,
    ports: Arc<dyn bedcode_wasm_core::host_api::ports::HostEnginePorts>,
    app_handle: Option<Arc<tauri::AppHandle>>,
}

impl HttpPorts for StateHttpPorts {
    fn check_permission(&self, _plugin_id: &str, permission: &str, _api: &str) -> bool {
        // 单调用方纪律：能力域传入的 plugin_id 恒等于本实例 plugin_id（逻辑层唯一入口），
        // 故直接查入口处快照的权限集
        self.granted.contains(permission)
    }

    fn authorize_outbound(&self, plugin_id: &str, url: &str, _may_prompt: bool) -> OutboundAuth {
        // egress 三层判定（桌面端目标 / 声明 / 记忆）+ NeedConsent 弹窗编排留宿主引擎
        // （D9）：能力域只见三态结论。无 app_handle 的无头 / 测试上下文 = 判定失败
        // （fail-closed：弹窗事务没有载体，收紧而非放行）。
        let origin = normalize_origin(url);
        let Some(app_handle) = self.app_handle.clone() else {
            return OutboundAuth::CheckFailed(
                "egress denied: app_handle unavailable (headless/test)".to_string(),
            );
        };
        let ports = self.ports.clone();
        let source = format!("plugin:{}", plugin_id);
        let url_owned = url.to_string();
        match tokio::task::block_in_place(|| {
            current_handle()
                .block_on(async move { ports.egress_check(&app_handle, &url_owned, &source).await })
        }) {
            Ok(()) => OutboundAuth::Allowed { origin },
            Err(reason) => OutboundAuth::Denied { reason, origin },
        }
    }

    /// 前端事件通道：缺席（`None`）= 无头上下文，能力域据此显性拒绝流式请求
    /// （缺席语义 ≠ 投递失败，见能力域 ports 头注）
    fn event_sink(&self) -> Option<Arc<dyn HttpEventSink>> {
        let app_handle = self.app_handle.clone()?;
        Some(Arc::new(AppHandleSink {
            app: (*app_handle).clone(),
        }))
    }

    fn block_on_any(&self, fut: BoxedBlocked) -> Box<dyn std::any::Any + Send> {
        // 能力域的执行是 async，宿主在同步调用点阻塞驱动（重入避让在内核
        // `runtime_util::block_on_async` 单点：嵌套异步上下文里它会换线程驱动）。
        // fut 已是擦除后的 `Box<dyn Any + Send>`（server-http 泛型助手装箱），
        // 直接返回驱动结果，不得再套一层 Box（套层会让 downcast 落空——
        // 与桌面宿主 plugin/http.rs 同形，2026-10-11 单形态对齐）。
        bedcode_wasm_core::runtime_util::block_on_async(fut)
    }

    /// `"jwtAuth": true` 的全局 token 代注（移动侧独有能力的托管口径：token 的
    /// 真源是宿主进程内状态，**不落插件**，C4；当下无 token 即显错，
    /// 禁止静默降级为匿名请求）
    fn jwt_auth_token(&self, _plugin_id: &str) -> Result<Option<String>, String> {
        let token = self.ports.global_token();
        if token.is_empty() {
            Err("jwtAuth requested but no global token available (not authenticated)".to_string())
        } else {
            Ok(Some(token))
        }
    }
}

/// 前端事件通道：`tauri::AppHandle` 的 emit 投影
///
/// 能力域不许依赖 tauri（WIT-无关、端口化）：事件怎么投出去是本宿主的事——
/// `emit` 失败只降级告警（前端没收着事件不是本次请求失败的判据：请求本体已发完）。
struct AppHandleSink {
    app: tauri::AppHandle,
}

impl HttpEventSink for AppHandleSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.app.emit(event, payload) {
            tracing::warn!(error = %e, event = %event, "emit stream event failed");
        }
    }
}

/// 归一化 `scheme://host[:port]`（错误串只带这三者：不带 path / query，
/// AGENTS §8 凭据红线）
fn normalize_origin(url: &str) -> String {
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, url),
    };
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    match scheme {
        Some(scheme) => format!("{}://{}", scheme, authority),
        None => authority.to_string(),
    }
}

// ==================== 逻辑层（薄转发） ====================

/// 逻辑层：发起 HTTP 请求（request 为 JSON；`stream: true` 走流式分支）
///
/// 流式分支：能力域注册 streamId 后立即返回（后台推流经 [`HttpPorts::event_sink`]
/// 广播），非流式由能力域同步返回响应 JSON。
pub(crate) fn http_fetch(
    state: &WasmPluginState,
    request_json: &str,
) -> Result<Option<String>, String> {
    // 端口快照（能力域端口要求 `'static`，故在此一次性取句柄而非借 state）
    let ctx = host_ctx(state);
    let ports: Arc<dyn HttpPorts> = Arc::new(StateHttpPorts {
        // 权限快照向内核真源结算（端口要求 `'static`；判定口径与
        // `WasmHostContext::has_permission` 同一份）
        granted: ctx.granted_permissions(&state.plugin_id),
        ports: ctx.ports.clone(),
        app_handle: ctx.app_handle.clone(),
    });
    guarded_host_call(
        &state.plugin_id,
        "host_http_fetch",
        Err("host_http_fetch panicked".to_string()),
        || core_http_fetch(&ports, &state.plugin_id, request_json, true),
    )
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn granted_state(plugin_id: &str) -> WasmPluginState {
        bedcode_wasm_core::test_support::build_component_state(
            plugin_id,
            &[bedcode_plugin_api_mobile::permission::PERMISSION_NETWORK_HTTP],
        )
    }

    /// 权限门 fail-closed（票 13 显性锁）：未授予 `network:http` → 请求在
    /// 权限层即拒绝（不进入 egress 判定，也不发起网络请求）
    #[tokio::test]
    async fn permission_denied_without_network_http() {
        let st = bedcode_wasm_core::test_support::build_component_state("com.bedcode.demo", &[]);
        let err = http_fetch(
            &st,
            &serde_json::json!({ "method": "GET", "url": "http://192.168.1.5:4455/api/health" })
                .to_string(),
        )
        .unwrap_err();
        assert!(err.contains("permission denied"), "got: {err}");
    }

    /// 缺 url 字段 → 执行层拒绝（能力域统一字眼「Missing 'url'」；
    /// 迁移前字眼为 `missing 'url'`，故按小写比较，不受能力域大小写变化影响）
    #[tokio::test]
    async fn missing_url_rejected() {
        let st = granted_state("com.bedcode.demo");
        let err = http_fetch(&st, &serde_json::json!({ "method": "GET" }).to_string()).unwrap_err();
        let lowered = err.to_lowercase();
        assert!(
            lowered.contains("missing 'url'"),
            "缺 url 的请求必须被拒，且错误点名 url 字段，got: {err}"
        );
    }

    /// 非法 JSON 请求体 → 拒绝（入参校验，能力域解析失败）
    #[tokio::test]
    async fn invalid_request_json_rejected() {
        let st = granted_state("com.bedcode.demo");
        let err = http_fetch(&st, "not json").unwrap_err();
        assert!(err.contains("invalid request JSON"), "got: {err}");
    }

    /// 无头上下文（无 app_handle）→ egress fail-closed 拒绝（禁「查不到就放行」）
    #[test]
    fn headless_context_denied_by_egress() {
        // 测试内自建 runtime 且该 runtime 的 handle 被桥使用：block_in_place 桥内
        // drop runtime 会 panic（tokio 禁止异步上下文 drop runtime），故 forget 保活
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime");
        std::mem::forget(rt);
        let st = granted_state("com.bedcode.demo");
        let err = http_fetch(
            &st,
            &serde_json::json!({ "method": "GET", "url": "http://192.168.1.5:4455/api/health" })
                .to_string(),
        )
        .unwrap_err();
        assert!(
            err.contains("egress denied") && err.contains("app_handle unavailable"),
            "无 app_handle 的无头上下文必须 fail-closed，got: {err}"
        );
    }

    /// 错误串只带归一化 origin（不带 path / query —— AGENTS §8 凭据红线）
    #[test]
    fn normalize_origin_strips_path_and_query() {
        assert_eq!(
            normalize_origin("https://h.example:8080/a/b?token=x"),
            "https://h.example:8080"
        );
        assert_eq!(
            normalize_origin("http://192.168.1.5:4455"),
            "http://192.168.1.5:4455"
        );
    }
}
