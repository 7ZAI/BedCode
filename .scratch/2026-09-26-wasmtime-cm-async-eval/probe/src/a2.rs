//! **A2 探针**：`call_concurrent` 任务的取消 / trap 语义
//!
//! 配套 spec：`.scratch/2026-09-26-plugin-concurrency-model/issues/02-p0-a2-cancellation-semantics.md`
//! （CM-async spec §5.3 A2 / §2 F8·F9 / 探针原 §5.3「取消语义未实测」）
//!
//! 本模块回答四个问题，全部是属主模型（P1）的设计输入：
//!
//! - **A2.1 丢 store 取消**：任务挂在宿主 future 上时 drop 掉整个 store
//!   ——进程是否存活？挂起中的**宿主 future 是否被 drop**（这决定本项目
//!   「在飞的 HTTP 请求 / 排队中的 PTY 写」在取消时会发生什么）？
//!   **guest 侧等待之后的代码是否还会跑**（决定 guest 能否做取消清理）？
//!   丢 store 之后能否立刻用新 store 恢复（trap→重载路径的前提）？
//! - **A2.2 停滞与恢复**：`run_concurrent` 作用域退出时任务**仍在跑但停滞**
//!   （F9）——重新进入作用域能否继续推进？退出的作用域会不会把任务连带取消？
//! - **A2.3 trap 是否污染 store**（**决定性**）：经典模型下一次 trap 会
//!   `set_trapped()` 污染 Store、之后所有调用持续 `CannotEnterComponent`、
//!   唯一恢复是整体重载（`commands.rs:66`）。属主模型下一个 task trap 是否同样
//!   污染整 store？挂起中的兄弟 task 是否被连带打死？
//! - **A2.4 请求方放弃**：`call_concurrent` 的 future 被 drop（请求超时 /
//!   前端断连）**不等于取消任务**（F8 原文）——任务继续跑并回调宿主吗？
//!
//! guest 为手写 async 组件文本（与 main.rs 探针同形状，另加一个会 trap 的导出）。

use anyhow::Result as AnyResult;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use wasmtime::component::{Accessor, Component, Linker};
use wasmtime::{Config, Engine, Store, StoreContextMut};

/// 宿主侧可观测状态（跨 store 共享，用 Arc 便于 store 之外观察）
#[derive(Clone)]
pub struct A2Host {
    /// 模拟「PTY 有新字节」→ 放行等待方。存量 permit，写早于 await 也不丢唤醒。
    pty_notify: Arc<Semaphore>,
    /// `slow`（async import）进入宿主的次数
    slow_calls: Arc<AtomicU32>,
    /// `slow` 的 await **返回之后**完成次数（= 任务真的被推进到底）
    slow_completed: Arc<AtomicU32>,
    /// `slow` 的宿主 future 在**挂起途中**被 drop 的次数
    /// （= 取消/丢 store 的唯一可观测面；正常完成后 drop 不计）
    slow_aborted: Arc<AtomicU32>,
    /// 同步 import `mark` 调用次数（guest 是否跑过等待之后的代码）
    mark_calls: Arc<AtomicU32>,
}

impl Default for A2Host {
    fn default() -> Self {
        A2Host {
            pty_notify: Arc::new(Semaphore::new(0)),
            slow_calls: Arc::new(AtomicU32::new(0)),
            slow_completed: Arc::new(AtomicU32::new(0)),
            slow_aborted: Arc::new(AtomicU32::new(0)),
            mark_calls: Arc::new(AtomicU32::new(0)),
        }
    }
}

/// 挂在宿主 future 上的守卫：future 在「未完成」时被 drop ⇒ 记一次 aborted
struct PendingGuard {
    host: A2Host,
    done: Arc<AtomicU32>,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        if self.done.load(Ordering::SeqCst) == 0 {
            self.host.slow_aborted.fetch_add(1, Ordering::SeqCst);
        }
    }
}

// ==================== guest 组件（手写文本，async ABI） ====================

