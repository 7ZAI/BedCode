//! Engine/AOT 缓存/燃料看门狗/monitor/trap 证据链（真实组件功能态）
//!
//! 自 wasm-core 迁入（票 04 绑定归位）——
//! `manager/runtime/common/tests/engine_limits.rs` 的夹具用例与
//! `engine_config.rs` 的 `customizer_overrides_unlocked_knobs_end_to_end`：
//! wasm-core 不再持有全 world 绑定后，其测试二进制无法实例化 SDK 夹具（夹具静态
//! import 生态含桌面扩展接口）。**形态无关的纯机制用例留在 wasm-core**（引擎配置
//! 逐项映射 / 校验 / 锁结构 / block_on_async 重入），需要真实组件的功能态用例迁来
//! 本文件。
//!
//! 迁移改造点（用例本体逐字保留）：
//! - 共享脚手架 `mod tests` → `mod support`；夹具读取改为只读
//!   `sdk_fixture_artifact_bytes("sdk")`（产物由 wasm-core fixture_keeper 构建）；
//! - `plugin.raw_store()` / `aot_cache_key` / `runtime.engine()` 是内核私有
//!   （pub(crate)）机制——经 `test_support` 的测试转发面（`plugin_raw_store` /
//!   `aot_cache_key` / `runtime_engine`）取用，不放宽内核生产 API；
//! - 导出直调用的绑定从内核全 world 绑定改为**宿主自有绑定**
//!   `bedcode_desktop_lib::plugin::bindings::Plugin`（与 `guest_events.rs` 同一份）。

mod support;

use std::sync::Arc;

use bedcode_desktop_lib::wasm_core::config::{plugin_debug_mode, CoreConfig};
use bedcode_desktop_lib::wasm_core::host_api::context::StorageScope;
use bedcode_desktop_lib::wasm_core::manifest::ResourceOverrides;
use bedcode_desktop_lib::wasm_core::manager::runtime::EngineSetup;
use bedcode_desktop_lib::wasm_core::runtime_util::block_on_async;
use bedcode_host_kit::limits::StoreLimits;
use support::capture::{capture, CapturedEvent};
use support::*;
use tracing::Level;
use wasmtime::ResourceLimiter;

/// 直连运行时的夹具属主 id（`setup_wasm_runtime` 的权限授予面固定按它装配；
/// storage 往返与私有库用例必须用它——见 `component_e2e.rs` 同名常量注释）
const TEST_PLUGIN_ID: &str = "com.bedcode.test";

/// 加载入口：load_plugin_from_file 直接走组件路径（阶段 C 起仅组件形态）
#[test]

