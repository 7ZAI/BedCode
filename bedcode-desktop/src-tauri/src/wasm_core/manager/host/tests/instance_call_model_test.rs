//! 装配条目 × 调用模型对照用例（票 06 P1 第 3 步：门面接线 + 两模型对照）
//!
//! 同一用例在 `mutex`（现状）与 `event-loop`（新模型）下各跑一遍：命令往返、
//! 停用 → 再激活（属主已停必须重建）、trap 错误文案与失败可见性必须一致。
//! 另配结构锁钉死 I1 的装配形态（Store 只经装配条目、属主启动单点、能力导出
//! 不再有直锁调用）。
//!
//! 约定：模型经 `install_instance(..., call_model)` 显式装配——不依赖全局配置，
//! 两种模型可并行跑（`call_model` 配置本身的默认值/解析由 `config.rs` 单测覆盖）。

use super::scaffold::*;
use super::wasm_flow_test::*;
use super::*;

/// 对照用调用模型列（mutex = 现状回退窗口；event-loop = 新模型）
const MODELS: [CallModel; 2] = [CallModel::Mutex, CallModel::EventLoop];

/// 把夹具组件字节落到临时插件目录
///
/// `rebuild_wasm_instance` 从**磁盘**重新加载（`<extension_path>/<rust_library>.wasm`），
/// 而 `setup_wasm_plugin_with_model` 只从字节实例化；不落盘则重建路径（再激活 /
/// 热重载 / trap 自愈）在测试里必然失败——那不是被测语义。
fn materialize_fixture_wasm(tmp_dir: &tempfile::TempDir) {
    let path = tmp_dir.path().join("bedcode_plugin_component_test.wasm");
    std::fs::write(&path, build_test_component()).expect("write fixture wasm");
}

/// 命令往返：两模型下都必须成功，且返回值一致（宿主 import 往返也走通）
#[tokio::test(flavor = "multi_thread")]
async fn command_round_trip_works_in_both_call_models() {
    for model in MODELS {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let host = setup_host_shared().await;
        let pid = setup_wasm_plugin_with_model(&host, &tmp_dir, model).await;
        materialize_fixture_wasm(&tmp_dir);

        let entry = host.get_instance(&pid).await.expect("装配条目必须存在");
        assert_eq!(entry.call_model(), model, "模型必须按装配参数生效");

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
                host.owner_cleanup_skipped(),
                1,
                "event-loop 停用必须记一次 guest 清理跳过（不得静默跳过）"
            ),
            CallModel::Mutex => assert_eq!(host.owner_cleanup_skipped(), 0, "mutex 模型不得计数"),
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
        let msg = err.error.to_string();
        assert!(
            msg.contains("WASM invoke_command() call failed"),
            "model={:?} trap 文案必须与现状一致，实际：{msg}",
            model
        );
        assert!(
            err.panic_message.is_none(),
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
        entry.shutdown().await.map(|report| {
            assert!(!report.forced_abort, "model={:?} 停止不得超时 abort", model);
            report
        });
    }
}

/// 结构锁（I1）：装配条目是 Store 的唯一宿主；属主启动单点；能力导出/调用面
/// 不再有绕过门面的直锁访问
///
/// 动态用例无法在编译期钉住「不存在第二个 Store 入口」，故按源码形态锁死：
/// 一旦有人在别处 `spawn_owner` / 重新引入 `wasm_plugins` 直锁取实例或
/// `call_capability_export` 直调，本锁立即转红。
#[test]
fn instance_slot_structural_lock() {
    let host_src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/wasm_core/manager/host.rs"))
        .expect("read host.rs");
    let owner_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/wasm_core/manager/host/owner.rs"
    ))
    .expect("read owner.rs");
    let commands_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/wasm_core/manager/host/commands.rs"
    ))
    .expect("read commands.rs");
    let capability_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/wasm_core/manager/capability.rs"
    ))
    .expect("read capability.rs");

    assert!(
        host_src.contains("wasm_plugins: Arc<RwLock<HashMap<String, Arc<WasmInstanceEntry>>>>"),
        "装配表必须只存装配条目（Store 只能存在于 InstanceSlot 内——I1）"
    );
    assert!(
        host_src.contains("enum InstanceSlot {"),
        "调用模型必须收敛在 InstanceSlot 枚举（mutex / event-loop 双形态）"
    );
    assert_eq!(
        host_src.matches("spawn_owner(").count(),
        1,
        "属主任务启动必须单点（只在装配条目构造里），且宿主侧不得有第二个 Store 入口"
    );
    assert!(
        host_src.contains("dispatch_mutex_op("),
        "mutex 分支必须委派既有导出方法（错误文案逐字等价 = I5 by construction）"
    );

    // 门面之外不得再有直锁实例的调用面（改造前 `call_plugin_capability_export`
    // 是唯一绕过 trap 封装的调用点，已并入 call_guest）
    assert!(
        !commands_src.contains("call_capability_export"),
        "命令面必须经统一门面（不得直锁实例调能力导出）"
    );
    assert!(
        !capability_src.contains("call_capability_export"),
        "能力转发必须经窄端口 CapabilityTarget（不得直锁提供者实例）"
    );
    assert!(
        capability_src.contains("target.storage_get(") && capability_src.contains("target.storage_set("),
        "能力转发必须走端口（超时兜底在端口实现内）"
    );

    // 属主循环形态：常驻 run_concurrent + 单点 start + biased（详见 owner_i4_structural_lock）
    assert_eq!(
        owner_src.matches("run_concurrent(").count(),
        1,
        "属主模型必须只有一处常驻 run_concurrent 作用域"
    );

    // fuel / 指标口径（spec §3.3「task 级计量」）：续费在 start 前发生、
    // 计时器绑到 task 生命周期（start → finish），不得退回「每调用一把锁」的形态
    assert!(
        owner_src.contains("refill_fuel(&mut access"),
        "燃料续费必须在 start 前同步发生（实例级预算，续费点差值记账）"
    );
    assert!(
        owner_src.contains("let _timer = timer;"),
        "调用计时器必须随 guest task 存活（start → finish 生命周期口径）"
    );
}