/// 三个导出：
/// - `run-slow`：启动 async import 并 await 它（挂起点）
/// - `run-poke`：只调同步 import（短任务，观察是否被兄弟 task 的挂起/trap 影响）
/// - `run-trap`：yield 一次后 `unreachable`（trap 源）
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

    (func (export "run_slow") (param $v i32) (result i32)
      (local $st i32) (local $sub i32) (local $ws i32) (local $low i32)
      (local.set $st (call $slow (local.get $v) (i32.const 0)))
      (local.set $low (i32.and (local.get $st) (i32.const 0xf)))
      (if (i32.ne (local.get $low) (i32.const 1))
        (then (return (i32.add (local.get $v) (i32.const 1000)))))
      (local.set $sub (i32.shr_u (local.get $st) (i32.const 4)))
      (local.set $ws (call $wsnew))
      (call $wsjoin (local.get $sub) (local.get $ws))
      (drop (call $wswait (local.get $ws) (i32.const 104)))
      (call $subdrop (local.get $sub))
      (call $wsdrop (local.get $ws))
      ;; 等待之后的 guest 代码：若本行被计数，说明任务在「取消/重载」后仍被推进
      (call $mark (i32.const 55))
      (i32.add (local.get $v) (i32.const 1000)))

    (func (export "run_poke") (param $v i32) (result i32)
      (call $mark (local.get $v))
      (drop (call $thread.yield))
      (call $mark (local.get $v))
      (local.get $v))

    (func (export "run_trap") (param $v i32) (result i32)
      (drop (call $thread.yield))
      (unreachable))
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
  (func (export "run-trap") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_trap")))
)
"#;

fn make_engine() -> AnyResult<Engine> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    Ok(Engine::new(&config)?)
}

/// 挂起式 async import：进入即计数 → 等 permit → 完成计数
fn slow_host(
    host: A2Host,
) -> impl Fn(
    &Accessor<A2Host>,
    (u32,),
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = wasmtime::Result<(u32,)>> + Send + '_>,
> {
    move |_acc: &Accessor<A2Host>, (v,): (u32,)| {
        let host = host.clone();
        Box::pin(async move {
            let done = Arc::new(AtomicU32::new(0));
            let _guard = PendingGuard {
                host: host.clone(),
                done: done.clone(),
            };
            host.slow_calls.fetch_add(1, Ordering::SeqCst);
            let _permit = host
                .pty_notify
                .acquire()
                .await
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            done.store(1, Ordering::SeqCst);
            host.slow_completed.fetch_add(1, Ordering::SeqCst);
            Ok((v + 1,))
        })
    }
}

fn mark_host() -> impl Fn(StoreContextMut<A2Host>, (u32,)) -> wasmtime::Result<()> {
    move |store: StoreContextMut<A2Host>, (_v,): (u32,)| {
        store.data().mark_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// 建引擎 + linker（slow=async import / mark=同步 import）
///
/// 注意：linker 闭包与 store 必须**共用同一份** `A2Host`（计数器是 Arc，
/// 分裂成两份会让断言读不到真实调用数——A2 首版踩过）。
fn build(engine: &Engine, host: &A2Host) -> AnyResult<(Component, Linker<A2Host>)> {
    let component = Component::new(engine, GUEST)?;
    let mut linker = Linker::new(engine);
    linker
        .root()
        .func_wrap_concurrent("slow", slow_host(host.clone()))?;
    linker.root().func_wrap("mark", mark_host())?;
    Ok((component, linker))
}

/// 轮询等待某个计数器达到期望值（避免依赖真实时间）
async fn spin_until(f: impl Fn() -> bool) -> bool {
    for _ in 0..200_000 {
        if f() {
            return true;
        }
        tokio::task::yield_now().await;
    }
    f()
}

/// 诊断用：RUST_LOG=trace 打开 wasmtime 事件循环日志（无副作用，可重复调用）
fn init_log() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
}

/// 有界轮询一个 future 直到就绪（noop waker：就绪最终会在下一次轮询被观察到）
/// 返回 `None` = 在限定轮数内始终未就绪。
async fn poll_ready_bounded<F: std::future::Future>(
    fut: &mut std::pin::Pin<Box<F>>,
    spins: usize,
) -> Option<F::Output> {
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    for _ in 0..spins {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return Some(v);
        }
        tokio::task::yield_now().await;
    }
    match fut.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => Some(v),
        std::task::Poll::Pending => None,
    }
}