fn test_load_plugin_from_file() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let temp_dir = std::env::temp_dir().join(format!("bedcode_component_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wasm_path = temp_dir.join("plugin.wasm");
    std::fs::write(&wasm_path, sdk_fixture_artifact_bytes("sdk")).unwrap();

    let mut plugin = wasm_runtime
        .load_plugin_from_file(&wasm_path, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("load_plugin_from_file should load component");
    // 加载成功即可调用：激活 + manifest 往返验证组件路径
    assert_eq!(plugin.activate().expect("activate"), 0);
    let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(manifest["id"], "com.bedcode.sdk-test");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 组件 AOT 缓存：产物写入、缓存命中、两次实例化等价
#[test]

fn test_compile_component_from_file_aot_cache() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();

    let temp_dir = std::env::temp_dir().join(format!("bedcode_component_aot_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wasm_path = temp_dir.join("test_component.wasm");
    // 组件缓存文件名带 c 前缀（与 core module 产物区分）
    let wasm_bytes = sdk_fixture_artifact_bytes("sdk");
    let cache_path = std::env::temp_dir()
        .join(format!("bedcode_aot_{}", std::process::id()))
        .join(format!(
            "c{:016x}.cwasm",
            aot_cache_key(&wasm_path, wasm_bytes.len() as u64)
        ));
    std::fs::write(&wasm_path, &wasm_bytes).unwrap();

    // 首次编译：生成缓存产物
    let component = wasm_runtime
        .compile_component_from_file(&wasm_path)
        .expect("first compile should succeed");
    assert!(cache_path.exists(), "component AOT cache file should be written");

    // 再次加载：命中缓存（产物不被重写，mtime 不变）——重编译路径会重写产物
    let cache_mtime_before = std::fs::metadata(&cache_path).unwrap().modified().unwrap();
    let cached = wasm_runtime
        .compile_component_from_file(&wasm_path)
        .expect("cached load should succeed");
    let cache_mtime_after = std::fs::metadata(&cache_path).unwrap().modified().unwrap();
    assert_eq!(
        cache_mtime_before, cache_mtime_after,
        "cache hit should not rewrite artifact"
    );

    for c in [component, cached] {
        wasm_runtime
            .instantiate_component(&c, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("component from cache should instantiate");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 组件 AOT 缓存：产物损坏时反序列化失败并回退到完整编译
///
/// 对应 core 路径的 `recompiles_on_stale` 测试；组件缓存文件名带 c 前缀
#[test]

fn test_compile_component_from_file_recompiles_on_stale() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();

    let temp_dir = std::env::temp_dir().join(format!("bedcode_component_aot_stale_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wasm_path = temp_dir.join("test_component.wasm");
    let cache_path = std::env::temp_dir()
        .join(format!("bedcode_aot_{}", std::process::id()))
        .join(format!(
            "c{:016x}.cwasm",
            aot_cache_key(&wasm_path, sdk_fixture_artifact_bytes("sdk").len() as u64)
        ));
    std::fs::write(&wasm_path, sdk_fixture_artifact_bytes("sdk")).unwrap();

    // 首次编译生成缓存
    wasm_runtime
        .compile_component_from_file(&wasm_path)
        .expect("first compile should succeed");

    // 篡改缓存为无效字节：deserialize 应失败并回退到完整编译
    std::fs::write(&cache_path, b"not a valid cwasm").unwrap();
    let component = wasm_runtime
        .compile_component_from_file(&wasm_path)
        .expect("invalid cache should fall back to full compile");
    wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("component from full compile should instantiate");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 燃料看门狗：guest 执行必须消耗燃料（组件形态下 fuel 生效），
/// 且每次导出调用前重置预算（预算不跨调用累积）
#[test]

fn test_component_fuel_watchdog() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    // 调用前剩余燃料 ≈ 单次预算（实例化/ABI 校验的消耗可忽略）
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let before = {
            let (store, _) = plugin_raw_store(&mut plugin);
            store.get_fuel().expect("get fuel")
        };
        plugin
            .invoke_command("test.echo", r#"{"hello":"x"}"#)
            .expect("invoke_command");
        let after = {
            let (store, _) = plugin_raw_store(&mut plugin);
            store.get_fuel().expect("get fuel")
        };
        assert!(
            after < before,
            "guest execution must consume fuel (before={}, after={})",
            before,
            after
        );

        // 预算重置：人为耗尽燃料后再调用——exports() 必须自动续费使其成功
        {
            let (store, _) = plugin_raw_store(&mut plugin);
            store.set_fuel(1000).expect("drain fuel");
        }
        plugin
            .invoke_command("test.echo", r#"{"hello":"z"}"#)
            .expect("refueled invoke must succeed");
        let after2 = {
            let (store, _) = plugin_raw_store(&mut plugin);
            store.get_fuel().expect("get fuel")
        };
        assert!(
            after2 > StoreLimits::default().fuel_per_call / 2,
            "fuel must be refilled per export call, got {}",
            after2
        );
    });
}

/// 燃料看门狗覆盖面：绕过 `exports()` 的三条可选导出回调也必须自带续费
///
/// 缺陷背景（2026-09-29 审查）：`on_message_binary` / `on_ws_frame` /
/// `on_task_event` 直接持探测句柄 `call_async`，不经 `exports()`，因而漏掉
/// `refill_call_fuel()`。后果是这三条回调的燃料**跳调用累积**：长会话下
/// （file-transfer 收 WS 帧、宿主任务事件密集）耗尽 → `all fuel consumed`
/// trap → Store 中毒 → 限频自动重载，且 `fuel_consumed_total` 少计。
/// `event-loop` 调用模型经 `start_op` → `refill_fuel` 覆盖全部 op，不受影响；
/// 本组用例锁的是默认 `mutex` 模型这条生产路径。
#[test]
fn test_optional_export_callbacks_refill_fuel() {
    use bedcode_desktop_lib::wasm_core::bus::WsFrameDispatch;
    use bedcode_desktop_lib::wasm_core::manager::runtime::LoadedWasmPlugin as Plugin;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let budget = StoreLimits::default().fuel_per_call;
    let client_frame = WsFrameDispatch::Client {
        handle: "wsc-fuel".to_string(),
        kind: "text".to_string(),
        payload: b"fuel probe".to_vec(),
    };

    // 排干到远低于单次预算：若回调不续费，调用后剩余仍是这个量级
    fn drain(plugin: &mut Plugin) {
        let (store, _) = plugin_raw_store(plugin);
        store.set_fuel(1_000_000).expect("drain fuel");
    }
    fn remaining(plugin: &mut Plugin) -> u64 {
        let (store, _) = plugin_raw_store(plugin);
        store.get_fuel().expect("get fuel")
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // C-102 / C-104 / C-105：SDK 产物同时导出 events-ws 与 events-binary
        let sdk_component = wasm_runtime
            .compile_component(&sdk_fixture_artifact_bytes("sdk"))
            .expect("compile SDK test component");
        let mut sdk_plugin = wasm_runtime
            .instantiate_component(&sdk_component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("instantiate SDK component");

        // C-105 边界：连续三轮，每轮都排干后调用——杀「只在首次续费」的变异
        for round in 1..=3u32 {
            drain(&mut sdk_plugin);
            let ws_delivered = sdk_plugin.on_ws_frame(&client_frame).expect("ws frame deliver");
            assert!(ws_delivered, "round {round}: SDK 产物必须导出 events-ws");
            let left = remaining(&mut sdk_plugin);
            assert!(
                left > budget / 2,
                "round {round}: on_ws_frame 未续费，剩余 {left}（预算 {budget}）"
            );

            drain(&mut sdk_plugin);
            sdk_plugin
                .on_message_binary("fuel-topic", "com.test.sender", b"\x00\xff\x01bin")
                .expect("binary frame deliver");
            let left = remaining(&mut sdk_plugin);
            assert!(
                left > budget / 2,
                "round {round}: on_message_binary 未续费，剩余 {left}（预算 {budget}）"
            );
        }

        // C-103：events-task 导出改由 SDK 夹具承载（票 02 批次 05）——host-task 的
        // WIT impl 迁宿主后内核测试二进制不再注册该 import，task 夹具无法在内核
        // 实例化；SDK 的 `wasm_entry!` 无条件导出全部 interface（含 events-task 默认
        // 空实现），燃料续费探针只需「产物导出 events-task」这一性质，与夹具语义无关。
        let sdk_component = wasm_runtime
            .compile_component(&sdk_fixture_artifact_bytes("sdk"))
            .expect("compile SDK test component");
        let mut sdk_plugin = wasm_runtime
            .instantiate_component(&sdk_component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
            .expect("instantiate SDK component");
        drain(&mut sdk_plugin);
        let task_delivered = sdk_plugin
            .on_task_event(r#"{"jobId":"fuel","phase":"completed"}"#)
            .expect("task event deliver");
        assert!(task_delivered, "SDK 产物必须导出 events-task");
        let left = remaining(&mut sdk_plugin);
        assert!(left > budget / 2, "on_task_event 未续费，剩余 {left}（预算 {budget}）");
        // C-106 反例（未导出 events-ws 的产物走降级路径——加续费不得改变其语义）
        // 随 `plugin-component-test` 删除而移除：SDK 的 `wasm_entry!` 无条件导出全部
        // interface，造不出缺可选导出的产物，故「补续费不得改变降级语义」这条反例
        // 目前无载体可测。
    });
}

/// 票据 02 验收：运行时覆盖生效——覆盖后新建立的 Store 按新上限运行；
/// 资源限制器行为等价：超限增长被拒绝，限额来自配置
#[test]

fn test_runtime_config_override_applies_to_new_stores() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");

    // 覆盖：收紧内存上限到 4MiB（须高于测试组件最小内存 17 页 ≈ 1.1MiB）
    let mut cfg = CoreConfig::default();
    cfg.store.max_memory_bytes = 4 * 1024 * 1024;
    wasm_runtime.set_config(cfg).expect("set config");

    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate under overridden limits");
    let (store, _) = plugin_raw_store(&mut plugin);
    // 限额内允许、超限拒绝——限额来自运行时覆盖的配置
    assert!(store
        .data_mut()
        .memory_growing(0, 3 * 1024 * 1024, None)
        .expect("within limit"));
    assert!(!store
        .data_mut()
        .memory_growing(0, 5 * 1024 * 1024, None)
        .expect("over limit"));
    assert_eq!(store.data().limits.max_memory_bytes, 4 * 1024 * 1024);
}

/// 票据 07 验收：manifest 资源覆盖经安全模块仲裁后作用于新建 Store——
/// 收紧请求生效（限额来自仲裁结果）、放宽请求钳回内核配置
#[test]

fn test_plugin_resource_overrides_apply_to_new_stores() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let base = StoreLimits::default();

    // 收紧请求：内存上限 4MiB（须高于测试组件最小内存 17 页 ≈ 1.1MiB）→ 生效
    let tightened = ResourceOverrides {
        max_memory_bytes: Some(4 * 1024 * 1024),
        ..ResourceOverrides::default()
    };
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], Some(&tightened))
        .expect("instantiate with tightened limits");
    {
        let (store, _) = plugin_raw_store(&mut plugin);
        assert_eq!(
            store.data().limits.max_memory_bytes,
            4 * 1024 * 1024,
            "Store 限额必须来自仲裁后的插件覆盖值"
        );
        // 限额内允许、超限拒绝——限额来自仲裁结果
        assert!(store
            .data_mut()
            .memory_growing(0, 3 * 1024 * 1024, None)
            .expect("within limit"));
        assert!(!store
            .data_mut()
            .memory_growing(0, 5 * 1024 * 1024, None)
            .expect("over limit"));
    }

    // 放宽请求：钳回内核配置（插件不得借 manifest 突破运维上限）
    let relaxed = ResourceOverrides {
        max_memory_bytes: Some(base.max_memory_bytes * 2),
        ..ResourceOverrides::default()
    };
    let mut relaxed_plugin = wasm_runtime
        .instantiate_component(&component, "com.bedcode.relaxed", host_ctx, &[], Some(&relaxed))
        .expect("instantiate with relaxed limits");
    let (store, _) = plugin_raw_store(&mut relaxed_plugin);
    assert_eq!(
        store.data().limits.max_memory_bytes,
        base.max_memory_bytes,
        "放宽请求必须被钳回内核配置值"
    );
}

/// 票据 03 验收：调用聚合（次数/燃料/耗时）+ 生命周期事件计数 + 内存记账
#[test]

fn test_monitor_metrics_end_to_end() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");
    let monitor = wasm_runtime.monitor();

    plugin.activate().expect("activate");
    plugin.invoke_command("test.echo", r#"{"n":1}"#).expect("invoke 1");
    plugin.invoke_command("test.echo", r#"{"n":2}"#).expect("invoke 2");

    // 内存记账：limiter 批准路径写入当前值/峰值（在真实增长之后注入，
    // 避免后续真实增长把 current 刷回实际值——真实内存 ~1.1MiB < 2MiB）
    {
        let (store, _) = plugin_raw_store(&mut plugin);
        store
            .data_mut()
            .memory_growing(0, 2 * 1024 * 1024, None)
            .expect("within limit");
    }

    let snap = monitor.snapshot();
    let m = &snap["plugins"][TEST_PLUGIN_ID];
    assert_eq!(m["lifecycle"]["instantiate"].as_u64().unwrap(), 1);
    assert_eq!(m["lifecycle"]["activate_ok"].as_u64().unwrap(), 1);
    assert_eq!(
        m["calls_total"].as_u64().unwrap(),
        3,
        "activate + 2×invoke = 3 次导出调用"
    );
    assert!(m["fuel_consumed_total"].as_u64().unwrap() > 0, "燃料消耗必须有记录");
    assert_eq!(
        m["call_duration_buckets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_u64().unwrap())
            .sum::<u64>(),
        3,
        "直方图桶计数必须等于调用数"
    );
    assert_eq!(m["memory_current_bytes"].as_u64().unwrap(), 2 * 1024 * 1024);
    assert_eq!(m["memory_peak_bytes"].as_u64().unwrap(), 2 * 1024 * 1024);
}

/// 燃料耗尽必须 trap：绕过 exports() 的自动续费，直接以小预算调用导出
#[test]

fn test_component_fuel_exhaustion_traps() {
    use bedcode_desktop_lib::plugin::bindings::Plugin;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    let (store, instance) = plugin_raw_store(&mut plugin);
    store.set_fuel(1).expect("set tiny fuel");
    let binding = Plugin::new(&mut *store, instance).expect("bind exports");
    // 票 02：导出绑定全部 async（bindgen `default: async`），燃料耗尽 trap 经
    // call_async 在 async 语义下生效（/tmp/wasip3-probe 场景 3 实证）
    let result = block_on_async(async {
        binding
            .bedcode_plugin_command()
            .call_invoke(store, "test.echo", r#"{"a":1}"#)
            .await
    });
    assert!(result.is_err(), "fuel exhausted must trap: {:?}", result);
}

/// ticket 01（wasm backtrace）：trap 错误串携带 WASM 内部函数调用栈
///
/// 显式 panic 走生产 Engine（WasmRuntime::new 已开 wasm_backtrace_max_frames
/// 32 帧）——错误串必须含 `wasm backtrace:` 且栈穿透到业务函数 invoke
/// （names section，release 构建即有），AI agent 无需重跑即可从错误串
/// 定位插件内部故障点
#[test]

fn test_component_trap_error_includes_wasm_backtrace() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(async {
        plugin
            .invoke_command("test.panic", "{}")
            .expect_err("panic must trap")
            .to_string()
    });
    assert!(
        err.contains("wasm backtrace:"),
        "trap error must include wasm backtrace marker, got: {}",
        err
    );
    // 栈内含插件业务函数名（invoke 是 command 导出实现），证明函数级可读
    assert!(
        err.contains("invoke"),
        "trap backtrace must include plugin function name, got: {}",
        err
    );
}

/// 回归：开启 backtrace 不改变正常调用行为（非 trap 路径零影响）
#[test]

fn test_component_backtrace_enabled_normal_calls_unaffected() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let echo = rt.block_on(async {
        plugin
            .invoke_command("test.echo", r#"{"hello":"backtrace-on"}"#)
            .expect("normal invoke must succeed with backtrace enabled")
    });
    assert!(echo.contains("backtrace-on"), "got: {}", echo);
    // 正常返回的 JSON 载荷不应混入 backtrace 文本（非 trap 路径零影响）
    assert!(!echo.contains("wasm backtrace:"), "got: {}", echo);
}

/// ticket 03（调试模式端到端冒烟）：后台日志行号栈
///
/// 仅当宿主持有 `BEDCODE_PLUGIN_DEBUG=1` 时有效（此时构建链路以 debug
/// profile 产出带 DWARF 的测试组件，WasmRuntime 也已置 WASMTIME_BACKTRACE_DETAILS
/// 开行号解析）——断言 trap 错误串含 `file:line` 行号而非仅函数名。
/// 未设该开关的正常测试环境自动跳过（SKIP 输出，不失败）
#[test]

fn test_debug_mode_trap_includes_line_info() {
    if !plugin_debug_mode() {
        eprintln!("SKIP: BEDCODE_PLUGIN_DEBUG 未设置，跳过行号冒烟（调试模式是手工开关）");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile debug test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate debug test component");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt.block_on(async {
        plugin
            .invoke_command("test.panic", "{}")
            .expect_err("panic must trap")
            .to_string()
    });
    assert!(
        err.contains(".rs:"),
        "debug backtrace should include file:line symbols, got: {}",
        err
    );
}

/// ticket 02（trap 宿主日志）：trap 时宿主侧产生含 plugin_id 的 error 级记录
///
/// 即使调用方静默忽略返回错误，崩溃证据也经 tracing error 落盘；
/// trap 详情（含 wasm backtrace）作为结构化字段随日志携带
#[test]

fn test_component_trap_emits_host_error_log() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    let captured = capture(|| {
        // 通过 invoke_command 触发显式 panic（确定性 trap，栈穿透到 invoke）。
        // 断言 trap 本身成立：把「无 trap」与「有 trap 但日志缺失」两种失败分开
        // （后者才是本用例要抓的断链；前者说明夹具/引擎语义变了）
        let err = plugin
            .invoke_command("test.panic", "{}")
            .expect_err("test.panic 必须 trap");
        assert!(
            err.to_string().contains("wasm backtrace:"),
            "trap 错误串必须带 wasm backtrace: {err}"
        );
    });

    let errors: Vec<&CapturedEvent> = captured.iter().filter(|e| e.level == Level::ERROR).collect();
    assert!(
        !errors.is_empty(),
        "trap must emit host error log, captured: {:?}",
        captured
    );
    let host_error = errors[0];
    let fields: std::collections::HashMap<&str, &str> = host_error
        .fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(fields.get("plugin_id"), Some(&TEST_PLUGIN_ID), "got: {:?}", fields);
    assert_eq!(fields.get("export"), Some(&"invoke_command"));
    assert!(
        fields
            .get("trap")
            .map(|t| t.contains("wasm backtrace:"))
            .unwrap_or(false),
        "trap field should carry wasm backtrace, got: {:?}",
        fields
    );
}

/// ticket 02：guest 自报失败（内层 Err）不升级为宿主 error
///
/// 双层 Result 语义：Ok(Err(msg)) 是插件自己报告的失败，按既有级别（warn/返回）
/// 记录，不产生宿主 error 日志——只有真 trap（外层 Err）才走 error 证据路径
#[test]

fn test_component_guest_self_reported_failure_no_host_error() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");

    let captured = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // 预写 storage key：guest on_startup 读到后返回 Err（见测试插件实现）
            host_ctx
                .storage()
                .set(TEST_PLUGIN_ID, "component-test-fail-startup", serde_json::json!("x"))
                .await
                .expect("preset failing-startup key");
            let mut plugin = wasm_runtime
                .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
                .expect("instantiate test component");
            capture(|| {
                let result = plugin.on_startup();
                assert!(
                    matches!(result, Ok(Err(_))),
                    "guest should self-report startup failure, got: {:?}",
                    result
                );
            })
        })
    };

    assert!(
        !captured.iter().any(|e| e.level == Level::ERROR),
        "guest self-reported failure must not emit host error, captured: {:?}",
        captured
    );
}

/// trap 后 Store 被污染：同一实例后续调用持续报 `cannot enter component instance`
/// （wasmtime 同步引擎 `set_trapped` 语义，宿主 trap 自动重载机制的立论依据）
#[test]

fn test_component_trap_poisons_store_and_reinstantiate_recovers() {
    use bedcode_desktop_lib::plugin::bindings::Plugin;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("compile test component");

    // 1. 实例 A：制造一次 trap（燃料耗尽）
    let mut plugin_a = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
        .expect("instantiate component A");
    {
        let (store, instance) = plugin_raw_store(&mut plugin_a);
        store.set_fuel(1).expect("set tiny fuel");
        let binding = Plugin::new(&mut *store, instance).expect("bind exports");
        let result = block_on_async(async {
            binding
                .bedcode_plugin_command()
                .call_invoke(store, "test.echo", r#"{"a":1}"#)
                .await
        });
        assert!(result.is_err(), "fuel exhausted must trap: {:?}", result);
    }

    // 2. 同一实例再次调用：必须持续失败且报 cannot enter component instance
    //    （不能自愈 —— 这正是宿主必须整体重载的原因）
    let err = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            plugin_a
                .invoke_command("test.echo", r#"{"a":2}"#)
                .expect_err("poisoned store must keep failing")
        })
    };
    assert!(
        err.to_string().contains("cannot enter component instance"),
        "poisoned store error should be CannotEnterComponent, got: {}",
        err
    );

    // 3. 重新实例化（等价宿主 reload_wasm_plugin 的重建）→ 新实例正常可用
    let mut plugin_b = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("re-instantiate after trap");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let echo = rt.block_on(async {
        plugin_b
            .invoke_command("test.echo", r#"{"hello":"recovered"}"#)
            .expect("fresh instance must work")
    });
    assert!(echo.contains("recovered"), "got: {}", echo);
}

// ==================== 引擎定制面：逃生舱端到端（自 engine_config.rs 迁入） ====================

/// 逃生舱能覆盖**非锁定**项，且在真实 trap 上生效
///
/// 三层断言：
/// 1. getter 直读：钩子关掉 backtrace / 并行编译，逐字反映在 `Engine::config()`；
/// 2. 端到端：真实 trap 错误串不再携带 `wasm backtrace:` 段；
/// 3. **对照组**：不装钩子的运行时必须仍带 backtrace——否则本用例在「trap 错误串
///    形状变了」时也会绿（backtrace 是诊断地基：trap 错误串是插件 Degraded 与
///    AI agent 定位故障的唯一证据，故不在锁定项内）。
#[test]
fn customizer_overrides_unlocked_knobs_end_to_end() {
    // 对照组：默认运行时（无钩子）必须带 backtrace
    let default_err = trap_error_with_setup(EngineSetup::new(CoreConfig::default()));
    assert!(
        default_err.contains("wasm backtrace:"),
        "对照组：默认运行时 trap 必须带 wasm backtrace，否则本用例失去对照意义: {default_err}"
    );
    assert!(
        runtime_engine(&setup_wasm_runtime_with_setup(EngineSetup::new(CoreConfig::default())).0)
            .get_parallel_compilation(),
        "对照组：默认并行编译开启（wasmtime 默认）"
    );

    // 实验组：钩子关掉 backtrace 与并行编译（配置面语义上「诊断地基 / 编译并发」可调）
    // 用 `wasm_backtrace_max_frames(None)` 而非已废弃的 `wasm_backtrace(false)`
    // （上游文档：二者等价，None = trap 错误串不再附带 wasm backtrace）
    let hooked = setup_wasm_runtime_with_setup(
        EngineSetup::new(CoreConfig::default()).with_customizer(|config| {
            config.wasm_backtrace_max_frames(None);
            config.parallel_compilation(false);
            Ok(())
        }),
    )
    .0;
    assert!(
        !runtime_engine(&hooked).get_parallel_compilation(),
        "逃生舱必须能覆盖非锁定项（parallel_compilation）"
    );
    let hooked_err = trap_error_with_setup(
        EngineSetup::new(CoreConfig::default()).with_customizer(|config| {
            config.wasm_backtrace_max_frames(None);
            config.parallel_compilation(false);
            Ok(())
        }),
    );
    assert!(
        !hooked_err.contains("wasm backtrace:"),
        "逃生舱必须能覆盖非锁定项（wasm_backtrace），且在真实 trap 上生效: {hooked_err}"
    );
}

/// 触发确定性 trap（`test.panic`）并取回错误串——trap 串是引擎诊断参数的唯一载体
fn trap_error_with_setup(setup: EngineSetup) -> String {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime_with_setup(setup);
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("sdk"))
        .expect("编译测试组件");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("实例化测试组件");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        plugin
            .invoke_command("test.panic", "{}")
            .expect_err("panic 必须 trap")
            .to_string()
    })
}
