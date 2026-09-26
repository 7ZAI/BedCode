//! 探针：wasmtime 48 组件模型异步（CM-async）在「插件实例」形态下的真实能力
//!
//! 配套 spec：`.scratch/2026-09-26-wasmtime-cm-async-eval/spec.md`
//! 零生产改动：本 crate 不被任何仓库构建引用，只回答四个问题——
//!
//! - **P0.1**：`concurrency_support` 门禁真实存在且默认可用（我们 runtime.rs:307 已开 async ABI）
//! - **P0.2**（类型系统事实）：`func_wrap_concurrent` **只能**满足 WIT 里声明为
//!   `async` 的 import；同步 import 必须用 `func_wrap` / `func_wrap_async`
//! - **P0.4**（现状的类型系统面）：WIT 声明为 `async` 的 import **不能**用经典
//!   `func_wrap` / `func_wrap_async` 满足 ⇒ 今天 WIT 里没有任何 async import，
//!   所有「要等」的宿主逻辑都独占 `&mut Store`
//! - **P1**（决定性）：async import + `func_wrap_concurrent` ⇒ 挂起的 guest task
//!   **不独占 store**，第二个 task（模拟 `session.input`）能先跑完
//! - **P2**：`run_concurrent` 退出后是否残留 task（wasmtime #11833 取消缺口的可见面）
//!
//! 形状取自上游官方测试（`tests/all/component_model/async.rs`、
//! `tests/misc_testsuite/component-model/async/cancel-host.wast`），无额外工具链。
//! 运行：`cargo run`（打印结论）或 `cargo test`（逐阶段断言）。

mod a1;
mod a2;

use anyhow::Result as AnyResult;
use futures::future::{select, Either};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use wasmtime::component::{Accessor, Component, Linker};
use wasmtime::{Config, Engine, Store, StoreContextMut};

type WasmResult<T> = wasmtime::Result<T>;

impl Default for Host {
    fn default() -> Self {
        Host {
            pty_notify: Arc::new(Semaphore::new(0)),
            mark_calls: Arc::new(AtomicU32::new(0)),
            slow_calls: Arc::new(AtomicU32::new(0)),
        }
    }
}

/// 宿主侧状态：唤醒信号与调用计数（跨 store 共享，用 Arc 便于测试线程观察）
#[derive(Clone)]
struct Host {
    /// 模拟「PTY 写入」→ 叫醒等待方。
    /// 用 Semaphore(0) 而非 Notify：permit 是存量，即使「写入」早于 guest task
    /// 真正 await 到它，下一次 acquire 也会立刻通过（Notify::notify_waiters 会丢）。
    pty_notify: Arc<Semaphore>,
    /// 同步 import `mark` 被调用次数（判定 guest 是否推进过）
    mark_calls: Arc<AtomicU32>,
    /// 异步 import `slow` 进入宿主的次数
    slow_calls: Arc<AtomicU32>,
}

fn make_engine(async_abi: bool, concurrency: bool) -> AnyResult<Engine> {
    let mut config = Config::new();
    config.wasm_component_model_async(async_abi);
    config.concurrency_support(concurrency);
    Ok(Engine::new(&config)?)
}

// ==================== guest 组件（手写文本，async ABI） ====================

