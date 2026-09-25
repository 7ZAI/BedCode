//! P3 async host import 垂直探针（.scratch/2026-09-25-wasip3-host-api-optimization/issues/01）
//!
//! 目标（不改生产 WIT / ABI / 插件契约 / 运行时行为，全部证据来自测试级世界）：
//! 证明**宿主实现侧 async 化**能让长等待的 host import 在等待期间把 Tokio 执行
//! 线程归还给调度器——即消除 `block_on_async` 桥「占住整条线程」的形态。
//!
//! 机制形态（与生产 `host-*` 契约同签名：同步 `func`）：
//! - guest：`run` 同步调用 `invoke`，guest 侧无 `block_on`、无等待原语；
//! - 宿主：`invoke` 经 `func_wrap_async` 注册为原生 async，实现里真 `.await`
//!   oneshot（禁止 `block_on_async` / `spawn_blocking` /「起后台任务再同步 wait」）。
//!
//! 断言链：
//! 1. import 挂起期间，不相关 heartbeat 在**单线程** runtime 上继续推进（让出）；
//! 2. 同一实例的第二次进入仍被串行化（优化目标是让出线程，不是同实例并发）；
//! 3. 另一实例同时挂起（不同实例互不阻塞，证明让出的是线程而非「全局一把锁」）；
//! 4. 一次性失败路径返回结构化错误，之后 Store 仍可执行下一次调用；
//! 5. 生产 `plugin` world / ABI 常量零变更（读文件断言）。
//!
//! 工具链：wasmtime 48 + `wasm32-wasip3`（`WASIP3_NIGHTLY`）+ wit-bindgen 0.60，
//! 详见 `docs/knowledge/wasip3-toolchain.md` 与 issue 01 `## Findings`。

use super::{wasip3_toolchain_ready, WASIP3_NIGHTLY};
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store, StoreContextMut};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../packages/plugin-p3-async-host-import-test/wit/p3-async-host-probe.wit",
    world: "p3-async-host-probe",
});

const FORCED_FAILURE: &str = "p3-probe-forced-failure";
const WAIT_RESULT_PREFIX: &str = "wait-complete:";
const FAIL_ONCE_RECOVERED_PREFIX: &str = "fail-once-recovered:";
const NORMAL_RESULT_PREFIX: &str = "normal-complete:";

