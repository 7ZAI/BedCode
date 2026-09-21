//! SDK 插件组件往返（api-call / binary bus）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
/// SDK 组件插件完整往返：真实 SDK（wasm_entry! 宏 + WasmHost）产物的组件
/// 加载、ABI 协商、生命周期、WasmHost 各 trait 经组件 import 的能力往返
#[test]

fn test_sdk_plugin_component_roundtrip() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_sdk_test_component())
        .expect("compile SDK test component");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut plugin = wasm_runtime
            .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
            .expect("instantiate SDK test component");

        // 生命周期（宏生成的 lifecycle::Guest）
        assert_eq!(plugin.activate().expect("activate"), 0);
        assert_eq!(plugin.deactivate().expect("deactivate"), 0);

        // manifest（宏生成的 manifest::Guest）
        let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.sdk-test");

        // storage 往返（WasmHost::storage_set/get 经组件 import）
        let result = plugin
            .invoke_command("test_storage", r#"{"key":"sdk-key","value":{"k":"v"}}"#)
            .expect("test_storage");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["got"]["k"], "v");

        // 主库往返（权限 + 表名前缀校验）
        let result = plugin.invoke_command("test_db", "{}").expect("test_db");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        let rows = r["rows"].as_array().expect("rows array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["val"], "sdk-db");

        // 配置读取（AppConfig 测试初始化 port=8765）
        let result = plugin.invoke_command("test_config", "{}").expect("test_config");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["port"], "8765");

        // 会话列表（权限 session:read，空列表）
        let result = plugin
            .invoke_command("test_session_list", "{}")
            .expect("test_session_list");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["sessions"], serde_json::json!([]));

        // 事件 emit（无头上下文幂等 Ok）
        let result = plugin.invoke_command("test_emit", "{}").expect("test_emit");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["emitted"], true);

        // 消息总线发布（同步投递）
        let result = plugin.invoke_command("test_bus", "{}").expect("test_bus");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["published"], true);

        // notify：无头上下文无 AppHandle，宿主错误经 WIT result 透传
        let result = plugin.invoke_command("test_notify", "{}").expect("test_notify");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"]
                .as_str()
                .map(|e| e.contains("headless") || e.contains("app_handle"))
                .unwrap_or(false),
            "unexpected notify error: {}",
            r["error"]
        );

        // v11 二进制回调（票据 06）：`wasm_entry!` 产物必须暴露可选导出
        // events-binary，宿主按 ItemName 路径语法动态探测命中且调用成功
        // （此前平名 `iface#func` 探测恒不命中，二进制回调实际从未接线）
        plugin
            .on_message_binary("binary-topic", "com.test.sender", b"\x00\xff\x01binary")
            .expect("SDK component must expose events-binary on_message_binary export");

        // 终端钩子（宏生成的 terminal_hooks::Guest，大写转换语义）
        assert_eq!(
            plugin.on_terminal_input("session-1", "sdk input").unwrap(),
            Some("SDK INPUT".to_string())
        );
    });
}

/// 插件互调端到端（issue 04，ADR-0017）：同一 sdk-test 组件以两个实例加载
/// —— caller（com.bedcode.api-caller）+ 目标（com.bedcode.sdk-test），
/// 覆盖：请求/响应配对成功、错误传播、超时（模拟无响应目标）、
/// 门禁拒绝（未声明 api）、停用注销后目标被拒。
///
/// 请求投递依赖 MessageBus 的 dispatcher 路由（生产 = PluginHost），
/// 本测试注入 TestInstanceDispatcher 把总线消息转发到共享实例。
#[test]

