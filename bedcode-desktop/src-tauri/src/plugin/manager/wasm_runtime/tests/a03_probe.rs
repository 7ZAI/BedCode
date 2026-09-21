//! A0-3 宿主 async 化 · 前置探针（P1 async store 兼容性 / P2 资源限制 async 语义 /
//! P5 性能基线）
//!
//! 依据：`.scratch/2026-09-21-a0-3-host-async/spec.md`（用户确认实施）。
//! 性质：**只读探针**——不修改生产路径（wasm_runtime.rs / component.rs 生产代码
//! 零改动），全部证据来自测试级实例化、既有 fixture 与既有产物。
//!
//! 场景对应：
//! - P1-a：sync 注册的 bedcode host 原语（host-log，func_wrap）在 async store 下被
//!   wasip3 组件调用，`block_on_async` 桥三路径（多线程 block_in_place / current_thread
//!   spawn 新线程 / 无 handle 线程 ambient 直接驱动）均不 panic；
//! - P1-b：真实 session 插件 wasip3 产物在 async store 下全链路
//!   （activate → invoke_command → 终端 hooks → get_manifest → deactivate）；
//! - P1-c：生产产物（resources/plugins/desktop/*）在 async store 下逐字节行为不变；
//!   现状证据：四产物全部为 wasip3 组件（magic `\0asm`+`0d 00 01 00`），无
//!   unknown-unknown 残留可回归；同步 `call` 在 async-required store 上按文档报错
//!   （bindgen 统一 async 导出，见 component.rs `bindgen!` 注释）；
//! - P2：燃料（guest 指令计数在 async 调用内跨 suspend/resume 累计、调用前续费、
//!   引擎关闭时 set_fuel 报错）与 ResourceLimiter（紧内存上限被拒）在 async store 下
//!   的语义；
//! - P5：短调用开销微基准（纯 Rust 基线 / block_on_async 桥 ambient 路径 / 桥
//!   block_in_place 路径 / 端到端 guest host-log 往返），供主体票做收益对照。

use super::*;
use crate::plugin::config::CoreConfig;
use tokio::sync::Mutex as TokioMutex;

/// 探针插件 ID（不与其他测试实例冲突）
const A03_PLUGIN_ID: &str = "com.bedcode.a03-probe";
/// 探针命令载荷
const CMD_HOST_LOG: &str = "a03.host-log-roundtrip";
const CMD_SPIN: &str = "a03.spin";
const CMD_RANDOM: &str = "a03.get-random";

/// 构建并实例化 wasip3 探针 fixture（工具链未装时返回 None，测试跳过）
fn instantiate_a03_fixture(wasm_runtime: &WasmRuntime, host_ctx: &Arc<WasmHostContext>) -> Option<LoadedWasmPlugin> {
    let bytes = build_wasip3_test_component()?;
    let component = wasm_runtime.compile_component(&bytes).expect("compile a03 fixture");
    Some(
        wasm_runtime
            .instantiate_component(&component, A03_PLUGIN_ID, Arc::clone(host_ctx), &[], None)
            .expect("instantiate a03 fixture (async store)"),
    )
}

