//! host-http 服务端域端到端（ABI v29，HTTP 路由代码注册下沉专项）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）。测试节奏（spec §6 / 用户裁定 ⑧）：本文件随阶段 1
//! 落盘，**统一在阶段 5 全量回归时运行**——fixture 构建 + 真实 WASM 闭环属
//! 集成测试执行，阶段 1–4 期间不单独运行。
//!
//! 覆盖：fixture 插件（`packages/plugin-http-test`）经 `host-http.register-endpoint`
//! 在 activate 期注册内部路径 + host 别名 + 模板别名 → 宿主动态注册表可查（内部 /
//! 精确 / 模板捕获）→ `_http_endpoint` 回显入参形状 → `unregister-endpoint` 属主
//! 注销幂等。

use super::*;
use crate::server::http::registry;

#[test]
fn test_http_route_registration_server_domain_roundtrip() {
    const PLUGIN_ID: &str = "com.bedcode.http-test";
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let rt = tokio::runtime::Runtime::new().expect("tokio multi-thread runtime");
    rt.block_on(ws_e2e_guard("http 服务端域 e2e", async {
        // 单测不走 manifest 授权路径：显式授予（network:http = 服务端域权限门）
        host_ctx
            .permission
            .grant_permissions(PLUGIN_ID, &["storage".to_string(), "network:http".to_string()]);
        let component = wasm_runtime
            .compile_component(&build_http_test_component())
            .expect("compile http fixture component");
        let plugin = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate http fixture"),
        ));

        plugin.lock().await.activate().expect("activate = 0");

        // ==================== 注册表可查（内部路径 / 精确别名 / 模板捕获） ====================
        let internal = format!("/api/plugin/{PLUGIN_ID}/hello");
        let entry = registry::find_by_internal(&internal).expect("内部路径必须已注册");
        assert_eq!(entry.owner, PLUGIN_ID);
        assert_eq!(entry.auth, bedcode_plugin_api::EndpointAuth::Jwt, "未声明 auth 落最严档");

        let exact = registry::find_by_host("/api/http-test/hello", "GET").expect("host 别名精确命中");
        assert_eq!(exact.entry.path, "hello");
        assert!(exact.params.is_empty(), "精确命中无模板捕获");

        let tpl = registry::find_by_host("/api/http-test/s-9/x", "GET").expect("模板别名命中");
        assert_eq!(tpl.params.get("id").map(String::as_str), Some("s-9"), "模板捕获必须随请求注入");

        // ==================== _http_endpoint 入参形状（网关转发同构） ====================
        let echoed: serde_json::Value = serde_json::from_str(
            &plugin.lock().await.invoke_command(
                "_http_endpoint",
                &serde_json::json!({
                    "method": "GET",
                    "path": "x/{id}",
                    "query": { "q": "1" },
                    "params": { "id": "s-9" },
                    "body": null,
                    "caller": "device",
                })
                .to_string(),
            )
            .expect("_http_endpoint"),
        )
        .expect("echo json");
        assert_eq!(echoed["body"]["data"]["path"], "x/{id}");
        assert_eq!(echoed["body"]["data"]["params"]["id"], "s-9");

        // ==================== 注销：属主命中 → 幂等 false；路由随之不可查 ====================
        let unreg: serde_json::Value = serde_json::from_str(
            &plugin.lock().await.invoke_command(
                "http-unregister",
                &serde_json::json!({ "endpointId": entry.endpoint_id }).to_string(),
            )
            .expect("http-unregister"),
        )
        .expect("unreg json");
        assert_eq!(unreg["hit"], true, "属主注销必须命中");
        assert!(registry::find_by_internal(&internal).is_none(), "注销后内部路径不可查");
        assert!(registry::find_by_host("/api/http-test/hello", "GET").is_none(), "注销后别名不可查");

        let again: serde_json::Value = serde_json::from_str(
            &plugin.lock().await.invoke_command(
                "http-unregister",
                &serde_json::json!({ "endpointId": entry.endpoint_id }).to_string(),
            )
            .expect("http-unregister again"),
        )
        .expect("unreg json");
        assert_eq!(again["hit"], false, "重复注销幂等 false");

        // 收尾：fixture 停用路径的 purge（宿主停用回收语义由 activation 单测覆盖）
        registry::purge_for_plugin(PLUGIN_ID);
    }));
}
