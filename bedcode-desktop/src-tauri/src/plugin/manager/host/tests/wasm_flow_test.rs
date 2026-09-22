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
        .env("RUSTUP_TOOLCHAIN", crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY)
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

#[tokio::test(flavor = "multi_thread")]
async fn test_dispatch_lifecycle_and_input_to_wasm_plugin() {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let host = setup_host().await;
    let pid = setup_wasm_plugin(&host, &tmp_dir).await;

    // 未激活：dispatch 被门禁静默丢弃（无 panic、不触碰 store）
    host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
    host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s1", "text": "hi"}));

    host.activate_plugin(&pid, false).await.unwrap();

    // 经真实 listener 走「事件 → payload 构造 → dispatch → wasm 回调」全链路
    let lifecycle_listener = PluginLifecycleListener::new(pid.clone(), host.clone());
    SessionLifecycleListener::on_session_lifecycle(
        &lifecycle_listener,
        &SessionLifecycleEvent::Created {
            session_id: "s1".to_string(),
            config_id: "c1".to_string(),
            name: "n".to_string(),
            working_dir: "/tmp".to_string(),
        },
    );
    let input_listener = PluginInputListener::new(pid.clone(), host.clone());
    SessionInputListener::on_input_submitted(&input_listener, "s1", "echo hi");

    // 直接分发不同 payload 形态
    host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "stopped", "sessionId": "s1"}));
    host.dispatch_input_to_plugin(&pid, &json!({"sessionId": "s2", "text": "ls"}));

    // store 未被污染：分发全部成功后 command 调用仍可用
    let result = host.invoke_rust_command(&pid, "test.echo", json!({})).await.unwrap();
    assert_eq!(result["name"], "test.echo");

    // 停用后 dispatch 门禁丢弃，invoke 拒绝
    host.deactivate_plugin(&pid, false).await.unwrap();
    host.dispatch_lifecycle_to_plugin(&pid, &json!({"type": "created"}));
    assert!(host.invoke_rust_command(&pid, "test.echo", json!({})).await.is_err());
}

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

#[tokio::test(flavor = "multi_thread")]
async fn test_session_listener_registration_via_services() {
    let host = setup_host().await;
    let session_manager = host.wasm_host_ctx().session_manager_arc();

    // PluginServices 实现的注册路径（listener 构造 + block_on_async 注册）。
    // 注册结果在 SessionManager 内部（无公开查询接口），此处验证不 panic、
    // 且注册的 listener 可被停用流程按 plugin_id 摘除
    PluginServices::register_session_lifecycle_listener(&host, TEST_PLUGIN_ID.to_string(), session_manager.clone());
    PluginServices::register_session_input_listener(&host, TEST_PLUGIN_ID.to_string(), session_manager.clone());

    // listener 自身携带正确 plugin_id（停用摘除依赖此标识）
    let l1 = PluginLifecycleListener::new(TEST_PLUGIN_ID.to_string(), host.clone());
    assert_eq!(l1.plugin_id(), TEST_PLUGIN_ID);
    assert_eq!(SessionLifecycleListener::plugin_id(&l1), Some(TEST_PLUGIN_ID));
    let l2 = PluginInputListener::new(TEST_PLUGIN_ID.to_string(), host);
    assert_eq!(SessionInputListener::plugin_id(&l2), Some(TEST_PLUGIN_ID));
}