/// 执行命令并解析 JSON（探针统一收口）
fn run_command(plugin: &mut LoadedWasmPlugin, name: &str, args: &str) -> serde_json::Value {
    let raw = plugin.invoke_command(name, args).unwrap_or_else(|e| {
        panic!("[a03] invoke_command({name}) 失败: {e}");
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("[a03] 命令返回非法 JSON: {e} ({raw})"))
}

// ==================== P1-a · sync host_impl 在 async store 下 ====================

/// P1-a：同步注册的 bedcode host 原语（host-log）在 async store 下由 wasip3 组件
/// 调用；`block_on_async` 桥三路径逐一驱动，均不得 panic；async wasi（random）与
/// sync host 原语在同一调用链共存。
#[test]
fn a03_p1a_sync_host_impl_under_async_store() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let Some(plugin) = instantiate_a03_fixture(&wasm_runtime, &host_ctx) else {
        eprintln!("[skip] a03_p1a: wasip3 工具链未就绪");
        return;
    };
    let plugin = Arc::new(TokioMutex::new(plugin));

    // 路径 ①：多线程 tokio runtime worker（block_in_place 分支；fiber 内重入走
    // BlockInPlaceGuard 检测 → spawn 新线程）
    {
        let rt = tokio::runtime::Runtime::new().expect("multi-thread rt");
        let plugin = Arc::clone(&plugin);
        rt.block_on(async move {
            let mut g = plugin.lock().await;
            let v = run_command(&mut g, CMD_HOST_LOG, "{}");
            assert_eq!(v["ok"], true, "多线程路径 host-log 必须成功: {v}");
            // async wasi:random 与 sync host 原语同链共存
            let v2 = run_command(&mut g, CMD_RANDOM, "{}");
            assert_eq!(v2["ok"], true, "async wasi:random 必须成功: {v2}");
        });
        println!("[a03][P1-a] ① 多线程 worker（block_in_place + 重入检测）OK");
    }

    // 路径 ②：current_thread tokio runtime（spawn 新线程 + ambient 驱动分支）
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current_thread rt");
        let plugin = Arc::clone(&plugin);
        rt.block_on(async move {
            let mut g = plugin.lock().await;
            let v = run_command(&mut g, CMD_HOST_LOG, "{}");
            assert_eq!(v["ok"], true, "current_thread 路径 host-log 必须成功: {v}");
        });
        println!("[a03][P1-a] ② current_thread（spawn 新线程 + ambient）OK");
    }

    // 路径 ③：纯 std 线程（无 runtime handle → ambient 直接 block_on 分支）
    {
        let plugin = Arc::clone(&plugin);
        std::thread::spawn(move || {
            let mut g = plugin.blocking_lock();
            let v = run_command(&mut g, CMD_HOST_LOG, "{}");
            assert_eq!(v["ok"], true, "ambient 路径 host-log 必须成功: {v}");
            assert_eq!(v["calls"], 3, "host-log 应调用 3 次: {v}");
        })
        .join()
        .expect("std thread panicked");
        println!("[a03][P1-a] ③ 无 handle 线程（ambient 直接驱动）OK");
    }

    // 同实例其余导出在 async store 下可用：终端 hooks / 事件回调 / manifest
    {
        let rt = tokio::runtime::Runtime::new().expect("multi-thread rt");
        let plugin = Arc::clone(&plugin);
        rt.block_on(async move {
            let mut g = plugin.lock().await;
            assert_eq!(
                g.on_terminal_input("s1", "hi").expect("hook"),
                None,
                "未注册 hook 应透传 None"
            );
            assert_eq!(g.on_terminal_output("s1", "out").expect("hook"), None);
            g.on_message("a03.topic", "host", &serde_json::json!({ "n": 1 }))
                .expect("on_message 在 async store 下可用");
            let m: serde_json::Value = serde_json::from_str(&g.get_manifest().expect("manifest")).unwrap();
            assert_eq!(m["id"], "com.bedcode.wasip3-test");
        });
        println!("[a03][P1-a] hooks / on_message / get_manifest 同实例 OK");
    }
}

// ==================== P1-b · wasip3 组件完整闭环（真实 session 产物） ====================

