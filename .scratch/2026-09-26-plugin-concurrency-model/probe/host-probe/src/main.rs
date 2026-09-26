//! 票 04 探针宿主：async-lifted 导出 + async import 的挂起 / 恢复
//!
//! 对应 issue：`.scratch/2026-09-26-plugin-concurrency-model/issues/04-p0-a4-wit-bindgen-async-guest.md`
//!
//! 三个调用形态（同一次运行内逐个执行，各自独立实例以避免相互污染）：
//! - `concurrent`：`store.run_concurrent + TypedFunc::call_concurrent`（官方 `round-trip` 用例形态）；
//! - `call_async`：`TypedFunc::call_async`（= 生产 `LoadedWasmPlugin` 现有形态：
//!   `call_async_concurrent` + `run_concurrent_trap_on_idle`）；
//! - `sync_probe`：不等待的短调用（对照：确认 store 仍可用）。
//!
//! 判定输入（供 issue 结论）：
//! 1. 挂起是否真的发生（release permit 之前 `wait_done == 0`）；
//! 2. 放行后是否恢复并返回 `v + 1 + 1000`；
//! 3. 失败时打印完整错误链（区分 guest 断言 trap / wasmtime 层错误）；
//! 4. 构建 profile 影响：`debug-assertions` 对 wasmtime 开关（同一命令跑两遍对比）。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::Semaphore;
use wasmtime::component::{Accessor, Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};

/// 探针 world 的 import 实例名（带版本：linker 的 NameMap 只做精确匹配 + semver 降级）
const HOST_IFACE: &str = "bedcode:async-export-probe/host-probe";
/// 探针 world 的导出（async-lifted）
///
/// 语法归属 wit-parser `ItemName`：`<pkg>/<iface>.<func>@<version>`——**版本在末尾**
/// （上游 doc 例：`"foo:bar/baz.bat@0.1.0"` → instance `foo:bar/baz@0.1.0` + item `bat`）。
/// 首次踩坑：写成 `.../guest-probe@0.1.0.run` → `failed to find function export`。
const EXPORT_RUN: &str = "bedcode:async-export-probe/guest-probe.run";
const EXPORT_POKE: &str = "bedcode:async-export-probe/guest-probe.poke";
/// 诊断用同步导出（装 guest 侧 panic 钩子，打印 context slot 实值）
const EXPORT_INSTALL_HOOK: &str = "bedcode:async-export-probe/guest-probe.install-hook";
/// 诊断用同步导出（读 context slot 0，不碰 WASI）
const EXPORT_CTX_READ: &str = "bedcode:async-export-probe/guest-probe.ctx-read";

// ==================== 宿主侧可观测面 ====================

#[derive(Clone)]
struct ProbeHost {
    /// async import 进入次数
    wait_calls: Arc<AtomicU32>,
    /// async import 完成次数（放行之后）
    wait_done: Arc<AtomicU32>,
    /// 放行闸门（探针侧投递 permit）
    release: Arc<Semaphore>,
}

impl ProbeHost {
    fn new() -> Self {
        Self {
            wait_calls: Arc::new(AtomicU32::new(0)),
            wait_done: Arc::new(AtomicU32::new(0)),
            release: Arc::new(Semaphore::new(0)),
        }
    }
}

struct ProbeState {
    host: ProbeHost,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl WasiView for ProbeState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// 放行任务：等 guest 真进入 async import（挂起）后投递一个 permit
fn spawn_releaser(host: ProbeHost, label: &'static str) {
    tokio::spawn(async move {
        for _ in 0..200_000 {
            if host.wait_calls.load(Ordering::SeqCst) >= 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        let entered = host.wait_calls.load(Ordering::SeqCst);
        let done_before = host.wait_done.load(Ordering::SeqCst);
        println!(
            "[{label}] releaser: entered={entered} done_before_release={done_before} (挂起成立判据：done_before_release==0)"
        );
        host.release.add_permits(1);
    });
}

fn register_probe(linker: &mut Linker<ProbeState>) -> Result<()> {
    linker
        .root()
        .instance(HOST_IFACE)?
        .func_wrap_concurrent("wait", |acc: &Accessor<ProbeState>, (v,): (u32,)| {
            // 句柄在闭包的同步段取出（不在 await 之后再碰 Accessor）
            let (calls, done, release) = acc.with(|mut a| {
                let host = &a.data_mut().host;
                (
                    host.wait_calls.clone(),
                    host.wait_done.clone(),
                    host.release.clone(),
                )
            });
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let _permit = release
                    .acquire()
                    .await
                    .map_err(|e| wasmtime::Error::msg(format!("probe release closed: {e}")))?;
                done.fetch_add(1, Ordering::SeqCst);
                Ok((v + 1,))
            })
        })?;
    Ok(())
}

fn build_engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config)?;
    // concurrency_support 默认 true（wasmtime 48 上游 `Config::concurrency_support` 文档）；
    // 探针侧显式依赖该默认值（生产 runtime.rs:307 同样只开 wasm_component_model_async）
    println!("[env] wasmtime=48.0.3（Cargo.toml 钉死）wasm_component_model_async=true");
    Ok(engine)
}