/// `slow` 在 WIT 里声明为 `async`（决定宿主能用哪种注册方式），
/// guest 以 `async` 方式 lower；`mark` 是普通同步 import（模拟 `session.input`）。
const GUEST: &str = r#"
(component
  (import "slow" (func $h_slow async (param "v" u32) (result u32)))
  (import "mark" (func $h_mark (param "v" u32)))

  (core module $Mem (memory (export "mem") 1))
  (core instance $mem (instantiate $Mem))

  (core module $m
    (import "" "slow" (func $slow (param i32 i32) (result i32)))
    (import "" "mark" (func $mark (param i32)))
    (import "" "thread.yield" (func $thread.yield (result i32)))
    (import "" "waitable-set.new" (func $wsnew (result i32)))
    (import "" "waitable.join" (func $wsjoin (param i32 i32)))
    (import "" "waitable-set.wait" (func $wswait (param i32 i32) (result i32)))
    (import "" "waitable-set.drop" (func $wsdrop (param i32)))
    (import "" "subtask.drop" (func $subdrop (param i32)))

    ;; 启动异步 import，拿 STARTED（低 4 位 == 1）+ subtask id，再等它完成
    (func (export "run_slow") (param $v i32) (result i32)
      (local $st i32) (local $sub i32) (local $ws i32) (local $low i32)
      (local.set $st (call $slow (local.get $v) (i32.const 0)))
      (local.set $low (i32.and (local.get $st) (i32.const 0xf)))
      ;; STARTED(1) = 异步挂起，需等待；其它低 4 位 = 宿主已同步完成，直接返回
      (if (i32.ne (local.get $low) (i32.const 1))
        (then (return (i32.add (local.get $v) (i32.const 1000)))))
      (local.set $sub (i32.shr_u (local.get $st) (i32.const 4)))
      (local.set $ws (call $wsnew))
      (call $wsjoin (local.get $sub) (local.get $ws))
      (drop (call $wswait (local.get $ws) (i32.const 104)))
      (call $subdrop (local.get $sub))
      (call $wsdrop (local.get $ws))
      (i32.add (local.get $v) (i32.const 1000)))

    ;; 只调同步 import：等价于「终端输入这类短命令」。
    ;; 中途 thread.yield 一次——保证 slow task 已被启动并挂在宿主 future 上之后，
    ;; 本任务才继续跑到完成（否则短任务会在一个 poll 内跑完，证明不了交错）
    (func (export "run_poke") (param $v i32) (result i32)
      (call $mark (local.get $v))
      (drop (call $thread.yield))
      (call $mark (local.get $v))
      (local.get $v))
  )

  (core func $slow (canon lower (func $h_slow) async (memory (core memory $mem "mem"))))
  (core func $mark (canon lower (func $h_mark)))
  (core func $thread.yield (canon thread.yield))
  (core func $wsnew (canon waitable-set.new))
  (core func $wsjoin (canon waitable.join))
  (core func $wswait (canon waitable-set.wait (memory (core memory $mem "mem"))))
  (core func $wsdrop (canon waitable-set.drop))
  (core func $subdrop (canon subtask.drop))
  (core instance $i (instantiate $m
    (with "" (instance
      (export "slow" (func $slow))
      (export "mark" (func $mark))
      (export "thread.yield" (func $thread.yield))
      (export "waitable-set.new" (func $wsnew))
      (export "waitable.join" (func $wsjoin))
      (export "waitable-set.wait" (func $wswait))
      (export "waitable-set.drop" (func $wsdrop))
      (export "subtask.drop" (func $subdrop))))))

  (func (export "run-slow") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_slow")))
  (func (export "run-poke") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_poke")))
)
"#;

/// 同一形状，但 `slow` 是**普通（非 async）** import —— 用于验证 P0.2 的类型系统约束
const GUEST_SYNC_IMPORT: &str = r#"
(component
  (import "slow" (func $h_slow (param "v" u32)))
  (import "mark" (func $h_mark (param "v" u32)))

  (core module $Mem (memory (export "mem") 1))
  (core instance $mem (instantiate $Mem))

  (core module $m
    (import "" "slow" (func $slow (param i32)))
    (import "" "mark" (func $mark (param i32)))
    (func (export "run_slow") (param $v i32) (result i32)
      (call $slow (local.get $v))
      (call $mark (local.get $v))
      (local.get $v))
  )
  (core func $slow (canon lower (func $h_slow)))
  (core func $mark (canon lower (func $h_mark)))
  (core instance $i (instantiate $m
    (with "" (instance (export "slow" (func $slow)) (export "mark" (func $mark))))))
  (func (export "run-slow") (param "v" u32) (result u32)
    (canon lift (core func $i "run_slow")))
)
"#;

/// 同步 import `mark` 的宿主实现：立即返回并记账（等价于 `session.input` 这类短命令）
fn mark_host() -> impl Fn(StoreContextMut<Host>, (u32,)) -> WasmResult<()> {
    move |store: StoreContextMut<Host>, (_v,): (u32,)| {
        store.data().mark_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ==================== 探针 ====================

/// P0.1：`concurrency_support` 门禁真实存在且默认可用
async fn probe_p0_1_concurrency_support_gate() -> AnyResult<()> {
    let engine = make_engine(true, true)?;
    let mut store = Store::new(&engine, Host::default());
    assert!(
        store
            .run_concurrent(async |_| Ok::<(), wasmtime::Error>(()))
            .await
            .is_ok(),
        "concurrency_support 应为可用"
    );

    // 反例：显式关闭后注册必须被拒（对齐上游 require_concurrency_support 测试）。
    // 注意此处**不能**同时开 async ABI——那会在 `Engine::new` 阶段就被
    // "concurrency support must be enabled to use CM_ASYNC" 校验挡住（另一道门）。
    let engine_off = make_engine(false, false)?;
    let mut linker = Linker::<Host>::new(&engine_off);
    let refused = linker
        .root()
        .func_wrap_concurrent("slow", |_: &Accessor<Host>, (): ()| {
            Box::pin(async { Ok(()) })
        })
        .is_err();
    assert!(
        refused,
        "concurrency_support=false 时 func_wrap_concurrent 应被拒"
    );
    println!("[P0.1] concurrency_support=true 可用；=false 时注册被拒 ✓");
    Ok(())
}

/// P0.2（类型系统事实）：`func_wrap_concurrent` 只能满足 WIT 里声明为 `async` 的 import
async fn probe_p0_2_concurrent_requires_async_import() -> AnyResult<()> {
    let engine = make_engine(true, true)?;
    let component = Component::new(&engine, GUEST_SYNC_IMPORT)?;
    let host = Host::default();

    let host_slow = host.clone();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap_concurrent("slow", move |_acc: &Accessor<Host>, (_v,): (u32,)| {
            let host = host_slow.clone();
            Box::pin(async move {
                let _permit = host
                    .pty_notify
                    .acquire()
                    .await
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                Ok(())
            })
        })?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host);
    let err = linker
        .instantiate_async(&mut store, &component)
        .await
        .expect_err("同步 import 不应被 func_wrap_concurrent 满足");
    let text = format!("{err:#}");
    assert!(
        text.contains("only for `async func`-typed imports"),
        "错误信息应点名 async-typed 约束，实际：{text}"
    );
    println!("[P0.2] 类型系统拒绝：同步 import 只能用 func_wrap/func_wrap_async ✓");
    println!("       └ {text}");
    Ok(())
}

/// P0.4（现状对照的类型系统面）：WIT 里声明为 `async` 的 import **不能**用经典
/// `func_wrap` / `func_wrap_async` 满足——尽管名字里有 async，它们实现的是
/// **sync-WIT-typed** 函数（阻塞式宿主代码），不是 async-WIT import。
///
/// 含义（对本项目的直接推论）：今天 WIT 里**没有任何 async import**，
/// `add_to_linker_async` 只是把 **sync** import 的宿主实现写成阻塞友好的形式；
/// 于是任何「要等」的宿主逻辑（如 `host_http_fetch` 的 `block_on_async` 等网络）
/// 都发生在一次 guest 调用的栈内、独占 `&mut Store`。
async fn probe_p0_4_async_import_requires_concurrent_host() -> AnyResult<()> {
    let engine = make_engine(true, true)?;
    let component = Component::new(&engine, GUEST)?;
    let host = Host::default();

    let host_slow = host.clone();
    let mut linker = Linker::new(&engine);
    linker.root().func_wrap_async(
        "slow",
        move |_store: StoreContextMut<Host>, (_v,): (u32,)| {
            let host = host_slow.clone();
            Box::new(async move {
                let _permit = host
                    .pty_notify
                    .acquire()
                    .await
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                Ok((0u32,))
            })
        },
    )?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host);
    let err = linker
        .instantiate_async(&mut store, &component)
        .await
        .expect_err("async-WIT import 不应被经典 func_wrap_async 满足");
    let text = format!("{err:#}");
    assert!(
        text.contains("declared `async func` in WIT") && text.contains("func_wrap_concurrent"),
        "错误信息应要求改用 concurrent 注册，实际：{text}"
    );
    println!("[P0.4] async-WIT import 只能由 func_wrap_concurrent 满足 ✓");
    println!("       └ {text}");
    Ok(())
}

/// P1（决定性）+ P2：`func_wrap_concurrent` ⇒ 挂起 task 不独占 store
async fn probe_p1_concurrent_tasks_interleave() -> AnyResult<()> {
    let engine = make_engine(true, true)?;
    let component = Component::new(&engine, GUEST)?;
    let host = Host::default();

    let host_slow = host.clone();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap_concurrent("slow", move |_acc: &Accessor<Host>, (v,): (u32,)| {
            let host = host_slow.clone();
            Box::pin(async move {
                host.slow_calls.fetch_add(1, Ordering::SeqCst);
                let _permit = host
                    .pty_notify
                    .acquire()
                    .await
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                Ok((v + 1,))
            })
        })?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, &component).await?;
    let run_slow = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-slow")?;
    let run_poke = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-poke")?;

    let (poke_value, slow_value) = tokio::time::timeout(
        Duration::from_secs(5),
        store.run_concurrent(async |accessor| {
            // 两个 guest task 并发推进：slow 挂在宿主 Notify 上，poke 是短任务
            // （中间 thread.yield，保证 slow 先被启动并挂起）
            let fut_poke = Box::pin(run_poke.call_concurrent(accessor, (9,)));
            let fut_slow = Box::pin(run_slow.call_concurrent(accessor, (7,)));
            match select(fut_poke, fut_slow).await {
                // 决定性：慢 task 已挂在宿主 future 上，快 task 仍能跑完（含 yield 后继续）
                Either::Left((poke, remaining_slow)) => {
                    let (poke_value,) = poke?;
                    // 关键证据：确认 slow task 确实已在宿主 future 上挂起，
                    // 再放行 permit（否则「交错」只是 slow 还没开始）
                    let mut spins = 0;
                    while host.slow_calls.load(Ordering::SeqCst) == 0 && spins < 10_000 {
                        tokio::task::yield_now().await;
                        spins += 1;
                    }
                    if host.slow_calls.load(Ordering::SeqCst) == 0 {
                        return Err(wasmtime::Error::msg("slow task 从未进入宿主，交错未被证明"));
                    }
                    host.pty_notify.add_permits(1);
                    let (slow_value,) = remaining_slow.await?;
                    Ok::<_, wasmtime::Error>((poke_value, Some(slow_value)))
                }
                Either::Right((slow, _)) => {
                    let (slow_value,) = slow?;
                    Ok::<_, wasmtime::Error>((u32::MAX, Some(slow_value)))
                }
            }
        }),
    )
    .await
    .expect("concurrent 模式下不应超时")??;

    assert_eq!(
        poke_value, 9,
        "run-poke 应在 run-slow 挂起期间先完成（挂起 task 未独占 store）"
    );
    assert_eq!(slow_value, Some(1007), "run-slow 被叫醒后应返回 v+1000");
    // 至少被调过一次：guest 的 run-poke 确实推进到了同步 import。
    // 注：run_poke 函数体**本身写了 2 处** `call $mark`（yield 前后各一次），
    // 因此正常执行 = 2 次；若 task 体重入一次 = 4 次。精确打印实测值：
    let mark_total = host.mark_calls.load(Ordering::SeqCst);
    println!("  [P1] mark 实测 {mark_total} 次（run_poke 函数体固有 2 处调用；>2 即重入）");
    assert!(
        mark_total >= 1,
        "run-poke 应至少调用一次 mark（实际 {mark_total}）"
    );
    assert_eq!(
        host.slow_calls.load(Ordering::SeqCst),
        1,
        "slow 宿主函数应已进入且挂起过（证明交错真实发生）"
    );

    // P2：事件循环退出后不应残留 guest task（取消缺口的可见面）
    store.assert_concurrent_state_empty();
    println!(
        "[P1] 挂起 task 不独占 store：poke={poke_value}（先完成） / slow={slow_value:?}（后完成）✓"
    );
    println!("[P2] run_concurrent 退出后 concurrent state 为空 ✓");
    Ok(())
}

/// 附加观测：多 task 下 store 级 fuel 仍可读写（本项目 `refill_call_fuel` 依赖它）
async fn probe_p3_fuel_still_works_with_tasks() -> AnyResult<()> {
    // fuel 需显式开启（本项目宿主 `EngineLimits`/fuel 配置另有一套，见 §2 F13 备注）
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    config.consume_fuel(true);
    let engine = Engine::new(&config)?;
    let component = Component::new(&engine, GUEST)?;
    let host = Host::default();

    let host_slow = host.clone();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap_concurrent("slow", move |_acc: &Accessor<Host>, (v,): (u32,)| {
            let host = host_slow.clone();
            Box::pin(async move {
                let _permit = host
                    .pty_notify
                    .acquire()
                    .await
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                Ok((v + 1,))
            })
        })?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host.clone());
    store.set_fuel(10_000_000)?;
    let instance = linker.instantiate_async(&mut store, &component).await?;
    let run_slow = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-slow")?;
    let run_poke = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-poke")?;

    let before: Option<u64> = store.get_fuel().ok();
    let poke_value = store
        .run_concurrent(async |accessor| {
            let poke = Box::pin(run_poke.call_concurrent(accessor, (9,)));
            let slow = Box::pin(run_slow.call_concurrent(accessor, (7,)));
            match select(poke, slow).await {
                Either::Left((p, remaining)) => {
                    // 与 P1 同法：先确认 slow 已挂在宿主 future 上再放行 permit
                    let mut spins = 0;
                    while host.slow_calls.load(Ordering::SeqCst) == 0 && spins < 100_000 {
                        tokio::task::yield_now().await;
                        spins += 1;
                    }
                    host.pty_notify.add_permits(1);
                    let _ = remaining.await;
                    p.map(|(v,)| v)
                }
                Either::Right((s, _)) => s.map(|(v,)| v),
            }
        })
        .await??;
    let after: Option<u64> = store.get_fuel().ok();
    assert_eq!(poke_value, 9);
    match (before, after) {
        (Some(b), Some(a)) => println!(
            "[P3] 多 task 下 fuel 可观测：{b} → {a}（消耗 {}）",
            b.saturating_sub(a)
        ),
        _ => println!("[P3] fuel 读取不可用（get_fuel 报错），跳过"),
    }
    store.assert_concurrent_state_empty();
    Ok(())
}