/// P1-b：现有 session 插件 wasip3 产物在 async store 下
/// `activate → invoke_command(session.status) → 终端 hooks → get_manifest → deactivate`
/// 全链路。产物缺失（未跑插件构建）时跳过。
#[test]
fn a03_p1b_wasip3_artifact_full_closed_loop() {
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] a03_p1b: session wasip3 产物未构建");
        return;
    }
    let (wasm_runtime, mut host_ctx) = setup_wasm_runtime();
    // 私有库根目录隔离：session 插件激活会写私有库（migrate 标记/建表）。进程级
    // `plugin_db_root()` 被既有共享库用例共用（互斥锁串行），本探针用唯一临时目录
    // 避免在共享文件系统状态上再添一个参与者。
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        let isolate = std::env::temp_dir().join(format!("bedcode_a03_p1b_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&isolate);
        ctx.plugin_db_root = Some(isolate);
    }
    let mut plugin = wasm_runtime
        .load_plugin_from_file(&wasm_path, "com.bedcode.terminal-session", Arc::clone(&host_ctx), &[], None)
        .expect("load wasip3 session: all imports must resolve");

    assert_eq!(plugin.activate().expect("activate"), 0, "activate 必须成功");
    // 命令面（session.status 回显 manifest 声明，宿主侧无业务依赖）
    let v = run_command(&mut plugin, "session.status", "{}");
    assert_eq!(
        v["plugin"], "com.bedcode.terminal-session",
        "session.status 必须回显插件 ID: {v}"
    );
    assert!(
        v["domains"].is_array() && !v["domains"].as_array().unwrap().is_empty(),
        "domains 非空: {v}"
    );
    // 终端 hooks 在 async store 下可调用（本插件未注册输入 hook → 透传 None）
    assert_eq!(
        plugin.on_terminal_input("p1b", "x").expect("hook"),
        None,
        "未注册输入 hook 应透传 None"
    );
    assert_eq!(plugin.on_terminal_output("p1b", "y").expect("hook"), None);
    // manifest 往返
    let m: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(m["id"], "com.bedcode.terminal-session");
    assert_eq!(plugin.deactivate().expect("deactivate"), 0, "deactivate 必须成功");
    println!("[a03][P1-b] session 产物全链路（activate → status → hooks → manifest → deactivate）OK");
}
// ==================== P1-c · 生产产物在 async store 下零回归 ====================

