//! 终端输出消费插件化 · 性能前置验证（只读探针，不碰生产路径）
//!
//! 依据：`.scratch/2026-09-21-terminal-output-consumer-perf/spec.md`（roadmap 阶段 3
//! 前置条件「输出分发管道的『内核保有 + 消费插件化』先行验证」——未验证，本探针即该项）。
//!
//! 验证问题：终端 UI/渲染下沉到插件后，PTY 输出路径多一跳 WASM 边界（guest 经
//! `ring-fetch` 拉取 `list<u8>`），这一跳的附加成本是否可接受？
//!
//! 场景对应：
//! - P1：宿主侧纯 Rust 基线——直接操作 `PtyRing`（无 WASM 边界）push 1 MiB 确定性
//!   字节再全量 fetch，测每字节成本与单次 fetch 固定开销（下界）；
//! - P2：插件侧确定性数据——spawn 真实命令（`head -c 1M /dev/zero | tr '\0' a; sleep`，
//!   输出**恰好 1 MiB 的 'a'** 且进程长驻保持句柄可寻址），guest 按不同 `maxBytes`
//!   （1K / 4K / 16K / 64K）批量拉满 1 MiB，测单次调用开销（µs/op）、每字节成本
//!   （ns/B）与批量-成本曲线；64K 档同时记录宿主钳制事实
//!   （`PLUGIN_PTY_RING_FETCH_MAX_BYTES`=16 KiB，>16K 被截断续拉）；
//! - P3：真 PTY 端到端追赶——spawn 持续输出 4 MiB 的命令，插件以 16K 批量边产边拉，
//!   验证「生产端零暂停 + 拉取侧追赶」（最终 offset 完整 + 无 truncated）；
//! - P4：结论折算在 report.md（本文件打印原始数据点）。
//!
//! 宽松门槛（同 a03 P5 先例）：数量级回归才失败；软性结论以贴档数据为准。
//! 迭代数可用 TERM_PERF_N 覆盖（默认每场景 1 轮全量，抑制 CI 负载）。

use super::*;
use crate::pty::pty_ring::PtyRing;
use std::time::Instant;

/// 确定性输出体量（P2 使用；1 MiB，环容量 4 MiB 下无淘汰干扰）
const PROBE_BYTES: u64 = 1024 * 1024;
/// P3 追赶场景输出体量（3 MiB：环容量声明受宿主上限 `PLUGIN_PTY_RING_MAX_BYTES`=4 MiB
/// 约束，环必须严格大于输出（PtyRing push 在 `resident + len > capacity` 时淘汰最旧块），
/// 故输出取 3 MiB 保证追赶全程无等级淘汰）
const CATCHUP_BYTES: u64 = 3 * 1024 * 1024;
/// spawn 环容量声明（取宿主上限 `PLUGIN_PTY_RING_MAX_BYTES`=4 MiB；
/// 声明必须 ≤ 上限，超限 spawn 直接 Err——本探针不挑战配额）
const PROBE_RING_BYTES: u64 = 4 * 1024 * 1024;

/// 探针专用插件 ID（不与其他测试实例冲突）
const TERM_PERF_PLUGIN: &str = "com.bedcode.term-perf";
/// 输出稳定性等待：spawn 后先轮询确认产出端已把全量字节写进环（而非固定睡），再开始计时拉取，
/// 使 P2 测的是「纯消费侧成本」而非「生产等待时间」。实测 1 MiB 经 PTY 管道输出需要数百 ms
/// （`tr` 逐块吞吐约束），固定 settle 会漏判生产未完成、把等待时间混进消费计时。
const PRODUCE_POLL_TIMEOUT_MS: u64 = 8000;
/// 宿主单次 ring-fetch 返回上限（对齐 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`=16 KiB；
/// 探针内自持常量以便钳制断言，不改生产常量定义）
const HOST_FETCH_MAX_BYTES: u32 = 16 * 1024;

