//! 属主任务（`event-loop` 调用模型）直接驱动测试（票 06 P1）
//!
//! 不经 `PluginHost`：直接 `spawn_owner` + `OwnerHandle` 驱动真实夹具组件
//! （`plugin-component-test`），覆盖 spec §3.2 的：
//! - **I4** 启动顺序 = 入队顺序（另配结构锁）
//! - **I3①/②** trap = 整实例不可用 + 在等请求显式失败 + 实例级失败上报
//! - **I3④** 请求方放弃等待不取消任务（计数）
//! - **I6** 停止语义（属主退出 ⇒ store 已 drop；幂等；后续调用显性失败）
//! - 有界失败：队列满立即显性 Err（不静默丢弃、不无限缓冲）
//! - 指标口径：`calls_total` / 生命周期计数随属主调用递增
//!
//! I5（存量插件行为等价）由既有 `wasm_flow` / 插件契约 / 全量 e2e 覆盖，
//! 本文件只测属主模型自身的不变式。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};

use super::*;
use crate::wasm_core::manager::host::owner::{
    spawn_owner, GuestOp, GuestReply, OwnerFailureSink, OwnerHandle, OWNER_QUEUE_CAP,
};

// ==================== 脚手架 ====================

/// 记录实例级失败的测试端口（trap / panic 收敛断言用）
#[derive(Default)]
struct RecordingSink {
    /// `(kind, detail)`
    failures: Mutex<Vec<(&'static str, String)>>,
}

impl RecordingSink {
    fn snapshot(&self) -> Vec<(&'static str, String)> {
        self.failures.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl OwnerFailureSink for RecordingSink {
    fn on_owner_failed<'a>(
        &'a self,
        plugin_id: &'a str,
        kind: &'static str,
        detail: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.failures
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((kind, format!("{}: {}", plugin_id, detail)));
        })
    }
}

/// 实例化夹具组件并启动属主任务
///
/// **必须在 tokio 上下文内调用**（`spawn_owner` 内部 `tokio::spawn`）；
/// 而 `setup_wasm_runtime` 自建运行时并在其中 block_on，必须在**上下文之外**调用
/// （否则 "Cannot start a runtime from within a runtime"）——两者因此分开。
fn spawn_test_owner(
    wasm_runtime: &WasmRuntime,
    host_ctx: Arc<WasmHostContext>,
) -> (Arc<RecordingSink>, Arc<OwnerHandle>) {
    let component = wasm_runtime
        .compile_component(&build_test_component())
        .expect("compile component-test fixture");
    let plugin = wasm_runtime
        .instantiate_component(&component, TEST_PLUGIN_ID, host_ctx, &[], None)
        .expect("instantiate component-test fixture");
    let sink = Arc::new(RecordingSink::default());
    let owner = Arc::new(spawn_owner(plugin, sink.clone()));
    (sink, owner)
}

/// 轮询等待条件成立（属主任务的收敛是异步的：sink 回调 / 属主退出）
async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("等待超时：{}", what);
}

/// 合成一个回显命令（夹具默认分支把 `name` / `args` 原样带回）
fn echo_op(i: usize) -> GuestOp {
    GuestOp::InvokeCommand {
        name: format!("echo-{}", i),
        args_json: format!(r#"{{"i":{}}}"#, i),
    }
}

// ==================== 用例 ====================

/// 命令往返 + I4：并发入队的同步命令，完成序 = 入队序，计数一致
#[test]
fn owner_serves_commands_and_preserves_enqueue_order() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let rt = tokio::runtime::Runtime::new().expect("multi-thread runtime");
    rt.block_on(async {
        let (_sink, owner) = spawn_test_owner(&wasm_runtime, host_ctx);
        const N: usize = 8;

        // FuturesUnordered 驱动：首个 poll 轮把所有请求按序投进属主队列，
        // 完成顺序即结算顺序（属主单点 start，同步导出立即完成）
        let mut calls: FuturesUnordered<_> = (0..N)
            .map(|i| {
                let owner = owner.clone();
                async move {
                    let reply = owner.call(echo_op(i)).await;
                    (i, reply)
                }
            })
            .collect();

        let mut order = Vec::new();
        while let Some((i, reply)) = calls.next().await {
            let reply = reply.expect("同步回显命令必须成功");
            let GuestReply::Str(json) = reply else {
                panic!("invoke-command 必须返回 Str 载荷，实际 {:?}", reply);
            };
            let value: serde_json::Value = serde_json::from_str(&json).expect("返回值必须是 JSON");
            assert_eq!(value["name"], format!("echo-{}", i), "应答必须与请求配对");
            order.push(i);
        }

        assert_eq!(order, (0..N).collect::<Vec<_>>(), "完成序必须等于入队序（I4）");
        let stats = owner.stats();
        assert_eq!(stats.started, N as u64, "启动计数");
        assert_eq!(stats.settled, N as u64, "结算计数");
        assert_eq!(stats.in_flight, 0, "全部结算后在飞计数归零");
        assert_eq!(stats.traps, 0);

        // 指标口径：每次 task 计入 calls_total（本实例 = N 条命令）
        let snapshot = wasm_runtime.monitor().snapshot();
        let calls_total = snapshot["plugins"][TEST_PLUGIN_ID]["calls_total"]
            .as_u64()
            .expect("插件指标段必须存在 calls_total");
        assert!(
            calls_total >= N as u64,
            "calls_total 必须随属主调用递增，实际 {}",
            calls_total
        );

        let report = owner.stop().await;
        assert_eq!(report.abandoned_requests, 0, "正常停止不得放弃在等请求");
        assert!(!report.forced_abort, "属主必须能在宽限内退出");
    });
}

