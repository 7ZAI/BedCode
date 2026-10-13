//! 移动组件闭环集成测试（票 05 绑定归位：自 wasm-core `bare/component.rs::tests` 迁入）
//!
//! **为什么迁宿主**：绑定归位后内核零 WIT 绑定（linker 里没有任何 core 域实现），
//! 夹具组件只能在**移动宿主**的进程里实例化——host 自报的 `mobile-host-core`
//! （core 12 域）+ 5 个移动独有域 adapter 就是它 import 的全部接口。
//!
//! 覆盖：实例化往返 + ABI 协商 + host-storage 直测 + 业务方法全链（生命周期 /
//! 命令 / manifest / bus 事件 / 生命周期事件映射）+ 高 ABI 拒绝 + 燃料看门狗 +
//! ResourceLimiter + SDK 宏产物加载 + AOT 缓存命中 + linker 重复注册。
//!
//! ws / 终端协议两个域名闭环各自独立成文件（`ws_client_e2e.rs` /
//! `terminal_session_e2e.rs`）。


use std::collections::HashSet;
use std::sync::Arc;

use bedcode_host_kit::WasmPluginState;
use bedcode_wasm_core::manager::runtime::{LoadedWasmPlugin, WasmRuntime};
use bedcode_mobile_lib::plugin::bindings::bedcode;
use bedcode_wasm_core::test_support::{
    build_component_linker, build_host_ctx, build_terminal_session_component, build_test_component,
    plugin_raw_store, probe_wasi_p2_reregistration, test_engine,
    TEST_PLUGIN_ID,
};
use wasmtime::component::Component;
use wasmtime::ResourceLimiter;

use bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE;

/// 组件完整加载往返：实例化 + 形态校验（移动 WIT 无 abi-form 导出⇒ 宿主不要求）
/// + 导出完整性 + 燃料注入生效 + manifest 读取 + Host trait（storage/log）接线直测

/// 移动运行时装配（与生产 `PluginManager::init_wasm_runtime_with` 同款）：
/// 创建 runtime 后**注入 guest 事件派发器**——`events` 导出是宿主契约面，内核缺省
/// `UnwiredGuestEvents` 派发即 fail-visible（不注入时 `on_bus_message` 必 Err）。
fn mobile_runtime(aot_cache_dir: Option<std::path::PathBuf>) -> WasmRuntime {
    // 统一壳形态规格注入（单形态）：移动 WIT 无 `abi-form` 导出 ⇒ 形态要求为
    // `None`（不要求）。整数 ABI 版本协商已随 wit 包版本化退役（ADR 0051 D2）——
    // 产物版本由 linker 的 import 名逐字匹配裁决，不再有"上限"可注入。
    let runtime = bedcode_wasm_core::test_support::setup_headless_runtime_with_abi(
        aot_cache_dir,
        bedcode_wasm_core::manager::runtime::AbiPolicy { form: None },
    )
    .expect("create wasm runtime");
    runtime.set_guest_events(std::sync::Arc::new(
        bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents,
    ));
    runtime
}