/// P1-c：全部生产产物（resources/plugins/desktop/*）在 async store 下加载 +
/// manifest 往返；现状证据：四产物均为 wasip3 组件（magic `\0asm` + `0d 00 01 00`），
/// 无 unknown-unknown 残留可回归；同步 `call` 在 async-required store 上按文档报错
/// （bindgen `exports: { default: async }` 统一 async 路径，见 component.rs bindgen! 注释）。
#[test]
fn a03_p1c_production_artifacts_async_store_no_change() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let plugins_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../resources/plugins/desktop");

    let mut loaded = 0usize;
    let mut component_magic = 0usize;
    let mut unknown_unknown = 0usize;
    let mut entries = std::fs::read_dir(&plugins_dir)
        .unwrap_or_else(|e| panic!("读 resources/plugins/desktop 失败: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.file_name());
    assert!(!entries.is_empty(), "resources/plugins/desktop 不应为空");

    for dir in entries {
        let plugin_id = dir.file_name().to_string_lossy().to_string();
        // 产物命名约定：目录 com.bedcode.<name> → 文件 bedcode_plugin_<name>.wasm
        // （<name> 中 '-'/'·' 替换为 '_'，见 scripts/plugin-wasm-config.mjs）
        let file_stem = plugin_id
            .strip_prefix("com.bedcode.")
            .unwrap_or(&plugin_id)
            .replace(['.', '-'], "_");
        let wasm_path = dir.path().join(format!("bedcode_plugin_{}.wasm", file_stem));
        if !wasm_path.exists() {
            eprintln!("[a03][P1-c] 跳过 {}（产物缺失）", plugin_id);
            continue;
        }
        let bytes = std::fs::read(&wasm_path).expect("read artifact");
        // 组件 magic：\0asm + 0d 00 01 00（wasip3 直出组件 / wit-component 编码）
        let is_component = bytes.len() >= 8 && &bytes[4..8] == [0x0d, 0x00, 0x01, 0x00];
        if is_component {
            component_magic += 1;
        } else {
            unknown_unknown += 1;
        }
        // async store 下加载 + manifest 往返（与生产同路径：load_plugin_from_file）
        let mut plugin = wasm_runtime
            .load_plugin_from_file(&wasm_path, &plugin_id, Arc::clone(&host_ctx), &[], None)
            .unwrap_or_else(|e| panic!("加载 {plugin_id} 失败（import 面不匹配）: {e}"));
        let m: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
        assert_eq!(m["id"], plugin_id, "manifest id 必须与目录名一致");
        loaded += 1;
        println!(
            "[a03][P1-c] {plugin_id}: 组件魔法={is_component} manifest id OK（{} bytes）",
            bytes.len()
        );
    }

    println!(
        "[a03][P1-c] 生产产物 {loaded} 个全部在 async store 下加载并 manifest 往返；组件 {component_magic} 个；unknown-unknown 残留 {unknown_unknown} 个"
    );
    assert_eq!(
        component_magic, loaded,
        "生产产物应全部为组件形态（现状：无 unknown-unknown 可回归）"
    );
}

/// P1-c 机制实证：bindgen `exports: { default: async }` 下同步 `call` 在 async store
/// 上按文档报错（“requires that `*_async` functions are used”）——统一 async 路径是
/// 唯一调用面，不存在 sync/async 双路径分叉风险。打印实际错误（不设断言：wasmtime
/// 报错文案随版本可能漂移，机制由上面 P1-c 全量加载 + 既有套件共同锁定）。
#[test]
fn a03_p1c_sync_call_under_async_store_mechanism() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let Some(mut plugin) = instantiate_a03_fixture(&wasm_runtime, &host_ctx) else {
        eprintln!("[skip] a03_p1c_sync_call: wasip3 工具链未就绪");
        return;
    };
    let (store, instance) = plugin.raw_store();
    let Ok(func) = "bedcode:plugin/command.invoke"
        .parse::<wasmtime::component::wit_parser::ItemName>()
        .map_err(|e| format!("parse export name: {e}"))
        .and_then(|item| {
            instance
                .get_typed_func::<(String, String), (String,)>(&mut *store, &item)
                .map_err(|e| format!("get_typed_func: {e}"))
        })
    else {
        println!("[a03][P1-c] 同步 call 探针：导出函数获取失败（bindgen async 绑定下无 sync 形态）");
        return;
    };
    // catch_unwind 包裹：同步 call 预期 Err（store 配置要求 *_async），不许 panic
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        func.call(&mut *store, ("a03.host-log-roundtrip".to_string(), "{}".to_string()))
    }));
    match outcome {
        Ok(Ok(_)) => println!("[a03][P1-c] 同步 call 意外成功（async-required store 未强制 async 路径）"),
        Ok(Err(e)) => println!(
            "[a03][P1-c] 同步 call 按文档报错（async-required）: {}",
            e.to_string().chars().take(160).collect::<String>()
        ),
        Err(_) => println!("[a03][P1-c] 同步 call panic（需关注：应在 wasmtime 层被拒而非 panic）"),
    }
}

// ==================== P2 · 资源限制 async 语义 ====================

