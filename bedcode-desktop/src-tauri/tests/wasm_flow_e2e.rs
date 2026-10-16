//! 宿主装配路径 × 调用模型 × 生命周期功能态（真实组件）
//!
//! 自 wasm-core 迁入（票 04 绑定归位，判决性探针实测：本族用例是**最后一批**
//! 依赖内核测试替身的夹具用例）：
//! - `manager/host/tests/wasm_flow_test.rs`：degraded 激活与恢复、持久化意图、
//!   激活/调用/停用闭环、热重载循环（4 条）；
//! - `manager/host/tests/instance_call_model_test.rs`：`mutex` / `event-loop`
//!   两调用模型对照——命令往返、停用再激活、trap 文案与 trap 后失败可见性（3 条）。
//!
//! **留在 wasm-core 的**：两原文件的纯机制部分（`PluginServices` 定时器 /
//! 重载限频、`instance_slot_structural_lock`、四张退役面源码扫描锁）不实例化夹具，
//! 随内核测试保留；本文件只接夹具功能态用例。
//!
//! 迁移改造点：`mod tests` → `mod support`；夹具读取改只读
//! `sdk_fixture_artifact_bytes("sdk")`；`entry.{call_model,shutdown}` /
//! `host.owner_cleanup_skipped()` 经 test_support 转发面取用（内核私有机制，
//! 生产 API 零变化）。

mod support;

use std::sync::Arc;

use bedcode_desktop_lib::wasm_core::manifest::PluginState;
use serde_json::json;
use support::*;

/// 两模型对照列（mutex = 现状回退窗口；event-loop = 新模型）
const MODELS: [CallModel; 2] = [CallModel::Mutex, CallModel::EventLoop];

/// 构造共享宿主（`Arc<PluginHost>`）
///
/// 宿主 support 的 `setup_host` 本就是 `Arc<PluginHost>`（`PluginHost::new` 真实
/// 装配路径），且 `PluginHost::new` 已按生产装配顺序绑定属主失败端口
/// （`owner_sink.bind`）——`event-loop` 实例的 trap 恢复路径依赖它，无需像内核
/// scaffold 那样手工补绑。本别名只为与内核 `setup_host_shared` 命名对齐。
async fn setup_host_shared() -> Arc<PluginHost> {
    setup_host().await
}

/// 构建测试用组件插件（合集 `packages/plugin-sdk-fixtures` `feature = "sdk"`）
///
/// 只读产物：构建由 wasm-core `fixture_keeper` 承担（宿主测试不建夹具）。
fn build_test_component() -> Vec<u8> {
    sdk_fixture_artifact_bytes("sdk")
}

/// 把夹具组件字节落到临时插件目录
///
/// `reload_wasm_plugin` 从**磁盘**重新加载（`<extension_path>/<rust_library>.wasm`），
/// 而 `setup_wasm_plugin_with_model` 只从字节实例化；不落盘则重建路径（再激活 /
/// 热重载 / trap 自愈）在测试里必然失败——那不是被测语义。
fn materialize_fixture_wasm(tmp_dir: &tempfile::TempDir) {
    let path = tmp_dir.path().join("bedcode_plugin_component_test.wasm");
    std::fs::write(&path, build_test_component()).expect("write fixture wasm");
}

// ==================== 生命周期 / 持久化意图（wasm_flow_test 迁入） ====================

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
        matches!(&info.state, PluginState::Degraded { error } if error.contains("simulated startup init failure")),
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

// ==================== 装配条目 × 调用模型对照（instance_call_model_test 迁入） ====================