struct ProbeInstance {
    store: Store<ProbeState>,
    run: wasmtime::component::TypedFunc<(u32,), (u32,)>,
    poke: wasmtime::component::TypedFunc<(u32,), (u32,)>,
    /// 同步诊断导出（装了 guest 侧 panic 钩子）；老 guest 产物没有 → None
    install_hook: Option<wasmtime::component::TypedFunc<(), ()>>,
    /// 同步诊断导出（读 context slot 0，不碰 WASI）
    ctx_read: Option<wasmtime::component::TypedFunc<(), (u32,)>>,
    host: ProbeHost,
}

async fn instantiate(engine: &Engine, component: &Component, linker: &Linker<ProbeState>) -> Result<ProbeInstance> {
    let host = ProbeHost::new();
    // 继承 stderr/stdout：guest 侧 panic 消息（含断言原文）必须可见——它是
    // 「abort 发生在哪个断言」的唯一证据来源
    let wasi = wasmtime_wasi::WasiCtxBuilder::new()
        .inherit_stdout()
        .inherit_stderr()
        .build();
    let state = ProbeState {
        host: host.clone(),
        wasi,
        table: ResourceTable::default(),
    };
    let mut store = Store::new(engine, state);
    let instance = linker.instantiate_async(&mut store, component).await?;
    // 必须经 `ItemName`（裸 &str 走单层精确匹配，接口导出是嵌套实例 → 必然找不到）
    let run_item: wasmtime::component::wit_parser::ItemName = EXPORT_RUN
        .parse()
        .map_err(|e| anyhow::anyhow!("parse item name {EXPORT_RUN}: {e}"))?;
    let poke_item: wasmtime::component::wit_parser::ItemName = EXPORT_POKE
        .parse()
        .map_err(|e| anyhow::anyhow!("parse item name {EXPORT_POKE}: {e}"))?;
    // wasmtime::Error 不是 std::error::Error（无 anyhow Context），显式转字符串附上下文
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, &run_item)
        .map_err(|e| anyhow::anyhow!("lookup export {EXPORT_RUN}: {e}"))?;
    let poke = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, &poke_item)
        .map_err(|e| anyhow::anyhow!("lookup export {EXPORT_POKE}: {e}"))?;
    let install_hook = EXPORT_INSTALL_HOOK
        .parse::<wasmtime::component::wit_parser::ItemName>()
        .ok()
        .and_then(|item| instance.get_typed_func::<(), ()>(&mut store, &item).ok());
    let ctx_read = EXPORT_CTX_READ
        .parse::<wasmtime::component::wit_parser::ItemName>()
        .ok()
        .and_then(|item| instance.get_typed_func::<(), (u32,)>(&mut store, &item).ok());
    Ok(ProbeInstance {
        store,
        run,
        poke,
        install_hook,
        ctx_read,
        host,
    })
}

/// 形态 1：`run_concurrent` + `call_concurrent`（官方 round-trip 形态）
async fn style_concurrent(engine: &Engine, component: &Component, linker: &Linker<ProbeState>) -> Result<()> {
    let mut probe = instantiate(engine, component, linker).await?;
    spawn_releaser(probe.host.clone(), "concurrent");
    let run = probe.run;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        probe.store.run_concurrent(async |accessor| run.call_concurrent(accessor, (7,)).await),
    )
    .await;
    match result {
        Err(_) => println!("[concurrent] 结果：超时（10s 未返回）"),
        Ok(Err(e)) => println!("[concurrent] 结果：wasmtime 层 Err（run_concurrent 外层）\n{e:?}"),
        Ok(Ok(Err(e))) => println!("[concurrent] 结果：guest 调用 Err\n{e:?}"),
        Ok(Ok(Ok((v,)))) => println!("[concurrent] 结果：OK v={v}（期望 1008 = 7+1+1000）"),
    }
    println!(
        "[concurrent] 计数：wait_calls={} wait_done={}",
        probe.host.wait_calls.load(Ordering::SeqCst),
        probe.host.wait_done.load(Ordering::SeqCst)
    );

    // store 仍可用（对照短调用）
    let poke = probe.poke;
    let p = tokio::time::timeout(
        Duration::from_secs(5),
        probe.store.run_concurrent(async |accessor| poke.call_concurrent(accessor, (4,)).await),
    )
    .await;
    match p {
        Err(_) => println!("[concurrent] poke：超时"),
        Ok(Err(e)) => println!("[concurrent] poke：wasmtime Err\n{e:?}"),
        Ok(Ok(Err(e))) => println!("[concurrent] poke：guest Err\n{e:?}"),
        Ok(Ok(Ok((v,)))) => println!("[concurrent] poke：OK v={v}（期望 5 = 4+1）"),
    }
    Ok(())
}

