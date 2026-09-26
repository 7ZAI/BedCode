//! **A1 判别探针**：async-lifted 导出的 task 体「重入」判定
//!
//! 配套票据：`.scratch/2026-09-26-plugin-concurrency-model/issues/01-p0-a1-async-lift-reentry-retest.md`
//! （CM-async spec §5.3 A1 / §8 待办 1）
//!
//! 探针现场（D5 判别实验）：
//! - 手写 **stackless** async 组件里，**async-lifted 导出**的 task 体会被重入一次
//!   （guest 的一处同步 import 被调 **2** 次）；
//! - 同样形状改为**同步导出**则只调 1 次。
//!
//! 本模块用**上游官方测试程序形状**做对照复测，回答：重入是手写组件构造问题
//! 还是 wasmtime 行为/缺陷。判别矩阵（**含 async import 形状**——重入最可能
//! 发生在 waitable-set.wait 挂起 → 恢复的恢复机制上，纯同步 import 函数体
//! 一次跑完、无恢复点，测不出重入）：
//!
//! | ID | 形状 | 期望 |
//! | --- | --- | --- |
//! | D1 | async-lifted 导出 + **同步** import + 无挂起（函数体只写一处 mark） | 若「重入」= 2 次；若不重入 = 1 次 |
//! | D2 | 同函数体改 **同步导出**（对照组） | 1 次 |
//! | D3 | async-lifted 导出 + 同步 import + **thread.yield 一次** | 观察 yield 是否触起重入 |
//! | D4 | D1 形状但引擎开 `component_model_async_stackful(true)` | 观察 stackful 是否消除重入 |
//! | D5 | D1 形状但 async lift **带 callback**（官方 stackless 形状） | 观察 callback 是否消除重入 |
//! | **D6** | **async-lifted 导出 + async import（waitable 等待）**：函数体 = mark(等待前) → 启动 slow → waitable 等待 → mark(等待后)。放行后计数等待前后 mark | 若恢复时从函数头重跑：等待前 mark = 2；若不重入：两处各 = 1 |
//! | **D7** | D6 形状但**同步导出**（对照；若 async import 在同步导出里 trap，则记录 trap 本身） | 观察同步导出下的行为 |
//! | **D8** | D6 形状但引擎开 stackful | 观察 stackful 是否消除恢复期重跑 |
//!
//! 官方对照（静态证据，来自 wasmtime v48.0.3 上游仓库）：
//! - `crates/test-programs/src/bin/async_round_trip_stackless_sync_import.rs`：
//!   官方工具链（wit-bindgen async trait）产物，「async 导出 + 同步 import」形状，
//!   官方测试断言字符串精确匹配（entered host 只出现一次）→ 官方产物不重入。
//! - `tests/misc_testsuite/component-model/async/cancel-host.wast`：async lift **无 callback**
//!   且不开 stackful 是官方合法形状（`assert_return (invoke "run")` 通过）。
//! - `tests/all/component_model/async.rs` `cancel_host_future`：同样无 callback + 不开
//!   stackful 的 async lift，官方测试通过。
//!
//! **结论判定规则**：若 D6（async import 形状）=2 且 D8（开 stackful）=1 ⇒ 手写组件
//! 构造问题（stackless 缺 `thread.resume-later` 恢复点标记，恢复时重跑）；若 D6=2 且
//! D8 仍 =2 ⇒ wasmtime 行为（恢复期重跑是 stackless 的普遍性质），需评估对 P2/P3
//! 的影响并给结构性规避；若 D6=1 ⇒ 探针 D5 现场不可复现（需重新审视原始记录）。

use anyhow::Result as AnyResult;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use wasmtime::component::{Accessor, Component, Linker};
use wasmtime::{Config, Engine, Store, StoreContextMut};

/// 宿主侧状态：mark（同步 import）调用计数 + slow（async import）进入计数
#[derive(Clone)]
struct A1Host {
    mark_calls: Arc<AtomicU32>,
    slow_calls: Arc<AtomicU32>,
    /// 模拟「PTY 写入」→ 放行等待方（permit 是存量，早于 await 也不丢唤醒）
    pty_notify: Arc<tokio::sync::Semaphore>,
}