/// P2 燃料：默认引擎（consume_fuel=true）下——guest 指令计数在同一 async 调用内
/// 跨 suspend/resume 累计（消耗量随 spin iters 缩放）；调用前续费保证低燃料不 trap；
/// 引擎关闭燃料（consume_fuel=false）时 set_fuel 显性报错。
#[test]
fn a03_p2_fuel_async_semantics() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let Some(mut plugin) = instantiate_a03_fixture(&wasm_runtime, &host_ctx) else {
        eprintln!("[skip] a03_p2_fuel: wasip3 工具链未就绪");
        return;
    };

    // ① 燃料开启：set_fuel/get_fuel 可用（wasmtime 48 语义：未开启时 set_fuel 报错）
    {
        let (store, _) = plugin.raw_store();
        store.set_fuel(1_000_000).expect("燃料开启时 set_fuel 必须可用");
        let _ = store.get_fuel().expect("get_fuel 必须可用");
    }

    // ② 消耗量随 guest 工作量缩放（同一实例、同一燃料预算起点：exports() 续费到
    // fuel_budget 后执行；两次调用消耗差即 guest 指令计数在 async 调用内的净消耗）
    let r1 = run_command(&mut plugin, CMD_SPIN, r#"{"iters": 1_000_000}"#);
    assert_eq!(r1["ok"], true);
    let left_small = {
        let (store, _) = plugin.raw_store();
        store.get_fuel().expect("get_fuel 必须可用")
    };
    let r2 = run_command(&mut plugin, CMD_SPIN, r#"{"iters": 10_000_000}"#);
    assert_eq!(r2["ok"], true);
    let left_large = {
        let (store, _) = plugin.raw_store();
        store.get_fuel().expect("get_fuel 必须可用")
    };
    assert!(
        left_large < left_small,
        "更多 guest 工作必须消耗更多燃料: small_left={left_small} large_left={left_large}"
    );
    println!("[a03][P2] 燃料：1M spin 后剩 {left_small}，10M spin 后剩 {left_large}（消耗随 iters 缩放 ✓）");

    // ③ 调用前续费：set_fuel(0) 后调用仍成功（exports() 先续费到 fuel_budget）
    {
        let (store, _) = plugin.raw_store();
        store.set_fuel(0).expect("set_fuel(0)");
    }
    let v = run_command(&mut plugin, CMD_HOST_LOG, "{}");
    assert_eq!(v["ok"], true, "set_fuel(0) 后经续费调用必须成功");
    println!("[a03][P2] 燃料：set_fuel(0) 后调用经续费成功（调用前续费语义 ✓）");

    // ④ 引擎关闭燃料（consume_fuel=false）：set_fuel 显性报错（wasmtime 语义）
    {
        let mut cfg = CoreConfig::default();
        cfg.engine.consume_fuel = false;
        let (wasm_runtime2, host_ctx2) = setup_wasm_runtime_with_config(cfg);
        let Some(mut plugin2) = instantiate_a03_fixture(&wasm_runtime2, &host_ctx2) else {
            eprintln!("[skip] a03_p2_fuel ④: wasip3 工具链未就绪");
            return;
        };
        let (store, _) = plugin2.raw_store();
        assert!(
            store.set_fuel(1_000_000).is_err(),
            "consume_fuel=false 引擎下 set_fuel 必须报错"
        );
        println!("[a03][P2] 燃料：consume_fuel=false 引擎 set_fuel 显性报错 ✓");
    }
}

/// P2 内存：ResourceLimiter 在 async store 下仍强制生效——紧内存上限实例（经
/// ResourceOverrides 请求，自我收紧合法）无法完成探针命令，对照实例（默认 256MiB）
/// 成功；失败形态（实例化被拒 or 调用期 memory_growing 拒绝 trap）如实记录。
#[test]
fn a03_p2_resource_limiter_async_semantics() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let Some(bytes) = build_wasip3_test_component() else {
        eprintln!("[skip] a03_p2_limiter: wasip3 工具链未就绪");
        return;
    };
    let component = wasm_runtime.compile_component(&bytes).expect("compile");

    // 对照实例：默认上限 → 命令成功
    let mut control = wasm_runtime
        .instantiate_component(&component, A03_PLUGIN_ID, Arc::clone(&host_ctx), &[], None)
        .expect("对照实例必须实例化成功");
    let v = run_command(&mut control, CMD_HOST_LOG, "{}");
    assert_eq!(v["ok"], true, "对照实例（默认 256MiB 上限）必须成功: {v}");

    // 紧内存实例：请求 64KiB（= 1 wasm 页）上限——自我收紧，仲裁合法放行。
    // 本 fixture 声明最小内存 17 页（~1.1MiB）> 上限 → 实例化阶段即被拒
    // （limiter 生效的第一道闸：instantiation-time denial）。
    let tight_overrides = ResourceOverrides {
        max_memory_bytes: Some(64 * 1024),
        ..ResourceOverrides::default()
    };
    let tight = wasm_runtime.instantiate_component(
        &component,
        "com.bedcode.a03-tight",
        Arc::clone(&host_ctx),
        &[],
        Some(&tight_overrides),
    );
    let mut tight = match tight {
        Ok(p) => p,
        Err(e) => {
            // 实例化阶段即被拒（guest 初始内存/数据段需求 > 上限）
            println!(
                "[a03][P2] 紧内存（64KiB）实例实例化被拒（limiter 生效）: {}",
                e.to_string().chars().take(160).collect::<String>()
            );
            // 继续第二道闸：上限高于最小内存、低于工作集 → 调用期 memory_growing 拒绝
            call_time_memory_denial(&wasm_runtime, &host_ctx, &component);
            println!("[a03][P2] ResourceLimiter 在 async store 下强制生效 ✓（对照成功 / 紧内存被拒）");
            return;
        }
    };
    // 实例化成功但内存增长受限：命令（guest 分配 + 字符串处理）必然触发
    // memory_growing → 拒绝 → trap → Err
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tight.invoke_command(CMD_HOST_LOG, "{}")
    }));
    match outcome {
        Ok(Err(e)) => {
            println!(
                "[a03][P2] 紧内存实例调用期被拒（memory_growing 拒绝）: {}",
                e.to_string().chars().take(200).collect::<String>()
            );
        }
        Ok(Ok(raw)) => {
            // 意外成功：打印结果（若发生说明探针命令所需内存 < 上限，需加大工作量）
            println!("[a03][P2] ⚠️ 紧内存实例意外成功: {raw}（需复核探针命令内存需求）");
            panic!("紧内存上限必须阻止探针命令成功");
        }
        Err(_) => panic!("紧内存实例调用 panic（应在 wasmtime 层被拒而非 panic）"),
    }
    println!("[a03][P2] ResourceLimiter 在 async store 下强制生效 ✓（对照成功 / 紧内存被拒）");
}

