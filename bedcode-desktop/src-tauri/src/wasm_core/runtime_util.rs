//! 异步桥基础设施（core-runtime-util，中立层）
//!
//! WASM host function 是同步的，但宿主能力实现（host-* 原语 / 内核管理面）需要
//! 驱动 async Tokio 代码（数据库、tokio 锁等）。本模块提供同步↔异步桥
//! [`block_on_async`] 及其依赖的 ambient runtime 基础设施（[`ambient_handle`]
//! 与 [`block_on_ambient`]）。
//!
//! **位置纪律（票 01：wasm_core 依赖单向化）**：这三个符号是应用无关的运行时
//! 工具，原定义在 `crate::wasm_core::manager::runtime`，使得 `host_api` 与
//! `security` 只为调桥就反向依赖 manager（见
//! `.scratch/2026-09-24-wasm-core-decouple/spec.md` C2）。归位后本模块**不引用
//! 任何 wasm_core 兄弟模块**，只能被依赖，不得反向依赖
//! （`manager` / `host_api` / `security` 皆可引用本模块）。
//!
//! 语义、重入保护与 ambient runtime 行为逐字保留自 `manager::runtime`——这是
//! CER（actix current_thread 互调自锁 / wasi ambient runtime 共存）的实证产物，
//! 改动前先读下方注释与 `.scratch/2026-09-24-wasm-core-decouple/issues/01`。

// ==================== Async Blocking Helper ====================

thread_local! {
    /// 当前线程是否已处于 block_in_place 让出后的阻塞上下文
    static IN_BLOCK_IN_PLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 无当前 runtime handle 的线程（spawn_blocking / 纯 std 线程）执行 block_on 时
/// 的全局收益运行时：与 wasmtime-wasi 的 ambient runtime 同策略，供宿主函数在
/// 无 handle 线程上仍可阻塞执行（WASI 预打开模式下插件调用跑在阻塞线程上）
static AMBIENT_RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .expect("create ambient tokio runtime")
});

/// 重入标志的 RAII 守卫：作用域退出（含 block_in_place panic 穿透）时复位标志，
/// 避免线程残留 `true` 导致后续调用恒走新线程路径（正确但多一次线程切换）
struct BlockInPlaceGuard;

impl BlockInPlaceGuard {
    /// 进入阻塞上下文：重入时返回 None（调用方改走新线程路径）
    fn enter() -> Option<Self> {
        if IN_BLOCK_IN_PLACE.with(|f| f.get()) {
            return None;
        }
        IN_BLOCK_IN_PLACE.with(|f| f.set(true));
        Some(BlockInPlaceGuard)
    }
}

impl Drop for BlockInPlaceGuard {
    fn drop(&mut self) {
        IN_BLOCK_IN_PLACE.with(|f| f.set(false));
    }
}

