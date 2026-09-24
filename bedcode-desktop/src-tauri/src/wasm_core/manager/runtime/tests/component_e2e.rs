//! Component Model 基础往返与 events-ws 导出探测
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
/// 组件完整往返：实例化、ABI 协商、生命周期、命令（guest 内 import 往返）、
/// 终端钩子、事件回调、上传钩子、manifest
#[test]

fn test_component_roundtrip() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");

    let rt = tokio::runtime::Runtime::new().unwrap();
    // 组件内 import 调用经 block_on_async 走 tokio（与 core 路径同机制），
    // 测试体整体在运行时上下文中执行
    rt.block_on(async {
        // 预写 storage key：验证 guest 内 host_storage import 读回（JSON 值往返）
        host_ctx
            .storage
            .set(TEST_PLUGIN_ID, "component-test-key", serde_json::json!({"k": "v"}))
            .await
            .expect("preset storage key");

        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate test component");

        // 生命周期
        assert_eq!(plugin.activate().expect("activate"), 0);
        assert_eq!(plugin.deactivate().expect("deactivate"), 0);

        // manifest
        let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.component-test");

        // 命令调用：guest 内 host_storage.get 往返
        let result = plugin
            .invoke_command("test.echo", r#"{"hello":"component"}"#)
            .expect("invoke_command");
        let result_json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result_json["name"], "test.echo");
        assert_eq!(result_json["stored"]["k"], "v");

        // 主库往返：前缀校验通过 + 建表 + 插入 + 查询
        let db_rows = result_json["dbRows"].as_array().expect("dbRows array");
        assert_eq!(db_rows.len(), 1);
        assert_eq!(db_rows[0]["val"], "hello");

        // 插件独立库：**票 08 起无头测试也走真实私有库**——`WasmHostContext`
        // 新增 `plugin_db_root` 注入（`aot_cache_dir` 同模式），不再退化为
        // 「headless 不可用」错误透传。故此处断言从「错误形状」升级为
        // **真实往返成立**：建表 + 插入 + 查询全链（比原断言更强，
        // 且是插件私有库在宿主测试里的第一条真实覆盖）。
        assert!(
            result_json["pdbCreateError"].is_null(),
            "私有库建表不得报错, got: {}",
            result_json["pdbCreateError"]
        );
        assert!(
            result_json["pdbQueryError"].is_null(),
            "私有库查询不得报错, got: {}",
            result_json["pdbQueryError"]
        );
        let pdb_rows = result_json["pdbRows"].as_array().expect("pdbRows array");
        // 行数不断言等于 1：私有库根目录是**进程级**注入（`plugin_db_root()`），
        // 同一进程内共享 `com.bedcode.test/plugin.db`，其它用例的插入会累积
        // （持久化正是私有库的语义）。这里断言的是「写入可读回」这一链路的
        // 正确性，而非行数。
        assert!(
            !pdb_rows.is_empty(),
            "私有库往返必须读到插入行, got: {}",
            result_json["pdbRows"]
        );
        assert!(
            pdb_rows.iter().all(|r| r["val"] == "pdb"),
            "私有库行内容必须与 fixture 插入一致, got: {}",
            result_json["pdbRows"]
        );

        // v27（票 10）：`sessions` 断言（原 `test_session_list`）与终端钩子断言
        // 随 host-session / terminal-hooks 两个 interface 退役一并删除。

        // 消息总线发布（同步投递）
        assert_eq!(result_json["busPublished"], serde_json::json!(true));

        // 事件回调 + 启动/关闭（`on_message` / `on_process_done` 仍是必选导出）
        plugin
            .on_message("topic", "sender", &serde_json::json!({"a": 1}))
            .expect("on_message");
        plugin
            .on_startup()
            .expect("on_startup")
            .expect("plugin on_startup returned Err");
        plugin
            .on_shutdown()
            .expect("on_shutdown")
            .expect("plugin on_shutdown returned Err");
    });
}

/// v14：`events-ws` 可选导出的探测与投递
///
/// - SDK 产物（`wasm_entry!` 无条件导出 `events-ws`）→ 探测命中：
///   `on_ws_frame` 投递成功（`Ok(true)`），客户端域与服务端域两条回调都可达；
/// - 手写绑定产物（`plugin-component-test`，未导出 `events-ws`）→ 探测为
///   None：`on_ws_frame` 返回 `Ok(false)`（调用方按 spec §2.2 降级：丢弃 +
///   首次 warn + 计数，宿主不缓存），**不影响加载与其余导出**
#[test]

fn test_ws_events_export_probe_and_dispatch() {
    use crate::wasm_core::bus::WsFrameDispatch;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let sdk_component = wasm_runtime
        .compile_component(&build_sdk_test_component())
        .expect("compile SDK test component");
    let legacy_component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile component-test");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut sdk_plugin = wasm_runtime
            .instantiate_component(&sdk_component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("instantiate SDK component");
        let client_frame = WsFrameDispatch::Client {
            handle: "wsc-test".to_string(),
            kind: "text".to_string(),
            payload: b"hello ws".to_vec(),
        };
        assert!(
            sdk_plugin.on_ws_frame(&client_frame).expect("deliver client frame"),
            "SDK 产物必须导出 events-ws（wasm_entry! 无条件导出）"
        );
        // 同一接口的第二个函数：服务端域回调同样命中
        let server_frame = WsFrameDispatch::EndpointClient {
            endpoint_id: "wse-test".to_string(),
            client_id: "wsc-peer".to_string(),
            kind: "binary".to_string(),
            payload: vec![0xff, 0x00, 0x7f],
        };
        assert!(
            sdk_plugin.on_ws_frame(&server_frame).expect("deliver server frame"),
            "服务端域回调必须可投递"
        );

        // 旧产物（v13 及更早，未导出 events-ws）：探测 None → 降级 Ok(false)
        let mut legacy_plugin = wasm_runtime
            .instantiate_component(
                &legacy_component,
                "com.bedcode.component-test",
                host_ctx.clone(),
                &[],
                None,
            )
            .expect("未导出 events-ws 的产物不得影响加载");
        assert!(
            !legacy_plugin.on_ws_frame(&client_frame).expect("legacy probe"),
            "未导出 events-ws 的产物必须走降级路径（Ok(false)）"
        );
        // 降级不得影响其余导出
        assert!(legacy_plugin.get_manifest().is_ok(), "降级后其余导出照常");
    });
}