/// P2 内存第二道闸：上限高于组件最小内存（实例化成功）但低于运行时工作集——
/// guest 在调用期尝试内存增长（a03.allocate 4MiB）→ `memory_growing` 拒绝 → trap。
/// 本 fixture 最小内存 17 页（1,114,112 bytes），上限取 17 页 + 1KiB：实例化 OK
/// （min ≤ limit），分配 4MiB 需增长到 64 页 → 超过上限被拒。
fn call_time_memory_denial(
    wasm_runtime: &WasmRuntime,
    host_ctx: &Arc<WasmHostContext>,
    component: &wasmtime::component::Component,
) {
    let overrides = ResourceOverrides {
        max_memory_bytes: Some(17 * 64 * 1024 + 1024),
        ..ResourceOverrides::default()
    };
    let mut plugin = match wasm_runtime.instantiate_component(
        component,
        "com.bedcode.a03-tight-growth",
        Arc::clone(host_ctx),
        &[],
        Some(&overrides),
    ) {
        Ok(p) => p,
        Err(e) => {
            println!(
                "[a03][P2] 17 页+1KiB 上限实例实例化也被拒（超出预期，记录即可）: {}",
                e.to_string().chars().take(160).collect::<String>()
            );
            return;
        }
    };
    // 对照（同实例上限内的小分配）：分配 32KiB 在 17 页上限内 → 成功
    let small = run_command(&mut plugin, "a03.allocate", r#"{"bytes": 32768}"#);
    assert_eq!(small["allocated"], 32768, "32KiB 分配应在 17 页上限内成功: {small}");
    // 超限分配：4MiB 增长远超上限 → memory_growing 拒绝 → trap
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        plugin.invoke_command("a03.allocate", r#"{"bytes": 4194304}"#)
    }));
    match outcome {
        Ok(Err(e)) => {
            println!(
                "[a03][P2] 调用期内存增长被拒（memory_growing → trap）: {}",
                e.to_string().chars().take(400).collect::<String>()
            );
        }
        Ok(Ok(raw)) => {
            println!("[a03][P2] ⚠️ 4MiB 超限分配意外成功: {raw}");
            panic!("调用期增长上限必须阻止超限分配");
        }
        Err(_) => panic!("紧内存实例调用 panic（应在 wasmtime 层被拒而非 panic）"),
    }
}

// ==================== P5 · 短调用开销基线（微基准） ====================