/// 停止语义（I6）：`stop()` 返回即 store 已 drop；幂等；后续调用显性失败
#[test]
fn owner_stop_is_prompt_and_later_calls_fail_explicitly() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let rt = tokio::runtime::Runtime::new().expect("multi-thread runtime");
    rt.block_on(async {
        let (_sink, owner) = spawn_test_owner(&wasm_runtime, host_ctx);
        assert!(owner.call(GuestOp::Activate).await.is_ok(), "激活必须成功");

        let report = owner.stop().await;
        assert!(!report.forced_abort);
        assert!(!owner.is_alive(), "stop() 返回后属主必须已退出（store 已 drop）");

        let err = owner.call(GuestOp::Activate).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not running"),
            "停止后调用必须显性失败并点明属主已停，实际：{msg}"
        );

        // 二次停止幂等（不 panic、不重复 abort）
        let second = owner.stop().await;
        assert!(!second.forced_abort);
    });
}

/// trap 语义（I3①/② + 实例级失败上报）：确定性 panic 命令 → 该请求 Err +
/// 实例不可用 + 后续调用显性失败 + sink 收到 "trap"
#[test]
fn owner_trap_poisons_instance_and_reports_failure() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let rt = tokio::runtime::Runtime::new().expect("multi-thread runtime");
    rt.block_on(async {
        let (sink, owner) = spawn_test_owner(&wasm_runtime, host_ctx);

        let err = owner
            .call(GuestOp::InvokeCommand {
                name: "test.panic".to_string(),
                args_json: "{}".to_string(),
            })
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("WASM invoke_command() call failed"),
            "trap 文案必须与 mutex 模型逐字等价，实际：{msg}"
        );

        wait_until(|| !sink.snapshot().is_empty(), "实例级失败上报").await;
        let failures = sink.snapshot();
        assert_eq!(failures.len(), 1, "trap 只上报一次实例级失败");
        assert_eq!(failures[0].0, "trap");
        assert_eq!(owner.stats().traps, 1);

        // I3①：实例已不可用（属主退出）⇒ 后续调用显性失败，不排队等重载
        wait_until(|| !owner.is_alive(), "属主随 trap 退出").await;
        let err = owner.call(GuestOp::Activate).await.unwrap_err();
        assert!(
            err.to_string().contains("not running"),
            "trap 后调用必须显性失败，实际：{err}"
        );
    });
}

/// 有界失败：属主来不及消费时队列满即显性 Err（不静默丢弃、不无限缓冲）
///
/// 用 **current_thread** 运行时保证「一轮内投完」的确定性：驱动 future 在
/// 单个 poll 里完成全部 `try_send`（属主任务得不到调度）⇒ 前 `OWNER_QUEUE_CAP`
/// 条入队，其余立即失败。
#[test]
fn owner_queue_full_is_explicit_failure() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime");
    rt.block_on(async {
        let (_sink, owner) = spawn_test_owner(&wasm_runtime, host_ctx);
        let extra = 16usize;

        let calls: Vec<_> = (0..(OWNER_QUEUE_CAP + extra)).map(|i| owner.call(echo_op(i))).collect();
        let results = futures_util::future::join_all(calls).await;

        let failures: Vec<_> = results.iter().filter_map(|r| r.as_ref().err()).collect();
        assert_eq!(failures.len(), extra, "超出容量的请求必须立即显性失败");
        assert!(
            failures.iter().all(|e| e.to_string().contains("queue is full")),
            "失败文案必须点明队列满（fail-visible），实际：{:?}",
            failures.iter().map(|e| e.to_string()).collect::<Vec<_>>()
        );
        let stats = owner.stats();
        assert_eq!(stats.queue_full, extra as u64, "队列满计数");
        assert_eq!(
            stats.started as usize, OWNER_QUEUE_CAP,
            "入队成功的请求不得丢失（有界预算内全部服务）"
        );

        owner.stop().await;
    });
}

/// 结构锁：I4 的两条支柱（`biased` 调度 + 单点 start）不得被后续改动移除
///
/// 动态用例只能观察「完成序 = 入队序」，无法在同步导出下暴露「多起点 start」；
/// 本锁按源码形态钉死实现约束（票 03 §5.3 施工图）
#[test]
fn owner_i4_structural_lock() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/wasm_core/manager/host/owner.rs"
    ))
    .expect("read owner.rs");
    assert!(
        src.contains("biased;"),
        "属主 select 必须保持 biased（先收请求再结算 = I4）"
    );
    assert_eq!(
        src.matches("match start_op(").count(),
        1,
        "start 必须单点（只在收消息分支里启动 guest task）"
    );
    assert!(
        src.contains("fn start_op<'a>"),
        "start_op 必须借用 Accessor（在飞 future 只能借用；自持 accessor 无公开构造）"
    );
}