/// 探针控制面：宿主 import 实现的等待闸门 + 进入记账
///
/// `active` / `max_active` 用于断言「同一实例串行」；`started` 用于断言
/// 「串行化发生在宿主进入之前（第二次调用根本没进入 host 实现）」。
struct ProbeControl {
    instance_id: &'static str,
    release: TokioMutex<Option<oneshot::Receiver<()>>>,
    entered: mpsc::UnboundedSender<String>,
    fail_once: AtomicBool,
    started: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl ProbeControl {
    fn new(
        instance_id: &'static str,
        release: oneshot::Receiver<()>,
        entered: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            instance_id,
            release: TokioMutex::new(Some(release)),
            entered,
            fail_once: AtomicBool::new(true),
            started: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }

    fn mark_started(&self) {
        self.started.fetch_add(1, Ordering::SeqCst);
    }
}

struct ProbeState {
    control: Arc<ProbeControl>,
    wasi_ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for ProbeState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

/// 宿主 import 实现：**原生 async**——等待期间不占线程
///
/// 三种 mode：
/// - `wait`：await oneshot（由测试侧释放），模拟长等待原语（WS/HTTP/PTY）；
/// - `fail-once`：首次立即返回结构化错误，之后成功（Store 可回收）；
/// - `normal`：立即成功。
async fn invoke_mode(control: Arc<ProbeControl>, mode: String) -> Result<String, String> {
    control.mark_started();

    match mode.as_str() {
        "wait" => {
            let active = control.active.fetch_add(1, Ordering::SeqCst) + 1;
            control.max_active.fetch_max(active, Ordering::SeqCst);
            if control.entered.send(control.instance_id.to_string()).is_err() {
                control.active.fetch_sub(1, Ordering::SeqCst);
                return Err("p3-probe-entered-channel-closed".to_string());
            }

            let release = {
                let mut receiver = control.release.lock().await;
                receiver.take()
            };
            let Some(release) = release else {
                control.active.fetch_sub(1, Ordering::SeqCst);
                return Err("p3-probe-release-already-consumed".to_string());
            };

            // 真挂起点：此处宿主执行线程必须被归还（heartbeat 断言）
            let result = release.await;
            control.active.fetch_sub(1, Ordering::SeqCst);
            match result {
                Ok(()) => Ok(format!("{WAIT_RESULT_PREFIX}{}", control.instance_id)),
                Err(_) => Err("p3-probe-release-canceled".to_string()),
            }
        }
        "fail-once" => {
            if control.fail_once.swap(false, Ordering::SeqCst) {
                Err(FORCED_FAILURE.to_string())
            } else {
                Ok(format!(
                    "{FAIL_ONCE_RECOVERED_PREFIX}{}",
                    control.instance_id
                ))
            }
        }
        "normal" => Ok(format!("{NORMAL_RESULT_PREFIX}{}", control.instance_id)),
        other => Err(format!("p3-probe-unknown-mode:{other}")),
    }
}

/// 宿主 import 在组件里的实例名（带版本：wasmtime linker 的 `NameMap` 只做
/// 精确匹配 + semver 兼容降级，不带版本的名字匹配不上带版本的 import）
const HOST_PROBE_INSTANCE: &str = "bedcode:p3-async-host-probe/host-probe@0.1.0";

/// 把 host import 注册为 async（`func_wrap_async`）：宿主侧真 `.await`
///
/// 与生产 `add_to_linker` 的差别只在注册方式（`func_wrap` → `func_wrap_async`），
/// WIT 签名与 guest 代码完全不变——这正是本票要验证的「实现侧 async 化」。
fn register_async_host_probe(linker: &mut Linker<ProbeState>) {
    linker
        .root()
        .instance(HOST_PROBE_INSTANCE)
        .expect("host-probe linker instance")
        .func_wrap_async(
            "invoke",
            |mut store: StoreContextMut<'_, ProbeState>, (mode,): (String,)| {
                let control = store.data().control.clone();
                Box::new(async move { Ok((invoke_mode(control, mode).await,)) })
                    as Box<dyn Future<Output = wasmtime::Result<(Result<String, String>,)>> + Send>
            },
        )
        .expect("register async host probe");
}

struct ProbeInstance {
    store: Store<ProbeState>,
    exports: P3AsyncHostProbe,
}

impl ProbeInstance {
    async fn call(&mut self, mode: &str) -> Result<String, String> {
        self.exports
            .bedcode_p3_async_host_probe_guest_probe()
            .func_run()
            .call_async(&mut self.store, (mode,))
            .await
            .expect("P3 probe guest export must execute")
            .0
            .map_err(|e| e.to_string())
    }
}

async fn instantiate_probe(
    engine: &Engine,
    linker: &Linker<ProbeState>,
    component: &Component,
    control: Arc<ProbeControl>,
) -> ProbeInstance {
    let state = ProbeState {
        control,
        wasi_ctx: WasiCtx::default(),
        table: ResourceTable::default(),
    };
    let mut store = Store::new(engine, state);
    let exports = P3AsyncHostProbe::instantiate_async(&mut store, component, linker)
        .await
        .expect("P3 probe component must instantiate");
    ProbeInstance { store, exports }
}

async fn call_locked(
    instance: Arc<TokioMutex<ProbeInstance>>,
    mode: &'static str,
) -> Result<String, String> {
    let mut instance = instance.lock().await;
    instance.call(mode).await
}

async fn join_probe_task(
    task: tokio::task::JoinHandle<Result<String, String>>,
) -> Result<String, String> {
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("P3 probe task timed out")
        .expect("P3 probe task panicked")
}

fn cargo_shim() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME must be set")
        .join(".cargo/bin/cargo")
}

