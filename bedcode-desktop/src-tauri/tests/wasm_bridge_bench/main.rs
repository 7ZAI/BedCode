//! 桥接基准工程 · 宿主侧 harness 入口
//!
//! 本 target 是 `bedcode-desktop/bench/` 基准工程的**宿主侧驱动**：
//! 它把 `packages/plugin-bench-test` 夹具编译成 wasm 应用目录，经
//! **生产同形**的 [`PluginHost`]（文件扫描 → 实例化 → 激活 → `invoke_rust_command`）
//! 下发 `bench.*` 命令，测量「前端命令面 → 宿主 → wasm → 宿主原语 → wasm → 宿主」
//! 整条桥接链的性能，并把结果打成表格（可选 JSON 报告）。
//!
//! # 跑法
//!
//! ```bash
//! cd bedcode-desktop/src-tauri
//! cargo test --test wasm_bridge_bench -- --full            # 全量场景（约 1~3 分钟）
//! cargo test --test wasm_bridge_bench -- --full --group C  # 只跑事件推送组
//! cargo test --test wasm_bridge_bench -- --full --json out.json
//! cargo test --release --test wasm_bridge_bench -- --full # release 构件（贴近产品）
//! ```
//!
//! 无参数（`cargo test` 默认）= **冒烟模式**：只跑 3 个数量级门禁探针（秒级），
//! 供 CI 守住「桥接链没有数量级回归」；`--full` 才是完整场景矩阵。
//!
//! # 为什么走 PluginHost 而不是直接 new WasmRuntime
//!
//! `plugin_invoke`（前端唯一命令入口）= 身份校验 + `PluginHost::invoke_rust_command`
//! → `invoke_wasm_command` → 实例锁 → `run_guest_call`（`spawn_blocking` +
//! `Arc<Mutex<LoadedWasmPlugin>>` + `block_on_async`）。直接 new runtime 会绕开
//! 「源判定 / 激活门 / 命令路由」这几段，测出的数就不是前端真实感受到的数。
//!
//! # 已知边界
//!
//! - 无头上下文（`app_handle = None`）：`host-events.emit` 降级为 warn + Ok，
//!   测到的是「guest 序列化 + import 桥」，**不含** Tauri IPC 与前端派发；
//!   后者由 `e2e/specs/bench.spec.ts`（真实 webview）补齐。
//! - guest 内部耗时有 `std::time::Instant`（wasi:clocks）分段回传，harness 的墙钟
//!   与之的差即宿主侧开销（锁 / 序列化 / 事件投递 / 任务调度）。

mod report;
mod scenarios;
mod support;

use std::time::Instant;

use report::Report;
use scenarios::Scenario;

/// 退出码：任一数量级门禁未过 = 1（与 `cargo test` 的失败语义一致）
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let full = args.iter().any(|a| a == "--full");
    let group_filter = arg_value(&args, "--group");
    let json_out = arg_value(&args, "--json");
    let iters = arg_value(&args, "--iters")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(if full { 5 } else { 1 });
    let list_only = args.iter().any(|a| a == "--list");

    if list_only {
        println!("可用场景（--group 过滤）:");
        for s in scenarios::all() {
            println!("  [{}] {:<28} {}", s.group, s.id, s.title);
        }
        return std::process::ExitCode::SUCCESS;
    }

    if !full {
        println!(
            "[bench] 冒烟模式（3 个数量级门禁探针）。全量场景：\n\
             [bench]   cargo test --test wasm_bridge_bench -- --full\n\
             [bench] 场景清单：cargo test --test wasm_bridge_bench -- --list"
        );
    }

    let scenarios: Vec<Scenario> = scenarios::all()
        .into_iter()
        .filter(|s| match &group_filter {
            None => true,
            Some(g) => s.group.eq_ignore_ascii_case(g) || s.id.contains(g.as_str()),
        })
        .filter(|s| full || s.smoke)
        .collect();

    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[bench] tokio runtime 构建失败: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let t_start = Instant::now();
    let env = match rt.block_on(support::build_env(full)) {
        Ok(env) => env,
        Err(e) => {
            eprintln!("[bench] 环境准备失败: {e:#}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!(
        "[bench] 环境就绪：{}（夹具编译 + 插件装载 {:.1}s）",
        env.describe(),
        t_start.elapsed().as_secs_f64()
    );

    let mut report = Report::new(if full { "full" } else { "smoke" }, iters);
    report.set_env(env.describe());

    for scenario in scenarios {
        let t0 = Instant::now();
        match rt.block_on((scenario.run)(&env, iters, &mut report)) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[bench] 场景 {} 失败: {e:#}", scenario.id);
                report.failures.push(format!("{}: {e:#}", scenario.id));
            }
        }
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        println!("[bench] ── {} 完成（{ms:.0} ms）", scenario.id);
    }

    println!("\n{}", report.render_table());
    report.print_budgets();

    if let Some(path) = json_out {
        if let Err(e) = report.write_json(&path) {
            eprintln!("[bench] JSON 报告写入失败: {e}");
        } else {
            println!("[bench] JSON 报告: {path}");
        }
    }

    // 收尾：停用插件 + 清理临时目录（不留监听端口 / 后台任务 / 临时库）
    rt.block_on(env.shutdown());

    if report.failures.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!("[bench] 失败 {} 项", report.failures.len());
        std::process::ExitCode::FAILURE
    }
}
/// 取 `--key value` 形态参数值
fn arg_value(args: &[String], key: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == key)?;
    args.get(pos + 1).cloned()
}