/// 形态 2：`call_async`（= 生产 `LoadedWasmPlugin` 现有形态）
async fn style_call_async(engine: &Engine, component: &Component, linker: &Linker<ProbeState>) -> Result<()> {
    let mut probe = instantiate(engine, component, linker).await?;
    spawn_releaser(probe.host.clone(), "call_async");
    let run = probe.run;
    let result = tokio::time::timeout(Duration::from_secs(10), run.call_async(&mut probe.store, (7,))).await;
    match result {
        Err(_) => println!("[call_async] 结果：超时（10s 未返回）"),
        Ok(Err(e)) => println!("[call_async] 结果：Err\n{e:?}"),
        Ok(Ok((v,))) => println!("[call_async] 结果：OK v={v}（期望 1008）"),
    }
    println!(
        "[call_async] 计数：wait_calls={} wait_done={}",
        probe.host.wait_calls.load(Ordering::SeqCst),
        probe.host.wait_done.load(Ordering::SeqCst)
    );
    Ok(())
}

/// 隔离实验：**不等待**的 async-lifted 导出（`poke`）单独调用
///
/// 判据：若它同样在入口 abort ⇒ 问题与「async import 挂起」无关，是 async-lift 入口本身；
/// 若它通过 ⇒ 问题只在「挂起后重新进入」链路（callback 路径）。
async fn style_poke_only(engine: &Engine, component: &Component, linker: &Linker<ProbeState>) -> Result<()> {
    let mut probe = instantiate(engine, component, linker).await?;

    // ① 入口前读槽（不碰 WASI）
    if let Some(ctx_read) = probe.ctx_read {
        let seen = tokio::time::timeout(
            Duration::from_secs(5),
            probe
                .store
                .run_concurrent(async |accessor| ctx_read.call_concurrent(accessor, ()).await),
        )
        .await;
        match seen {
            Ok(Ok(Ok((v,)))) => println!("[poke-only] ① 入口前 ctx[0]={v:#x}"),
            other => println!("[poke-only] ① ctx-read 失败：{other:?}"),
        }
    }

    // ② 装 guest 侧 panic 钩子（同步导出；它自身会写 stderr = 触发 async WASI 宿主调用）
    let install = probe
        .install_hook
        .ok_or_else(|| anyhow::anyhow!("guest 未导出 install-hook"))?;
    let installed = tokio::time::timeout(
        Duration::from_secs(5),
        probe
            .store
            .run_concurrent(async |accessor| install.call_concurrent(accessor, ()).await),
    )
    .await;
    match installed {
        Err(_) => println!("[poke-only] ② install-hook：超时"),
        Ok(Err(e)) => println!("[poke-only] ② install-hook：wasmtime Err\n{e:?}"),
        Ok(Ok(Err(e))) => println!("[poke-only] ② install-hook：guest Err\n{e:?}"),
        Ok(Ok(Ok(()))) => println!("[poke-only] ② install-hook：OK（钩子已装）"),
    }

    // ③ WASI 调用之后再读一次槽：定位「谁写脏了槽」
    if let Some(ctx_read) = probe.ctx_read {
        let seen = tokio::time::timeout(
            Duration::from_secs(5),
            probe
                .store
                .run_concurrent(async |accessor| ctx_read.call_concurrent(accessor, ()).await),
        )
        .await;
        match seen {
            Ok(Ok(Ok((v,)))) => println!("[poke-only] ③ WASI 调用后 ctx[0]={v:#x}"),
            other => println!("[poke-only] ③ ctx-read 失败：{other:?}"),
        }
    }

    let poke = probe.poke;
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        probe.store.run_concurrent(async |accessor| poke.call_concurrent(accessor, (4,)).await),
    )
    .await;
    match result {
        Err(_) => println!("[poke-only] 结果：超时"),
        Ok(Err(e)) => println!("[poke-only] 结果：wasmtime Err\n{e:?}"),
        Ok(Ok(Err(e))) => println!("[poke-only] 结果：guest Err\n{e:?}"),
        Ok(Ok(Ok((v,)))) => println!("[poke-only] 结果：OK v={v}（期望 5）"),
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let wasm = args
        .next()
        .context("usage: concurrency-owner-probe <guest.wasm> [both|concurrent|call_async|poke]")?;
    let mode = args.next().unwrap_or_else(|| "both".to_string());

    let engine = build_engine()?;
    let component =
        Component::from_file(&engine, &wasm).map_err(|e| anyhow::anyhow!("load probe component: {e}"))?;
    let mut linker = Linker::<ProbeState>::new(&engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker).map_err(|e| anyhow::anyhow!("register WASI p3: {e}"))?;
    register_probe(&mut linker)?;

    println!("=== 票 04 探针：async 导出 + async import（mode={mode}）===");
    if mode == "both" || mode == "call_async" {
        style_call_async(&engine, &component, &linker).await?;
    }
    if mode == "both" || mode == "concurrent" {
        style_concurrent(&engine, &component, &linker).await?;
    }
    if mode == "poke" {
        style_poke_only(&engine, &component, &linker).await?;
    }
    Ok(())
}