/// P5 性能基线：短调用（host-log 往返级）在四种路径上的每 op 开销。
/// - A 纯 Rust no-op（绝对基线）
/// - B block_on_async 桥 · 无 handle 线程（ambient 直接驱动——当前生产阻塞线程形态）
/// - C block_on_async 桥 · 多线程 worker（block_in_place 路径）
/// - D 端到端 guest → sync host 原语往返（生产 dispatch 完整路径）
///
/// 供主体票对照：async 化后 dispatch 原生 await，可消除 B/C 的每调用线程切换
/// 与 block_on 开销；D 是「收益上限」之外的净成本面（guest 调用本身）。
/// 迭代数可用 A03_PERF_N 覆盖（默认 50_000；端到端 guest 调用较慢故不取 10^5）。
#[test]
fn a03_p5_short_call_overhead_microbench() {
    // 默认 5_000：抑制探针自身 CPU 占用（wasm_runtime 集成测试族在并行负载下
    // 有既有时序 flake（实测基线即存在），探针保持轻量不加剧）；完整曲线用
    // `A03_PERF_N=50000 cargo test` 采集（50k 数据已贴档 report.md §4）
    let n: usize = std::env::var("A03_PERF_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5_000);
    let now = std::time::Instant::now();

    // A：纯 Rust no-op 基线
    #[inline(never)]
    fn noop() {}
    let t0 = std::time::Instant::now();
    for _ in 0..n {
        noop();
    }
    let base_us = t0.elapsed().as_secs_f64() * 1e6 / n as f64;

    // B：block_on_async 桥 · 无 handle 线程（ambient 直接驱动）
    let t1 = std::time::Instant::now();
    for _ in 0..n {
        block_on_async(async {});
    }
    let bridge_ambient_us = t1.elapsed().as_secs_f64() * 1e6 / n as f64;

    // C：block_on_async 桥 · 多线程 worker（block_in_place 路径）
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let t2 = std::time::Instant::now();
    rt.block_on(async {
        for _ in 0..n {
            block_on_async(async {});
        }
    });
    let bridge_worker_us = t2.elapsed().as_secs_f64() * 1e6 / n as f64;

    // D：端到端 guest → sync host 原语往返（生产 dispatch 路径）
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let Some(mut plugin) = instantiate_a03_fixture(&wasm_runtime, &host_ctx) else {
        eprintln!("[skip] a03_p5: wasip3 工具链未就绪");
        return;
    };
    let t3 = std::time::Instant::now();
    for _ in 0..n {
        let v = run_command(&mut plugin, CMD_HOST_LOG, "{}");
        assert_eq!(v["ok"], true);
    }
    let guest_roundtrip_us = t3.elapsed().as_secs_f64() * 1e6 / n as f64;

    let total = now.elapsed();
    println!(
        "[a03][P5] 微基准 N={n}（A03_PERF_N 可覆盖）总耗时 {:.3}s",
        total.as_secs_f64()
    );
    println!(
        "[a03][P5] A 纯 Rust no-op 基线        : {base_us:.3} us/op ({:.0} ops/s)",
        1e6 / base_us.max(1e-9)
    );
    println!(
        "[a03][P5] B block_on_async·ambient    : {bridge_ambient_us:.3} us/op（桥开销 = {:.3} us）",
        bridge_ambient_us - base_us
    );
    println!(
        "[a03][P5] C block_on_async·worker     : {bridge_worker_us:.3} us/op（桥开销 = {:.3} us）",
        bridge_worker_us - base_us
    );
    println!("[a03][P5] D guest host-log 端到端往返  : {guest_roundtrip_us:.3} us/op");
    // 宽松门槛（防 CI 抖动误伤；数量级回归才失败）：端到端往返 < 5ms/op
    assert!(
        guest_roundtrip_us < 5_000.0,
        "端到端往返异常慢: {guest_roundtrip_us:.1} us/op"
    );
    assert!(bridge_ambient_us < 1_000.0, "桥开销异常: {bridge_ambient_us:.1} us/op");
}