// ==================== A2.1 丢 store 取消 ====================

/// 任务挂在宿主 future 上时 drop 整个 store（F8 的唯一取消途径）
async fn probe_a2_1_drop_store_cancels() -> AnyResult<()> {
    let engine = make_engine()?;
    let host = A2Host::default();
    let (component, linker) = build(&engine, &host)?;

    {
        let mut store = Store::new(&engine, host.clone());
        let instance = linker.instantiate_async(&mut store, &component).await?;
        let run_slow = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-slow")?;

        // 起一个 task 并让它挂在宿主 future 上，然后**放弃宿主侧 future**
        // （等价于请求方超时/断连：F8 —— drop future 不取消任务）
        let entered = store
            .run_concurrent(async |accessor| {
                let mut fut = Box::pin(run_slow.call_concurrent(accessor, (7,)));
                // 先 poll 一次让任务入队
                let _ = futures::poll!(fut.as_mut());
                // 事件循环只在 closure 被重新 poll 时推进（spin 即反复让出）
                let entered = spin_until(|| host.slow_calls.load(Ordering::SeqCst) >= 1).await;
                // 放弃宿主侧 future（不取消任务）
                drop(fut);
                Ok::<bool, wasmtime::Error>(entered)
            })
            .await??;
        assert!(entered, "宿主函数应已进入并挂起");
        assert_eq!(
            host.slow_calls.load(Ordering::SeqCst),
            1,
            "宿主函数应已进入并挂起"
        );
        assert_eq!(
            host.slow_aborted.load(Ordering::SeqCst),
            0,
            "放弃 future 不应取消任务"
        );

        // F9：作用域已退出，任务仍在但停滞 —— 状态表非空
        let size = store.concurrent_state_table_size();
        assert!(
            size > 0,
            "作用域退出后任务应仍在状态表里（停滞而非消失），实际 size={size}"
        );
        println!("[A2.1] 作用域退出后任务仍在表内（size={size}，停滞未取消）✓");

        // ★ 丢 store：唯一的取消途径
        drop(store);
        assert_eq!(
            host.slow_aborted.load(Ordering::SeqCst),
            1,
            "丢 store 应 drop 掉挂起中的宿主 future（取消的可观测面）"
        );
        assert_eq!(
            host.slow_completed.load(Ordering::SeqCst),
            0,
            "被取消的任务不得跑到完成"
        );
        assert_eq!(
            host.mark_calls.load(Ordering::SeqCst),
            0,
            "guest 等待之后的代码不得执行"
        );
        println!("[A2.1] drop store：进程存活、挂起宿主 future 被 drop ✓");
        println!("[A2.1] guest 等待之后的代码未执行（取消不做 guest 侧清理）✓");
    }

    // 丢 store 之后能否立刻用新 store 恢复（= trap→重载路径的前提）
    {
        let host2 = A2Host::default();
        let mut store = Store::new(&engine, host2.clone());
        let instance = linker.instantiate_async(&mut store, &component).await?;
        let run_poke = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-poke")?;
        let res = store
            .run_concurrent(async |accessor| run_poke.call_concurrent(accessor, (9,)).await)
            .await;
        let (v,) = res??;
        assert_eq!(v, 9, "丢 store 后新实例应能正常调用");
        println!("[A2.1] 丢 store 后新 store 可正常调用（重载恢复成立）✓");
    }
    Ok(())
}

// ==================== A2.2 停滞 → 重新进入作用域 ====================