/// 命令往返：两模型下都必须成功，且返回值一致（宿主 import 往返也走通）
#[tokio::test(flavor = "multi_thread")]
async fn command_round_trip_works_in_both_call_models() {
    for model in MODELS {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host_shared().await;
        let pid = setup_wasm_plugin_with_model(&host, &tmp_dir, model).await;
        materialize_fixture_wasm(&tmp_dir);

        let entry = host.get_instance(&pid).await.expect("装配条目必须存在");
        assert_eq!(instance_call_model(&entry), model, "模型必须按装配参数生效");

        host.activate_plugin(&pid, false).await.expect("activate");
        let value = host
            .invoke_rust_command(&pid, "test.echo", json!({"k": "v"}))
            .await
            .unwrap_or_else(|e| panic!("model={:?} invoke failed: {e}", model));
        assert_eq!(value["name"], "test.echo", "model={:?}", model);
        assert_eq!(
            value["stored"],
            json!({"k": "v"}),
            "model={:?} 宿主 import（host-storage）往返必须一致",
            model
        );

        host.deactivate_plugin(&pid, false).await.expect("deactivate");
        assert!(!host.is_activated(&pid).await, "model={:?}", model);
    }
}

/// 停用 → 再激活：`event-loop` 实例的属主已停（store 已丢）⇒ 必须先重建；
/// `mutex` 实例沿用原实例。两模型都必须能再次激活并正常服务（I5 / I6）
#[tokio::test(flavor = "multi_thread")]
async fn deactivate_then_reactivate_works_in_both_call_models() {
    for model in MODELS {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host_shared().await;
        let pid = setup_wasm_plugin_with_model(&host, &tmp_dir, model).await;
        materialize_fixture_wasm(&tmp_dir);

        host.activate_plugin(&pid, false).await.expect("first activate");
        host.deactivate_plugin(&pid, false).await.expect("deactivate");

        host.activate_plugin(&pid, false)
            .await
            .unwrap_or_else(|e| panic!("model={:?} 再激活必须成功（属主模型走重建）: {e}", model));
        let value = host
            .invoke_rust_command(&pid, "test.echo", json!({}))
            .await
            .unwrap_or_else(|e| panic!("model={:?} 再激活后 invoke failed: {e}", model));
        assert_eq!(value["name"], "test.echo", "model={:?}", model);

        // I3③ fail-visible：`event-loop` 的 guest 清理（on_shutdown/deactivate）被
        // 显性跳过并计数；`mutex` 实例照常执行这两条导出（不计数）
        match model {
            CallModel::EventLoop => assert_eq!(
                host_owner_cleanup_skipped(&host),
                1,
                "event-loop 停用必须记一次 guest 清理跳过（不得静默跳过）"
            ),
            CallModel::Mutex => assert_eq!(host_owner_cleanup_skipped(&host), 0, "mutex 模型不得计数"),
        }
    }
}

/// trap 语义（I3①）：两模型下 trap 文案必须与现状逐字等价（同前缀），
/// 且 trap 后实例不可用——后续调用显性失败，不静默成功、不排队等重载
#[tokio::test(flavor = "multi_thread")]
async fn trap_error_text_and_post_trap_failure_match_across_models() {
    for model in MODELS {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host_shared().await;
        let pid = setup_wasm_plugin_with_model(&host, &tmp_dir, model).await;
        let entry = host.get_instance(&pid).await.expect("装配条目必须存在");

        let err = entry
            .call_guest(GuestOp::InvokeCommand {
                name: "test.panic".to_string(),
                args_json: "{}".to_string(),
            })
            .await
            .expect_err("trap 必须显性失败");
        let msg = err.to_string();
        assert!(
            msg.contains("WASM invoke_command() call failed"),
            "model={:?} trap 文案必须与现状一致，实际：{msg}",
            model
        );
        assert!(
            err.panic_message().is_none(),
            "model={:?} guest trap 不得被当成宿主函数 panic",
            model
        );

        let second = entry
            .call_guest(GuestOp::InvokeCommand {
                name: "test.echo".to_string(),
                args_json: "{}".to_string(),
            })
            .await;
        assert!(
            second.is_err(),
            "model={:?} trap 后调用必须显性失败（实例已不可用），实际：{:?}",
            model,
            second.map(|reply| format!("{:?}", reply))
        );

        // 恢复路径可用（两模型同一条：停用 → 重建 → 再激活），且 store 随属主停止
        // 释放（mutex 模型无属主，shutdown 为 no-op 返回 None）
        instance_shutdown(&entry).await.map(|report| {
            assert!(!report.forced_abort, "model={:?} 停止不得超时 abort", model);
            report
        });
    }
}
