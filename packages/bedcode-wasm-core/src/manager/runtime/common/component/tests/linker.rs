//! add_to_linker 注册闭环 + 组件完整往返（component.rs 直测）
//!
//! 自 `common/component.rs` 的 `mod tests` 拆出（共享脚手架在父 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;

/// 内核本地表里逐行列举的 import 接口全部注册成功（add_to_linker 是纯接线代码，
/// 任何一组接口名冲突/接线参数错误都会在此失败）。含 ADR 0036 归位的
/// `host-storage` / `host-database` / `host-plugin-database` 三组。
#[test]
fn test_add_to_linker_registers_all_interfaces() {
    let engine = test_engine();
    let mut linker = Linker::new(&engine);
    add_to_linker(&mut linker).expect("register all host interfaces");
}

/// 重复注册同一组接口必须报错
///
/// 防止 add_to_linker 被调用两次时静默覆盖接线（实例化时会以
/// 意外行为失败，不如注册期直接暴露）
#[test]
fn test_add_to_linker_rejects_duplicate_registration() {
    let engine = test_engine();
    let mut linker = Linker::new(&engine);
    add_to_linker(&mut linker).expect("first registration");

    let err = add_to_linker(&mut linker).expect_err("duplicate registration should fail");
    // wasmtime 对已注册的同名 interface instance 报 "defined twice"
    assert!(
        err.to_string().contains("defined twice"),
        "unexpected duplicate registration error: {}",
        err
    );
}

/// 组件完整往返（component.rs 直测，不经 WasmRuntime）：
/// 实例化 + ABI 协商 + 燃料注入 + 生命周期 + 命令（guest 内 import 往返）
/// + manifest + 终端钩子
#[test]
fn test_loaded_plugin_component_roundtrip() {
    let engine = test_engine();
    let component =
        Component::from_binary(&engine, &build_test_component()).expect("compile test component");
    let mut linker = Linker::new(&engine);
    add_to_linker(&mut linker).expect("register host interfaces");

    let host_ctx = build_host_ctx();
    // 授予 guest 往返所需权限（与 wasm_runtime.rs setup_wasm_runtime 同组）
    grant_permissions(
        &host_ctx,
        TEST_PLUGIN_ID,
        &[
            "storage",
            "broadcast",
            "terminal:input",
            "terminal:output",
            "session:read",
        ],
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    // 组件内 import 调用经 block_on_async 走 tokio，测试体整体在运行时上下文中执行
    rt.block_on(async {
        // 预写 storage key：验证 guest 内 host_storage import 读回（JSON 值往返）
        host_ctx
            .storage
            .set(
                TEST_PLUGIN_ID,
                "component-test-key",
                serde_json::json!({"k": "v"}),
            )
            .await
            .expect("preset storage key");

        let mut plugin = LoadedWasmPlugin::new(
            &engine,
            &linker,
            &component,
            TEST_PLUGIN_ID,
            host_ctx,
            &[],
            StoreSpec {
                limits: StoreLimits::default(),
                fuel_enabled: true,
                metrics: crate::monitor::MetricsRegistry::new().plugin(TEST_PLUGIN_ID),
            },
        )
        .expect("instantiate component");

        // 生命周期（new 内已隐式通过 verify_abi：form=1 且 version<=ABI_VERSION）
        assert_eq!(plugin.activate().expect("activate"), 0);

        // 燃料注入生效：activate 内含宿主 import 调用，guest 必然有燃料消耗
        // （组件.rs 的 new/exports 每次调用前 set_fuel 重置预算，此处只验证注入链路）
        {
            let (store, _) = plugin.raw_store();
            let remaining = store.get_fuel().expect("get fuel");
            assert!(
                remaining < StoreLimits::default().fuel_budget(),
                "activate must consume fuel, remaining={}",
                remaining
            );
        }

        // manifest（guest 静态导出）
        let manifest: serde_json::Value =
            serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.sdk-test");

        // 命令调用：guest 内 host_storage.get 读回预写值（跨边界往返）
        let result = plugin
            .invoke_command("test.echo", r#"{"hello":"component"}"#)
            .expect("invoke_command");
        let result_json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result_json["name"], "test.echo");
        assert_eq!(result_json["stored"]["k"], "v");

        // 票 10：终端钩子导出的调用断言已删（`terminal-hooks` interface 退役）——
        // 本用例仍覆盖「加载 → 命令调用 → 跨边界 storage 往返 → 停用」全链路

        assert_eq!(plugin.deactivate().expect("deactivate"), 0);
    });
}
