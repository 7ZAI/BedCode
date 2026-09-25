# 01: P3 async host import 垂直探针

**Type:** prototype
**Blocked by:** None — can start immediately
**Status:** done（Go：宿主实现侧 async 化可用；WIT 层 async import 本轮阻塞）
**Spec:** `../spec.md`

**What to build:** 在不改生产 WIT、ABI、插件契约和运行时行为的前提下，证明当前锁定的 Wasmtime 48 + `wasm32-wasip3` 工具链支持自定义 async host import，并能在一个 import 等待期间让出宿主 Tokio 执行线程。

- 新增测试专用 WIT world，定义至少一个 `async func` host import；不得把该接口加入生产 `plugin` world。
- 新增或扩展 P3 fixture，从 guest export 调用该 async import；宿主测试实现使用原生 `.await`，不得用 `block_on_async`、`spawn_blocking` 或“启动后台任务后同步 wait”伪装 async。
- 宿主实现提供可控的延迟与一次性失败路径，fixture 返回完成值或结构化错误。
- 在 import 挂起期间启动不相关 Tokio heartbeat；断言 heartbeat 在 import 完成前继续推进。
- 同时建立第二实例或第二进入路径，证明优化目标只是“宿主线程让出”，不是同实例并发进入。
- 记录 Wasmtime、Rust toolchain、wit-bindgen、P3 target 与测试环境版本；若生成绑定需要额外 feature，记录精确 feature 组合。
- 探针结论写回本 issue 的 `## Findings`。通过后另开生产纵切 issue；失败则停止公共契约扩面并记录阻塞，不允许把探针接口塞进生产 WIT 兜底。

**Out of scope:**

- 不修改 `bedcode.wit`、desktop ABI、SDK 对外 trait 或任何生产 host implementation。
- 不改 `host-task`、HTTP、WS、peer、fs 或插件源码。
- 不升级 Wasmtime、wit-bindgen、Rust nightly 或移动端工具链。
- 不以探针结果宣称生产吞吐提升；本票只验证机制与测量方法。

**Acceptance:**

- [x] 测试专用 async import 可实例化并完成一次正常调用。
- [x] import 等待期间，不相关 heartbeat 继续推进。
- [x] 同一实例的第二个 guest 进入仍被串行化。
- [x] 立即失败路径返回结构化错误，之后 Store 仍可执行下一次调用。
- [x] 生产 `plugin` world、ABI 常量和所有生产 `host-*` 行为零变更。
- [x] 针对性测试通过：

```bash
cd bedcode-desktop/src-tauri && ~/.cargo/bin/cargo test test_p3_async_host_import_yields_runtime -- --nocapture
# P3 async host import probe passed: heartbeat=8, same_instance_max_active=1, different_instance=true, store_recovered=true
# test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 920 filtered out
```

- [x] issue 末尾记录实际命令、结果、测量环境与 Go/No-Go 裁决。

## Findings

### 1. 测量环境（单一事实来源）

| 项 | 值 | 来源 |
| --- | --- | --- |
| wasmtime / wasmtime-wasi | **48.0.2** | `bedcode-desktop/src-tauri/Cargo.lock` |
| Rust（宿主，stable） | 见 `rustc -vV`（CI 用 `dtolnay/rust-toolchain@stable`） | AGENTS §2 |
| Rust（P3 fixture，pinned nightly） | **nightly-2026-09-16**（`rustc 1.100.0-nightly (215a8af4b 2026-09-15)`） | `scripts/wasip3-toolchain.sh` / `WASIP3_NIGHTLY` |
| P3 target | `wasm32-wasip3` | 同上 |
| wit-bindgen（guest 侧） | **0.60.0**（默认 features 已含 `async`；本票 fixture 只需 `features = ["macros"]`，无需额外 feature） | `packages/plugin-p3-async-host-import-test/Cargo.lock` |
| Engine 配置 | `Config::wasm_component_model_async(true)` | 探针测试内 |

产物与源码：