/// `run_concurrent` 作用域退出时任务停滞（F9）；重新进入能否继续推进
async fn probe_a2_2_detached_task_resumes() -> AnyResult<()> {
    let engine = make_engine()?;
    let host = A2Host::default();
    let (component, linker) = build(&engine, &host)?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, &component).await?;
    let run_slow = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-slow")?;

    // 第一个作用域：起任务 → 挂起 → 放弃 future → 退出
    store
        .run_concurrent(async |accessor| {
            let mut fut = Box::pin(run_slow.call_concurrent(accessor, (7,)));
            let _ = futures::poll!(fut.as_mut());
            let entered = spin_until(|| host.slow_calls.load(Ordering::SeqCst) >= 1).await;
            assert!(entered, "slow 宿主函数应已进入");
            Ok::<(), wasmtime::Error>(())
        })
        .await??;
    assert_eq!(
        host.slow_aborted.load(Ordering::SeqCst),
        0,
        "第一个作用域退出不应取消任务"
    );

    // 第二个作用域：放行 permit → 事件循环应继续推进那个「已脱离」的任务
    // （必须**在作用域内**等待：F9 —— 任务只在活跃 run_concurrent 作用域内推进）
    host.pty_notify.add_permits(1);
    let advanced = store
        .run_concurrent(async |_accessor| {
            // 等到宿主 future 完成（slow_completed）且 guest 跑过等待之后的代码
            // （mark_calls，含 subtask.drop / waitable-set.drop）才算任务真正跑完
            Ok::<bool, wasmtime::Error>(
                spin_until(|| {
                    host.slow_completed.load(Ordering::SeqCst) >= 1
                        && host.mark_calls.load(Ordering::SeqCst) >= 1
                })
                .await,
            )
        })
        .await??;
    assert!(
        advanced,
        "重新进入 run_concurrent 后，已脱离句柄的任务应继续推进"
    );
    assert_eq!(
        host.slow_aborted.load(Ordering::SeqCst),
        0,
        "推进完成不等于被取消"
    );
    println!(
        "[A2.2] 作用域内放行 → 脱离句柄的任务继续推进（宿主 future 完成 + guest 等待后代码执行）✓"
    );
    store.assert_concurrent_state_empty();
    println!("[A2.2] 任务跑完后 concurrent state 归空 ✓");
    Ok(())
}

// ==================== A2.3 trap 是否污染 store（决定性） ====================

/// 一个 task trap 是否连带整个实例不可用（I3 的核心问题）
///
/// **不带挂起的兄弟任务**（否则会被 A2.4 的 do_not_enter 推迟现象污染观测）。
async fn probe_a2_3_trap_scope() -> AnyResult<()> {
    let engine = make_engine()?;
    let host = A2Host::default();
    let (component, linker) = build(&engine, &host)?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, &component).await?;
    let run_poke = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-poke")?;
    let run_trap = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-trap")?;

    // 先证明正常调用可用
    let baseline = store
        .run_concurrent(async |accessor| run_poke.call_concurrent(accessor, (1,)).await)
        .await;
    assert!(matches!(baseline, Ok(Ok(_))), "trap 前调用应正常");

    // 单独跑 trap（无兄弟挂起任务）
    let trap_res = store
        .run_concurrent(async |accessor| run_trap.call_concurrent(accessor, (1,)).await)
        .await;
    let trap_text = match &trap_res {
        Ok(Ok(_)) => panic!("run-trap 应当 trap"),
        Ok(Err(e)) => format!("{e:#}"),
        Err(e) => format!("[run_concurrent 自身返回 Err] {e:#}"),
    };
    println!("[A2.3] trap 观测：{trap_text}");

    // ★ 决定性：trap 之后同一 store 还能不能再调（经典模型下持续 CannotEnterComponent）
    let after = store
        .run_concurrent(async |accessor| run_poke.call_concurrent(accessor, (3,)).await)
        .await;
    match &after {
        Ok(Ok((v,))) => {
            println!("[A2.3] trap 后同 store 再次调用成功（v={v}）⇒ task trap **不**污染整实例");
        }
        Ok(Err(e)) => println!("[A2.3] trap 后同 store 调用被拒：{e:#} ⇒ 仍按经典模型污染整实例"),
        Err(e) => println!("[A2.3] trap 后连 run_concurrent 都进不去：{e:#}"),
    }
    let size = store.concurrent_state_table_size();
    println!("[A2.3] trap 后状态表 size={size}");
    Ok(())
}

// ==================== A2.4 挂起任务是否阻塞同实例的新调用 ====================