/// 在同步上下文中执行 async 闭包，兼容多线程和 current_thread 运行时
///
/// WASM host functions 是同步的，但需要调用 async Tokio 代码（数据库、锁等）。
/// 标准做法 `block_in_place(|| block_on(...))` 仅在多线程运行时上可用，
/// Actix Web 的 `actix-rt` 使用 `current_thread` 运行时，会导致 panic。
///
/// 策略：
/// - 多线程运行时：`block_in_place` + `block_on`（不阻塞 worker 线程）
/// - current_thread 运行时或非运行时线程：`std::thread::spawn` + `block_on`（新线程上运行）
///
/// 重入安全：`dispatch_to_wasm` → 插件 on_message → host http_fetch 的调用链会
/// 嵌套调用本函数。嵌套 `block_in_place` 在已让出的线程上会 panic；而嵌套
/// `handle.block_on` 同样 panic——外层 `block_in_place(|| handle.block_on(...))`
/// 的 tokio enter 守卫仍挂在当前线程上（block_in_place 只是把线程让出 worker 池，
/// 守卫不释放），实证见 panic.log 的 wasm_runtime.rs:82 FATAL
/// （"Cannot start a runtime from within a runtime"）。两种 panic 都会穿透污染
/// wasmtime Store、插件永久不可用，故用线程局部标志检测重入，重入时改在
/// **新线程上 block_on**：新线程无 enter 守卫、非 worker，任意 flavor 均合法，
/// 外层线程 join 等待（runtime 其他 worker 推进 IO，无死锁）。
pub(crate) fn block_on_async<F, R>(fut: F) -> R
where
    F: std::future::Future<Output = R> + Send,
    R: Send + 'static,
{
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        // 无当前 runtime 上下文（spawn_blocking 阻塞线程 / 纯 std 线程）：
        // 在全局 ambient multi-thread 运行时上阻塞执行。
        // 与 wasmtime-wasi 的 ambient runtime 同策略——这是 WASI 预打开模式的关键：
        // 插件调用被搬到无 handle 线程后，wasi 同步绑定（in_tokio）走其自身 ambient
        // runtime，宿主函数经此 ambient runtime 阻塞执行，两者互不冲突。
        return AMBIENT_RT.block_on(fut);
    };
    match handle.runtime_flavor() {
        tokio::runtime::RuntimeFlavor::MultiThread => {
            if let Some(_guard) = BlockInPlaceGuard::enter() {
                // guard 持有期间当前线程在 worker 池外阻塞；退出（含 panic）时复位重入标志
                tokio::task::block_in_place(|| handle.block_on(fut))
            } else {
                // 重入：当前线程已被外层 block_in_place + handle.block_on 占据
                // （enter 守卫仍生效），嵌套 handle.block_on 必然 panic
                // （Cannot start a runtime from within a runtime）。
                // 新线程无 enter 守卫，block_on 合法；外层同步 join 等待结果。
                std::thread::scope(|s| {
                    s.spawn(|| handle.block_on(fut))
                        .join()
                        .expect("block_on_async: spawned thread panicked")
                })
            }
        }
        _ => {
            // current_thread 运行时（#[tokio::test] / Actix-rt worker）：当前线程
            // 已在 runtime context 内，两条路都走不通：
            // - `handle.block_on`（current_thread 调度器由 owner 线程独占驱动，
            //   本线程即 owner 线程，直接调用必然死锁；跨线程驱动 IO/process
            //   future 同样永久空转——历史死锁：process_kill 测试）；
            // - `AMBIENT_RT.block_on`（本线程）：重入检查 panic
            //   （"Cannot start a runtime from within a runtime"）。
            // 方案：在 scoped 新线程（无 runtime 上下文、支持非 'static future）
            // 上 AMBIENT_RT.block_on。ambient runtime 是 multi_thread + enable_all，
            // IO/process/time 驱动齐全，multi_thread 的 block_on 契约本就允许任意
            // 线程调用（future 在调用线程内执行、spawned 任务进线程池）。
            //
            // ⚠️ 本分支会阻塞调用线程直到 future 完成：**调用方必须是「不驱动
            // future 所依赖资源」的线程**。actix arbiter 是反例——宿主 WS 原语要
            // await arbiter 上的连接 actor，投递任务若在 arbiter 线程上同步等待即
            // 自锁（见 `ambient_handle` 说明）。
            std::thread::scope(|s| {
                s.spawn(|| AMBIENT_RT.block_on(fut))
                    .join()
                    .expect("block_on_async: spawned thread panicked")
            })
        }
    }
}

/// 在全局 ambient runtime 上同步阻塞驱动 future（供无 handle 的阻塞线程使用）
///
/// 与 [`block_on_async`] 的 ambient 兜底同 runtime，但**不要求** future/
/// 输出满足 `'static`——仅同步驱动当前 future 并返回结果，不把 future
/// 交给其它执行器接管。`run_guest_call` 在 `spawn_blocking` 线程驱动 tokio
/// Mutex 锁获取用（借用闭包内 Arc，无法满足 `'static` 约束）。
pub(crate) fn block_on_ambient<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send,
{
    AMBIENT_RT.block_on(fut)
}

/// ambient runtime 句柄：在「调用方线程不可被占用」的场景派生后台任务
///
/// 典型场景是 **actix arbiter**：它是 `current_thread` 运行时、由本线程独占驱动，
/// 而插件投递用的是同步桥 [`block_on_async`]（会阻塞调用线程）。若投递任务跑在
/// arbiter 上，客人回调里的宿主原语（如 WS 端点的 `send-text-to-client`）需要
/// await arbiter 上的连接 actor —— arbiter 被投递自己占住，双方互等形成自锁
/// （实证：插件端点回显帧）。故此类投递改在 ambient runtime 上派生，arbiter 保持
/// 空闲以推进 actor。
pub(crate) fn ambient_handle() -> tokio::runtime::Handle {
    AMBIENT_RT.handle().clone()
}