impl Default for A1Host {
    fn default() -> Self {
        A1Host {
            mark_calls: Arc::new(AtomicU32::new(0)),
            slow_calls: Arc::new(AtomicU32::new(0)),
            pty_notify: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }
}

/// 挂起式 async import：进入即计数 → 等 permit（模拟「等 PTY 输出」）→ 返回 v+1
fn slow_host(
    host: A1Host,
) -> impl Fn(
    &Accessor<A1Host>,
    (u32,),
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = wasmtime::Result<(u32,)>> + Send + '_>,
> {
    move |_acc: &Accessor<A1Host>, (v,): (u32,)| {
        let host = host.clone();
        Box::pin(async move {
            host.slow_calls.fetch_add(1, Ordering::SeqCst);
            let _permit = host
                .pty_notify
                .acquire()
                .await
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok((v + 1,))
        })
    }
}

fn mark_host() -> impl Fn(StoreContextMut<A1Host>, (u32,)) -> wasmtime::Result<()> {
    move |store: StoreContextMut<A1Host>, (_v,): (u32,)| {
        store.data().mark_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ==================== guest 组件（函数体只写一处 mark） ====================

/// 三个导出，函数体形状逐个只差「lift 方式」与「是否 yield」：
/// - `run-once`：**async-lifted** 导出，函数体只调一次同步 import（无挂起点）
/// - `run-once-sync`：同函数体，**同步** lift（对照组）
/// - `run-yield`：**async-lifted** 导出，函数体调一次同步 import + 一次 thread.yield
const GUEST: &str = r#"
(component
  (import "mark" (func $h_mark (param "v" u32)))

  (core module $Mem (memory (export "mem") 1))
  (core instance $mem (instantiate $Mem))

  (core module $m
    (import "" "mark" (func $mark (param i32)))
    (import "" "thread.yield" (func $thread.yield (result i32)))

    ;; 只写一处同步 import：若 task 体被重入，mark 会被调 2 次
    (func (export "run_once") (param $v i32) (result i32)
      (call $mark (local.get $v))
      (local.get $v))

    (func (export "run_yield") (param $v i32) (result i32)
      (call $mark (local.get $v))
      (drop (call $thread.yield))
      (local.get $v))
  )

  (core func $mark (canon lower (func $h_mark)))
  (core func $thread.yield (canon thread.yield))
  (core instance $i (instantiate $m
    (with "" (instance
      (export "mark" (func $mark))
      (export "thread.yield" (func $thread.yield))))))

  (func (export "run-once") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_once")))
  (func (export "run-once-sync") (param "v" u32) (result u32)
    (canon lift (core func $i "run_once")))
  (func (export "run-yield") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_yield")))
)
"#;

/// 带 callback 的 async-lifted 导出（官方 stackless 形状）
/// 结果经 `task.return` 传出；core 返回值 = CallbackCode.EXIT(0)
///（官方 cancel-host.wast / wait-forever.wast 同款：callback 为 `(param i32 i32 i32) (result i32)`）
const GUEST_CALLBACK: &str = r#"
(component
  (import "mark" (func $h_mark (param "v" u32)))

  (core module $Mem (memory (export "mem") 1))
  (core instance $mem (instantiate $Mem))

  (core module $m
    (import "" "mark" (func $mark (param i32)))
    (import "" "task.return" (func $task.return (param i32)))
    (func (export "callback") (param i32 i32 i32) (result i32)
      ;; EXIT = 0：任务体正常结束
      i32.const 0)
    (func (export "run_once") (param $v i32) (result i32)
      (call $mark (local.get $v))
      ;; 经 task.return 传结果，core 返回值 = EXIT
      (call $task.return (local.get $v))
      i32.const 0)
  )

  (core func $mark (canon lower (func $h_mark)))
  (core func $task.return (canon task.return (result u32)))
  (core instance $i (instantiate $m
    (with "" (instance
      (export "mark" (func $mark))
      (export "task.return" (func $task.return))))))

  (func (export "run-once") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_once") async (callback (core func $i "callback"))))
)
"#;

/// D6 形状：**async import + waitable 等待**（探针 D5 现场的完整形状）
///
/// 函数体 = mark(等待前) → 启动 slow（async import，挂起） → waitable 等待
/// → subdrop/wsdrop → mark(等待后)。若恢复时从函数头重跑，等待前 mark 会变 2 次。
///
/// `slow` 在 WIT 里声明为 `async`（func_wrap_concurrent 注册）；
/// `mark` 是普通同步 import。
const GUEST_WAIT: &str = r#"
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

    (func (export "run_wait") (param $v i32) (result i32)
      (local $st i32) (local $sub i32) (local $ws i32) (local $low i32)
      ;; 等待前：同步 import（若恢复时从函数头重跑，此行会变 2 次）
      (call $mark (i32.const 1))
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
      ;; 等待后：同步 import（若恢复后继续执行，此处 = 1）
      (call $mark (i32.const 2))
      (i32.add (local.get $v) (i32.const 1000)))
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

  (func (export "run-wait") async (param "v" u32) (result u32)
    (canon lift (core func $i "run_wait")))
  (func (export "run-wait-sync") (param "v" u32) (result u32)
    (canon lift (core func $i "run_wait")))
)
"#;

fn make_engine(stackful: bool) -> AnyResult<Engine> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    if stackful {
        config.wasm_component_model_async_stackful(true);
    }
    Ok(Engine::new(&config)?)
}

/// 跑一个导出函数并统计 mark 调用次数
async fn run_one<T: Send + 'static>(
    store: &mut Store<T>,
    linker: &Linker<T>,
    component: &Component,
    export: &str,
    arg: u32,
) -> AnyResult<(u32,)> {
    let instance = linker.instantiate_async(&mut *store, component).await?;
    let f = instance.get_typed_func::<(u32,), (u32,)>(&mut *store, export)?;
    let value = tokio::time::timeout(
        Duration::from_secs(5),
        store.run_concurrent(async |accessor| f.call_concurrent(accessor, (arg,)).await),
    )
    .await
    .map_err(|_| anyhow::anyhow!("超时：{export} 未在 5s 内完成"))???;
    Ok(value)
}

/// 通用判别：给定 engine + 组件，跑 async-lifted 与同步导出，统计 mark
/// （D1/D4 共用：D1 用无 stackful engine，D4 用开 stackful 的 engine）
async fn discriminate(engine: &Engine, component: &Component) -> AnyResult<()> {
    let host = A1Host::default();
    let mut linker = Linker::new(&engine);
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host.clone());
    let (v,) = run_one(&mut store, &linker, component, "run-once", 5).await?;
    let async_count = host.mark_calls.load(Ordering::SeqCst);
    assert_eq!(v, 5, "run-once 应返回入参");

    let host2 = A1Host::default();
    let mut linker2 = Linker::new(&engine);
    linker2.root().func_wrap("mark", mark_host())?;
    let mut store2 = Store::new(&engine, host2.clone());
    let (v2,) = run_one(&mut store2, &linker2, component, "run-once-sync", 6).await?;
    let sync_count = host2.mark_calls.load(Ordering::SeqCst);
    assert_eq!(v2, 6, "run-once-sync 应返回入参");

    println!(
        "  async-lifted 导出：mark 被调 {async_count} 次（函数体只写 1 处）{}",
        if async_count == 1 {
            "✓ 不重入"
        } else {
            "⚠ 重入"
        }
    );
    println!(
        "  同步导出（对照）：mark 被调 {sync_count} 次 {}",
        if sync_count == 1 {
            "✓ 不重入"
        } else {
            "⚠ 重入"
        }
    );
    Ok(())
}

/// D3：async-lifted 导出 + 同步 import + thread.yield 一次
async fn probe_yield() -> AnyResult<()> {
    let engine = make_engine(false)?;
    let host = A1Host::default();
    let mut linker = Linker::new(&engine);
    linker.root().func_wrap("mark", mark_host())?;

    let component = Component::new(&engine, GUEST)?;
    let mut store = Store::new(&engine, host.clone());
    let (v,) = run_one(&mut store, &linker, &component, "run-yield", 7).await?;
    let count = host.mark_calls.load(Ordering::SeqCst);
    assert_eq!(v, 7);
    println!(
        "  async-lifted + yield：mark 被调 {count} 次（函数体只写 1 处）{}",
        if count == 1 {
            "✓ 不重入"
        } else {
            "⚠ 重入"
        }
    );
    Ok(())
}

/// D6/D8：async import + waitable 等待形状（探针 D5 现场完整形状）
/// D6 = 不开 stackful；D8 = 开 stackful。精确计数「等待前 mark」「等待后 mark」。
async fn discriminate_wait(engine: &Engine, component: &Component) -> AnyResult<()> {
    let host = A1Host::default();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap_concurrent("slow", slow_host(host.clone()))?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, component).await?;
    let f = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-wait")?;

    let fut = store.run_concurrent(async |accessor| {
        let mut call = Box::pin(f.call_concurrent(accessor, (7,)));
        // 先 poll 一次让任务入队，然后等 slow 宿主函数真正进入（挂起真实发生）
        let _ = futures::poll!(call.as_mut());
        let entered = spin_until(|| host.slow_calls.load(Ordering::SeqCst) >= 1).await;
        if entered {
            host.pty_notify.add_permits(1);
        }
        let result = tokio::time::timeout(Duration::from_secs(5), call).await;
        Ok::<_, wasmtime::Error>((entered, result))
    });
    let (entered, inner) = tokio::time::timeout(Duration::from_secs(8), fut)
        .await
        .map_err(|_| anyhow::anyhow!("run_concurrent 超时"))???;
    let result = inner.map_err(|_| anyhow::anyhow!("run-wait 调用超时"))?;
    let (v,) = result.map_err(|e| anyhow::anyhow!("{e:#}"))?;
    assert!(entered, "slow 宿主函数应已进入（挂起真实发生）");
    assert_eq!(v, 1007, "run-wait 应返回 v+1000");

    let mark_before = host.mark_calls.load(Ordering::SeqCst);
    let slow_calls = host.slow_calls.load(Ordering::SeqCst);
    // mark(1) = 等待前，mark(2) = 等待后。若恢复时从函数头重跑：mark(1) 变 2 次；
    // 若任务体完全重入（重新启动 slow）：slow_calls 也变 2。
    let reentry = slow_calls != 1 || mark_before > 2;
    println!(
        "  async-lifted + async import：slow 进入 {slow_calls} 次 / mark 总 {mark_before} 次（等待前 1 + 等待后 1 = 2 为正常）{} {}",
        if reentry { "⚠ 重入" } else { "✓ 不重入" },
        if mark_before > 2 { "(等待前 mark 重复)" } else if slow_calls > 1 { "(slow 被重复启动)" } else { "" }
    );
    Ok(())
}

/// D7：同一 wait 形状但**同步导出**（async import 等待在同步导出里预期 trap：
/// 同步导出不是 async task，waitable 挂起 = "cannot block a synchronous task"）。
async fn probe_wait_sync_export(engine: &Engine, component: &Component) -> AnyResult<()> {
    let host = A1Host::default();
    let mut linker = Linker::new(&engine);
    linker
        .root()
        .func_wrap_concurrent("slow", slow_host(host.clone()))?;
    linker.root().func_wrap("mark", mark_host())?;

    let mut store = Store::new(&engine, host.clone());
    let instance = linker.instantiate_async(&mut store, component).await?;
    let f = instance.get_typed_func::<(u32,), (u32,)>(&mut store, "run-wait-sync")?;
    let result = store
        .run_concurrent(async |accessor| f.call_concurrent(accessor, (7,)).await)
        .await;
    match result {
        Ok(Ok((v,))) => {
            println!(
                "  同步导出 + async import：返回 v={v}（未 trap）——mark 总 {}（等待前 1 + 等待后 1 = 2 为正常）",
                host.mark_calls.load(Ordering::SeqCst)
            );
        }
        Ok(Err(e)) => println!(
            "  同步导出 + async import：trap {e:#}（预期：async 等待只能发生在 async task 内）"
        ),
        Err(e) => println!("  同步导出 + async import：run_concurrent 层错误 {e:#}"),
    }
    Ok(())
}

async fn spin_until(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..200_000 {
        if f() {
            return true;
        }
        tokio::task::yield_now().await;
    }
    f()
}

// ==================== 入口 ====================

pub async fn run_all() -> AnyResult<()> {
    println!("-- A1 重入判别（票 01）--");
    let engine_plain = make_engine(false)?;
    let component_plain = Component::new(&engine_plain, GUEST)?;

    println!("[D1] 手写组件·无 callback·不开 stackful（探针现场形状）");
    discriminate(&engine_plain, &component_plain).await?;

    println!("[D3] 同上 + thread.yield 一次");
    probe_yield().await?;

    println!("[D4] 手写组件·无 callback·开 stackful（D1 形状 + stackful feature）");
    let engine_stackful = make_engine(true)?;
    let component_stackful = Component::new(&engine_stackful, GUEST)?;
    discriminate(&engine_stackful, &component_stackful).await?;

    println!("[D5] 手写组件·带 callback（官方 stackless 形状）");
    let engine_cb = make_engine(false)?;
    let component_cb = Component::new(&engine_cb, GUEST_CALLBACK)?;
    {
        let host = A1Host::default();
        let mut linker = Linker::new(&engine_cb);
        linker.root().func_wrap("mark", mark_host())?;
        let mut store = Store::new(&engine_cb, host.clone());
        let (v,) = run_one(&mut store, &linker, &component_cb, "run-once", 8).await?;
        let count = host.mark_calls.load(Ordering::SeqCst);
        assert_eq!(v, 8);
        println!(
            "  async-lifted + callback：mark 被调 {count} 次（函数体只写 1 处）{}",
            if count == 1 {
                "✓ 不重入"
            } else {
                "⚠ 重入"
            }
        );
    }

    println!("[D5] 手写组件·带 callback（官方 stackless 形状）");
    let engine_cb = make_engine(false)?;
    let component_cb = Component::new(&engine_cb, GUEST_CALLBACK)?;
    {
        let host = A1Host::default();
        let mut linker = Linker::new(&engine_cb);
        linker.root().func_wrap("mark", mark_host())?;
        let mut store = Store::new(&engine_cb, host.clone());
        let (v,) = run_one(&mut store, &linker, &component_cb, "run-once", 8).await?;
        let count = host.mark_calls.load(Ordering::SeqCst);
        assert_eq!(v, 8);
        println!(
            "  async-lifted + callback：mark 被调 {count} 次（函数体只写 1 处）{}",
            if count == 1 {
                "✓ 不重入"
            } else {
                "⚠ 重入"
            }
        );
    }

    println!(
        "[D6] async-lifted 导出 + async import（waitable 等待）·不开 stackful（探针现场完整形状）"
    );
    let engine_wait = make_engine(false)?;
    let component_wait = Component::new(&engine_wait, GUEST_WAIT)?;
    discriminate_wait(&engine_wait, &component_wait).await?;

    println!("[D7] 同一 wait 形状但同步导出（对照）");
    probe_wait_sync_export(&engine_wait, &component_wait).await?;

    println!("[D8] async import 等待形状·开 stackful");
    let engine_wait_sf = make_engine(true)?;
    let component_wait_sf = Component::new(&engine_wait_sf, GUEST_WAIT)?;
    discriminate_wait(&engine_wait_sf, &component_wait_sf).await?;

    println!("-- A1 判别完成 --");
    Ok(())
}

#[tokio::test]
async fn a1_d1_plain() {
    let engine = make_engine(false).unwrap();
    let component = Component::new(&engine, GUEST).unwrap();
    discriminate(&engine, &component).await.unwrap();
}

#[tokio::test]
async fn a1_d3_yield() {
    probe_yield().await.unwrap();
}

#[tokio::test]
async fn a1_d4_stackful() {
    let engine = make_engine(true).unwrap();
    let component = Component::new(&engine, GUEST).unwrap();
    discriminate(&engine, &component).await.unwrap();
}

#[tokio::test]
async fn a1_d5_callback() {
    let engine = make_engine(false).unwrap();
    let component = Component::new(&engine, GUEST_CALLBACK).unwrap();
    let host = A1Host::default();
    let mut linker = Linker::new(&engine);
    linker.root().func_wrap("mark", mark_host()).unwrap();
    let mut store = Store::new(&engine, host.clone());
    let (v,) = run_one(&mut store, &linker, &component, "run-once", 8)
        .await
        .unwrap();
    assert_eq!(v, 8);
    assert_eq!(
        host.mark_calls.load(Ordering::SeqCst),
        1,
        "带 callback 不应重入"
    );
}

#[tokio::test]
async fn a1_d6_wait_async_import() {
    let engine = make_engine(false).unwrap();
    let component = Component::new(&engine, GUEST_WAIT).unwrap();
    discriminate_wait(&engine, &component).await.unwrap();
}

#[tokio::test]
async fn a1_d8_wait_stackful() {
    let engine = make_engine(true).unwrap();
    let component = Component::new(&engine, GUEST_WAIT).unwrap();
    discriminate_wait(&engine, &component).await.unwrap();
}
