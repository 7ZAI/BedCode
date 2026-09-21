//! WASI preopen 与 wasip3 产物闭环
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
/// WASI 预打开端到端：wasip2 插件经 std::fs 直写宿主预打开目录
///
/// 验证链路：manifest 声明 wasiPreopenDirs（已授权）→
/// 实例化时宿主 preopen /data → 插件 std::fs::write("/data/demo.txt") →
/// 宿主侧校验文件落盘 + 读回 + 沙箱边界（根外路径不可达）。
#[test]

fn test_wasi_preopen_std_fs_e2e() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&build_wasi_test_component())
        .expect("compile wasi test component");
    let pid = "com.bedcode.wasi-test";

    let rt = tokio::runtime::Runtime::new().unwrap();
    // 阶段 1（runtime 上下文）：宿主侧准备——授权 + 实例化。
    // 实例化需当前 handle（resolve_preopen_dirs 校验授权）；
    // 组件 ctor 不触发 wasi 文件访问，故此时有 handle 仍安全。
    let (mut plugin, dir) = rt.block_on(async {
        // 授权插件（storage 权限用于 seed fs_granted_paths）
        host_ctx.permission.grant_permissions(pid, &["storage".to_string()]);
        let dir = tempfile::tempdir().expect("tempdir");
        crate::plugin::manager::wasm_runtime::host_impl::storage::storage_set(
            &host_ctx,
            pid,
            "fs_granted_paths",
            serde_json::json!([dir.path().to_string_lossy()]),
        )
        .expect("seed granted path");

        // 实例化：组件导入 wasi 接口，宿主按声明（授权过滤后）preopen /data
        let declared = vec![dir.path().to_string_lossy().to_string()];
        let plugin = wasm_runtime
            .instantiate_component(&component, pid, host_ctx.clone(), &declared, None)
            .expect("instantiate wasi test component");
        (plugin, dir)
    });

    // 阶段 2（无 handle 阻塞线程）：guest 经 std::fs 访问 preopen 目录。
    // 与生产 run_guest_call 对齐——wasi 同步绑定（in_tokio）要求调用线程
    // 不处于任何 tokio runtime 内，否则 "Cannot start a runtime..." panic。
    std::thread::spawn(move || {
        // 1. 插件经 std::fs 直写 /data/demo.txt → 宿主侧落盘校验
        let r = plugin
            .invoke_command("wasi-test.write-file", "{}")
            .expect("write command");
        assert!(serde_json::from_str::<serde_json::Value>(&r)
            .unwrap()
            .get("ok")
            .and_then(|v| v.as_bool())
            .unwrap_or(false));
        let host_file = dir.path().join("demo.txt");
        assert_eq!(
            std::fs::read_to_string(&host_file).expect("host must see the file"),
            "hello-from-wasi"
        );

        // 2. 读回（guest 内同路径）
        let r = plugin
            .invoke_command("wasi-test.read-file", "{}")
            .expect("read command");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&r)
                .unwrap()
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or(""),
            "hello-from-wasi"
        );

        // 3. 列举 preopen 根目录，demo.txt 可见
        let r = plugin.invoke_command("wasi-test.list", "{}").expect("list command");
        let entries = serde_json::from_str::<serde_json::Value>(&r)
            .unwrap()
            .get("entries")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            entries.iter().any(|e| e.as_str() == Some("demo.txt")),
            "preopen dir entries must include demo.txt, got {:?}",
            entries
        );

        // 4. 沙箱边界：preopen 根外路径不可达（WASI 能力沙箱）
        let r = plugin
            .invoke_command("wasi-test.outside-root", "{}")
            .expect("outside command");
        let leaked = serde_json::from_str::<serde_json::Value>(&r)
            .unwrap()
            .get("leaked")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        assert!(!leaked, "WASI sandbox must block access outside preopen root");
    })
    .join()
    .expect("guest call thread panicked");
}

/// 票 02 A1：wasip3 fixture async 闭环（本地装有 pinned nightly + wasm32-wasip3
/// target 时执行；未装则跳过——CI stable 无该 target）
///
/// 断言链（/tmp/wasip3-probe 场景 1-3 实证的机制落地到宿主测试）：
/// - 实例化走 async 路径（instantiate_async）：wasip3 组件导入 async wasi 0.3
///   函数，Store 为 async-required，同步实例化会报错——能实例化即证明 async 化
/// - `read-clock`：wasi:clocks 接口在 async 语义下可读（SystemTime 走 clocks）
/// - `get-random`：async `wasi:random` get-random-bytes 返回熵——非空、非全零、
///   两次调用结果不同（同一实例两次独立调用，证明每次都是新鲜熵）
#[test]

fn test_wasip3_fixture_async_closure() {
    let Some(component_bytes) = build_wasip3_test_component() else {
        eprintln!(
            "[skip] wasip3 工具链（{} + wasm32-wasip3）未安装，先执行 scripts/wasip3-toolchain.sh install",
            WASIP3_NIGHTLY
        );
        return;
    };
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let component = wasm_runtime
        .compile_component(&component_bytes)
        .expect("compile wasip3 test component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, "com.bedcode.wasip3-test", host_ctx, &[], None)
        .expect("instantiate wasip3 component (async store)");

    // 时钟（wasi:clocks，async 语义下可读）
    let r = plugin
        .invoke_command("wasip3-test.read-clock", "{}")
        .expect("clock command");
    let unix_ms = serde_json::from_str::<serde_json::Value>(&r)
        .unwrap()
        .get("unix_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(unix_ms > 0, "async clock must be readable, got {}", unix_ms);

    // 熵（async wasi:random get-random-bytes）
    let r1 = plugin
        .invoke_command("wasip3-test.get-random", "{}")
        .expect("random command 1");
    let hex1 = serde_json::from_str::<serde_json::Value>(&r1)
        .unwrap()
        .get("hex")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert_eq!(hex1.len(), 64, "32 字节 → 64 hex 字符");
    assert!(hex1.chars().any(|c| c != '0'), "entropy must not be all zeros");

    // 同一实例第二次调用：结果必须不同（每次新鲜熵，非缓存/伪随机重复）
    let r2 = plugin
        .invoke_command("wasip3-test.get-random", "{}")
        .expect("random command 2");
    let hex2 = serde_json::from_str::<serde_json::Value>(&r2)
        .unwrap()
        .get("hex")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert_ne!(hex1, hex2, "two get-random calls must differ");
}