/// **对 spec §3.2 I2 的直接威胁**：一个 task 真的停在 Pending 的宿主 future 上时，
/// 它所在 component 实例保持 `do_not_enter`，此后对**同一实例**的新调用被无限期推迟
/// （直到那个挂起任务跑完、`exit_instance` 触发 `partition_pending`）。
///
/// 观测项：
/// - 新调用在挂起期间是否完成（预期：不完成）
/// - 放行挂起任务后，被推迟的新调用是否补上（预期：补上）
async fn probe_a2_4_parked_task_blocks_sibling() -> AnyResult<()> {
    let engine = make_engine()?;
    let host = A2Host::default();
    let (component, linker) = build(&engine, &host)?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, &component).await?;
    let run_poke = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-poke")?;
    let run_slow = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-slow")?;

    let (still_parked, poke_done_while_parked, poke_after_release) = store
        .run_concurrent(async |accessor| {
            // 1) 先让 slow 真的停在 Pending 的宿主 future 上（不放行）
            let mut slow = Box::pin(run_slow.call_concurrent(accessor, (7,)));
            let _ = futures::poll!(slow.as_mut());
            let entered = spin_until(|| host.slow_calls.load(Ordering::SeqCst) >= 1).await;
            assert!(entered, "slow 宿主函数应已进入（已挂在 future 上）");
            // 证明它确实停住：此刻 spawn 一小段真实时间再检查计数不变
            tokio::time::sleep(Duration::from_millis(20)).await;
            let still_parked = host.slow_completed.load(Ordering::SeqCst) == 0;
            drop(slow);

            // 2) 挂起期间发一个短调用（要求：不阻塞属主 / I2）
            let mut poke = Box::pin(run_poke.call_concurrent(accessor, (9,)));
            let _ = futures::poll!(poke.as_mut());
            let done_while_parked = poll_ready_bounded(&mut poke, 50_000).await.is_some();

            // 3) 放行挂起任务 → 被推迟的 poke 是否补上
            host.pty_notify.add_permits(1);
            let after_release = poll_ready_bounded(&mut poke, 200_000).await;
            Ok::<(bool, bool, Option<wasmtime::Result<(u32,)>>), wasmtime::Error>((
                still_parked,
                done_while_parked,
                after_release,
            ))
        })
        .await??;

    assert!(still_parked, "观测前提：slow 应确实停在 Pending");
    if poke_done_while_parked {
        println!("[A2.4] 挂起任务**没有**阻塞同实例的新调用（I2 成立）✓");
    } else {
        println!("[A2.4] 挂起任务阻塞了同实例的新调用（do_not_enter 推迟）⇒ I2 在同实例内不成立 ⚠");
    }
    assert!(
        poke_after_release.is_some(),
        "放行挂起任务后，被推迟的新调用应补上完成（否则是死锁而非推迟）"
    );
    let v = poke_after_release
        .expect("已就绪")
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    println!(
        "[A2.4] 放行后被推迟的调用补上完成（v={}）⇒ 是「推迟」不是「死锁」",
        v.0
    );
    store.assert_concurrent_state_empty();
    Ok(())
}

// ==================== 入口 ====================

pub async fn run_all() -> AnyResult<()> {
    init_log();
    println!("-- A2 取消 / trap 语义 --");
    probe_a2_1_drop_store_cancels().await?;
    probe_a2_2_detached_task_resumes().await?;
    probe_a2_3_trap_scope().await?;
    probe_a2_4_parked_task_blocks_sibling().await?;
    println!("-- A2 探针完成 --");
    Ok(())
}

#[tokio::test]
async fn a2_1_drop_store_cancels() {
    init_log();
    probe_a2_1_drop_store_cancels().await.unwrap();
}

#[tokio::test]
async fn a2_2_detached_task_resumes() {
    init_log();
    probe_a2_2_detached_task_resumes().await.unwrap();
}

#[tokio::test]
async fn a2_3_trap_scope() {
    init_log();
    probe_a2_3_trap_scope().await.unwrap();
}

#[tokio::test]
async fn a2_4_parked_task_blocks_sibling() {
    init_log();
    probe_a2_4_parked_task_blocks_sibling().await.unwrap();
}