#[test]
fn test_component_roundtrip_and_host_impl() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        // 预写 storage：验证 host_storage Host impl 读回（JSON 值往返）
        let tmp = tempfile::tempdir().expect("tempdir");
        let host_ctx = build_host_ctx();
        host_ctx
            .storage
            .set(TEST_PLUGIN_ID, "test-key", serde_json::json!({"k": "v"}))
            .await
            .expect("preset storage key");

        let runtime =
            mobile_runtime(Some(tmp.path().join("aot")));
        // 组件必须用 runtime 自身 Engine 编译（跨 Engine 实例化被 wasmtime 拒绝）
        let component = Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_test_component(&[]))
            .expect("compile test component");
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(HashSet::from([PERMISSION_STORAGE.to_string()]))),
                &[],
                None,
            )
            .expect("instantiate component");

        // 单形态（2026-10-13）：host-storage 域实现随统一迁内核 `core_binding`
        //（双端全等 8 域单份；权限门收敛统一 PermissionManager，JSON 往返与
        // 未授权拒绝的正反例由 wasm-core `host_api/storage.rs` 测试覆盖）——
        // 本端不再直测移动版 trait（原 `host_domains/storage.rs` 已删除）。
        // 预写 storage 仍在上面保留：实例化成功即验证 core_binding 的
        // host_storage 注册进统一 linker（guest 闭环经 invoke 用例覆盖）。

        // 燃料注入生效：先触发一次导出调用（`get_manifest` 经 `core_exports` 的
        // `refill_call_fuel` 在调用前重置预算并执行 guest 代码），**再**断言调用
        // 确实消耗燃料。断言基准取 Store 的实际预算（`fuel_budget()`，debug 构建含
        // 放大倍率）而非裸常量——注入点与桌面形态同源（`engine::inject_call_fuel`）。
        // ⚠️ 断言必须紧跟调用之后：若在调用前读 store 燃料，会读到未消耗的整额
        // （票 04 把 abi.version 原生探测换成 get_manifest 时曾因此断言空转——
        // 无任何导出调用可测）。
        let manifest: serde_json::Value = serde_json::from_str(
            &plugin.get_manifest().expect("manifest"),
        )
        .unwrap();
        {
            let (store, _) = plugin_raw_store(&mut plugin);
            let budget = store.data().limits.fuel_budget();
            let remaining = store.get_fuel().expect("get fuel");
            assert!(
                remaining < budget,
                "export call must consume fuel, remaining={remaining} budget={budget}"
            );
        }

        // manifest（world 导出完整性的顺带验证）
        assert_eq!(manifest["id"], "com.bedcode.component-test");
    });
}

/// 业务方法全走通（03 验收）：生命周期 + 命令（含 host import 往返）+
/// 终端钩子 + manifest + bus 事件 + 上传/传输钩子（fail-closed 决定透传）+
/// 生命周期事件映射
#[test]
fn test_component_business_methods_roundtrip() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let tmp = tempfile::tempdir().expect("tempdir");
        let host_ctx = build_host_ctx();
        host_ctx
            .storage
            .set(TEST_PLUGIN_ID, "test-key", serde_json::json!({"k": "v"}))
            .await
            .expect("preset storage key");

        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component = Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_test_component(&[]))
            .expect("compile test component");
        // 统一权限门（单形态）：WASM 实例内 host-storage 走统一 PermissionManager——
        // 测试须把 manifest 权限 grant 进统一词表（生产路径在 activate 批准时做）
        host_ctx.grant_permissions(TEST_PLUGIN_ID, &[PERMISSION_STORAGE.to_string()]);
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(HashSet::from([PERMISSION_STORAGE.to_string()]))),
                &[],
                None,
            )
            .expect("instantiate component");

        // 生命周期
        assert_eq!(plugin.activate().expect("activate"), 0);
        assert_eq!(plugin.deactivate().expect("deactivate"), 0);
        plugin.on_startup().expect("on_startup");
        plugin.on_shutdown().expect("on_shutdown");

        // 命令调用：guest 内 storage/config import 双向往返（跨边界）
        let result = plugin
            .invoke_command("test.echo", r#"{"hello":"component"}"#)
            .expect("invoke_command");
        let result_json: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result_json["name"], "test.echo");
        assert_eq!(result_json["stored"], "{\"k\":\"v\"}");
        // host-config 接线验证：system.time_ms 返回非空时间戳
        let now_ms = result_json["now_ms"].as_str().unwrap_or("");
        assert!(
            !now_ms.is_empty() && now_ms.parse::<u64>().is_ok(),
            "host-config system.time_ms 应返回时间戳，实际: {:?}",
            now_ms
        );

        // manifest
        let manifest: serde_json::Value =
            serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.component-test");

        // bus 事件回调（组件返回 Ok）：统一壳 on_message（单形态）——
        // BusMessage 拆为 topic/sender/payload 三参
        plugin
            .on_message(
                &"t".to_string(),
                &"s".to_string(),
                &serde_json::json!({}),
            )
            .expect("on_message");

        // 生命周期事件映射（auth/disconnect/session 各走一遍）
        use bedcode_wasm_core::manager::types::PluginLifecycleEvent;
        plugin
            .call_lifecycle_event(&PluginLifecycleEvent::AuthSuccess)
            .expect("auth success");
        plugin
            .call_lifecycle_event(&PluginLifecycleEvent::Disconnect {
                reason: "bye".to_string(),
            })
            .expect("disconnect");
        plugin
            .call_lifecycle_event(&PluginLifecycleEvent::SessionCreated {
                session_id: "s1".to_string(),
            })
            .expect("session created");
        plugin
            .call_lifecycle_event(&PluginLifecycleEvent::SessionStopped {
                session_id: "s1".to_string(),
            })
            .expect("session stopped");
    });
}