fn build_p3_async_host_import_component() -> Vec<u8> {
    assert!(
        wasip3_toolchain_ready(),
        "{WASIP3_NIGHTLY} + wasm32-wasip3 is required; run scripts/wasip3-toolchain.sh install"
    );

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir.join("../packages/plugin-p3-async-host-import-test");
    let module_path = fixture_dir.join(
        "target/wasm32-wasip3/release/bedcode_plugin_p3_async_host_import_test.wasm",
    );

    if module_path.exists() {
        let module_modified = fs::metadata(&module_path)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let sources = [
            fixture_dir.join("src/lib.rs"),
            fixture_dir.join("Cargo.toml"),
            fixture_dir.join("wit/p3-async-host-probe.wit"),
        ];
        let needs_rebuild = sources.iter().any(|source| {
            fs::metadata(source)
                .and_then(|metadata| metadata.modified())
                .map(|modified| modified > module_modified)
                .unwrap_or(true)
        });
        if !needs_rebuild {
            return fs::read(&module_path).expect("read cached P3 async host import component");
        }
    }

    let status = Command::new(cargo_shim())
        .env("RUSTUP_TOOLCHAIN", WASIP3_NIGHTLY)
        .args([
            "build",
            "--target",
            "wasm32-wasip3",
            "--release",
            "--manifest-path",
            fixture_dir.join("Cargo.toml").to_str().unwrap(),
        ])
        .status()
        .expect("run cargo shim for P3 async host import component");
    assert!(
        status.success(),
        "P3 async host import component WASM build failed"
    );

    fs::read(&module_path).expect("read built P3 async host import component")
}

/// 挂起看门狗：把「宿主实现占住线程导致的死锁」变成明确失败
///
/// 变异自检实证：把等待改成「起后台任务 await + 本线程忙等」（等价生产
/// `block_on_async` 桥占住线程）后，单线程 runtime 下 heartbeat **和**
/// `tokio::time::timeout` 都无法推进——timer 由同一条线程驱动，于是用例永久
/// 挂起。看门狗用独立 std 线程计时，超时即终止进程，避免整轮 `cargo test`
/// 永不返回（与 `ws_e2e_guard` 的兜底超时同思路）。
struct HangWatchdog {
    finished: Arc<AtomicBool>,
}

impl HangWatchdog {
    fn start(label: &'static str, secs: u64) -> Self {
        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            if !flag.load(Ordering::SeqCst) {
                eprintln!(
                    "[p3-probe] {label}: 超过 {secs}s 未完成（宿主实现占住执行线程 → 死锁）"
                );
                std::process::exit(1);
            }
        });
        Self { finished }
    }
}

impl Drop for HangWatchdog {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
    }
}