// ==================== 入口 ====================

#[tokio::main]
async fn main() -> AnyResult<()> {
    // A2 诊断用：RUST_LOG=trace 打开 wasmtime 内部事件循环日志
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    println!("== wasmtime CM-async 探针（48）==");
    probe_p0_1_concurrency_support_gate().await?;
    probe_p0_2_concurrent_requires_async_import().await?;
    probe_p0_4_async_import_requires_concurrent_host().await?;
    probe_p1_concurrent_tasks_interleave().await?;
    probe_p3_fuel_still_works_with_tasks().await?;
    a1::run_all().await?;
    a2::run_all().await?;
    println!("\n=== 全部通过：CM-async 在本项目 wasmtime 48 上可用（P1 为决定性指标） ===");
    Ok(())
}

// ==================== cargo test 包装（探针本体是上面几个 probe_* ） ====================

#[tokio::test]
async fn p0_1_concurrency_support_gate() {
    probe_p0_1_concurrency_support_gate().await.unwrap();
}

#[tokio::test]
async fn p0_2_concurrent_requires_async_import() {
    probe_p0_2_concurrent_requires_async_import().await.unwrap();
}

#[tokio::test]
async fn p0_4_async_import_requires_concurrent_host() {
    probe_p0_4_async_import_requires_concurrent_host()
        .await
        .unwrap();
}

#[tokio::test]
async fn p1_concurrent_tasks_interleave() {
    probe_p1_concurrent_tasks_interleave().await.unwrap();
}

#[tokio::test]
async fn p3_fuel_still_works_with_tasks() {
    probe_p3_fuel_still_works_with_tasks().await.unwrap();
}