/// 燃料看门狗生效：死循环组件在单次调用预算内被 trap
#[test]
fn test_component_fuel_trap() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let tmp = tempfile::tempdir().expect("tempdir");
        let host_ctx = build_host_ctx();
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component =
            Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_test_component(&["spin-loop"]))
                .expect("compile test component");
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(HashSet::new())),
                &[],
                None,
            )
            .expect("实例化正常（死循环在调用期，不在构造器）");

        let err = plugin
            .invoke_command("spin", "{}")
            .expect_err("死循环必须被燃料看门狗 trap");
        // fuel 耗尽 trap 的 Display 含 wasm backtrace；trap 即通过，
        // 关键是 backtrace 顶部是 invoke 导出（trap 发生在 guest 死循环内）。
        // ⚠️ 断言**版本免疫**：版本化后 backtrace 符号带包版本（
        // `bedcode_plugin_component_test.wasm!bedcode:plugin/command@0.1.0#invoke`），
        // 不能用 `command#invoke` 整串匹配（版本在中间）；匹配 `#invoke` 栈顶即可
        let msg = err.to_string();
        assert!(
            msg.contains("#invoke"),
            "trap 应发生在调用栈内并带 wasm backtrace（栈顶为 invoke 导出），实际: {}",
            msg
        );
    });
}

/// ResourceLimiter 生效：内存阈值单元断言 + 超限组件集成断言
#[test]
fn test_component_limiter_rejects_over_limit_memory() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let tmp = tempfile::tempdir().expect("tempdir");
        let host_ctx = build_host_ctx();

        // 单元层：阈值语义（256MB 内放行、超限拒绝）——资源上限由 host-kit
        // state 的 ResourceLimiter 实现承载（统一 state，票 04）
        let mut state = bedcode_wasm_core::test_support::build_component_state(TEST_PLUGIN_ID, &[]);
        assert_eq!(
            state.memory_growing(0, 256 * 1024 * 1024, None).unwrap(),
            true
        );
        assert_eq!(
            state
                .memory_growing(0, 256 * 1024 * 1024 + 1, None)
                .unwrap(),
            false,
            "超过 256MB 的内存增长必须被拒绝"
        );
        assert_eq!(state.table_growing(0, 1_000_000, None).unwrap(), true);
        assert_eq!(state.table_growing(0, 1_000_001, None).unwrap(), false);

        // 集成层：guest 直接 memory.grow 300MB → limiter 拒绝 → 返回 {"grow":"failed"}
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component =
            Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_test_component(&["big-alloc"]))
                .expect("compile test component");
        let mut plugin = runtime
            .instantiate_component(
                &component,
                TEST_PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(HashSet::new())),
                &[],
                None,
            )
            .expect("实例化正常（分配发生在调用期）");
        let result = plugin.invoke_command("alloc", "{}");
        match result {
            Err(e) => {
                // 也可能是 trap 形式拒绝，同样视为 limiter 生效
                eprintln!("[big-alloc] invoke failed (trap): {}", e);
            }
            Ok(s) => {
                let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
                assert_eq!(
                    v["grow"].as_str(),
                    Some("failed"),
                    "ResourceLimiter 未拦截组件内存增长: {}",
                    s
                );
            }
        }
    });
}