fn test_sdk_plugin_api_call_roundtrip() {
    const CALLER_ID: &str = "com.bedcode.api-caller";
    const TARGET_ID: &str = "com.bedcode.sdk-test";

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_sdk_test_component())
        .expect("compile SDK test component");

    // 登记目标插件声明的 api（等价 PluginHost::activate_plugin 的登记）
    host_ctx.api_registry().register(
        TARGET_ID,
        &[
            "com.bedcode.sdk-test.echo".to_string(),
            "com.bedcode.sdk-test.fail".to_string(),
        ],
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let target = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, TARGET_ID, host_ctx.clone(), &[], None)
                .expect("instantiate target"),
        ));
        let caller = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, CALLER_ID, host_ctx.clone(), &[], None)
                .expect("instantiate caller"),
        ));

        // 注入消息投递器（生产为 PluginHost）：总线消息 → 插件实例 on_message
        let instances = Arc::new(RwLock::new(HashMap::from([
            (TARGET_ID.to_string(), target.clone()),
            (CALLER_ID.to_string(), caller.clone()),
        ])));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
            .await;

        // 激活两实例：宏生成的 register() 订阅请求 topic（宿主订阅去重）
        target.lock().await.activate().expect("target activate");
        caller.lock().await.activate().expect("caller activate");

        // 请求/响应配对成功：caller 经 JSON-RPC 调目标 echo
        let result = caller
            .lock()
            .await
            .invoke_command("test_api_echo", r#"{"text":"hi"}"#)
            .expect("test_api_echo");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["echo"], "echo: hi", "got: {}", result);

        // 错误传播：目标方法返回 error → JSON-RPC error 对象 → 调用方报错
        let result = caller
            .lock()
            .await
            .invoke_command("test_api_fail", "{}")
            .expect("test_api_fail");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("boom")).unwrap_or(false),
            "fail error must propagate, got: {}",
            result
        );

        // 门禁拒绝：未声明的 api（ghost 不在注册表）在发布前被拒，不等待
        let result = caller
            .lock()
            .await
            .invoke_command("test_api_undeclared", "{}")
            .expect("test_api_undeclared");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
            "undeclared api must be rejected by gate, got: {}",
            result
        );

        // 超时：目标声明并订阅了 no-response topic（模拟构建期不可能出现的
        // 声明未实现场景），分派器不处理 → 不回复 → 调用方 800ms 超时
        host_ctx
            .api_registry()
            .register(TARGET_ID, &["com.bedcode.sdk-test.no-response".to_string()]);
        host_ctx
            .message_bus
            .subscribe_wasm(TARGET_ID, "bedcode.api.com.bedcode.sdk-test.no-response")
            .await;
        let result = caller
            .lock()
            .await
            .invoke_command("test_api_timeout", "{}")
            .expect("test_api_timeout");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("timeout")).unwrap_or(false),
            "no-reply target must time out, got: {}",
            result
        );

        // 停用注销：目标 api 从注册表移除后，调用被门禁拒绝（验收「未激活
        // 插件目标调用被拒」；注销由 PluginHost::deactivate_plugin 执行，
        // 此处等价手动注销）
        host_ctx.api_registry().unregister(TARGET_ID);
        let result = caller
            .lock()
            .await
            .invoke_command("test_api_echo", r#"{"text":"again"}"#)
            .expect("test_api_echo after unregister");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            r["error"].as_str().map(|e| e.contains("not declared")).unwrap_or(false),
            "unregistered target must be rejected, got: {}",
            result
        );
    });
}

/// v11 二进制总线端到端（票据 06 补齐 SDK 二进制发布/订阅缺口后覆盖）：
/// 同一 SDK 组件以两个实例加载——发布方经 SDK `bus_publish_binary` 发字节列，
/// 订阅方 activate 内以 SDK `bus_subscribe_binary` 声明二进制偏好；断言订阅方
/// `on_message_binary` 回调收到的 topic/sender/字节列与发布完全一致（含非
/// UTF-8）。覆盖「SDK 通道 → 总线格式过滤 → 宿主 dispatcher → guest
/// events-binary 回调」全链。
#[test]

fn test_sdk_plugin_binary_bus_roundtrip() {
    const PUB_ID: &str = "com.bedcode.bin-pub";
    const SUB_ID: &str = "com.bedcode.bin-sub";
    const TOPIC: &str = "sdk:binary-topic";

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_sdk_test_component())
        .expect("compile SDK test component");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let publisher = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, PUB_ID, host_ctx.clone(), &[], None)
                .expect("instantiate publisher"),
        ));
        let subscriber = Arc::new(Mutex::new(
            wasm_runtime
                .instantiate_component(&component, SUB_ID, host_ctx.clone(), &[], None)
                .expect("instantiate subscriber"),
        ));

        let instances = Arc::new(RwLock::new(HashMap::from([
            (PUB_ID.to_string(), publisher.clone()),
            (SUB_ID.to_string(), subscriber.clone()),
        ])));
        host_ctx
            .message_bus
            .set_dispatcher(Arc::new(TestInstanceDispatcher { instances }))
            .await;

        // 激活：SDK activate 内以二进制偏好订阅 TOPIC（订阅为异步投递，稍候生效）
        publisher.lock().await.activate().expect("publisher activate");
        subscriber.lock().await.activate().expect("subscriber activate");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 非 UTF-8 + 边界字节（0x00 / 0xff / 0x80）
        let payload: Vec<u8> = vec![0x00, 0xff, 0x80, b'b', b'i', b'n', 0x7f];
        let result = publisher
            .lock()
            .await
            .invoke_command(
                "test_binary_publish",
                &serde_json::json!({ "topic": TOPIC, "bytes": payload }).to_string(),
            )
            .expect("test_binary_publish");
        let r: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(r["published"], payload.len(), "publish result: {}", result);

        // 总线投递为异步：轮询订阅方记录直到命中（上限 2s）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let received = loop {
            let result = subscriber
                .lock()
                .await
                .invoke_command("test_binary_received", "{}")
                .expect("test_binary_received");
            let r: serde_json::Value = serde_json::from_str(&result).unwrap();
            if !r["received"].is_null() {
                break r["received"].clone();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "subscriber never received binary message, last: {}",
                result
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };

        assert_eq!(received["topic"], TOPIC, "got: {}", received);
        assert_eq!(received["sender"], PUB_ID, "got: {}", received);
        assert_eq!(
            received["bytes"],
            serde_json::json!(payload),
            "guest-received bytes must match published payload exactly, got: {}",
            received
        );
    });
}