/// 取得环境覆盖的轮数（默认 1；`TERM_PERF_N=3` 取均值）
fn rounds() -> usize {
    std::env::var("TERM_PERF_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// 便捷：向 fixture 发一条命令并解析 JSON（探针统一收口；命令失败即 panic 暴露）
async fn probe_command(plugin: &Mutex<LoadedWasmPlugin>, name: &str, args: serde_json::Value) -> serde_json::Value {
    let mut guard = plugin.lock().await;
    let raw = guard
        .invoke_command(name, &args.to_string())
        .unwrap_or_else(|e| panic!("[perf] invoke_command({name}) 失败: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("[perf] 命令返回非法 JSON: {e} ({raw})"))
}

/// 实例化探针 fixture（spawn 权限 + IO 权限；activate 订阅 exit 事件）
async fn instantiate_probe_fixture(runtime: &WasmRuntime, host_ctx: &Arc<WasmHostContext>) -> Arc<Mutex<LoadedWasmPlugin>> {
    host_ctx.permission.grant_permissions(
        TERM_PERF_PLUGIN,
        &[
            "storage".to_string(),
            "pty:spawn".to_string(),
            "pty:io".to_string(),
        ],
    );
    let component = runtime
        .compile_component(&build_pty_test_component())
        .expect("compile pty fixture for perf probe");
    let plugin = Arc::new(Mutex::new(
        runtime
            .instantiate_component(&component, TERM_PERF_PLUGIN, Arc::clone(host_ctx), &[], None)
            .expect("instantiate pty fixture for perf probe"),
    ));
    host_ctx
        .message_bus
        .set_dispatcher(Arc::new(TestInstanceDispatcher {
            instances: Arc::new(RwLock::new(HashMap::from([(TERM_PERF_PLUGIN.to_string(), Arc::clone(&plugin))]))),
        }))
        .await;
    plugin
        .lock()
        .await
        .activate()
        .expect("activate probe fixture = 订阅 <owner>::pty:exit");
    plugin
}

/// spawn 输出恰好 `bytes` 字节的确定性载体的命令配置（`/bin/sh -c` 裸 argv exec）
fn deterministic_producer(bytes: u64) -> serde_json::Value {
    // `head -c <bytes> /dev/zero | tr '\0' a` 输出恰好 bytes 个 'a'（无换行无 stderr），
    // 后接 `sleep 30` 保持进程长驻——host-pty 语义：进程退出即摘环，探针需要句柄
    // 在测量期间稳定可寻址（对齐 pty_e2e 的 `echo marker; read go` 常驻先例）
    serde_json::json!({
        "command": "/bin/sh",
        "args": ["-c", format!("head -c {bytes} /dev/zero | tr '\\0' a; sleep 30")],
        "ringBytes": PROBE_RING_BYTES,
    })
}

/// 等待产出端把 `target` 字节写满环（轮询 `fetch(offset=target-1, maxBytes=1)`：
/// max_offset 达到 target 时该调用返回 nextOffset==target 的 1 字节；fetch 无状态，
/// 游标由调用方传入，此轮询不污染后续消费游标）
async fn wait_produced(plugin: &Mutex<LoadedWasmPlugin>, pty_id: &str, target: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(PRODUCE_POLL_TIMEOUT_MS);
    loop {
        let resp = probe_command(
            plugin,
            "pty-ring-fetch",
            serde_json::json!({ "ptyId": pty_id, "fromOffset": target - 1, "maxBytes": 1 }),
        )
        .await;
        // none 分支（生产未完成）无 nextOffset 键，必须显式区分——unwflat_or 兜底会把
        // 未写完误判成已完成（fixture none 载荷为 `{ none: true, fromOffset }`）
        if resp.get("none").and_then(|v| v.as_bool()) == Some(true) {
            assert!(
                std::time::Instant::now() < deadline,
                "[perf] 生产端 {PRODUCE_POLL_TIMEOUT_MS}ms 内未写满 {target} B"
            );
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            continue;
        }
        let next = resp["nextOffset"].as_u64().unwrap_or(target);
        if next >= target {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "[perf] 生产端 {PRODUCE_POLL_TIMEOUT_MS}ms 内未写满 {target} B (nextOffset={next})"
        );
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

/// 从 from_offset 循环拉取直到 next_offset 到达 target（返回调用次数与截断标记）
async fn fetch_until(
    plugin: &Mutex<LoadedWasmPlugin>,
    pty_id: &str,
    from_offset: u64,
    target: u64,
    max_bytes: u32,
) -> (usize, Vec<bool>) {
    let mut cursor = from_offset;
    let mut calls = 0usize;
    let mut seen_truncated = Vec::new();
    loop {
        calls += 1;
        let resp = probe_command(
            plugin,
            "pty-ring-fetch",
            serde_json::json!({ "ptyId": pty_id, "fromOffset": cursor, "maxBytes": max_bytes }),
        )
        .await;
        if resp.get("none").and_then(|v| v.as_bool()) == Some(true) {
            // 追平分支：仅当已到 target 时合法（防御性循环保护；数据已 settle 时不应出现）
            if cursor >= target {
                return (calls, seen_truncated);
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            continue;
        }
        cursor = resp["nextOffset"].as_u64().expect("nextOffset");
        seen_truncated.push(resp["truncated"].as_bool().unwrap_or(false));
        if cursor >= target {
            return (calls, seen_truncated);
        }
    }
}

// ==================== P1 · 宿主侧纯 Rust 基线（无 WASM 边界） ====================

/// P1：直接操作 `PtyRing` —— push 1 MiB（4 KiB 块 × 256）→ 全量 fetch。
/// 输出：每字节成本（ns/B）与单次 fetch 固定开销。这是「内核保有 + 无插件」的下界，
/// P2 的所有倍数以它为分母。
#[test]
fn perf_p1_ring_baseline() {
    let block = vec![b'x'; 4096];
    let total = PROBE_BYTES;

    for _ in 0..rounds() {
        let mut ring = PtyRing::new(PROBE_RING_BYTES);
        for _ in 0..(total as usize / block.len()) {
            ring.push(&block);
        }

        // 全量拉取：一次 fetch 取满 vs 按 16K 分片循环（对齐 P2 的钳制批量）
        let t0 = Instant::now();
        let big = ring.fetch(0, total as usize);
        let big_dur = t0.elapsed();
        assert_eq!(big.data.len() as u64, total, "一次性拉取应取满全部字节");

        let t1 = Instant::now();
        let mut cursor = 0u64;
        let mut calls = 0usize;
        while cursor < total {
            calls += 1;
            let f = ring.fetch(cursor, HOST_FETCH_MAX_BYTES as usize);
            cursor = f.next_offset;
        }
        let chunked_dur = t1.elapsed();

        println!(
            "[perf][P1] ring 原生（无 WASM）· total={total} B | 1 次取满 {:.1} us（{:.2} us/MB）| 16K 分片 {calls} 次 {:.1} us（{:.2} us/op, {:.2} us/MB）",
            big_dur.as_micros(),
            big_dur.as_secs_f64() * 1e6 * 1024.0 * 1024.0 / total as f64,
            chunked_dur.as_micros(),
            chunked_dur.as_secs_f64() * 1e6 / calls as f64,
            chunked_dur.as_secs_f64() * 1e6 * 1024.0 * 1024.0 / total as f64,
        );
        let per_op_budget_us = 500.0; // 宽松门：单次原生 fetch 不得 > 0.5ms
        let per_op_us = chunked_dur.as_secs_f64() * 1e6 / calls as f64;
        assert!(per_op_us < per_op_budget_us, "P1 原生 fetch 单次异常慢: {per_op_us} us");
    }
}

// ==================== P2 · 插件侧确定性数据（跨 WASM 边界） ====================

/// P2：同一 1 MiB 确定性数据，guest 经 `pty-ring-fetch` 按不同批量拉取。
/// 输出三条曲线：单次调用开销（µs/op，含 wasmtime 桥 + list<u8> 编解码）、
/// 每字节成本（ns/B）、相对 P1 倍数；64K 档验证宿主钳制事实。
#[test]
fn perf_p2_guest_ring_fetch_batch_curve() {
    let (runtime, host_ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let plugin = instantiate_probe_fixture(&runtime, &host_ctx).await;

        for &max_bytes in &[1024u32, 4096, HOST_FETCH_MAX_BYTES, 64 * 1024] {
            for _ in 0..rounds() {
                let spawned = probe_command(
                    &plugin,
                    "pty-spawn",
                    deterministic_producer(PROBE_BYTES),
                )
                .await;
                let pty_id = spawned["ptyId"].as_str().expect("ptyId").to_string();

                // 先确认产出端已把 1 MiB 写满环，再计时拉取（测纯消费侧成本）
                wait_produced(&plugin, &pty_id, PROBE_BYTES).await;

                let t0 = Instant::now();
                let (calls, seen_truncated) =
                    fetch_until(&plugin, &pty_id, 0, PROBE_BYTES, max_bytes).await;
                let dur = t0.elapsed();

                // 数控事实断言：宿主 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`=16 KiB 截断续拉。
                // 数据已 settle 在环内，1 MiB 全量拉完的调用次数只由单批实际返回字节决定：
                // - maxBytes ≤ 16K：calls = ceil(1 MiB / maxBytes)；
                // - maxBytes = 64K：被钳制到 16K → calls 仍 = 64（若钳制失效则 16）。
                // 调用次数即钳制证据，不用额外请求污染消费游标。
                // 追平竞争窗口（生产末尾块恰在 fetch 前写完 → none 分支多计 1 次）容忍 ±1。
                let effective = max_bytes.min(HOST_FETCH_MAX_BYTES);
                let expected_calls = (PROBE_BYTES as usize).div_ceil(effective as usize);
                let diff = (calls as i64 - expected_calls as i64).unsigned_abs();
                assert!(
                    diff <= 1,
                    "[perf][P2] maxBytes={max_bytes} 调用次数应≈ceil(1MiB/单批实际字节)（64K 档钳制证据）: got {calls}, want {expected_calls}"
                );

                probe_command(&plugin, "pty-kill", serde_json::json!({ "ptyId": pty_id })).await;

                let per_op_us = dur.as_secs_f64() * 1e6 / calls as f64;
                let per_mb_us = dur.as_secs_f64() * 1e6 * 1024.0 * 1024.0 / PROBE_BYTES as f64;
                println!(
                    "[perf][P2] maxBytes={max_bytes}（单批实际 {effective} B）· 1 MiB 拉满：{calls} 次调用 {:.1} us 总（{:.1} us/op, {:.2} us/MB）truncated={}",
                    dur.as_secs_f64() * 1e6,
                    per_op_us,
                    per_mb_us,
                    seen_truncated.iter().any(|&t| t),
                );
                assert!(
                    per_op_us < 5000.0,
                    "[perf][P2] JSON 命令通道单次往返数量级异常（16K 载荷基线 ~1.2ms，>5ms 为回归）: {per_op_us:.1} us/op"
                );
            }
        }
    });
}

// ==================== P3 · 真 PTY 端到端追赶（边产边拉） ====================

/// P3：spawn 持续输出 4 MiB 的命令，插件以 16K 批量**边产边拉**（不预等 settle），
/// 验证「生产端零暂停 + 拉取侧追赶」：最终 offset 完整、无 truncated、无丢块。
#[test]
fn perf_p3_end_to_end_catchup() {
    let (runtime, host_ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let plugin = instantiate_probe_fixture(&runtime, &host_ctx).await;

        for _ in 0..rounds() {
            let spawned = probe_command(
                &plugin,
                "pty-spawn",
                deterministic_producer(CATCHUP_BYTES),
            )
            .await;
            let pty_id = spawned["ptyId"].as_str().expect("ptyId").to_string();

            // 不预等：spawn 返回后立即开始拉取，模拟真实消费节奏
            let t0 = Instant::now();
            let (calls, seen_truncated) =
                fetch_until(&plugin, &pty_id, 0, CATCHUP_BYTES, HOST_FETCH_MAX_BYTES).await;
            let dur = t0.elapsed();

            let truncated = seen_truncated.iter().any(|&t| t);
            probe_command(&plugin, "pty-kill", serde_json::json!({ "ptyId": pty_id })).await;

            let per_op_us = dur.as_secs_f64() * 1e6 / calls.max(1) as f64;
            let per_mb_us = dur.as_secs_f64() * 1e6 * 1024.0 * 1024.0 / CATCHUP_BYTES as f64;
            println!(
                "[perf][P3] 边产边拉 {CATCHUP_BYTES} B：{calls} 次调用 {:.1} us 总（{:.1} us/op, {:.2} us/MB）truncated={truncated}",
                dur.as_secs_f64() * 1e6,
                per_op_us,
                per_mb_us,
            );
            // 生产端经 `tr` 管道吞吐慢是生产侧属性（per_op 含等生产 sleep，不做门槛）；
            // P3 断言的是追赶语义：环容量 > 输出 → 无缺口、offset 完整
            assert!(
                !truncated,
                "[perf][P3] 环容量 {PROBE_RING_BYTES} > 输出 {CATCHUP_BYTES}，边产边拉不应出现缺口"
            );
        }
    });
}

// ==================== P2b · 宿主侧直调原语（隔离 JSON 通道与 wasmtime 桥） ====================

/// P2b：同一 1 MiB 确定性数据，**宿主侧直接调 `pty_ring_fetch` 原语**（不经 guest，
/// 不经 JSON-RPC 命令通道）——量化「原语 + 权限门 + 环」的净开销，作为 P2 与
/// a03 P5 D（guest host-log 往返 38µs/op）之间的分界：P2 的 ~75µs/KB 增量由此归因到
/// fixture 命令面的 JSON 数组编解码，而非 WASM 原语本体。真实插件消费路径（WIT
/// `list<u8>` 经线性内存直传）落在 P2b 与 a03 P5 D 之间。
#[test]
fn perf_p2b_host_side_primitive_call() {
    let (runtime, host_ctx) = setup_wasm_runtime();
    let _e2e_guard = lock_pty_fixture_e2e();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let plugin = instantiate_probe_fixture(&runtime, &host_ctx).await;

        for _ in 0..rounds() {
            let spawned = probe_command(
                &plugin,
                "pty-spawn",
                deterministic_producer(PROBE_BYTES),
            )
            .await;
            let pty_id = spawned["ptyId"].as_str().expect("ptyId").to_string();
            wait_produced(&plugin, &pty_id, PROBE_BYTES).await;

            // 宿主侧直调：权限门 + 注册表锁 + 环 fetch，全程无 WASM 往返
            let primitive = crate::wasm_core::host_api::pty::pty_ring_fetch;
            let t0 = Instant::now();
            let mut cursor = 0u64;
            let mut calls = 0usize;
            while cursor < PROBE_BYTES {
                calls += 1;
                let fetched = primitive(host_ctx.as_ref(), TERM_PERF_PLUGIN, &pty_id, cursor, HOST_FETCH_MAX_BYTES)
                    .expect("pty_ring_fetch")
                    .expect("数据已 settle 必有返回");
                cursor = fetched.next_offset;
                assert!(!fetched.truncated, "settle 后直调不应截断");
            }
            let dur = t0.elapsed();

            probe_command(&plugin, "pty-kill", serde_json::json!({ "ptyId": pty_id })).await;

            let per_op_us = dur.as_secs_f64() * 1e6 / calls as f64;
            let per_mb_us = dur.as_secs_f64() * 1e6 * 1024.0 * 1024.0 / PROBE_BYTES as f64;
            println!(
                "[perf][P2b] 宿主直调原语（无 WASM 无 JSON）· 1 MiB 拉满：{calls} 次调用 {:.1} us 总（{:.1} us/op, {:.2} us/MB）",
                dur.as_secs_f64() * 1e6,
                per_op_us,
                per_mb_us,
            );
            // 原语净开销数量级门：单次（权限门+锁+环 fetch）应 < 100µs（期望 µs 级）
            assert!(per_op_us < 100.0, "[perf][P2b] 原语净开销异常: {per_op_us:.1} us/op");
        }
    });
}