//! WASM 插件实例化后的激活 / 调用 / 重载 / 定时器 / 监听器用例（含组件 fixture 脚手架）。

use super::scaffold::*;
use super::*;

// ==================== WASM 插件（真实组件测试插件） ====================

/// 将 wit-bindgen 产出的 core module 编码为组件
/// （与 wasm_runtime.rs 测试同策略，等价于 `wasm-tools component new`）

/// 构建测试用组件插件并编码为组件（packages/plugin-component-test）
pub(super) fn build_test_component() -> Vec<u8> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packages_dir = manifest_dir.join("../packages");
    let plugin_dir = packages_dir.join("plugin-component-test");

    let output_dir = plugin_dir.join("target/wasm32-wasip3/release");
    let module_path = output_dir.join("bedcode_plugin_component_test.wasm");

    if module_path.exists() {
        let src_files = [
            plugin_dir.join("src/lib.rs"),
            packages_dir.join("plugin-sdk-desktop/rust/wit/bedcode.wit"),
        ];
        let module_modified = std::fs::metadata(&module_path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let needs_rebuild = src_files.iter().any(|f| {
            std::fs::metadata(f)
                .and_then(|m| m.modified())
                .map(|t| t > module_modified)
                .unwrap_or(true)
        });

        if !needs_rebuild {
            return std::fs::read(&module_path).expect("Failed to read test component module");
        }
    }

    let manifest_path = plugin_dir.join("Cargo.toml");
    let status = std::process::Command::new("cargo")
        .env("RUSTUP_TOOLCHAIN", crate::wasm_core::manager::runtime::WASIP3_NIGHTLY)
        .args([
            "build",
            "--target",
            "wasm32-wasip3",
            "--release",
            "--manifest-path",
            manifest_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to run cargo build for test component");
    assert!(status.success(), "Test component WASM build failed");

    std::fs::read(&module_path).expect("Failed to read test component after build")
}

/// 将组件形态测试插件实例化并注入宿主（plugins + wasm_plugins 双表）
///
/// 返回插件 ID；组件 invoke 内 host_storage 读回的 key 预写入
/// `component-test-key`。extension_path 指向临时目录（invoke 的
/// resource_dir 注入断言用）。
pub(super) async fn setup_wasm_plugin(host: &PluginHost, tmp_dir: &tempfile::TempDir) -> String {
    let component = host
        .wasm_runtime()
        .compile_component(&build_test_component())
        .expect("compile test component");
    let plugin = host
        .wasm_runtime()
        .instantiate_component(&component, TEST_WASM_PLUGIN_ID, host.wasm_host_ctx().clone(), &[], None)
        .expect("instantiate test component");

    host.storage()
        .set(TEST_WASM_PLUGIN_ID, "component-test-key", json!({"k": "v"}))
        .await
        .expect("preset storage key");

    let extension_path = tmp_dir.path().to_string_lossy().to_string();
    host.wasm_plugins
        .write()
        .await
        .insert(TEST_WASM_PLUGIN_ID.to_string(), Arc::new(Mutex::new(plugin)));

    let mut loaded = make_plugin(TEST_WASM_PLUGIN_ID, PluginSource::Wasm, PluginState::Loaded);
    loaded.manifest.rust_library = "bedcode_plugin_component_test".to_string();
    loaded.extension_path = extension_path;
    host.plugins
        .write()
        .await
        .insert(TEST_WASM_PLUGIN_ID.to_string(), loaded);

    TEST_WASM_PLUGIN_ID.to_string()
}

/// v8 契约端到端（宿主侧）：on_startup 自报失败 → Degraded 终态；
/// 移除故障开关后重试 → Activated；随后停用干净回落。
/// 失败开关为组件测试插件的 storage key `component-test-fail-startup`
#[tokio::test(flavor = "multi_thread")]
async fn test_activate_degraded_on_startup_failure_then_retry_recovers() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let pid = setup_wasm_plugin(&host, &tmp_dir).await;

    // 预置启动失败开关：激活调用本身成功，但终态必须如实落 Degraded
    host.storage()
        .set(&pid, "component-test-fail-startup", json!(true))
        .await
        .expect("preset fail-startup switch");
    host.activate_plugin(&pid, false)
        .await
        .expect("activation call must succeed even when on_startup reports failure");
    let info = host.get_plugin(&pid).await.unwrap();
    assert!(
        matches!(&info.state, PluginState::Degraded(reason) if reason.contains("simulated startup init failure")),
        "expected Degraded with guest-reported reason, got {:?}",
        info.state
    );

    // 持久化意图映射：Degraded 视为已启用（下次启动仍重试）
    let activated_state = host.get_activated_state().await;
    assert_eq!(
        activated_state.get(&pid),
        Some(&true),
        "degraded counts as enabled intent"
    );

    // 重试激活（移除开关）→ 回到 Activated
    host.storage()
        .delete(&pid, "component-test-fail-startup")
        .await
        .expect("clear fail-startup switch");
    host.activate_plugin(&pid, false)
        .await
        .expect("retry activation must succeed");
    let info = host.get_plugin(&pid).await.unwrap();
    assert_eq!(info.state, PluginState::Activated);

    // 激活态停用干净回落
    host.deactivate_plugin(&pid, false)
        .await
        .expect("deactivate after recovery ok");
    let info = host.get_plugin(&pid).await.unwrap();
    assert_eq!(info.state, PluginState::Deactivated);
}

/// 持久化写入路径：persist=true 时 Degraded 以 true 落库（用户意图语义）
#[tokio::test(flavor = "multi_thread")]
async fn test_persisted_intent_keeps_degraded_enabled() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let pid = setup_wasm_plugin(&host, &tmp_dir).await;

    host.storage()
        .set(&pid, "component-test-fail-startup", json!(true))
        .await
        .expect("preset fail-startup switch");
    host.activate_plugin(&pid, true)
        .await
        .expect("activation call must succeed");

    let map = host.storage().load_activated_plugins().await.unwrap();
    assert_eq!(
        map.get(&pid),
        Some(&true),
        "degraded plugin must persist as enabled intent"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_wasm_plugin_activate_invoke_deactivate() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let pid = setup_wasm_plugin(&host, &tmp_dir).await;

    // 激活：调用组件 __bedcode_activate + on_startup，无错误码
    host.activate_plugin(&pid, false).await.unwrap();
    assert!(host.is_activated(&pid).await);

    // 命令调用：组件 echo + host_storage 读回 + resource_dir 自动注入
    let result = host
        .invoke_rust_command(&pid, "test.echo", json!({"hello": "host"}))
        .await
        .unwrap();
    assert_eq!(result["name"], "test.echo");
    assert_eq!(result["stored"], json!({"k": "v"}));
    let args: serde_json::Value = serde_json::from_str(result["args"].as_str().unwrap()).unwrap();
    assert_eq!(args["resource_dir"], json!(tmp_dir.path().to_string_lossy()));

    // 停用：调用组件 on_shutdown + __bedcode_deactivate
    host.deactivate_plugin(&pid, false).await.unwrap();
    assert!(!host.is_activated(&pid).await);
    assert_eq!(host.get_plugin(&pid).await.unwrap().state, PluginState::Deactivated);

    // 停用后调用被门禁拒绝
    let err = host
        .invoke_rust_command(&pid, "test.echo", json!({}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not activated"));
}

// 票 03 删除的用例 `test_dispatch_lifecycle_and_input_to_wasm_plugin`：
// 它覆盖的「事件 → payload → dispatch → wasm 回调」链路随宿主观察面退役
// （派发点、监听器实现、注册表三处同批删除），无对象可测。
// 插件导出的 `on_session_lifecycle` / `on_input_submitted` 仍由 SDK 默认实现兜底，
// 其 WIT 面随票 10 整 interface 删除时收口。

#[tokio::test(flavor = "multi_thread")]
async fn test_reload_wasm_plugin_cycle() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    // 把组件字节写入临时插件目录（reload 从文件重新加载）
    let wasm_bytes = build_test_component();
    let wasm_path = tmp_dir.path().join("bedcode_plugin_component_test.wasm");
    std::fs::write(&wasm_path, &wasm_bytes).unwrap();

    let pid = setup_wasm_plugin(&host, &tmp_dir).await;
    host.activate_plugin(&pid, false).await.unwrap();

    // 完整卸载-重载-激活循环
    host.reload_wasm_plugin(&pid).await.unwrap();
    assert!(host.is_activated(&pid).await);

    // 重载后的新实例可用
    let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
    assert_eq!(result["name"], "test.echo");
}

// ==================== PluginServices（可测部分） ====================

#[tokio::test(flavor = "multi_thread")]
async fn test_plugin_timer_register_replace_abort() {
    let host = setup_host().await;
    host.plugins.write().await.insert(
        TEST_PLUGIN_ID.to_string(),
        make_plugin(TEST_PLUGIN_ID, PluginSource::FileScan, PluginState::Activated),
    );

    // 注册定时器（3600s 间隔：测试期间不会触发 tick 回调）
    PluginServices::register_plugin_timer(&host, TEST_PLUGIN_ID.to_string(), 3600, "tick".to_string());
    assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

    // 重复注册替换旧句柄（v6 ADR 0003：同一插件仅保留一个定时器）
    PluginServices::register_plugin_timer(&host, TEST_PLUGIN_ID.to_string(), 3600, "tick".to_string());
    assert_eq!(host.plugin_timers.lock().unwrap().len(), 1);

    // 停用中止定时器（不再到点回调）
    host.deactivate_plugin(TEST_PLUGIN_ID, false).await.unwrap();
    assert!(host.plugin_timers.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_schedule_plugin_reload_throttle() {
    let host = setup_host().await;
    // 未激活插件：调度后后台任务直接退出，仅验证限频表行为
    host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
    {
        let throttle = host.wasm_reload_throttle.lock().unwrap();
        assert!(throttle.contains_key(TEST_PLUGIN_ID));
    }
    // 30 秒窗口内再次调度被限频跳过：不新增条目
    host.schedule_plugin_reload_after_trap(TEST_PLUGIN_ID);
    {
        let throttle = host.wasm_reload_throttle.lock().unwrap();
        assert_eq!(throttle.len(), 1);
    }
}

// 票 03 删除的用例 `test_session_listener_registration_via_services`：
// `PluginServices` 的两条注册面（生命周期 / 输入）已随宿主观察面退役，
// 注册表与派发点不复存在，本用例无对象可测。

// ==================== 内核会话域防回接锁（票 11） ====================

/// 源码扫描锁：宿主侧不得再出现内核会话域的任何符号。
///
/// 票 11 删除了 `src/session/` 整个目录（`SessionManager` / `SessionConfigManager` /
/// `SessionOutputManager` / `GlobalOutputManager` / `SessionInfoRegistry` / 环与
/// 订阅者实现）。这不是「换了个位置放」——会话真源在 `com.bedcode.terminal-session`
/// 登记域，宿主只剩 `host-pty`（引擎）+ `host-connection`（连接清单）+ `session_gateway`
/// （互调窄转发层）。谁把内核会话对象加回来，谁就要先推翻票 11 的裁定。
///
/// 与票 03 的锁同理：只扫**非注释行**（各模块里「为什么删」的说明段落是记账），
/// 且跳过两张锁自身（`sync_handler.rs` 与本文件——它们把标识符当字符串去匹配）。
#[test]
fn retired_kernel_session_domain_is_not_reintroduced() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations: Vec<String> = Vec::new();

    let forbidden: [&str; 8] = [
        "crate::session::",
        "SessionManager",
        "SessionConfigManager",
        "SessionOutputManager",
        "GlobalOutputManager",
        "SessionInfoRegistry",
        "SessionOutputSink",
        "UnifiedOutputQueue",
    ];

    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // 两张锁自身：它们以字符串形式携带这些标识符去匹配
            if path.ends_with("wasm_flow_test.rs") || path.ends_with("sync_handler.rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            for (idx, raw_line) in content.lines().enumerate() {
                let line = raw_line.trim_start();
                if line.starts_with("//") || line.starts_with("///") {
                    continue;
                }
                for needle in forbidden {
                    if line.contains(needle) {
                        violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "内核会话域（票 11 已整目录删除）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

// ==================== 对等传输编排域防回接锁（传输编排下沉票 3） ====================

/// 源码扫描锁：宿主 peer 域不得再把传输任务编排状态机接回来。
///
/// 票 3 把发送/接收任务编排整体下沉 `file-transfer` 插件（事件归约状态机，
/// 真源在其私有库）：宿主 `peer_engine_*` 收敛为「会话句柄表 + 引擎事件桥」——
/// 旧版所持的任务 DTO 形状（`PeerTransferDto`）、并发闸门（`pick_pending_to_start`
/// / `pump_send_queue`）、历史封顶（`HISTORY_CAP` / `RECEIVE_TERMINAL_CAP`）、
/// serve 供流记账（`register_serve_task` / `settle_serve_terminal`）、pull 任务行
/// 预登记（`register_remote_pull`）、peer_name 解析（`resolve_peer_name`）、
/// 批量恢复编排（`peer_resume_all_transfers` / `resume_all_transfers_for_plugin`）
/// 与并发脉冲（`set_transfer_concurrency_for_plugin`）一并退役。谁把这些加回来，
/// 谁就要先推翻票 3 裁决。
///
/// 与票 11 的锁同理：只扫**非注释行**（模块头「为什么删」的说明段落是记账），
/// 且跳过锁自身。fail-visible 双保险之一——另一保险是 `send-files` 载荷
/// `concurrency` 字段检测（host_api/peer.rs，行为级）。
#[test]
fn retired_peer_transfer_orchestration_is_not_reintroduced() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations: Vec<String> = Vec::new();

    let forbidden: [&str; 15] = [
        "PeerTransferDto",
        "PeerTransferFileDto",
        "register_remote_pull",
        // 带冒号精确匹配常量名（避免误中 server 指标的 METRICS_HISTORY_CAPACITY）
        "HISTORY_CAP:",
        "RECEIVE_TERMINAL_CAP",
        "pick_pending_to_start",
        "pump_send_queue",
        "running_send_count_locked",
        "register_serve_task",
        "settle_serve_terminal",
        "set_serve_pause_status",
        "evict_history_cap_locked",
        "evict_terminal_cap_locked",
        "resume_all_transfers_for_plugin",
        "set_transfer_concurrency_for_plugin",
    ];

    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // 锁自身：以字符串形式携带这些标识符去匹配
            if path.ends_with("wasm_flow_test.rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            for (idx, raw_line) in content.lines().enumerate() {
                let line = raw_line.trim_start();
                if line.starts_with("//") || line.starts_with("///") {
                    continue;
                }
                for needle in forbidden {
                    if line.contains(needle) {
                        violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "对等传输编排域（票 3 已整体下沉插件）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

// ==================== 退役面防回接锁（票 03） ====================

/// 源码扫描锁：宿主侧不得再把「注册了就能收到会话回调」的观察面接回来。
///
/// 票 03 的裁决是**整体退役**（不换成总线 topic）：这两条通道在 P1-b 真源下沉后
/// 生产流量归零，留着「代码在、永远不触发」是下一处断链的种子。本锁把裁决变成
/// 可执行判据——谁把注册表 / 派发点 / 修饰链加回来，谁就要先推翻票 03 的裁定。
///
/// 只扫「宿主源码里的定义与调用」，不扫注释：本文件与各模块的说明段落里出现这些
/// 名字是**记账**（说清为什么删），不是回接。
#[test]
fn retired_session_observation_surface_is_not_reintroduced() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations: Vec<String> = Vec::new();

    // 逐条列出「退役面的形状」。命中即红。
    let forbidden: [&str; 6] = [
        "register_lifecycle_listener",
        "register_input_listener",
        "register_session_lifecycle_listener",
        "register_session_input_listener",
        "dispatch_lifecycle_to_plugin",
        "dispatch_input_to_plugin",
    ];

    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            // 本文件是锁自身，跳过（避免自匹配）
            if path.ends_with("wasm_flow_test.rs") {
                continue;
            }
            for (idx, raw_line) in content.lines().enumerate() {
                let line = raw_line.trim_start();
                if line.starts_with("//") {
                    continue;
                }
                for needle in forbidden {
                    if line.contains(needle) {
                        violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "会话观察面（票 03 已整体退役）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}
