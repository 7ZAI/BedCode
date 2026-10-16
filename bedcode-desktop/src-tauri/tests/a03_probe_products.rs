//! a03 探针的产品产物闭环（P1-b / P1-c 段，wasm-core 纯净性收口票 05c 拆出）
//!
//! 原在 `manager/runtime/tests/a03_probe.rs`：`a03_p1b_wasip3_artifact_full_closed_loop`
//! 与 `a03_p1c_production_artifacts_async_store_no_change` 加载真实生产产物
//! （`resources/plugins/desktop/*.wasm`），用户裁定加载真实产物的跨 crate 集成测试
//! 全部归宿主侧 `src-tauri/tests/`。机制用例（P1-a / P1-c 同步调用实证 / P2 燃料与
//! 资源限制 / P5 微基准）留在 wasm-core `a03_probe.rs`。
//!
//! 注册表闸门：P1-b 的 activate/deactivate 写进程级单中心注册表（ADR 0031 K1），
//! 本文件持 `hold_registry_desk` permit 只包写表那几行（与 wasm-core 内同口径）。

use std::sync::Arc;

use bedcode_desktop_lib::wasm_core::manager::runtime::LoadedWasmPlugin;
use bedcode_desktop_lib::wasm_core::test_support::{hold_registry_desk, setup_wasm_runtime};

/// 执行命令并解析 JSON（探针统一收口）
fn run_command(plugin: &mut LoadedWasmPlugin, name: &str, args: &str) -> serde_json::Value {
    let raw = plugin.invoke_command(name, args).unwrap_or_else(|e| {
        panic!("[a03] invoke_command({name}) 失败: {e}");
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("[a03] 命令返回非法 JSON: {e} ({raw})"))
}

/// P1-b：真实 terminal-session wasip3 产物全链路闭环（activate → status → manifest
/// → deactivate）——all imports must resolve + 注册表闸门写表。
#[test]
fn a03_p1b_wasip3_artifact_full_closed_loop() {
    // 认证中心注册表闸门（ADR 0031 K1 单中心 desk）：同步用例用自带 runtime 取 permit
    let gate_rt = tokio::runtime::Runtime::new().expect("auth center gate runtime");
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
        ctx.set_plugin_db_root(Some(isolate));
    }
    // 会话登记域建表（P1-b 起阻断激活）需要 storage；会话面 host-pty 需要
    // pty:spawn/pty:io（与 manifest 权限表同源）
    host_ctx.grant_permissions(
        "com.bedcode.terminal-session",
        &[
            "auth".to_string(),
            "broadcast".to_string(),
            "fs:read".to_string(),
            "fs:write".to_string(),
            "peer".to_string(),
            "pty:spawn".to_string(),
            "pty:io".to_string(),
            "session:read".to_string(),
            "storage".to_string(),
            "task:run".to_string(),
            "terminal:input".to_string(),
            "terminal:observe".to_string(),
            "timer:schedule".to_string(),
            "ui:input".to_string(),
            "ui:settings".to_string(),
            "ui:sidebar".to_string(),
        ],
    );
    let mut plugin = wasm_runtime
        .load_plugin_from_file(
            &wasm_path,
            "com.bedcode.terminal-session",
            Arc::clone(&host_ctx),
            &[],
            None,
        )
        .expect("load wasip3 session: all imports must resolve");

    // guest `activate` 会 `auth-center-register` / `deactivate` 会 `auth-center-unregister`
    // → 写进程级单中心注册表（进程级单例）；本用例是同步 `#[test]`，故用自带
    // runtime 取闸门 permit，并只包住写表的那一行。
    {
        let _center_desk = gate_rt.block_on(hold_registry_desk());
        assert_eq!(plugin.activate().expect("activate"), 0, "activate 必须成功");
    }
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
    // v27（票 10）：终端 hooks 导出已删，不再断言透传语义。
    // manifest 往返
    let m: serde_json::Value = serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(m["id"], "com.bedcode.terminal-session");
    // guest `activate` 会 `auth-center-register` / `deactivate` 会 `auth-center-unregister`
    // → 写进程级单中心注册表（进程级单例）；本用例是同步 `#[test]`，故用自带
    // runtime 取闸门 permit，并只包住写表的那一行。
    {
        let _center_desk = gate_rt.block_on(hold_registry_desk());
        assert_eq!(plugin.deactivate().expect("deactivate"), 0, "deactivate 必须成功");
    }
    println!("[a03][P1-b] session 产物全链路（activate → status → hooks → manifest → deactivate）OK");
}

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