/// P0 门禁：宿主 async host import 在等待期间让出**单线程** runtime 的执行线程
///
/// `current_thread` 是最强的形态——若宿主实现占住线程（同步 `func_wrap` +
/// `block_on_async` 桥），heartbeat 在 import 返回前一次都不会推进，本用例红。
#[tokio::test(flavor = "current_thread")]
async fn test_p3_async_host_import_yields_runtime() {
    let _watchdog = HangWatchdog::start("test_p3_async_host_import_yields_runtime", 20);
    let component_bytes = build_p3_async_host_import_component();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let production_wit =
        fs::read_to_string(manifest_dir.join("../packages/plugin-sdk-desktop/rust/wit/bedcode.wit"))
            .expect("read production WIT");
    let production_abi =
        fs::read_to_string(manifest_dir.join("../packages/plugin-sdk-desktop/rust/src/abi.rs"))
            .expect("read production ABI");
    assert!(!production_wit.contains("p3-async-host-probe"));
    assert!(!production_abi.contains("P3_ASYNC_HOST_IMPORT_PROBE"));

    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config).expect("create P3 async probe engine");
    let component = Component::from_binary(&engine, &component_bytes)
        .expect("compile P3 async host import component");

    let mut linker = Linker::<ProbeState>::new(&engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker).expect("register P3 WASI interfaces");
    register_async_host_probe(&mut linker);

    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (release_a_tx, release_a_rx) = oneshot::channel();
    let (release_b_tx, release_b_rx) = oneshot::channel();
    let control_a = Arc::new(ProbeControl::new("a", release_a_rx, entered_tx.clone()));
    let control_b = Arc::new(ProbeControl::new("b", release_b_rx, entered_tx));
    let instance_a = Arc::new(TokioMutex::new(
        instantiate_probe(&engine, &linker, &component, Arc::clone(&control_a)).await,
    ));
    let instance_b = Arc::new(TokioMutex::new(
        instantiate_probe(&engine, &linker, &component, Arc::clone(&control_b)).await,
    ));

    // ① 实例 A 进入长等待 import 并挂起
    let first_a = tokio::spawn(call_locked(Arc::clone(&instance_a), "wait"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), entered_rx.recv())
            .await
            .expect("instance A host import timed out")
            .expect("instance A host import entry missing"),
        "a"
    );
    assert_eq!(control_a.started.load(Ordering::SeqCst), 1);
    assert_eq!(control_a.active.load(Ordering::SeqCst), 1);
    assert!(!first_a.is_finished());

    // ② import 仍挂起期间，不相关 heartbeat 必须在单线程 runtime 上跑完
    let heartbeat = Arc::new(AtomicUsize::new(0));
    let heartbeat_task = {
        let heartbeat = Arc::clone(&heartbeat);
        tokio::spawn(async move {
            for _ in 0..8 {
                heartbeat.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
        })
    };
    tokio::time::timeout(Duration::from_secs(5), heartbeat_task)
        .await
        .expect("heartbeat timed out")
        .expect("heartbeat panicked");
    assert_eq!(heartbeat.load(Ordering::SeqCst), 8);
    assert!(!first_a.is_finished());

    // ③ 同实例第二次进入：被实例锁串行化（未进入 host 实现）
    let queued = oneshot::channel();
    let second_a = {
        let instance = Arc::clone(&instance_a);
        let queued = queued.0;
        tokio::spawn(async move {
            queued
                .send(())
                .expect("record second caller lock attempt");
            call_locked(instance, "fail-once").await
        })
    };
    tokio::time::timeout(Duration::from_secs(5), queued.1)
        .await
        .expect("second caller did not attempt the instance lock")
        .expect("second caller stopped before the instance lock");
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert!(!second_a.is_finished());
    assert_eq!(control_a.started.load(Ordering::SeqCst), 1);
    assert_eq!(control_a.max_active.load(Ordering::SeqCst), 1);

    // ④ 另一实例同时挂起：让出的是执行线程，不是「全局一次一个调用」
    let first_b = tokio::spawn(call_locked(Arc::clone(&instance_b), "wait"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), entered_rx.recv())
            .await
            .expect("instance B host import timed out")
            .expect("instance B host import entry missing"),
        "b"
    );
    assert!(!first_b.is_finished());
    assert_eq!(control_b.started.load(Ordering::SeqCst), 1);
    assert_eq!(control_b.active.load(Ordering::SeqCst), 1);

    release_b_tx
        .send(())
        .expect("release instance B host import");
    assert_eq!(join_probe_task(first_b).await, Ok("wait-complete:b".to_string()));
    assert_eq!(control_b.active.load(Ordering::SeqCst), 0);

    release_a_tx
        .send(())
        .expect("release instance A host import");
    assert_eq!(join_probe_task(first_a).await, Ok("wait-complete:a".to_string()));
    assert_eq!(
        join_probe_task(second_a).await,
        Err(FORCED_FAILURE.to_string())
    );
    assert_eq!(control_a.started.load(Ordering::SeqCst), 2);
    assert_eq!(control_a.max_active.load(Ordering::SeqCst), 1);

    // ⑤ 立即失败路径之后 Store 仍可用（错误经 result<string,string> 结构化回带）
    let mut instance_a = instance_a.lock().await;
    assert_eq!(
        instance_a.call("fail-once").await,
        Ok("fail-once-recovered:a".to_string())
    );
    assert_eq!(
        instance_a.call("normal").await,
        Ok("normal-complete:a".to_string())
    );
    assert_eq!(control_a.active.load(Ordering::SeqCst), 0);
    assert_eq!(control_a.started.load(Ordering::SeqCst), 4);

    println!(
        "P3 async host import probe passed: heartbeat={}, same_instance_max_active={}, different_instance=true, store_recovered=true",
        heartbeat.load(Ordering::SeqCst),
        control_a.max_active.load(Ordering::SeqCst)
    );
}