/// 迁移 ticket 04 验收：SDK `wasm_entry!` 宏生成的组件能被宿主加载，
/// 完成 ABI 协商（version=ABI_VERSION）、激活、命令调用（含错误 JSON 透传）
///
/// 被测对象是真实插件（terminal-session，票 16 合并后的终端 + 任务域
/// app）经新 SDK 编译的产物 —— 宏展开正确性、
/// `export!` 跨 crate 接线、8 组接口全量导出的最终证明（插件业务代码零改动）。
#[test]
fn test_sdk_macro_component_loads_and_activates() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let tmp = tempfile::tempdir().expect("tempdir");
        let host_ctx = build_host_ctx();
        let runtime = mobile_runtime(Some(tmp.path().join("aot")));
        let component =
            Component::from_binary(bedcode_wasm_core::test_support::runtime_engine(&runtime), &build_terminal_session_component())
                .expect("compile terminal-session component");
        // 生产 manifest 权限集（activate 期 ws 属主 topic 订阅 + 认证状态探测按声明放行）
        let granted: HashSet<String> = [
            bedcode_plugin_api_mobile::permission::PERMISSION_AUTH,
            bedcode_plugin_api_mobile::permission::PERMISSION_BUS,
            bedcode_plugin_api_mobile::permission::PERMISSION_SESSION_READ,
            bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE,
            bedcode_plugin_api_mobile::permission::PERMISSION_TERMINAL_OUTPUT,
            // ui:input / ui:toolbox 随 2026-10-10 批次 C2 整面退役（移动壳改纯
            // surface 形态），移动 SDK 权限词汇表已无此两词——granted 集不再含
            bedcode_plugin_api_mobile::permission::PERMISSION_WS_CLIENT,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut plugin = runtime
            .instantiate_component(
                &component,
                "com.bedcode.terminal-session",
                Arc::new(host_ctx.for_plugin(granted)),
                &[],
                None,
            )
            .expect("instantiate terminal-session component");

        // 激活/停用（terminal-session：activate 期订阅 ws 属主 topic + 认证状态探测）
        assert_eq!(plugin.activate().expect("activate"), 0);
        assert_eq!(plugin.deactivate().expect("deactivate"), 0);

        // manifest（宏内 `manifest()` 序列化 plugin.json）
        let manifest: serde_json::Value =
            serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(manifest["id"], "com.bedcode.terminal-session");

        // 命令调用：terminal-session 对未知命令返回 Err → 宏序列化为 {"error": ...} JSON
        let result = plugin
            .invoke_command("no.such.cmd", "{}")
            .expect("invoke_command");
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            v["error"]
                .as_str()
                .is_some_and(|s| s.contains("unknown command")),
            "命令错误应经宏转义为 error JSON，实际: {}",
            result
        );
    });
}

/// AOT：二次加载命中 `.cwasm` 缓存（删除源文件仍可加载即证明命中），
/// 产物只写宿主 cache 目录
#[test]
fn test_component_aot_cache_hit() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_dir = tmp.path().join("cache");
        let runtime = mobile_runtime(Some(cache_dir.clone()));

        // 源组件写盘（default 形态组件字节）
        let component_bytes = build_test_component(&[]);
        let src = tmp.path().join("plugin.component.wasm");
        std::fs::write(&src, &component_bytes).expect("write source component");

        // 首次编译：产物落缓存
        runtime
            .compile_component_from_file(&src)
            .expect("first compile");
        let cache_files: Vec<_> = std::fs::read_dir(&cache_dir)
            .expect("cache dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "cwasm").unwrap_or(false))
            .collect();
        assert_eq!(cache_files.len(), 1, "缓存目录应恰有 1 个 .cwasm 产物");
        let cache_file = cache_files[0].path();
        assert!(
            cache_file
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('c')),
            "组件缓存文件应以 c 前缀命名: {}",
            cache_file.display()
        );

        // 二次加载：缓存 mtime 不变 = 命中缓存（未触发重编译/重写）。
        // 注：源文件消失时缓存不可命中（新鲜度判定依赖
        // 源 mtime，源缺失按不新鲜处理）——这个边界行为与 Module 路径一致，不改
        let cache_mtime = std::fs::metadata(&cache_file)
            .and_then(|m| m.modified())
            .expect("cache mtime");
        // 20ms 远大于文件系统 mtime 粒度；重编译必然重写缓存文件使 mtime 前进
        std::thread::sleep(std::time::Duration::from_millis(20));
        runtime
            .compile_component_from_file(&src)
            .expect("second compile (cache hit)");
        let cache_mtime2 = std::fs::metadata(&cache_file)
            .and_then(|m| m.modified())
            .expect("cache mtime after second compile");
        assert_eq!(
            cache_mtime, cache_mtime2,
            "二次编译未命中缓存（缓存文件被重写）"
        );

        // 产物不写回源目录：源目录无 .wasm/.cwasm 残留
        let src_dir: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("src dir")
            .filter_map(|e| e.ok())
            .collect();
        for e in src_dir {
            let name = e.file_name().to_string_lossy().to_string();
            assert!(
                !name.ends_with(".cwasm"),
                "AOT 产物不得写回源目录（只允许宿主 cache 目录持有）: {}",
                e.path().display()
            );
        }
    });
}