- 测试专用 world：`bedcode-desktop/packages/plugin-p3-async-host-import-test/wit/p3-async-host-probe.wit`
- fixture guest：`bedcode-desktop/packages/plugin-p3-async-host-import-test/src/lib.rs`
- 宿主探针：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/p3_async_host_import.rs`（`mod tests` 内 `mod p3_async_host_import;`）

### 2. 形态裁决：**宿主实现侧 async 化**（WIT 签名保持同步）

探针落地形态与生产 `host-*` 契约**同签名**（`invoke: func(mode: string) -> result<string, string>`），
guest 侧是纯同步代码；差别只在宿主的注册方式：

```text
生产现状：WIT 同步 func + Linker::func_wrap（同步实现）+ block_on_async 桥 → 占住线程
本票探针：WIT 同步 func + Linker::func_wrap_async（原生 async 实现，真 .await）→ 挂起 fiber
```

`func_wrap_async` 的语义（wasmtime 48 `LinkerInstance::func_wrap_async` 文档 + `runtime/fiber.rs`
`StoreFiber::block_on` 实证）：guest 侧仍然“阻塞”，但宿主侧 future 为 `Pending` 时**挂起
fiber**（`suspend(StoreFiberYield::KeepStore)`），外层 `call_async` future 返回 `Pending`，
Tokio worker 因此被归还。这正是本项目要的收益形态。

结论（对 spec 的关键影响）：**让出宿主执行线程不需要改 WIT、不需要 bump ABI、不需要动
guest/SDK**。spec §4 D4 描述的是「WIT 函数改成 `async func`」这一条路径的 ABI 义务；本票
证明存在另一条更低成本的路径（只换宿主注册方式），P1 应优先评估它，D4 的 ABI bump /
旧产物 fail-visible / 移动端双端评估义务在该路径下**均不触发**。

### 3. 通过的证据链（`current_thread` runtime，最强形态）

单线程 Tokio runtime 上：① 实例 A 进入长等待 import 并挂起 → ② 不相关 heartbeat 跑满 8 次
且 import 仍未完成 → ③ 同实例第二次进入被实例锁串行化（`started` 仍为 1，未进入 host 实现）
→ ④ 实例 B 同时挂起（不同实例互不阻塞）→ ⑤ 释放后 A/B 各自拿到 `wait-complete:{a,b}` →
⑥ 一次性失败路径返回 `p3-probe-forced-failure`，随后 Store 仍能完成 `fail-once-recovered:a`
与 `normal-complete:a`。生产 WIT/ABI 由测试内读文件断言零变更。

### 4. 变异自检（必过项，已执行）

把等待改成「起后台任务 `.await` + 本线程忙等」（等价生产 `block_on_async` 桥占住线程）：
单线程 runtime 下 heartbeat **和** `tokio::time::timeout` 都无法推进（timer 也由同一条线程
驱动），用例**挂死** → 断言确实杀死该变异。为免回归时整轮 `cargo test` 永不返回，测试内加了
`HangWatchdog`（独立 std 线程 20s 兜底 → `eprintln!` + `process::exit(1)`，用例正常结束经
`Drop` 撤销）。变异已还原并复跑通过。

### 5. 阻塞项：WIT 层「`async func` import」本轮走不通（两条死路，均已实证）

1. **同步导出 + async import**：guest 经 `wit_bindgen::rt::async_support::block_on` 等待 →
   wasm trap `cannot block a synchronous task before returning`
   （`wasmtime` 判据 `may_block(task) = task.async_function || returned_or_cancelled()`）。
   即：**async-lowered import 必须由 async-lifted 导出调用**。
2. **async 导出 + async import**：guest 一进入 async-lifted 导出即 abort——
   `wit-bindgen 0.60.0/src/rt/async_support.rs:560` 断言
   `assert!(context_get().is_null())` 失败（`[context-get-0]` 返回非 0）。
   与 guest 是否真的 await 无关（把 `run` 改成立即返回同样 abort）。
   可疑根因（未证实，不下结论）：wasmtime 48 在 `set_thread` 里当 `debug_assertions` 打开时
   会把 context slot 写成 `[u32::MAX; N]` 哨兵（`runtime/component/concurrent.rs`），与
   wit-bindgen 0.60「导出入口 slot 为 0」的假设冲突；验证方式是 `cargo test --release`
   或给 wasmtime 关 debug-assertions 后复跑（本票未做，成本：整棵依赖树 release 重编）。

裁决：**不把探针接口塞进生产 WIT 兜底**（issue 明文禁止）。公共契约扩面前必须先解除上面
第 2 条；在此之前 P1 走第 2 节的宿主实现侧路径。

### 6. Go / No-Go

- **Go**：宿主实现侧 async 化（`func_wrap_async`）在当前锁定工具链下可用，且能在
  `current_thread` runtime 上让出执行线程；同实例串行、跨实例并行、失败后 Store 健康、
  生产 WIT/ABI 零变更全部通过。P0 目标达成，可开 P1 生产纵切 issue。
- **No-Go（本轮）**：WIT 层 `async func` 扩面。理由见第 5 节；它同时会触发 D4 的
  ABI bump、SDK/插件重建、旧产物 fail-visible 与移动端双端评估，代价远大于收益。

### 7. 未做 / 遗留（不在本票范围）

- 未测量生产吞吐；本票只证明机制与测量方法（heartbeat 计数 = 让出证据）。
- 未验证 release 构建下 async 导出是否可用（第 5 节第 2 条的根因验证）。
- 未评估 `func_wrap_concurrent`（`async func` 专用注册面）——它随第 5 节第 2 条一起被阻塞。
- 生产接口纵切（P1）另开 issue：候选按 spec D5 排序，优先「实现侧 async 化」路径。
