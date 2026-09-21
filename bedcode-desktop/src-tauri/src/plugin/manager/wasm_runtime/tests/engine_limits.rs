//! Engine/AOT 缓存/燃料看门狗/monitor/重入
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
/// 加载入口：load_plugin_from_file 直接走组件路径（阶段 C 起仅组件形态）
#[test]

fn test_load_plugin_from_file() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let temp_dir = std::env::temp_dir().join(format!("bedcode_component_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wasm_path = temp_dir.join("plugin.wasm");
    std::fs::write(&wasm_path, build_test_component()).unwrap();

    let mut plugin = wasm_runtime
        .load_plugin_from_file(&wasm_path, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("load_plugin_from_file should load component");
    // 加载成功即可调用：激活 + manifest 往返验证组件路径
    assert_eq!(plugin.activate().expect("activate"), 0);
    let manifest: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(manifest["id"], "com.bedcode.component-test");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// 缓存 key：源码大小变化必须换 key（防解压器保留旧 mtime 时误加载旧产物）
#[test]

fn test_aot_cache_key_factors_source_size() {
    let path = std::path::Path::new("plugin.wasm");
    assert_eq!(aot_cache_key(path, 100), aot_cache_key(path, 100));
    assert_ne!(aot_cache_key(path, 100), aot_cache_key(path, 200));
}

/// 组件 AOT 缓存：产物写入、缓存命中、两次实例化等价
#[test]

fn test_compile_component_from_file_aot_cache() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();

    let temp_dir = std::env::temp_dir().join(format!("bedcode_component_aot_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wasm_path = temp_dir.join("test_component.wasm");
    // 组件缓存文件名带 c 前缀（与 core module 产物区分）
    let wasm_bytes = build_test_component();
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
            aot_cache_key(&wasm_path, build_test_component().len() as u64)
        ));
    std::fs::write(&wasm_path, build_test_component()).unwrap();

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
        .compile_component(&build_test_component())
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    // 调用前剩余燃料 ≈ 单次预算（实例化/ABI 校验的消耗可忽略）
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let before = {
            let (store, _) = plugin.raw_store();
            store.get_fuel().expect("get fuel")
        };
        plugin
            .invoke_command("test.echo", r#"{"hello":"x"}"#)
            .expect("invoke_command");
        let after = {
            let (store, _) = plugin.raw_store();
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
            let (store, _) = plugin.raw_store();
            store.set_fuel(1000).expect("drain fuel");
        }
        plugin
            .invoke_command("test.echo", r#"{"hello":"z"}"#)
            .expect("refueled invoke must succeed");
        let after2 = {
            let (store, _) = plugin.raw_store();
            store.get_fuel().expect("get fuel")
        };
        assert!(
            after2 > StoreLimits::default().fuel_per_call / 2,
            "fuel must be refilled per export call, got {}",
            after2
        );
    });
}

/// 票据 02 验收：运行时覆盖生效——覆盖后新建立的 Store 按新上限运行；
/// 资源限制器行为等价：超限增长被拒绝，限额来自配置
#[test]

fn test_runtime_config_override_applies_to_new_stores() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");

    // 覆盖：收紧内存上限到 4MiB（须高于测试组件最小内存 17 页 ≈ 1.1MiB）
    let mut cfg = CoreConfig::default();
    cfg.store.max_memory_bytes = 4 * 1024 * 1024;
    wasm_runtime.set_config(cfg).expect("set config");

    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate under overridden limits");
    let (store, _) = plugin.raw_store();
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

/// 票据 02 验收：非法配置（燃料为 0）被 set_config 拒绝且带上下文；旧配置不受影响
#[test]

fn test_set_config_rejects_invalid() {
    let (wasm_runtime, _host_ctx) = setup_wasm_runtime();
    let mut cfg = CoreConfig::default();
    cfg.store.fuel_per_call = 0;
    let err = wasm_runtime.set_config(cfg).expect_err("zero fuel must be rejected");
    assert!(format!("{err}").contains("fuel_per_call"));
    assert_eq!(wasm_runtime.config(), CoreConfig::default());
}

/// 票据 07 验收：manifest 资源覆盖经安全模块仲裁后作用于新建 Store——
/// 收紧请求生效（限额来自仲裁结果）、放宽请求钳回内核配置
#[test]

fn test_plugin_resource_overrides_apply_to_new_stores() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
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
        let (store, _) = plugin.raw_store();
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
    let (store, _) = relaxed_plugin.raw_store();
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
        .compile_component(&build_test_component())
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
        let (store, _) = plugin.raw_store();
        use wasmtime::ResourceLimiter;
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
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    let (store, instance) = plugin.raw_store();
    store.set_fuel(1).expect("set tiny fuel");
    let binding =
        crate::plugin::manager::wasm_runtime::component::Plugin::new(&mut *store, instance).expect("bind exports");
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
        .compile_component(&build_test_component())
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
        .compile_component(&build_test_component())
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
        .compile_component(&build_test_component())
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
    use crate::plugin::manager::wasm_runtime::host_impl::log::capture::{capture, CapturedEvent};
    use tracing::Level;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate test component");

    let captured = capture(|| {
        // 通过 invoke_command 触发显式 panic（确定性 trap，栈穿透到 invoke）
        let _ = plugin.invoke_command("test.panic", "{}");
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
    use crate::plugin::manager::wasm_runtime::host_impl::log::capture::{capture, CapturedEvent};
    use tracing::Level;

    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");

    let captured = {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // 预写 storage key：guest on_startup 读到后返回 Err（见测试插件实现）
            host_ctx
                .storage
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
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile test component");

    // 1. 实例 A：制造一次 trap（燃料耗尽）
    let mut plugin_a = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx.clone(), &[], None)
        .expect("instantiate component A");
    {
        let (store, instance) = plugin_a.raw_store();
        store.set_fuel(1).expect("set tiny fuel");
        let binding =
            crate::plugin::manager::wasm_runtime::component::Plugin::new(&mut *store, instance).expect("bind exports");
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

#[test]

fn block_on_async_reentrant_nested_call_no_panic() {
    // 回归（panic.log 实证 wasm_runtime.rs:82 FATAL）：
    // 插件分发路径 dispatch_*_to_plugin → block_on_async（block_in_place +
    // handle.block_on）包着插件调用，插件回调里的宿主函数（session_get /
    // config_get / db 查询等）再调 block_on_async 构成重入。旧实现重入分支
    // 直接 handle.block_on —— 外层 block_on 的 enter 守卫仍挂在当前线程上，
    // 必然 panic（Cannot start a runtime from within a runtime），panic 穿透
    // 污染 wasmtime Store 导致插件 trap → 重载循环 → 插件整体失效。
    // 修复：重入分支改在新线程上 block_on，此处验证重入可返回且嵌套 future
    // 真正挂起（sleep）时也能被 runtime 唤醒（无死锁）。
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // tokio::spawn：模拟真实分发在 worker 线程执行（block_in_place 前置条件）
        tokio::spawn(async move {
            // 外层 block_on_async：模拟 dispatch_*_to_plugin 的同步桥接
            let outer = block_on_async(async {
                // 内层 block_on_async：模拟插件回调内的宿主函数调用（重入分支）
                let inner = block_on_async(async {
                    // 真实挂起：验证新线程上的 block_on 能被 runtime 定时器唤醒
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    42u32
                });
                inner * 2
            });
            assert_eq!(outer, 84);
        })
        .await
        .expect("spawned task must not panic");
    });
}