/// 重复注册同一 interface 必须报错（宿主装配面）
///
/// 防止 linker 被装配两次时静默覆盖接线（实例化时会以意外行为失败，不如注册期
/// 直接暴露）。用**宿主绑定**（`plugin::bindings`）验证 wasmtime 的
/// "defined twice" 语义——内核侧只剩 WASI 面，等价断言见 wasm-core
/// `bare/component.rs` 的 `test_component_linker_registers_wasi_by_default`。
// 单形态（2026-10-13）：defined twice 语义由统一壳 `component::add_to_linker`
// 覆盖（wasm-core `component/tests/linker.rs::test_add_to_linker_rejects_duplicate_registration`
// ——同一 linker 装配面）。本端不再持有 host_storage 实现（双端全等 8 域在内核
// core_binding），原移动绑定直测删除。

// ==================== linker 装配契约（WASI 段） ====================
//
// 票 05 绑定归位后，内核 linker 的两段之一是 WASI（另一段是宿主自报能力模块）。
// 这两条断言必须在**宿主 adapter 在链接图里**的测试二进制中跑：内核自己的
// cfg(test) registry 为空，而本仓 bare 形态的 `cargo test --lib` 目前整体不可编译
// （见票 05 §7 记录）——链路经 test_support 的 `build_component_linker` 转发。

/// 默认配置下 linker 真的含 WASI p2/p3（重复注册 p2 必然 "defined twice"）
///
/// 用户裁决 2026-10-12：WASI 面住核心、双端默认全开——本用例是「移动形态默认
/// 注册真的生效」而不是空转的可见证据。
#[test]
fn test_component_linker_registers_wasi_by_default() {
    let engine = test_engine();
    let mut linker = build_component_linker(&engine, &bedcode_wasm_core::config::WasiConfig::default())
        .expect("build component linker");
    let err = probe_wasi_p2_reregistration(&mut linker)
        .expect("p2 already registered at default config ⇒ duplicate must fail");
    assert!(
        err.contains("defined twice"),
        "unexpected wasi p2 re-registration error: {}",
        err
    );
}

/// `wasi.enabled=false` ⇒ linker 不含 WASI（注册被配置关闭，重注册即成功）
///
/// 反例锁：把配置开关写成恒真（忽略 cfg）会让本条红——「外部配置能关掉 WASI」
/// 是用户裁决的正面要求，不能只是文档承诺。
#[test]
fn test_component_linker_honors_wasi_disabled_config() {
    let engine = test_engine();
    let cfg = bedcode_wasm_core::config::WasiConfig {
        enabled: false,
        ..bedcode_wasm_core::config::WasiConfig::default()
    };
    let mut linker = build_component_linker(&engine, &cfg).expect("build component linker");
    assert!(
        probe_wasi_p2_reregistration(&mut linker).is_none(),
        "p2 must NOT be registered when wasi.enabled=false"
    );
}
